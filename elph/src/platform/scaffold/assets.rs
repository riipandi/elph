//! Embed and unpack built-in user-guide + skills from workspace `assets/`.
//!
//! Destination: `CONFIG_DIR/bundled/{user-guide,skills}/…`
//! Existing files are never overwritten so local edits win.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::write_json_file;

use super::bundled::BundledManifest;

/// Relative path under `CONFIG_DIR/bundled/` → embedded UTF-8 body.
///
/// Paths use `/` separators. Sources live in the workspace `assets/` tree and are
/// embedded at compile time via `include_str!`.
fn embedded_bundled_files() -> &'static [(&'static str, &'static str)] {
    // Paths relative to the `elph` crate manifest: `elph/../assets/…`
    macro_rules! asset {
        ($rel:literal) => {
            ($rel, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/", $rel)))
        };
    }
    &[
        asset!("user-guide/README.md"),
        asset!("user-guide/01-getting-started.md"),
        asset!("user-guide/02-authentication.md"),
        asset!("user-guide/03-keyboard-shortcuts.md"),
        asset!("user-guide/04-slash-commands.md"),
        asset!("user-guide/05-configuration.md"),
        asset!("user-guide/06-mcp-servers.md"),
        asset!("user-guide/07-skills.md"),
        asset!("user-guide/08-custom-models.md"),
        asset!("user-guide/09-sessions.md"),
        asset!("user-guide/10-memory.md"),
        asset!("user-guide/11-plan-mode.md"),
        asset!("user-guide/12-subagents.md"),
        asset!("user-guide/13-permissions-and-safety.md"),
        asset!("skills/create-skill/SKILL.md"),
    ]
}

/// Unpack embedded bundled assets into `CONFIG_DIR/bundled/`.
pub struct BundledAssets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledAssetsReport {
    pub written: usize,
    pub skipped: usize,
}

impl BundledAssets {
    /// Ensure every embedded bundled file exists under `paths.bundled_dir()`.
    ///
    /// Missing files are created; existing files are left untouched. Merges simple
    /// checksums (length + FNV-1a) into `bundled/manifest.json` for newly written files.
    pub fn ensure<P: AppPaths>(paths: &P, app_id: &str, app_version: &str) -> Result<BundledAssetsReport> {
        let bundled = paths.bundled_dir();
        fs::create_dir_all(&bundled).with_context(|| format!("create bundled dir {}", bundled.display()))?;

        let mut written = 0usize;
        let mut skipped = 0usize;
        let mut new_checksums: BTreeMap<String, String> = BTreeMap::new();

        for (rel, body) in embedded_bundled_files() {
            let dest = bundled_join(&bundled, rel);
            if dest.exists() {
                skipped += 1;
                continue;
            }
            write_text_file(&dest, body).with_context(|| format!("write bundled asset {}", dest.display()))?;
            new_checksums.insert((*rel).to_string(), content_fingerprint(body));
            written += 1;
        }

        if !new_checksums.is_empty() {
            merge_manifest_checksums(paths, app_id, app_version, new_checksums)?;
        }

        Ok(BundledAssetsReport { written, skipped })
    }

    /// All embedded relative paths (for tests / inventory).
    pub fn embedded_paths() -> Vec<&'static str> {
        embedded_bundled_files().iter().map(|(p, _)| *p).collect()
    }
}

fn bundled_join(bundled: &Path, rel: &str) -> std::path::PathBuf {
    let mut path = bundled.to_path_buf();
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            continue;
        }
        path.push(segment);
    }
    path
}

fn write_text_file(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut content = body.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    let mut file = fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Lightweight stable fingerprint for manifest bookkeeping (not cryptographic).
fn content_fingerprint(body: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
    for byte in body.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}:len={}", body.len())
}

fn merge_manifest_checksums<P: AppPaths>(
    paths: &P,
    app_id: &str,
    app_version: &str,
    new_checksums: BTreeMap<String, String>,
) -> Result<()> {
    let path = paths.bundled_manifest_path();
    let mut manifest = if path.exists() {
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str::<BundledManifest>(&raw).unwrap_or_else(|_| BundledManifest::defaults(app_id, app_version))
    } else {
        BundledManifest::defaults(app_id, app_version)
    };
    for (k, v) in new_checksums {
        manifest.checksums.insert(k, v);
    }
    write_json_file(&path, &manifest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;

    #[test]
    fn embedded_inventory_is_non_empty() {
        let paths = BundledAssets::embedded_paths();
        assert!(paths.iter().any(|p| p.starts_with("user-guide/")));
        assert!(paths.contains(&"skills/create-skill/SKILL.md"));
    }

    #[test]
    fn unpack_writes_missing_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("project"));
        fs::create_dir_all(paths.bundled_dir()).unwrap();

        let first = BundledAssets::ensure(&paths, "elph", "0.0.1-test").expect("first");
        assert!(first.written > 0);
        assert_eq!(first.skipped, 0);

        let guide = paths.bundled_dir().join("user-guide/01-getting-started.md");
        assert!(guide.is_file());
        let skill = paths.bundled_dir().join("skills/create-skill/SKILL.md");
        assert!(skill.is_file());
        let body = fs::read_to_string(&skill).unwrap();
        assert!(body.contains("name: create-skill"));

        // User edit must survive second unpack.
        fs::write(&guide, "# custom\n").unwrap();
        let second = BundledAssets::ensure(&paths, "elph", "0.0.1-test").expect("second");
        assert_eq!(second.written, 0);
        assert!(second.skipped > 0);
        assert_eq!(fs::read_to_string(&guide).unwrap(), "# custom\n");

        let manifest_raw = fs::read_to_string(paths.bundled_manifest_path()).unwrap();
        assert!(manifest_raw.contains("user-guide/01-getting-started.md"));
        assert!(manifest_raw.contains("skills/create-skill/SKILL.md"));
    }

    #[test]
    fn relative_paths_skip_parent_segments() {
        let base = Path::new("/cfg/bundled");
        let joined = bundled_join(base, "skills/../create-skill/SKILL.md");
        // ".." is skipped; path never leaves bundled root via ParentDir.
        assert!(
            !joined
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        );
        assert_eq!(joined, Path::new("/cfg/bundled/skills/create-skill/SKILL.md"));
    }
}
