//! Execution environment — ported from pi-agent `harness/env/`.

mod local;

use std::path::{Component, Path, PathBuf};

pub use local::LocalExecutionEnv;

pub(crate) fn join_env_path(base: &str, child: &str) -> String {
    let base = base.trim_end_matches('/');
    let child = child.trim_start_matches('/');
    if base.is_empty() {
        child.to_string()
    } else {
        format!("{base}/{child}")
    }
}

pub(crate) fn dirname_env_path(path: &str) -> String {
    let normalized = path.trim_end_matches('/');
    let slash_index = normalized.rfind('/').unwrap_or(0);
    if slash_index == 0 {
        "/".to_string()
    } else {
        normalized[..slash_index].to_string()
    }
}

pub(crate) fn basename_env_path(path: &str) -> String {
    let normalized = path.trim_end_matches('/');
    normalized.rsplit('/').next().unwrap_or(normalized).to_string()
}

pub(crate) fn relative_env_path(root: &str, path: &str) -> String {
    let normalized_root = root.trim_end_matches('/');
    let normalized_path = path.trim_end_matches('/');
    if normalized_path == normalized_root {
        return String::new();
    }
    let prefix = format!("{normalized_root}/");
    if normalized_path.starts_with(&prefix) {
        normalized_path[prefix.len()..].to_string()
    } else {
        normalized_path.trim_start_matches('/').to_string()
    }
}

#[allow(dead_code)]
pub(crate) fn absolute_env_path(cwd: &str, path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        LocalExecutionEnv::normalize_path(path)
    } else {
        let mut base = PathBuf::from(cwd);
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    base.pop();
                }
                Component::Normal(part) => base.push(part),
                Component::RootDir => base = PathBuf::from("/"),
                Component::Prefix(_) => base.push(component.as_os_str()),
            }
        }
        LocalExecutionEnv::normalize_path(&base)
    }
}
