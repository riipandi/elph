use crate::agent::ensure_global_agents_md;
use crate::platform::scaffold::{BundledManifest, ChangelogScaffold, ProvidersUnpack, TrustStore, VersionFile};
use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::InitProgress;
use elph_agent::{ensure_dirs, try_block_on};

use super::paths::Paths;

const INIT_STEPS: u64 = 4;
const APP_ID: &str = "elph";

/// Scaffold required directories and default files for a fresh Elph home.
pub async fn ensure(app_version: &str) -> Result<Paths> {
    let progress = InitProgress::new(INIT_STEPS).with_quiet_env("ELPH_QUIET");
    progress.advance("Resolving home directories");
    let paths = Paths::resolve()?;
    run_init_steps(&paths, app_version, &progress).await?;
    progress.finish();
    Ok(paths)
}

/// Scaffold a specific home directory tree (useful in tests and custom setups).
#[allow(dead_code)]
pub async fn ensure_with_paths(paths: &Paths, app_version: &str) -> Result<()> {
    let progress = InitProgress::new(INIT_STEPS).with_quiet_env("ELPH_QUIET");
    run_init_steps(paths, app_version, &progress).await?;
    progress.finish();
    Ok(())
}

/// Blocking wrapper for home initialization (dirs + config, no databases).
pub fn ensure_home_blocking(app_version: &str) -> Result<Paths> {
    try_block_on(ensure(app_version))?
}

async fn run_init_steps(paths: &Paths, app_version: &str, progress: &InitProgress) -> Result<()> {
    progress.advance("Creating directories");
    ensure_home_dirs(paths)?;

    progress.advance("Writing configuration");
    ensure_files(paths, app_version)?;

    progress.advance("Unpacking provider catalogs");
    let report = ProvidersUnpack::ensure(paths)?;
    if report.written > 0 {
        log::debug!(
            "providers unpack: wrote {} catalogs (skipped {} existing)",
            report.written,
            report.skipped
        );
    }
    // Install disk overrides for get_builtin_* / model resolution this process.
    if let Err(err) = crate::agent::install_providers_dir(&paths.providers_dir()) {
        log::warn!("provider catalog install: {err:#}");
    }

    Ok(())
}

fn ensure_home_dirs(paths: &Paths) -> Result<()> {
    ensure_dirs(&paths.required_dirs())
}

fn ensure_files(paths: &Paths, app_version: &str) -> Result<()> {
    super::settings::Settings::ensure(paths)?;
    TrustStore::ensure(paths)?;
    VersionFile::ensure(paths, app_version)?;
    ChangelogScaffold::ensure(paths)?;
    BundledManifest::ensure(paths, APP_ID, app_version)?;
    let _ = ensure_global_agents_md(paths);
    super::project::ensure(paths)?;
    Ok(())
}
