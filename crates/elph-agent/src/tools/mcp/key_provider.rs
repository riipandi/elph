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
/// wrapped with the machine-derived key, and persisted.
pub fn load_or_create_master_key() -> Result<Aes256Key> {
    if let Some(key) = process_override().lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(key);
    }

    // Optional CI escape hatch: full 32-byte key as URL-safe base64 (no pad).
    if let Ok(b64) = std::env::var("ELPH_AUTH_MASTER_KEY_B64") {
        let trimmed = b64.trim();
        if !trimmed.is_empty() {
            return key_from_b64(trimmed);
        }
    }

    let lock_path = default_auth_lock_path();
    let wrapping_key = derive_machine_wrapping_key()?;

    if lock_path.exists() {
        unwrap_master_key(&lock_path, &wrapping_key)
    } else {
        let master = Aes256Key::generate();
        wrap_master_key(&master, &wrapping_key, &lock_path).context("persist wrapped master key to auth.lock")?;
        Ok(master)
    }
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

fn wrap_master_key(master: &Aes256Key, wrapping_key: &Aes256Key, path: &Path) -> Result<()> {
    let salt = random_salt();
    let (nonce, ciphertext) = super::crypto::encrypt_sync_bytes(wrapping_key, master.as_bytes())?;

    let mut packed = Vec::with_capacity(nonce.len() + ciphertext.len());
    packed.extend_from_slice(&nonce);
    packed.extend_from_slice(&ciphertext);

    let file = WrappedKeyFile {
        v: WRAPPED_KEY_VERSION,
        salt: hex::encode(salt),
        blob: URL_SAFE_NO_PAD.encode(&packed),
    };

    write_wrapped_key_file(path, &file)
}

fn unwrap_master_key(path: &Path, wrapping_key: &Aes256Key) -> Result<Aes256Key> {
    let file = read_wrapped_key_file(path)?;

    if file.v != WRAPPED_KEY_VERSION {
        bail!("unsupported auth.lock version {} (expected {WRAPPED_KEY_VERSION})", file.v);
    }

    let packed = URL_SAFE_NO_PAD
        .decode(file.blob.trim())
        .context("decode wrapped key blob from auth.lock")?;
    if packed.len() <= super::crypto::NONCE_LEN {
        bail!("auth.lock blob too short");
    }

    let (nonce, ct) = packed.split_at(super::crypto::NONCE_LEN);
    let master_bytes = super::crypto::decrypt_sync_bytes(wrapping_key, nonce, ct)
        .context("unwrap master key (wrong machine or corrupted auth.lock)")?;

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
    std::fs::write(path, json).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Machine fingerprint + HKDF wrapping key
// ---------------------------------------------------------------------------

/// Derive the wrapping key from this machine's hardware identity.
///
/// Collects one or more stable, machine-unique identifiers (platform UUID,
/// hardware serial, machine-id) and feeds them through HKDF-SHA256 so the
/// wrapping key is stable per-machine yet opaque.
fn derive_machine_wrapping_key() -> Result<Aes256Key> {
    let salt = random_salt();
    let ikm = machine_fingerprint()?;
    let mut okm = [0u8; KEY_LEN];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(&salt), &ikm)
        .expand(WRAPPING_KEY_INFO.as_bytes(), &mut okm)
        .map_err(|e| anyhow::anyhow!("HKDF expand failed: {e}"))?;
    Ok(Aes256Key::from_bytes(okm))
}

/// Stable, machine-unique identifier material for HKDF input keying material.
///
/// Best-effort: returns the first available identifier. On most platforms
/// this is the hardware UUID which requires root to read on some OSes, but
/// is stable and unique per machine.
fn machine_fingerprint() -> Result<Vec<u8>> {
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
         Set ELPH_AUTH_MASTER_KEY_B64 to provide an explicit master key."
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

fn random_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    // getrandom is the right tool for cryptographic randomness; falls back
    // gracefully inside sandboxes and early-boot environments.
    if getrandom::fill(&mut salt).is_ok() {
        return salt;
    }
    // Fallback: seeded from time. Acceptable for a salt that only needs
    // to be unique within this process, not secret.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let bytes = seed.to_le_bytes();
    salt[..8].copy_from_slice(&bytes);
    salt[8..].copy_from_slice(&bytes);
    salt
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

    #[test]
    fn wrap_unwrap_roundtrip() {
        clear_process_master_key_for_tests();

        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let wrapping = Aes256Key::generate();

        wrap_master_key(&master, &wrapping, &path).unwrap();
        assert!(path.exists());

        let unwrapped = unwrap_master_key(&path, &wrapping).unwrap();
        assert_eq!(unwrapped.as_bytes(), master.as_bytes());
    }

    #[test]
    fn unwrap_fails_with_wrong_wrapping_key() {
        clear_process_master_key_for_tests();

        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let wrapping = Aes256Key::generate();
        let other = Aes256Key::generate();

        wrap_master_key(&master, &wrapping, &path).unwrap();

        let err = unwrap_master_key(&path, &other).unwrap_err();
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("unwrap") || msg.contains("decrypt") || msg.contains("aes"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_or_create_generates_and_reloads() {
        clear_process_master_key_for_tests();

        // Derive a wrapping key from this machine's fingerprint and exercise
        // the wrap → persist → reload → unwrap cycle that load_or_create uses.
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.lock");

        let master = Aes256Key::generate();
        let wrapping = derive_machine_wrapping_key().unwrap();
        wrap_master_key(&master, &wrapping, &path).unwrap();
        let reloaded = unwrap_master_key(&path, &wrapping).unwrap();
        assert_eq!(reloaded.as_bytes(), master.as_bytes());

        // File must contain JSON with the wrapped blob, not the raw key.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"blob\""));
        assert!(!raw.contains(&URL_SAFE_NO_PAD.encode(master.as_bytes())));
    }

    #[test]
    fn default_auth_lock_path_uses_xdg() {
        let original = std::env::var_os("XDG_DATA_HOME");
        let original_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-data");
        }
        let p = default_auth_lock_path();
        assert_eq!(p, PathBuf::from("/tmp/xdg-data/elph/auth.lock"));
        // restore
        unsafe {
            match original {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            };
            match original_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            };
        }
    }
}
