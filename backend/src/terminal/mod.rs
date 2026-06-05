use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ServerMessage {
    Output { data: String },
    Pong,
    Error { message: String },
    Closed,
}

struct UserInfo {
    shell: String,
    home: String,
    name: String,
}

fn detect_user_info() -> UserInfo {
    #[cfg(unix)]
    {
        use nix::unistd::{getuid, User};
        if let Ok(Some(u)) = User::from_uid(getuid()) {
            let shell = u.shell.to_string_lossy().to_string();
            let home  = u.dir.to_string_lossy().to_string();
            let name  = u.name.clone();
            let valid_shell = !shell.is_empty()
                && shell != "/sbin/nologin"
                && shell != "/bin/false";
            return UserInfo {
                shell: if valid_shell { shell } else { "/bin/bash".into() },
                home:  if home.is_empty() { std::env::var("HOME").unwrap_or_else(|_| "/root".into()) } else { home },
                name,
            };
        }
    }
    UserInfo {
        shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()),
        home:  std::env::var("HOME").unwrap_or_else(|_| "/root".into()),
        name:  std::env::var("USER").unwrap_or_else(|_| "root".into()),
    }
}

pub async fn handle_terminal_ws(
    socket: WebSocket,
    shell: Option<String>,
    _user_id: String,
) {
    if let Err(e) = run_terminal(socket, shell).await {
        tracing::error!("Terminal error: {e}");
    }
}

async fn run_terminal(socket: WebSocket, shell: Option<String>) -> Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;

    let info = detect_user_info();
    let shell_cmd = shell.unwrap_or(info.shell);

    let mut cmd = CommandBuilder::new(&shell_cmd);
    cmd.env("HOME",    &info.home);
    cmd.env("USER",    &info.name);
    cmd.env("LOGNAME", &info.name);
    cmd.env("TERM",    "xterm-256color");
    cmd.cwd(&info.home);
    let _child = pair.slave.spawn_command(cmd)?;

    let reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let (mut ws_sink, mut ws_stream) = socket.split();
    let writer_clone = writer.clone();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<String>(64);

    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if output_tx.blocking_send(data).is_err() { break; }
                }
            }
        }
    });

    loop {
        tokio::select! {
            // PTY output -> WebSocket
            Some(data) = output_rx.recv() => {
                let msg = serde_json::to_string(&ServerMessage::Output { data }).unwrap_or_default();
                if ws_sink.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
            // WebSocket input -> PTY
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Input { data }) => {
                                let mut w = writer_clone.lock().await;
                                let _ = w.write_all(data.as_bytes());
                            }
                            Ok(ClientMessage::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                            Ok(ClientMessage::Ping) => {
                                let msg = serde_json::to_string(&ServerMessage::Pong).unwrap_or_default();
                                let _ = ws_sink.send(Message::Text(msg)).await;
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

pub async fn handle_ssh_ws(
    socket: WebSocket,
    host: String,
    port: u16,
    username: String,
) {
    if let Err(e) = run_ssh(socket, host, port, username).await {
        tracing::error!("SSH terminal error: {e}");
    }
}

async fn run_ssh(socket: WebSocket, host: String, port: u16, username: String) -> Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;

    let mut cmd = CommandBuilder::new("ssh");
    cmd.args([
        "-o", "StrictHostKeyChecking=accept-new",
        "-o", "ServerAliveInterval=30",
        "-p", &port.to_string(),
        &format!("{}@{}", username, host),
    ]);
    cmd.env("TERM", "xterm-256color");
    let _child = pair.slave.spawn_command(cmd)?;

    let reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let (mut ws_sink, mut ws_stream) = socket.split();
    let writer_clone = writer.clone();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<String>(64);

    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if output_tx.blocking_send(data).is_err() { break; }
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(data) = output_rx.recv() => {
                let msg = serde_json::to_string(&ServerMessage::Output { data }).unwrap_or_default();
                if ws_sink.send(Message::Text(msg)).await.is_err() { break; }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Input { data }) => {
                                let mut w = writer_clone.lock().await;
                                let _ = w.write_all(data.as_bytes());
                            }
                            Ok(ClientMessage::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                            }
                            Ok(ClientMessage::Ping) => {
                                let msg = serde_json::to_string(&ServerMessage::Pong).unwrap_or_default();
                                let _ = ws_sink.send(Message::Text(msg)).await;
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
