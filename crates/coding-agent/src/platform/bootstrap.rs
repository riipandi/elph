use crate::platform::scaffold::{
    BundledAssets, BundledManifest, ChangelogScaffold, ProvidersUnpack, TrustStore, VersionFile,
};
use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::fs::ensure_dirs;
use elph_agent::runtime::try_block_on;
use elph_tui::CliSpinner;

use super::paths::Paths;

const APP_ID: &str = "elph";

/// Scaffold required directories and default files for a fresh Elph home.
pub async fn ensure(app_version: &str) -> Result<Paths> {
    let spinner = CliSpinner::new("Resolving home directories");
    log::info!("Resolving home directories");
    let paths = Paths::resolve()?;
    run_init_steps(&paths, app_version, &spinner).await?;
    log::info!("Startup complete");
    spinner.finish_and_clear();
    Ok(paths)
}

/// Scaffold a specific home directory tree (useful in tests and custom setups).
#[allow(dead_code)]
pub async fn ensure_with_paths(paths: &Paths, app_version: &str) -> Result<()> {
    run_init_steps(paths, app_version, &CliSpinner::disabled()).await?;
    Ok(())
}

/// Blocking wrapper for home initialization (dirs + config, no databases).
pub fn ensure_home_blocking(app_version: &str) -> Result<Paths> {
    try_block_on(ensure(app_version))?
}

async fn run_init_steps(paths: &Paths, app_version: &str, spinner: &CliSpinner) -> Result<()> {
    spinner.set_message("Creating directories");
    log::info!("Creating directories");
    ensure_home_dirs(paths)?;

    spinner.set_message("Writing configuration");
    log::info!("Writing configuration");
    ensure_files(paths, app_version)?;

    spinner.set_message("Unpacking provider catalogs");
    log::info!("Unpacking provider catalogs");
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

    spinner.set_message("Unpacking bundled skills and user guide");
    log::info!("Unpacking bundled skills and user guide");
    let assets = BundledAssets::ensure(paths, APP_ID, app_version)?;
    if assets.written > 0 {
        log::debug!(
            "bundled assets unpack: wrote {} files (skipped {} existing)",
            assets.written,
            assets.skipped
        );
    }

    Ok(())
}

fn ensure_home_dirs(paths: &Paths) -> Result<()> {
    ensure_dirs(&paths.required_dirs())?;
    // Legacy layout: APP_DATA/projects/<id> → APP_DATA/sessions/<id>
    if let Err(err) = paths.migrate_projects_to_sessions() {
        log::warn!("migrate projects→sessions: {err}");
    }
    // Legacy layout: <project>/.elph/sessions/<id>/tool_outputs.jsonl →
    // APP_DATA/sessions/<id>/tool_outputs.jsonl
    if let Err(err) = paths.migrate_legacy_session_tool_outputs() {
        log::warn!("migrate legacy session tool outputs: {err}");
    }
    Ok(())
}

fn ensure_files(paths: &Paths, app_version: &str) -> Result<()> {
    super::settings::Settings::ensure(paths)?;
    super::mcp::ensure(paths)?;
    TrustStore::ensure(paths)?;
    VersionFile::ensure(paths, app_version)?;
    ChangelogScaffold::ensure(paths)?;
    BundledManifest::ensure(paths, APP_ID, app_version)?;
    super::project::ensure(paths)?;
    Ok(())
}
