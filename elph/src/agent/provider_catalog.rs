//! Install runtime catalog from `CONFIG_DIR/providers/*.json`.

use std::path::Path;

use anyhow::{Context, Result};
use elph_ai::{load_provider_catalogs_dir, set_disk_catalog_overrides};

/// Load provider JSON files and install process-wide overrides for `get_builtin_*`.
///
/// Safe to call multiple times (e.g. bootstrap then session resolve). Missing dir → no-op.
pub fn install_providers_dir(providers_dir: &Path) -> Result<usize> {
    let map = load_provider_catalogs_dir(providers_dir)
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| format!("load providers from {}", providers_dir.display()))?;
    let count = map.len();
    set_disk_catalog_overrides(map);
    if count > 0 {
        log::debug!(
            "installed {count} provider catalog override file(s) from {}",
            providers_dir.display()
        );
    }
    Ok(count)
}
