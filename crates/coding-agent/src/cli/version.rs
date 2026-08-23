use crate::platform::{EXIT_SUCCESS, ExitCode};

pub fn handle() -> ExitCode {
    println!("{}", version_line());
    EXIT_SUCCESS
}

/// Single-line version used by `-V` and `elph doctor`.
pub fn version_line() -> String {
    let identity = build_identity();
    format!(
        "elph {}{} {} ({} {})",
        identity.version, identity.suffix, identity.os_arch, identity.hash, identity.build_date
    )
}

#[derive(Debug, Clone)]
pub struct BuildIdentity {
    pub version: String,
    pub suffix: String,
    pub profile: String,
    pub os_arch: String,
    pub target: String,
    pub hash: String,
    pub git_sha: String,
    pub build_date: String,
}

pub fn build_identity() -> BuildIdentity {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let profile = option_env!("BUILD_PROFILE").unwrap_or("debug").to_string();
    let target = option_env!("BUILD_TARGET").unwrap_or("").to_string();
    let build_date = option_env!("BUILD_DATE").unwrap_or("unknown").to_string();
    let build_hash = option_env!("BUILD_HASH").unwrap_or("").to_string();
    let git_sha = option_env!("BUILD_GIT_SHA").unwrap_or("").to_string();

    let suffix = match profile.as_str() {
        "dist" => String::new(),
        "release" => "-canary".to_string(),
        _ => "-debug".to_string(),
    };

    let os_arch = simplify_target(&target);
    let hash = match profile.as_str() {
        "dist" | "release" => git_sha.chars().take(7).collect(),
        _ => build_hash,
    };

    BuildIdentity {
        version,
        suffix,
        profile,
        os_arch,
        target,
        hash,
        git_sha,
        build_date,
    }
}

/// Convert a Cargo target triple like `aarch64-apple-darwin` into `darwin/arm64`.
fn simplify_target(target: &str) -> String {
    let parts: Vec<&str> = target.split('-').collect();
    let arch = match parts.first() {
        Some(&"aarch64") => "arm64",
        Some(&"x86_64") => "amd64",
        other => other.unwrap_or(&""),
    };
    let os = match parts.get(1) {
        Some(&"apple") => "darwin",
        Some(&"linux") => "linux",
        Some(&"windows") => "windows",
        _ => "",
    };
    format!("{os}/{arch}")
}
