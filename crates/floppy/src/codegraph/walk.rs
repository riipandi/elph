//! File-walk skip rules and binary detection for the codegraph indexer.

use std::path::Path;

/// Returns true if a path should be skipped during indexing: known build/dep
/// directories, lockfiles/minified assets, and non-text (binary) extensions.
pub(crate) fn should_skip_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let skip_dirs = [
        "/.git/",
        "/target/",
        "/node_modules/",
        "/.elph/",
        "/dist/",
        "/build/",
        "/.next/",
        "/vendor/",
        "/__pycache__/",
        "/.venv/",
        "/venv/",
        "/.cargo/",
        "/.idea/",
        "/.vscode/",
        "/coverage/",
        "/.turbo/",
        "/.cache/",
        "/Pods/",
        "/.gradle/",
        "/out/",
        "/site-packages/",
        "/third_party/",
        "/third-party/",
        "/.svelte-kit/",
        "/.nuxt/",
        "/.output/",
    ];
    if skip_dirs.iter().any(|d| s.contains(d)) {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || matches!(
            name.as_str(),
            "package-lock.json"
                | "yarn.lock"
                | "pnpm-lock.yaml"
                | "cargo.lock"
                | "composer.lock"
                | "go.sum"
                | "poetry.lock"
        )
    {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "pdf"
            | "zip"
            | "gz"
            | "tar"
            | "woff"
            | "woff2"
            | "ttf"
            | "eot"
            | "mp4"
            | "mp3"
            | "wasm"
            | "so"
            | "dylib"
            | "a"
            | "o"
            | "class"
            | "jar"
            | "exe"
            | "dll"
            | "bin"
            | "lock"
            | "rlib"
            | "rmeta"
            | "pyc"
            | "pyo"
            | "db"
            | "sqlite"
            | "parquet"
    )
}

/// Heuristic: a file looks binary if it contains a NUL byte in its first 8 KiB
/// (text source rarely does).
pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}
