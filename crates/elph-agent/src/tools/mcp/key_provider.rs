//! Machine-bound master key for the sealed auth store.
//!
//! The master AES-256 key never appears on disk in cleartext. Instead it is
//! wrapped (AES-256-GCM encrypted) with a key derived from this machine's
//! hardware UUID via HKDF-SHA256, then persisted at `~/.local/share/elph/auth.lock`.
//!
//! No OS keychain and no user passphrase are required. The wrapped key is
//! bound to this hardware: copying `auth.json` + `auth.lock` to another
//! machine will not decrypt.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;

use super::crypto::Aes256Key;

/// Wrapped-master-key file format version.
const WRAPPED_KEY_VERSION: u8 = 1;

/// HKDF info string for the machine-derived wrapping key.
const WRAPPING_KEY_INFO: &str = "elph-auth-master-wrapping-key-v1";

/// File name for the wrapped master key under the app data dir.
const AUTH_LOCK_FILE_NAME: &str = "auth.lock";

/// AES-256 key length in bytes.
const KEY_LEN: usize = 32;

static PROCESS_KEY_OVERRIDE: OnceLock<Mutex<Option<Aes256Key>>> = OnceLock::new();

fn process_override() -> &'static Mutex<Option<Aes256Key>> {
    PROCESS_KEY_OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// Install a process-local master key (unit tests / headless CI). Bypasses machine binding.
pub fn set_process_master_key_for_tests(key: Aes256Key) {
    *process_override().lock().unwrap_or_else(|e| e.into_inner()) = Some(key);
}

/// Clear process-local override (tests).
pub fn clear_process_master_key_for_tests() {
    *process_override().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Default path for the wrapped master key: `~/.local/share/elph/auth.lock`.
///
/// Respects `XDG_DATA_HOME` when set (`$XDG_DATA_HOME/elph/auth.lock`).
pub fn default_auth_lock_path() -> PathBuf {
    let data_dir = if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.trim().is_empty()
    {
        PathBuf::from(xdg.trim())
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        std::env::temp_dir()
    };
    data_dir.join("elph").join(AUTH_LOCK_FILE_NAME)
}

/// Load the master key from the process override, else unwrap from the machine-bound file.
///
/// On first run (no `auth.lock` present), a random master key is generated,
/// wrapped with the machine-derived key, and persisted. An exclusive flock
/// guards creation so concurrent processes cannot race to create different
/// master keys.
pub fn load_or_create_master_key() -> Result<Aes256Key> {
    load_or_create_master_key_with_prefix("ELPH")
}

/// Same as [`load_or_create_master_key`], reading `{prefix}_AUTH_KEY`.
pub fn load_or_create_master_key_with_prefix(prefix: &str) -> Result<Aes256Key> {
    if let Some(key) = process_override().lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(key);
    }

    // Optional CI escape hatch: full 32-byte key as URL-safe base64 (no pad).
    if let Ok(b64) = std::env::var(format!("{prefix}_AUTH_KEY")) {
        let trimmed = b64.trim();
        if !trimmed.is_empty() {
            return key_from_b64(trimmed);
        }
    }

    let lock_path = default_auth_lock_path();

    // Fast path: file exists → derive wrapping key from stored salt and unwrap.
    if lock_path.exists() {
        return unwrap_master_key(&lock_path).or_else(|e| {
            // Provide actionable guidance when the machine fingerprint has
            // changed (hardware swap, VM clone) or the file was tampered with.
            bail!(
                "{e}. This happens when the machine identifier changes (hardware \
                 change, VM clone) or auth.lock is corrupt. Recovery: delete \
                 auth.lock and auth.json, then re-connect providers/MCP. Set \
                 {prefix}_AUTH_KEY to preserve the same key across machines."
            );
        });
    }

    // Slow path: create a new master key under an exclusive lock so concurrent
    // processes cannot race.
    create_master_key_locked(&lock_path)
}

/// Create a new wrapped master key at `path`, guarded by an exclusive lock.
///
/// If another process wins the lock and creates the file first, we fall back to
/// reading its file instead of overwriting. Uses a `.lock` sidecar with
/// `create_new` for atomic lock acquisition across platforms.
fn create_master_key_locked(path: &Path) -> Result<Aes256Key> {
    // Ensure parent dir exists before locking.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let lock_sidecar = path.with_extension("lock");

    // Spin briefly to acquire the lock sidecar. The holder creates auth.lock
    // then deletes the sidecar; waiters re-check auth.lock each iteration.
    for _ in 0..200 {
        // Try to atomically create the sidecar.
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_sidecar)
        {
            Ok(_lock_handle) => {
                // We hold the lock. Re-check: another process may have
                // created auth.lock between our existence check and now.
                if path.exists() {
                    let _ = std::fs::remove_file(&lock_sidecar);
                    return unwrap_master_key(path);
                }

                // Create the master key.
                let salt = random_salt();
                let wrapping_key = derive_machine_wrapping_key_with_salt(&salt)?;
                let master = Aes256Key::generate();
                wrap_master_key(&master, &wrapping_key, &salt, path)
                    .context("persist wrapped master key to auth.lock")?;

                // Release the lock.
                let _ = std::fs::remove_file(&lock_sidecar);
                return Ok(master);
            }
            Err(_) => {
                // Sidecar exists — another process is creating. Wait briefly
                // then re-check for auth.lock.
                std::thread::sleep(std::time::Duration::from_millis(50));
                if path.exists() {
                    return unwrap_master_key(path);
                }
            }
        }
    }

    // Timeout: the holder may have crashed. Clean up and retry once.
    let _ = std::fs::remove_file(&lock_sidecar);
    bail!(
        "timed out waiting for auth.lock creation. A previous process may have \
         crashed mid-creation. Try deleting {} and restarting.",
        lock_sidecar.display()
    );
}

/// Re-wrap the master key with the current machine fingerprint.
///
/// Call this after a hardware change / OS reinstall when you want to keep using
/// the existing `auth.json` (encrypted credentials) but the old `auth.lock` no
/// longer unwraps. The caller must supply the current plaintext master key (e.g.
/// via `ELPH_AUTH_KEY` or after a successful unwrap with the old
/// machine identity).
///
/// If `auth.lock` is missing, this generates a brand-new master key — which will
/// NOT be able to decrypt the existing `auth.json`. Use with care.
pub fn rewrap_master_key(master: &Aes256Key) -> Result<()> {
    let path = default_auth_lock_path();
    let salt = random_salt();
    let wrapping_key = derive_machine_wrapping_key_with_salt(&salt)?;
    wrap_master_key(master, &wrapping_key, &salt, &path)
}

// ---------------------------------------------------------------------------
// On-disk wrapped key format
// ---------------------------------------------------------------------------

/// On-disk representation of the wrapped master key.
///
/// `blob` is URL-safe base64 of `nonce(12) || AES-256-GCM(master_key)`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WrappedKeyFile {
    /// Format version.
    v: u8,
    /// HKDF salt (hex) so the wrapping key can be re-derived independently of machine id.
    salt: String,
    /// Wrapped key blob: `base64url(nonce || ciphertext+tag)`.
    blob: String,
}

fn wrap_master_key(master: &Aes256Key, wrapping_key: &Aes256Key, salt: &[u8; 16], path: &Path) -> Result<()> {
    let (nonce, ciphertext) = super::crypto::encrypt_sync_bytes(wrapping_key, master.as_bytes())?;

    let mut packed = Vec::with_capacity(nonce.len() + ciphertext.len());
    packed.extend_from_slice(&nonce);
    packed.extend_from_slice(&ciphertext);

    let file = WrappedKeyFile {
        v: WRAPPED_KEY_VERSION,
        salt: crate::utils::hex::encode(salt),
        blob: URL_SAFE_NO_PAD.encode(&packed),
    };

    write_wrapped_key_file(path, &file)
}

#[cfg(test)]
/// Read the salt from an existing lock file, if present and parseable.
fn read_salt_from_lock(path: &Path) -> Option<[u8; 16]> {
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let file: WrappedKeyFile = serde_json::from_str(&raw).ok()?;
    let salt_bytes = crate::utils::hex::decode(&file.salt)?;
    if salt_bytes.len() != 16 {
        return None;
    }
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&salt_bytes);
    Some(salt)
}

fn unwrap_master_key(path: &Path) -> Result<Aes256Key> {
    let file = read_wrapped_key_file(path)?;

    if file.v != WRAPPED_KEY_VERSION {
        bail!("unsupported auth.lock version {} (expected {WRAPPED_KEY_VERSION})", file.v);
    }

    // Decode the salt stored in the same file and derive the wrapping key.
    let salt_bytes = crate::utils::hex::decode(&file.salt).context("decode salt from auth.lock")?;
    if salt_bytes.len() != 16 {
        bail!("auth.lock salt has unexpected length {}", salt_bytes.len());
    }
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&salt_bytes);
    let wrapping_key = derive_machine_wrapping_key_with_salt(&salt)?;

    let packed = URL_SAFE_NO_PAD
        .decode(file.blob.trim())
        .context("decode wrapped key blob from auth.lock")?;
    if packed.len() <= super::crypto::NONCE_LEN {
        bail!("auth.lock blob too short");
    }

    let (nonce, ct) = packed.split_at(super::crypto::NONCE_LEN);
    let master_bytes = super::crypto::decrypt_sync_bytes(&wrapping_key, nonce, ct)
        .context("unwrap master key (machine identifier may have changed, or auth.lock is corrupt)")?;

    if master_bytes.len() != KEY_LEN {
        bail!("unwrapped master key has unexpected length {}", master_bytes.len());
    }

    let mut arr = [0u8; KEY_LEN];
    arr.copy_from_slice(&master_bytes);
    Ok(Aes256Key::from_bytes(arr))
}

fn read_wrapped_key_file(path: &Path) -> Result<WrappedKeyFile> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn write_wrapped_key_file(path: &Path, file: &WrappedKeyFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(file).context("serialize wrapped key file")?;

    // Atomic write via temp file + rename: a crash mid-write cannot corrupt
    // the existing lock file.
    let mut tmp_path = path.as_os_str().to_os_string();
    tmp_path.push(".tmp");
    let tmp_path = PathBuf::from(tmp_path);
    std::fs::write(&tmp_path, json).with_context(|| format!("write temp {}", tmp_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod {}", tmp_path.display()))?;
    }
    std::fs::rename(&tmp_path, path).with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Machine fingerprint + HKDF wrapping key
// ---------------------------------------------------------------------------

/// Derive the wrapping key from this machine's hardware identity using a
/// specific salt.
///
/// The salt must be stable across wrap and unwrap: `wrap_master_key` generates
/// a random salt and stores it in `auth.lock`; `unwrap_master_key` reads that
/// same salt back so the HKDF output is identical.
fn derive_machine_wrapping_key_with_salt(salt: &[u8]) -> Result<Aes256Key> {
    let ikm = machine_fingerprint()?;
    let mut okm = [0u8; KEY_LEN];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), &ikm)
        .expand(WRAPPING_KEY_INFO.as_bytes(), &mut okm)
        .map_err(|e| anyhow::anyhow!("HKDF expand failed: {e}"))?;
    Ok(Aes256Key::from_bytes(okm))
}

/// Generate a fresh random salt for a new wrapping operation.
fn random_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    salt
}

/// Memoized machine fingerprint. Identical to [`machine_fingerprint_uncached`]
/// but computed at most once per process: the machine identity is immutable
/// while Elph runs, yet the macOS/Linux backends shell out to a subprocess
/// (`ioreg` / `wmic`) that costs tens of milliseconds per call. Without
/// memoization the sealed auth store would re-spawn that subprocess on every
/// read (e.g. once per provider per TUI render), freezing the UI.
static MACHINE_FINGERPRINT: OnceLock<Option<Vec<u8>>> = OnceLock::new();

/// Stable, machine-unique identifier material for HKDF input keying material.
///
/// Best-effort: returns the first available identifier. On most platforms
/// this is the hardware UUID which requires root to read on some OSes, but
/// is stable and unique per machine.
///
/// Cached for the process lifetime — see [`MACHINE_FINGERPRINT`].
fn machine_fingerprint() -> Result<Vec<u8>> {
    if let Some(cached) = MACHINE_FINGERPRINT.get() {
        return cached.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "could not determine a stable machine identifier on this platform. \
                 Set ELPH_AUTH_KEY to provide an explicit master key."
            )
        });
    }
    let to_store = machine_fingerprint_uncached().ok();
    let stored = MACHINE_FINGERPRINT.get_or_init(|| to_store);
    match stored {
        Some(bytes) => Ok(bytes.clone()),
        None => Err(anyhow::anyhow!(
            "could not determine a stable machine identifier on this platform. \
             Set ELPH_AUTH_KEY to provide an explicit master key."
        )),
    }
}

/// Uncached implementation of [`machine_fingerprint`]. Prefer the memoized
/// wrapper; this only performs the (potentially slow) platform lookup.
fn machine_fingerprint_uncached() -> Result<Vec<u8>> {
    // macOS: IORegistry platform UUID (stable, unique per hardware).
    #[cfg(target_os = "macos")]
    {
        if let Some(uuid) = read_command_stdout(&["ioreg", "-rd1", "-c", "IOPlatformExpertDevice"], |out| {
            out.lines().find_map(|line| {
                let t = line.trim();
                t.strip_prefix("\"IOPlatformUUID\" = \"")
                    .and_then(|s| s.strip_suffix('\"'))
                    .map(str::to_owned)
            })
        })? {
            return Ok(uuid.into_bytes());
        }
        // Fallback: system_profiler hardware UUID.
        if let Some(uuid) = read_command_stdout(&["system_profiler", "SPHardwareDataType"], |out| {
            out.lines()
                .find_map(|line| line.trim().strip_prefix("Hardware UUID:").map(|s| s.trim().to_owned()))
        })? {
            return Ok(uuid.into_bytes());
        }
    }

    // Linux: machine-id is stable across reboots, unique per install.
    #[cfg(target_os = "linux")]
    {
        if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.as_bytes().to_vec());
            }
        }
        if let Ok(id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.as_bytes().to_vec());
            }
        }
        // Fallback: DMI product UUID (may need root).
        if let Some(uuid) = read_command_stdout(&["cat", "/sys/class/dmi/id/product_uuid"], |out| {
            let trimmed = out.trim();
            if trimmed.is_empty() || trimmed == "Not Settable" || trimmed == "Not Present" {
                None
            } else {
                Some(trimmed.to_owned())
            }
        })? {
            return Ok(uuid.into_bytes());
        }
    }

    // Windows: WMI UUID (VMware/physical) or MachineGuid.
    #[cfg(target_os = "windows")]
    {
        if let Some(uuid) = read_command_stdout(&["wmic", "csproduct", "get", "UUID"], |out| {
            out.lines()
                .nth(1)
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "UUID")
                .map(str::to_owned)
        })? {
            return Ok(uuid.into_bytes());
        }
    }

    bail!(
        "could not determine a stable machine identifier on this platform. \
         Set ELPH_AUTH_KEY to provide an explicit master key."
    );
}

/// Run `cmd`, capture its stdout, and pass it through `extractor`.
///
/// Returns Ok(None) when the command fails or extracts nothing — callers
/// fall back to the next identifier source.
fn read_command_stdout(cmd: &[&str], extractor: impl FnOnce(&str) -> Option<String>) -> Result<Option<String>> {
    let output = std::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .output()
        .with_context(|| format!("run {}", cmd.join(" ")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(extractor(&stdout))
}

fn key_from_b64(secret: &str) -> Result<Aes256Key> {
    let bytes = URL_SAFE_NO_PAD
        .decode(secret.trim())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(secret.trim()))
        .context("master key is not valid base64")?;
    if bytes.len() != KEY_LEN {
        bail!("master key must be {KEY_LEN} bytes, got {}", bytes.len());
    }
    let mut arr = [0u8; KEY_LEN];
    arr.copy_from_slice(&bytes);
    Ok(Aes256Key::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn process_override_roundtrip() {
        clear_process_master_key_for_tests();
        let key = Aes256Key::generate();
        let bytes = *key.as_bytes();
        set_process_master_key_for_tests(key);
        let loaded = load_or_create_master_key().unwrap();
        assert_eq!(loaded.as_bytes(), &bytes);
        clear_process_master_key_for_tests();
    }

    #[test]
    fn key_from_b64_roundtrip() {
        let key = Aes256Key::generate();
        let encoded = URL_SAFE_NO_PAD.encode(key.as_bytes());
        let decoded = key_from_b64(&encoded).unwrap();
        assert_eq!(decoded.as_bytes(), key.as_bytes());
    }

    #[test]
    fn key_from_b64_rejects_wrong_length() {
        let short = URL_SAFE_NO_PAD.encode([0u8; 16]);
        assert!(key_from_b64(&short).is_err());
        let long = URL_SAFE_NO_PAD.encode([0u8; 64]);
        assert!(key_from_b64(&long).is_err());
    }

    /// Wrap with one salt, then read that salt back and unwrap — verifies the
    /// actual persist → reload flow where the salt is stored in the file.
    #[test]
    fn wrap_persist_unwrap_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let salt = random_salt();
        let wrapping = derive_machine_wrapping_key_with_salt(&salt).unwrap();

        wrap_master_key(&master, &wrapping, &salt, &path).unwrap();
        assert!(path.exists());

        // unwrap_master_key reads the salt from the file itself.
        let unwrapped = unwrap_master_key(&path).unwrap();
        assert_eq!(unwrapped.as_bytes(), master.as_bytes());
    }

    #[test]
    fn unwrap_rejects_tampered_blob() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let salt = random_salt();
        let wrapping = derive_machine_wrapping_key_with_salt(&salt).unwrap();
        wrap_master_key(&master, &wrapping, &salt, &path).unwrap();

        // Tamper with the stored file: keep the same salt, corrupt the blob.
        let mut file: WrappedKeyFile = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut blob_bytes = URL_SAFE_NO_PAD.decode(&file.blob).unwrap();
        // Flip a byte in the ciphertext region (after the nonce).
        if blob_bytes.len() > crate::tools::mcp::crypto::NONCE_LEN + 5 {
            blob_bytes[crate::tools::mcp::crypto::NONCE_LEN + 5] ^= 0xff;
        }
        file.blob = URL_SAFE_NO_PAD.encode(&blob_bytes);
        std::fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        let err = unwrap_master_key(&path).unwrap_err();
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("unwrap") || msg.contains("decrypt") || msg.contains("aes"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_or_create_generates_and_reloads() {
        // Exercise the full wrap → persist → reload cycle using the real
        // machine fingerprint. This mirrors what load_or_create does.
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let salt = random_salt();
        let wrapping = derive_machine_wrapping_key_with_salt(&salt).unwrap();
        wrap_master_key(&master, &wrapping, &salt, &path).unwrap();

        // Now unwrap using only the file (reads its own salt).
        let reloaded = unwrap_master_key(&path).unwrap();
        assert_eq!(reloaded.as_bytes(), master.as_bytes());

        // File must contain JSON with the wrapped blob, not the raw key.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"blob\""));
        assert!(!raw.contains(&URL_SAFE_NO_PAD.encode(master.as_bytes())));
    }

    #[test]
    fn read_salt_from_lock_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let salt = random_salt();
        let wrapping = derive_machine_wrapping_key_with_salt(&salt).unwrap();
        wrap_master_key(&Aes256Key::generate(), &wrapping, &salt, &path).unwrap();

        let stored_salt = read_salt_from_lock(&path).unwrap();
        assert_eq!(stored_salt, salt);
    }

    #[test]
    fn read_salt_from_lock_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");
        assert!(read_salt_from_lock(&path).is_none());
    }

    #[test]
    fn read_salt_from_lock_returns_none_when_malformed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");
        std::fs::write(&path, "not json").unwrap();
        assert!(read_salt_from_lock(&path).is_none());
    }

    #[test]
    fn default_auth_lock_path_uses_xdg() {
        let original_xdg = std::env::var_os("XDG_DATA_HOME");
        let original_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-data");
        }
        let p = default_auth_lock_path();
        assert_eq!(p, PathBuf::from("/tmp/xdg-data/elph/auth.lock"));
        unsafe {
            match original_xdg {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            };
            match original_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            };
        }
    }

    /// rewrap_master_key should succeed without panic — it is a thin
    /// wrap around wrap_master_key using the current machine fingerprint.
    /// (Full wrap → unwrap semantics are already tested above.)
    #[test]
    fn rewrap_master_key_does_not_panic() {
        let master = Aes256Key::generate();
        // This calls default_auth_lock_path() internally.
        rewrap_master_key(&master).unwrap();
    }
}
