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

/// Acquire a cross-process compatibility lock for a database inode.
/// Inode identity unifies canonical paths, symlinks, and hard-link aliases while
/// bounding lock files to one per live database inode rather than one per key.
pub(crate) async fn acquire_scoped(
    db_path: &Path,
    _scope: &str,
    _key: &str,
    timeout: Duration,
) -> Result<MigrationLock> {
    #[cfg(unix)]
    {
        use std::os::unix::{fs::MetadataExt, fs::PermissionsExt};
        let canonical_db = std::fs::canonicalize(db_path)
            .with_context(|| format!("failed to resolve database path {}", db_path.display()))?;
        let metadata = std::fs::metadata(&canonical_db).with_context(|| {
            format!("failed to inspect database path {}", canonical_db.display())
        })?;
        let uid = nix::unistd::Uid::effective().as_raw();
        let lock_dir = std::env::temp_dir().join(format!("voidtower-locks-{uid}"));
        let created = match std::fs::create_dir(&lock_dir) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to create lock directory {}", lock_dir.display())
                })
            }
        };
        if created {
            std::fs::set_permissions(&lock_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let dir_metadata = std::fs::symlink_metadata(&lock_dir)?;
        if !dir_metadata.file_type().is_dir()
            || dir_metadata.uid() != uid
            || dir_metadata.permissions().mode() & 0o077 != 0
        {
            bail!("compatibility lock directory is not owner-only and trustworthy");
        }
        let identity_path = lock_dir.join(format!("db-{:x}-{:x}", metadata.dev(), metadata.ino()));
        acquire(&identity_path, timeout).await
    }
    #[cfg(not(unix))]
    {
        acquire(db_path, timeout).await
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

#[cfg(test)]
mod compatibility_lock_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn child_process_cannot_acquire_parent_held_key() {
        let Ok(db_path) = std::env::var("VOIDTOWER_LOCK_CHILD_DB") else {
            return;
        };
        let result = acquire_scoped(
            Path::new(&db_path),
            "human:user-cross-process",
            "service-restart-cross-process",
            Duration::from_millis(75),
        )
        .await;
        assert!(
            result.is_err(),
            "independent process acquired a held idempotency lock"
        );
    }

    #[tokio::test]
    async fn scoped_file_lock_excludes_an_independent_process() {
        let dir = std::env::temp_dir().join(format!(
            "voidtower-lock-process-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("voidtower.db");
        std::fs::write(&db_path, b"test database").unwrap();
        let guard = acquire_scoped(
            &db_path,
            "human:user-cross-process",
            "service-restart-cross-process",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "db::lock::compatibility_lock_tests::child_process_cannot_acquire_parent_held_key",
                "--nocapture",
            ])
            .env("VOIDTOWER_LOCK_CHILD_DB", &db_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child lock assertion failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        drop(guard);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn hard_link_aliases_of_the_database_share_the_compatibility_lock() {
        let dir = std::env::temp_dir().join(format!(
            "voidtower-lock-hardlink-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("primary.db");
        let alias = dir.join("alias.db");
        std::fs::write(&primary, b"test database").unwrap();
        std::fs::hard_link(&primary, &alias).unwrap();
        let held = acquire_scoped(&primary, "scope", "key", Duration::from_secs(1))
            .await
            .unwrap();
        let competing = acquire_scoped(&alias, "scope", "key", Duration::from_millis(50)).await;
        assert!(
            competing.is_err(),
            "hard-link aliases must resolve to one lock"
        );
        drop(held);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn same_credential_key_is_exclusive_across_independent_file_descriptors() {
        let dir =
            std::env::temp_dir().join(format!("voidtower-lock-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("voidtower.db");
        std::fs::write(&db_path, b"test database").unwrap();
        let first = acquire_scoped(
            &db_path,
            "human:user-1",
            "service-restart-1",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let second = acquire_scoped(
            &db_path,
            "human:user-1",
            "service-restart-1",
            Duration::from_millis(50),
        )
        .await;
        assert!(
            second.is_err(),
            "the same scope/key must serialize even with separate opens"
        );
        drop(first);
        let third = acquire_scoped(
            &db_path,
            "human:user-1",
            "service-restart-1",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let other_key = acquire_scoped(
            &db_path,
            "human:user-1",
            "service-restart-2",
            Duration::from_millis(50),
        )
        .await;
        assert!(
            other_key.is_err(),
            "a stable per-database lock serializes distinct keys without creating per-key files"
        );
        drop(other_key);
        drop(third);
        let next = acquire_scoped(
            &db_path,
            "human:user-1",
            "service-restart-2",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        drop(next);
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(&db_path).unwrap();
        let uid = nix::unistd::Uid::effective().as_raw();
        let lock_dir = std::env::temp_dir().join(format!("voidtower-locks-{uid}"));
        let expected_name = format!(
            "db-{:x}-{:x}.migration.lock",
            metadata.dev(),
            metadata.ino()
        );
        let compatibility_lock_files = std::fs::read_dir(&lock_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy() == expected_name)
            .count();
        assert_eq!(
            compatibility_lock_files, 1,
            "unique request keys must reuse one stable database lock file"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
