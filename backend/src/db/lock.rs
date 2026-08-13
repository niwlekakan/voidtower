use anyhow::{bail, Context, Result};
use std::{path::Path, time::Duration};

#[cfg(unix)]
pub(crate) struct MigrationLock {
    file: std::fs::File,
}

#[cfg(unix)]
impl Drop for MigrationLock {
    fn drop(&mut self) {
        use nix::fcntl::{flock, FlockArg};
        use std::os::fd::AsRawFd;
        let _ = flock(self.file.as_raw_fd(), FlockArg::Unlock);
    }
}

#[cfg(unix)]
pub(crate) async fn acquire(db_path: &Path, timeout: Duration) -> Result<MigrationLock> {
    use nix::{
        errno::Errno,
        fcntl::{flock, FlockArg},
    };
    use std::{fs::OpenOptions, os::fd::AsRawFd, os::unix::fs::PermissionsExt};

    let file_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("database filename is not valid UTF-8")?;
    let lock_path = db_path.with_file_name(format!("{file_name}.migration.lock"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("failed to open migration lock at {}", lock_path.display()))?;
    std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600)).with_context(
        || {
            format!(
                "failed to protect migration lock at {}",
                lock_path.display()
            )
        },
    )?;

    let started = tokio::time::Instant::now();
    loop {
        match flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            Ok(()) => return Ok(MigrationLock { file }),
            Err(Errno::EWOULDBLOCK) if started.elapsed() < timeout => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(Errno::EWOULDBLOCK) => {
                bail!(
                    "timed out after {}ms waiting for migration lock at {}",
                    timeout.as_millis(),
                    lock_path.display()
                );
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to acquire migration lock at {}",
                        lock_path.display()
                    )
                });
            }
        }
    }
}

#[cfg(not(unix))]
static PROCESS_MIGRATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(not(unix))]
pub(crate) struct MigrationLock {
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

#[cfg(not(unix))]
pub(crate) async fn acquire(_db_path: &Path, timeout: Duration) -> Result<MigrationLock> {
    let guard = tokio::time::timeout(timeout, PROCESS_MIGRATION_LOCK.lock())
        .await
        .context("timed out waiting for process migration lock")?;
    Ok(MigrationLock { _guard: guard })
}
