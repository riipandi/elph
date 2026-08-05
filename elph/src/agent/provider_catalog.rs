//! Point the runtime catalog at `CONFIG_DIR/providers/*.json`.

use std::path::Path;

use anyhow::{Context, Result};
use elph_ai::install_provider_catalog_dir;

/// Register the providers directory as the catalog source for `get_builtin_*`.
///
/// Files are only listed here; each catalog is parsed lazily on first use. Safe to call multiple
/// times (bootstrap, session resolve, `/reload`) — every call drops cached catalogs so edited
/// files take effect. Missing dir → registration cleared.
pub fn install_providers_dir(providers_dir: &Path) -> Result<usize> {
    let count = install_provider_catalog_dir(providers_dir)
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| format!("load providers from {}", providers_dir.display()))?;
    if count > 0 {
        log::debug!("registered {count} provider catalog file(s) from {}", providers_dir.display());
    }
    Ok(count)
}
