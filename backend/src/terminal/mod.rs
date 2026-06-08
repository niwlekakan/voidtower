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

    // portable_pty 0.8 has no setuid API.  When the backend runs as root but the
    // target user is someone else, use `su` to actually drop privileges.
    // Setting env vars alone still leaves the process running as root.
    #[cfg(unix)]
    let use_su = {
        use nix::unistd::getuid;
        getuid().is_root() && info.name != "root"
    };
    #[cfg(not(unix))]
    let use_su = false;

    let user_shell = shell.unwrap_or_else(|| info.shell.clone());

    let mut cmd = if use_su {
        // Use `setpriv` instead of `su` for privilege drop.
        //
        // `su -` calls setsid() internally (via PAM/login session management), which
        // creates a new session and detaches from the controlling terminal that
        // portable_pty just set up with TIOCSCTTY.  Fish then has no controlling
        // terminal, tcsetpgrp() silently fails when it tries to put a child (btop, etc.)
        // in the foreground process group, and the PTY line discipline discards input
        // because there is no foreground group to deliver it to.
        //
        // `setpriv` only calls setresuid/setresgid/initgroups then execvp — no setsid,
        // no PAM, no session changes — so the controlling terminal is preserved and job
        // control works correctly inside the shell.
        let mut c = CommandBuilder::new("setpriv");
        c.args([
            &format!("--reuid={}", info.name),
            &format!("--regid={}", info.name),
            "--init-groups",
            "--",
            &user_shell,
        ]);
        c
    } else {
        let parts: Vec<&str> = user_shell.split_whitespace().collect();
        let binary = parts.first().copied().unwrap_or(user_shell.as_str());
        let args = if parts.len() > 1 { &parts[1..] } else { &[][..] };
        let mut c = CommandBuilder::new(binary);
        if !args.is_empty() { c.args(args); }
        c
    };

    // setpriv does not set up a login environment — supply everything explicitly.
    // Same block runs for the non-root path where CommandBuilder clears env.
    cmd.env("TERM",      "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("HOME",    &info.home);
    cmd.env("USER",    &info.name);
    cmd.env("LOGNAME", &info.name);
    cmd.env("SHELL",   &user_shell);
    cmd.env("PATH",    default_path());
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "LANGUAGE",
                "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                "DBUS_SESSION_BUS_ADDRESS", "WAYLAND_DISPLAY", "DISPLAY"] {
        if let Ok(val) = std::env::var(key) { cmd.env(key, val); }
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
// Two worker threads + one async task:
//   reader_thread — blocking PTY read  → output_tx (try_send: never blocks)
//   writer_thread — write_rx channel   → blocking PTY write (plain OS thread)
//   main loop     — select! on output_rx AND ws_stream (single task, no BiLock contest)
//
// socket.split() puts ws_sink and ws_stream behind a shared BiLock.  If they live in
// separate tasks both sides race for the lock: while out_task holds it for a send,
// the input task blocks on ws_stream.next() and cannot receive keystrokes.
//
// By handling both in a single task with select! we guarantee the lock is never held
// by two concurrent pollers.  ws_sink.send() inside the output arm completes, then the
// loop restarts and ws_stream.next() runs — sequential, zero contention.
//
// The reader uses try_send so it NEVER blocks: if the channel is momentarily full the
// chunk is dropped (not the cascade path) rather than stalling the PTY read, which
// would deadlock fish when its PTY output buffer fills.
async fn run_pty_loop(socket: WebSocket, pair: portable_pty::PtyPair) -> Result<()> {
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    // Drop parent's slave fd; child keeps its own copy.
    drop(pair.slave);

    let (mut ws_sink, mut ws_stream) = socket.split();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<String>(256);
    let (write_tx, write_rx) = std::sync::mpsc::sync_channel::<String>(256);

    // PTY reader: blocking reads on a dedicated thread.
    // try_send prevents the reader from ever stalling — chunks are dropped rather than
    // backing up into the PTY buffer and deadlocking the child process.
    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => { tracing::info!("terminal: PTY EOF — process exited"); break; }
                Err(e) => { tracing::warn!("terminal: PTY read error: {e}"); break; }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if output_tx.try_send(data).is_err() {
                        tracing::debug!("terminal: output channel full — dropping chunk");
                    }
                }
            }
        }
        // Dropping output_tx closes the channel → signals EOF to the main loop.
    });

    // PTY writer: plain OS thread — no tokio runtime dependency
    std::thread::spawn(move || {
        let mut writer = writer;
        for data in write_rx {
            if let Err(e) = writer.write_all(data.as_bytes()) {
                tracing::warn!("terminal: PTY write error: {e}");
                break;
            }
        }
        tracing::info!("terminal: writer thread exiting");
    });

    // Main loop — biased: input arm is always checked before output so keystrokes
    // are never starved by a burst of PTY output (e.g. btop's full-screen refresh).
    // CPR (\x1b[6n) handling lives on the frontend: the browser strips the query,
    // responds with the actual xterm.js cursor position, and sends it back as a
    // normal Input message.  Doing it here would inject CPR replies into the PTY
    // alongside user keystrokes which confuses applications like btop that use
    // \x1b[6n to detect terminal dimensions.
    let exit_reason = loop {
        tokio::select! {
            biased;
            // Browser input → PTY  (checked first so keystrokes are never starved)
            frame = ws_stream.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Input { data }) => {
                                if write_tx.try_send(data).is_err() {
                                    tracing::warn!("terminal: write channel full — dropping input");
                                }
                            }
                            Ok(ClientMessage::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                            }
                            Ok(ClientMessage::Ping) | Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) => break "browser closed",
                    None => break "WS stream ended",
                    Some(Err(e)) => { tracing::warn!("terminal: WS error: {e}"); break "WS error"; }
                    _ => {}
                }
            }
            // PTY output → browser
            data = output_rx.recv() => {
                match data {
                    Some(data) => {
                        if !data.is_empty() {
                            let msg = serde_json::to_string(&ServerMessage::Output { data }).unwrap_or_default();
                            if ws_sink.send(Message::Text(msg)).await.is_err() {
                                break "WS send failed";
                            }
                        }
                    }
                    None => {
                        let msg = serde_json::to_string(&ServerMessage::Closed).unwrap_or_default();
                        let _ = ws_sink.send(Message::Text(msg)).await;
                        break "process exited";
                    }
                }
            }
        }
    };
    tracing::info!("terminal: loop exit — {exit_reason}");
    Ok(())
}
