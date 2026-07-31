use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::bail;
use anyhow::{Context, Result};

/// Optional local pi clone for **image** catalog scripts only (not chat origin).
///
/// Chat catalogs use models.dev. Image generation may still read from a sibling
/// earendil-works/pi checkout when present.
pub const DEFAULT_CATALOG_DIR_FROM_WORKSPACE: &str = "../../earendil-works/pi/packages/ai";

/// Resolve optional pi packages/ai path for image tooling.
pub fn default_catalog_dir(crate_root: &Path) -> PathBuf {
    let workspace_root = crate_root
        .parent()
        .and_then(|crates| crates.parent())
        .unwrap_or(crate_root);
    let raw = workspace_root.join(DEFAULT_CATALOG_DIR_FROM_WORKSPACE);
    raw.canonicalize().unwrap_or(raw)
}

/// Run an npm script in a directory (image catalog helper).
pub fn run_catalog_npm_script(catalog_dir: &Path, script: &str) -> Result<()> {
    super::term::info(format!("Running catalog source {script} in {}…", catalog_dir.display()));

    if Command::new("npm")
        .args(["run", script, "--silent"])
        .current_dir(catalog_dir)
        .status()
        .with_context(|| format!("spawn npm run {script}"))?
        .success()
    {
        return Ok(());
    }

    let script_path = format!("scripts/{script}.ts");
    for (bin, args) in [
        ("npx", vec!["tsx", &script_path]),
        ("node", vec!["--experimental-strip-types", &script_path]),
        ("node", vec![&script_path]),
    ] {
        let status = Command::new(bin)
            .args(&args)
            .current_dir(catalog_dir)
            .status()
            .with_context(|| format!("spawn {bin} {}", args.join(" ")))?;
        if status.success() {
            return Ok(());
        }
    }

    bail!(
        "failed to run catalog source `{script}`; install deps with `npm install` in {}",
        catalog_dir.display()
    );
}
