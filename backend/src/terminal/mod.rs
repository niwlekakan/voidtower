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
    uid:   Option<u32>,
    gid:   Option<u32>,
}

fn detect_user_info() -> UserInfo {
    #[cfg(unix)]
    {
        use nix::unistd::{getuid, User};
        // When the backend runs as root (sudo/doas), use the original caller's identity
        let by_name = std::env::var("SUDO_USER")
            .or_else(|_| std::env::var("DOAS_USER"))
            .ok()
            .and_then(|n| User::from_name(&n).ok().flatten());
        let u = by_name.or_else(|| User::from_uid(getuid()).ok().flatten());
        if let Some(u) = u {
            let shell = u.shell.to_string_lossy().to_string();
            let home  = u.dir.to_string_lossy().to_string();
            let name  = u.name.clone();
            let valid = !shell.is_empty() && shell != "/sbin/nologin" && shell != "/bin/false";
            return UserInfo {
                shell: if valid { shell } else { "/bin/bash".into() },
                home:  if home.is_empty() { std::env::var("HOME").unwrap_or_else(|_| "/root".into()) } else { home },
                name,
                uid: Some(u.uid.as_raw()),
                gid: Some(u.gid.as_raw()),
            };
        }
    }
    UserInfo {
        shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()),
        home:  std::env::var("HOME").unwrap_or_else(|_| "/root".into()),
        name:  std::env::var("USER").unwrap_or_else(|_| "root".into()),
        uid:   None,
        gid:   None,
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

    // CommandBuilder clears the env — repopulate what fish needs to behave correctly
    cmd.env("HOME",    &info.home);
    cmd.env("USER",    &info.name);
    cmd.env("LOGNAME", &info.name);
    cmd.env("SHELL",   binary);
    cmd.env("TERM",    "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("PATH",    default_path());

    // Forward locale, XDG dirs, and session bus so fish finds its config and renders correctly
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "LANGUAGE",
                "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                "DBUS_SESSION_BUS_ADDRESS", "WAYLAND_DISPLAY", "DISPLAY"] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

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

    let askpass_path: Option<String> = if let Some(ref pw) = password {
        let path = format!("/tmp/.vt-askpass-{}.sh", uuid::Uuid::new_v4());
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
        if let Some(ref ap) = askpass_path {
            c.args(["-o", "BatchMode=no"]);
            c.env("SSH_ASKPASS", ap);
            c.env("SSH_ASKPASS_REQUIRE", "force");
            c.env("DISPLAY", ":0");
        }
        c.arg(format!("{}@{}", username, host));
        c
    };

    cmd.env("TERM", "xterm-256color");
    cmd.env("PATH", default_path());
    if let Some(ref pw) = password {
        cmd.env("SSHPASS", pw);
    }

    let _child = pair.slave.spawn_command(cmd)?;

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

// PTY↔WebSocket relay loop.
//
// Four independent workers so no single blocking operation can stall the others:
//   reader_thread — blocking PTY read  → output_tx channel
//   writer_thread — write_rx channel   → blocking PTY write  (plain OS thread)
//   out_task      — output_rx channel  → ws_sink.send        (separate tokio::spawn task)
//   input loop    — ws_stream.next     → write_tx.try_send   (this async task)
//
// out_task is a separate spawned task, NOT a future inside select!, so a slow
// WebSocket send (browser receive buffer full) can never starve the input loop.
// fish produces heavy output per keypress (syntax highlighting, right-prompt redraws);
// without task separation that output backs up the single-task select! and drops input.
async fn run_pty_loop(socket: WebSocket, pair: portable_pty::PtyPair) -> Result<()> {
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    // Drop parent's slave fd; child keeps its own copy. This lets the reader
    // thread detect EOF cleanly when the child process exits.
    drop(pair.slave);

    let (mut ws_sink, mut ws_stream) = socket.split();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Option<String>>(256);
    let (write_tx, write_rx) = std::sync::mpsc::sync_channel::<String>(256);

    // PTY reader: blocking reads on a dedicated thread
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

    // PTY writer: plain OS thread — no tokio runtime dependency
    std::thread::spawn(move || {
        let mut writer = writer;
        for data in write_rx { let _ = writer.write_all(data.as_bytes()); }
    });

    // Output task: its own tokio task so ws_sink.send().await blocking on a full
    // WS buffer is completely independent from the input loop below.
    let out_task = tokio::spawn(async move {
        while let Some(item) = output_rx.recv().await {
            match item {
                Some(data) => {
                    let msg = serde_json::to_string(&ServerMessage::Output { data }).unwrap_or_default();
                    if ws_sink.send(Message::Text(msg.into())).await.is_err() { break; }
                }
                None => {
                    let msg = serde_json::to_string(&ServerMessage::Closed).unwrap_or_default();
                    let _ = ws_sink.send(Message::Text(msg.into())).await;
                    break;
                }
            }
        }
    });

    // Input loop: purely async, never touches blocking I/O
    loop {
        match ws_stream.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Input { data }) => { let _ = write_tx.try_send(data); }
                    Ok(ClientMessage::Resize { cols, rows }) => {
                        let _ = pair.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                    }
                    Ok(ClientMessage::Ping) | Err(_) => {}
                }
            }
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
            _ => {}
        }
    }

    out_task.abort();
    Ok(())
}
