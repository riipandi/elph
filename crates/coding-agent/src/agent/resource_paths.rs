//! Identity for skill/prompt/agent search directories.
//!
//! The same folder can appear twice as an absolute project path and as a
//! cwd-relative extra (`resources.skills: [".agents/skills"]`). Treat those as
//! one source so the startup conflict notice is not a false positive.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Stable key for a resource directory (canonicalize when the path exists).
pub fn resource_dir_identity(path: impl AsRef<Path>, bases: &[&Path]) -> PathBuf {
    let path = path.as_ref();
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        resolve_relative(path, bases)
    };
    candidate
        .canonicalize()
        .unwrap_or_else(|_| normalize_lexically(&candidate))
}

fn resolve_relative(path: &Path, bases: &[&Path]) -> PathBuf {
    for base in bases {
        let joined = base.join(path);
        if joined.exists() {
            return joined;
        }
    }
    bases
        .first()
        .map(|base| base.join(path))
        .unwrap_or_else(|| path.to_path_buf())
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Keep the first entry for each directory identity (built-in labels win over extras).
pub fn dedupe_resource_dirs(entries: Vec<(String, String)>, bases: &[&Path]) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for (path, label) in entries {
        let key = resource_dir_identity(&path, bases);
        if !seen.insert(key) {
            continue;
        }
        out.push((path, label));
    }
    out
}

/// Same as [`dedupe_resource_dirs`] for `PathBuf` search entries (agents).
pub fn dedupe_resource_pathbufs(entries: Vec<(PathBuf, String)>, bases: &[&Path]) -> Vec<(PathBuf, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for (path, label) in entries {
        let key = resource_dir_identity(&path, bases);
        if !seen.insert(key) {
            continue;
        }
        out.push((path, label));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn relative_and_absolute_same_existing_dir() {
        let tmp = TempDir::new().unwrap();
        let abs = tmp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&abs).unwrap();
        let a = resource_dir_identity(&abs, &[tmp.path()]);
        let b = resource_dir_identity(".agents/skills", &[tmp.path()]);
        let c = resource_dir_identity("./.agents/skills", &[tmp.path()]);
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn dedupe_drops_extra_alias() {
        let tmp = TempDir::new().unwrap();
        let abs = tmp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&abs).unwrap();
        let entries = vec![
            (
                abs.to_string_lossy().into_owned(),
                format!("{}/.agents/skills", tmp.path().display()),
            ),
            (".agents/skills".into(), ".agents/skills".into()),
        ];
        let out = dedupe_resource_dirs(entries, &[tmp.path()]);
        assert_eq!(out.len(), 1);
        assert!(out[0].0.ends_with(".agents/skills") || Path::new(&out[0].0).ends_with("skills"));
        assert_ne!(out[0].1, ".agents/skills");
    }

    #[test]
    fn distinct_dirs_stay() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let entries = vec![
            (a.to_string_lossy().into(), "a".into()),
            (b.to_string_lossy().into(), "b".into()),
        ];
        assert_eq!(dedupe_resource_dirs(entries, &[tmp.path()]).len(), 2);
    }
}
