//! Embed build metadata into the binary.
//!
//! Exposes `BUILD_PROFILE`, `BUILD_TARGET`, `BUILD_DATE`, `BUILD_HASH`, and
//! `BUILD_GIT_SHA` as env vars consumed by `cli/version.rs`. The version line
//! changes by profile:
//!   - debug:   `elph 0.0.0-debug os/arch (build_hash yyyy-mm-dd)`
//!   - release: `elph 0.0.0-canary os/arch (commit_hash yyyy-mm-dd)`
//!   - dist:    `elph 0.0.0 os/arch (commit_hash yyyy-mm-dd)`

use sha2::Digest;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // Re-run when the CI-provided SHA changes (e.g. between jobs), so the
    // emitted BUILD_HASH is deterministic per commit and sccache can cache the
    // final crate instead of recompiling it on every build.
    println!("cargo::rerun-if-env-changed=BUILD_GIT_SHA");
    // Track the git HEAD so local branch switches re-run the script.
    if let Some(head) = git_head_path() {
        println!("cargo::rerun-if-changed={}", head.display());
    }

    let profile = env::var("PROFILE").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();
    let opt_level = env::var("OPT_LEVEL").unwrap_or_default();
    let git_sha = resolve_git_sha();
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();

    // Cargo sets PROFILE to the inherited base for custom profiles. The dist
    // profile inherits from release, so both report "release". Distinguish
    // dist by its higher opt-level (3 vs release's 1).
    let is_dist = profile == "dist" || (profile == "release" && opt_level == "3");

    let build_date = format_date(timestamp);
    let build_hash = build_build_hash(git_sha.as_deref());
    let effective_profile = if is_dist { "dist" } else { &profile };

    println!("cargo::rustc-env=BUILD_PROFILE={effective_profile}");
    println!("cargo::rustc-env=BUILD_TARGET={target}");
    println!("cargo::rustc-env=BUILD_DATE={build_date}");
    println!("cargo::rustc-env=BUILD_HASH={build_hash}");
    if let Some(sha) = git_sha {
        println!("cargo::rustc-env=BUILD_GIT_SHA={sha}");
    }
}

/// Resolve the current HEAD SHA. Prefers the env var (set by CI), then falls
/// back to `git rev-parse HEAD`. Returns `None` if git is unavailable or the
/// cwd is not a repository.
fn resolve_git_sha() -> Option<String> {
    if let Ok(sha) = env::var("BUILD_GIT_SHA")
        && !sha.is_empty()
    {
        return Some(sha);
    }
    Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")))
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Format a Unix timestamp as `YYYY-MM-DD` (UTC).
fn format_date(timestamp: u64) -> String {
    let days = timestamp / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Convert days since Unix epoch to `(year, month, day)`.
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let year_days = if is_leap(year) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }
    let mdays = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for (i, &md) in mdays.iter().enumerate() {
        let md = if i == 1 && is_leap(year) { 29 } else { md };
        if days < md {
            month = (i + 1) as u64;
            break;
        }
        days -= md;
        month = (i + 2) as u64;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Produce a short, deterministic build hash from the git SHA.
/// Uses SHA-256 over the SHA and returns the first 8 hex chars. Deterministic
/// per commit so rebuilds of the same commit (e.g. a workflow retry) hit the
/// sccache entry for the final crate instead of recompiling it.
fn build_build_hash(git_sha: Option<&str>) -> String {
    let input = git_sha.unwrap_or("nogit");
    let hash = sha2::Sha256::digest(input.as_bytes());
    encode_hex(&hash[..4])
}

/// Path to the git HEAD file (`.git/HEAD` or a gitdir worktree), used to
/// re-run this build script when the checked-out commit changes.
fn git_head_path() -> Option<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
    let git_dir = PathBuf::from(&manifest_dir).join(".git");
    if git_dir.is_file() {
        // Worktree: `.git` is a file pointing to the real git dir.
        let contents = std::fs::read_to_string(&git_dir).ok()?;
        let path = contents.strip_prefix("gitdir:")?.trim();
        return Some(PathBuf::from(path).join("HEAD"));
    }
    if git_dir.is_dir() {
        return Some(git_dir.join("HEAD"));
    }
    None
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
