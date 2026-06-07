use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

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
    home:  String,
    name:  String,
}

fn detect_user_info() -> UserInfo {
    #[cfg(unix)]
    {
        use nix::unistd::{getuid, User};
        if let Ok(Some(u)) = User::from_uid(getuid()) {
            let shell = u.shell.to_string_lossy().to_string();
            let home  = u.dir.to_string_lossy().to_string();
            let name  = u.name.clone();
            let valid = !shell.is_empty() && shell != "/sbin/nologin" && shell != "/bin/false";
            return UserInfo {
                shell: if valid { shell } else { "/bin/bash".into() },
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

fn default_path() -> String {
    let base = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    match std::env::var("PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => base.into(),
    }
}

pub async fn handle_terminal_ws(socket: WebSocket, shell: Option<String>, _user_id: String) {
    if let Err(e) = run_terminal(socket, shell).await {
        tracing::error!("Terminal error: {e}");
    }
}

async fn run_terminal(socket: WebSocket, shell: Option<String>) -> Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;

    let info = detect_user_info();
    let shell_cmd = shell.unwrap_or(info.shell.clone());
    let parts: Vec<&str> = shell_cmd.split_whitespace().collect();
    let (binary, args) = parts.split_first()
        .map(|(b, a)| (*b, a.to_vec()))
        .unwrap_or((&info.shell, vec![]));

    let mut cmd = CommandBuilder::new(binary);
    if !args.is_empty() { cmd.args(&args); }
    cmd.env("HOME",    &info.home);
    cmd.env("USER",    &info.name);
    cmd.env("LOGNAME", &info.name);
    cmd.env("TERM",    "xterm-256color");
    cmd.env("PATH",    default_path());
    cmd.cwd(&info.home);
    let _child = pair.slave.spawn_command(cmd)?;

    run_pty_loop(socket, pair).await
}

pub async fn handle_ssh_ws(
    socket: WebSocket,
    host: String,
    port: u16,
    username: String,
    key_path: Option<String>,
    password: Option<String>,
) {
    if let Err(e) = run_ssh(socket, host, port, username, key_path, password).await {
        tracing::error!("SSH terminal error: {e}");
    }
}

async fn run_ssh(
    socket: WebSocket,
    host: String,
    port: u16,
    username: String,
    key_path: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;

    // Write a temp askpass script when password is set — works even without sshpass
    let askpass_path: Option<String> = if let Some(ref pw) = password {
        let path = format!("/tmp/.vt-askpass-{}.sh", uuid::Uuid::new_v4());
        // Escape single quotes in password for shell safety
        let safe_pw = pw.replace('\'', "'\\''");
        let script = format!("#!/bin/sh\nprintf '%s' '{}'\n", safe_pw);
        if std::fs::write(&path, &script).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
            }
            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    let use_sshpass = password.is_some() && which_bin("sshpass");

    let mut cmd = if use_sshpass {
        // sshpass reads password from SSHPASS env var via -e flag
        let mut c = CommandBuilder::new("sshpass");
        c.arg("-e");
        c.args(["ssh",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ServerAliveInterval=30",
            "-o", "ServerAliveCountMax=3",
            "-o", "BatchMode=no",
            "-p", &port.to_string(),
        ]);
        if let Some(ref kp) = key_path { c.args(["-i", kp]); }
        c.arg(format!("{}@{}", username, host));
        c
    } else {
        let mut c = CommandBuilder::new("ssh");
        c.args([
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ServerAliveInterval=30",
            "-o", "ServerAliveCountMax=3",
            "-p", &port.to_string(),
        ]);
        if let Some(ref kp) = key_path { c.args(["-i", kp]); }
        // SSH_ASKPASS path — works on OpenSSH 8.4+ with SSH_ASKPASS_REQUIRE=force
        // On older versions falls back to interactive prompt
        if let Some(ref ap) = askpass_path {
            c.args(["-o", "BatchMode=no"]);
            c.env("SSH_ASKPASS", ap);
            c.env("SSH_ASKPASS_REQUIRE", "force");
            c.env("DISPLAY", ":0"); // needed for older OpenSSH fallback
        }
        c.arg(format!("{}@{}", username, host));
        c
    };

    cmd.env("TERM", "xterm-256color");
    cmd.env("PATH", default_path());
    // For sshpass -e: set SSHPASS in env
    if let Some(ref pw) = password {
        cmd.env("SSHPASS", pw);
    }

    let _child = pair.slave.spawn_command(cmd)?;

    // Clean up askpass script after session ends
    let result = run_pty_loop(socket, pair).await;
    if let Some(path) = askpass_path {
        let _ = std::fs::remove_file(&path);
    }
    result
}

fn which_bin(name: &str) -> bool {
    std::process::Command::new("which").arg(name).output()
        .map(|o| o.status.success()).unwrap_or(false)
}

// Shared PTY↔WebSocket relay loop
async fn run_pty_loop(socket: WebSocket, pair: portable_pty::PtyPair) -> Result<()> {
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let (mut ws_sink, mut ws_stream) = socket.split();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Option<String>>(64);
    // Single persistent writer thread: select loop does fast try_send, never blocks
    let (write_tx, write_rx) = tokio::sync::mpsc::channel::<String>(128);

    // Reader thread: Some(data) on output, None on EOF / process exit
    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => { let _ = output_tx.blocking_send(None); break; }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if output_tx.blocking_send(Some(data)).is_err() { break; }
                }
            }
        }
    });

    // Writer thread: one thread owns the PTY writer; exits when write_tx is dropped
    tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        let mut rx = write_rx;
        while let Some(data) = rx.blocking_recv() {
            let _ = writer.write_all(data.as_bytes());
            let _ = writer.flush();
        }
    });

    loop {
        tokio::select! {
            item = output_rx.recv() => {
                match item {
                    Some(Some(data)) => {
                        let msg = serde_json::to_string(&ServerMessage::Output { data }).unwrap_or_default();
                        if ws_sink.send(Message::Text(msg.into())).await.is_err() { break; }
                    }
                    Some(None) | None => {
                        let msg = serde_json::to_string(&ServerMessage::Closed).unwrap_or_default();
                        let _ = ws_sink.send(Message::Text(msg.into())).await;
                        break;
                    }
                }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Input { data }) => {
                                let _ = write_tx.try_send(data);
                            }
                            Ok(ClientMessage::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                            }
                            Ok(ClientMessage::Ping) => {
                                let msg = serde_json::to_string(&ServerMessage::Pong).unwrap_or_default();
                                let _ = ws_sink.send(Message::Text(msg.into())).await;
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
