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
    // Arc<Mutex> makes the !Sync master safe across async .await boundaries
    let master = std::sync::Arc::new(std::sync::Mutex::new(pair.master));
    // Parent slave fd isn't needed once the child is running; dropping it lets
    // the PTY reader detect EOF cleanly when the child exits.
    drop(pair.slave);

    let (mut ws_sink, mut ws_stream) = socket.split();
    // Large buffer: fish produces heavy output per keystroke (highlighting, prompts)
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Option<String>>(256);
    // std::sync::mpsc: no tokio runtime, safe in a plain OS thread
    let (write_tx, write_rx) = std::sync::mpsc::sync_channel::<String>(256);

    // PTY reader thread → output channel
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

    // PTY writer thread — plain OS thread; exits when write_tx is dropped
    std::thread::spawn(move || {
        let mut w = writer;
        for data in write_rx { let _ = w.write_all(data.as_bytes()); }
    });

    // Two independent async loops via top-level select!:
    //   output_fut owns ws_sink and drains PTY output → WS
    //   input_fut  owns ws_stream and routes WS input → PTY
    // Neither can block the other, eliminating fish's output/input flow-control deadlock.
    let output_fut = async {
        while let Some(item) = output_rx.recv().await {
            let done = item.is_none();
            let msg = match item {
                Some(data) => serde_json::to_string(&ServerMessage::Output { data }),
                None       => serde_json::to_string(&ServerMessage::Closed),
            }.unwrap_or_default();
            if ws_sink.send(Message::Text(msg.into())).await.is_err() || done { break; }
        }
    };

    let input_fut = async {
        while let Some(Ok(Message::Text(text))) = ws_stream.next().await {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Input { data }) => { let _ = write_tx.send(data); }
                Ok(ClientMessage::Resize { cols, rows }) => {
                    if let Ok(m) = master.lock() {
                        let _ = m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                    }
                }
                Ok(ClientMessage::Ping) | Err(_) => {}
            }
        }
    };

    tokio::select! {
        _ = output_fut => {}
        _ = input_fut  => {}
    }
    Ok(())
}
