//! In-process locking + atomic write for the sealed auth store.
//!
//! **No `auth.json.lock` sidecar** (legacy name is never created).
//!
//! Cross-process writers take an exclusive flock. On Unix that flock is on the
//! store file itself. On Windows locking the data file is mandatory and blocks
//! `read`/`rename` (os error 33), so the lock is a sibling `auth.json.flock`
//! instead. In-process async mutexes still serialize same-process writers.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use anyhow::{Context, Result};
use tokio::sync::Mutex as AsyncMutex;

/// Process-wide async mutexes keyed by canonical store path.
fn path_mutexes() -> &'static StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>> {
    static MAP: OnceLock<StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    MAP.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn path_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn async_mutex_for(path: &Path) -> Arc<AsyncMutex<()>> {
    let key = path_key(path);
    let mut map = path_mutexes().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(key).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone()
}

/// Guard returned by [`lock_auth_store`].
///
/// Holds an in-process async mutex and an exclusive flock (data file on Unix,
/// sibling `.flock` on Windows). Never creates `auth.json.lock`.
pub struct AuthStoreGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    /// Keep the lock file open for the duration of the guard (lock releases on drop).
    _file: Option<File>,
}

/// Acquire exclusive access to the auth store at `path`.
///
/// Does **not** create `auth.json.lock`. Unix flocks `path`; Windows flocks `path.flock`.
pub async fn lock_auth_store(path: &Path) -> Result<AuthStoreGuard> {
    // Best-effort cleanup of legacy sidecars from older builds.
    let legacy_lock = {
        let mut s = path.as_os_str().to_os_string();
        s.push(".lock");
        PathBuf::from(s)
    };
    let _ = tokio::fs::remove_file(&legacy_lock).await;

    let mutex = async_mutex_for(path);
    let guard = mutex.clone().lock_owned().await;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create auth store dir {}", parent.display()))?;
    }

    // Ensure the store file exists so we can flock the data file itself.
    if !path.exists() {
        // Empty placeholder; seal will overwrite with a real envelope on first save.
        atomic_write_private(path, b"").await?;
    }

    let path_clone = path.to_path_buf();
    let file = tokio::task::spawn_blocking(move || {
        // Unix: advisory flock on the data file (rename-over is allowed while locked).
        // Windows: locking the data file is mandatory and blocks read/rename (os error 33).
        // Use a sibling `.flock` handle so atomic replace of `auth.json` still works.
        let lock_target = {
            #[cfg(windows)]
            {
                let mut s = path_clone.as_os_str().to_os_string();
                s.push(".flock");
                PathBuf::from(s)
            }
            #[cfg(not(windows))]
            {
                path_clone.clone()
            }
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_target)
            .with_context(|| format!("open auth store lock {}", lock_target.display()))?;
        file.lock()
            .with_context(|| format!("exclusive flock on {}", lock_target.display()))?;
        Ok::<File, anyhow::Error>(file)
    })
    .await
    .context("join flock")??;

    Ok(AuthStoreGuard {
        _guard: guard,
        _file: Some(file),
    })
}

/// Atomically write `bytes` to `path` (temp file + rename) under an existing lock.
pub async fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = path.to_path_buf();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || atomic_write_private_sync(&path, &bytes))
        .await
        .context("join atomic write")?
}

fn atomic_write_private_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = {
        let mut p = path.as_os_str().to_os_string();
        p.push(".tmp");
        PathBuf::from(p)
    };
    std::fs::write(&tmp, bytes).with_context(|| format!("write temp {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&tmp, perms).with_context(|| format!("chmod {}", tmp.display()))?;
    }
    replace_file(&tmp, path)
}

/// Atomically replace `to` with `from`. Unix `rename` replaces; Windows needs `MoveFileExW`.
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        replace_file_windows(from, to)
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to).with_context(|| format!("rename {} → {}", from.display(), to.display()))
    }
}

#[cfg(windows)]
fn replace_file_windows(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    let from_w = wide(from);
    let to_w = wide(to);
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        anyhow::bail!("replace {} → {} failed", from.display(), to.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn lock_does_not_create_sidecar_lock_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let _g = lock_auth_store(&path).await.unwrap();
        let mut lock_sidecar = path.as_os_str().to_os_string();
        lock_sidecar.push(".lock");
        assert!(!PathBuf::from(lock_sidecar).exists());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn atomic_write_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        atomic_write_private(&path, b"{\"ok\":true}").await.unwrap();
        let s = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(s, "{\"ok\":true}");
    }
}
