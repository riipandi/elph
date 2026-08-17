use crate::platform::{EXIT_SUCCESS, ExitCode};

pub fn handle() -> ExitCode {
    let version = env!("CARGO_PKG_VERSION");
    let profile = option_env!("BUILD_PROFILE").unwrap_or("debug");
    let target = option_env!("BUILD_TARGET").unwrap_or("");
    let build_date = option_env!("BUILD_DATE").unwrap_or("unknown");
    let build_hash = option_env!("BUILD_HASH").unwrap_or("");
    let git_sha = option_env!("BUILD_GIT_SHA").unwrap_or("");

    let version_suffix = match profile {
        "dist" => "",
        "release" => "-canary",
        _ => "-debug",
    };

    let os_arch = simplify_target(target);
    let hash = if profile == "dist" {
        &git_sha[..7.min(git_sha.len())]
    } else {
        build_hash
    };

    println!("elph {version}{version_suffix} {os_arch} ({hash} {build_date})");

    EXIT_SUCCESS
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
