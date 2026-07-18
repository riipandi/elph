use anyhow::Result;
use elph_agent::InitProgress;
use elph_agent::{ensure_dirs, try_block_on};
use elph_core::{BundledManifest, TrustStore, VersionFile};

use super::paths::Paths;

const INIT_STEPS: u64 = 3;
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

    Ok(())
}

fn ensure_home_dirs(paths: &Paths) -> Result<()> {
    ensure_dirs(&paths.required_dirs())
}

fn ensure_files(paths: &Paths, app_version: &str) -> Result<()> {
    super::settings::Settings::ensure(paths)?;
    TrustStore::ensure(paths)?;
    VersionFile::ensure(paths, app_version)?;
    BundledManifest::ensure(paths, APP_ID, app_version)?;
    super::project::ensure(paths)?;
    Ok(())
}
