use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::bail;
use anyhow::{Context, Result};

/// Catalog generator script path (relative to the pi-ai package root).
pub const CATALOG_CHAT_SCRIPT: &str = "scripts/generate-models.ts";

/// Permanent catalog source package root relative to the **elph workspace root**.
///
/// Layout (see `docs/porting/README.md` — local pi clone):
/// ```text
/// github.com/
///   earendil-works/pi/packages/ai/   ← this path
///   riipandi/elph/                   ← this repo
///     crates/elph-ai/
/// ```
pub const DEFAULT_CATALOG_DIR_FROM_WORKSPACE: &str = "../../earendil-works/pi/packages/ai";

/// Resolve the fixed catalog source directory from `CARGO_MANIFEST_DIR` (`crates/elph-ai`).
pub fn default_catalog_dir(crate_root: &Path) -> PathBuf {
    let workspace_root = crate_root
        .parent()
        .and_then(|crates| crates.parent())
        .unwrap_or(crate_root);
    let raw = workspace_root.join(DEFAULT_CATALOG_DIR_FROM_WORKSPACE);
    // Prefer a cleaned absolute path for error messages; fall back to raw if missing.
    raw.canonicalize().unwrap_or(raw)
}

/// Run the upstream pi-ai npm script that regenerates the `src/providers/data/*.json` files.
pub fn run_catalog_npm_script(catalog_dir: &Path, script: &str) -> Result<()> {
    println!("Running catalog source {script} in {}...", catalog_dir.display());

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
