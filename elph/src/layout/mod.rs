mod bundled;
mod datastore;
mod migrations;
mod paths;
mod project;
mod settings;
mod trust;
mod version;

use elph_agent::{InitProgress, ensure_dirs, try_block_on};

pub use datastore::ensure_blocking as ensure_datastore_blocking;
pub use paths::Paths;

pub type InitError = elph_agent::InitError;
pub type Result<T> = std::result::Result<T, InitError>;

const INIT_STEPS: u64 = 3;

/// Create required directories and default files for a fresh Elph install.
pub async fn ensure(app_version: &str) -> Result<Paths> {
    let progress = InitProgress::new(INIT_STEPS).with_quiet_env("ELPH_QUIET");
    progress.advance("Resolving home directories");
    let paths = Paths::resolve()?;
    run_init_steps(&paths, app_version, &progress).await?;
    progress.finish();
    Ok(paths)
}

/// Initialize a specific config/data layout (useful in tests and custom installs).
#[allow(dead_code)]
pub async fn ensure_with_paths(paths: &Paths, app_version: &str) -> Result<()> {
    let progress = InitProgress::new(INIT_STEPS).with_quiet_env("ELPH_QUIET");
    run_init_steps(paths, app_version, &progress).await?;
    progress.finish();
    Ok(())
}

/// Blocking wrapper for layout initialization (dirs + config, no databases).
pub fn ensure_layout_blocking(app_version: &str) -> Result<Paths> {
    try_block_on(ensure(app_version)).map_err(InitError::Io)?
}

async fn run_init_steps(paths: &Paths, app_version: &str, progress: &InitProgress) -> Result<()> {
    progress.advance("Creating directories");
    ensure_layout_dirs(paths)?;

    progress.advance("Writing configuration");
    ensure_files(paths, app_version)?;

    Ok(())
}

fn ensure_layout_dirs(paths: &Paths) -> Result<()> {
    let mut dirs = vec![paths.config_dir().clone(), paths.data_dir().clone()];
    dirs.extend(paths.required_dirs());
    ensure_dirs(&dirs)
}

fn ensure_files(paths: &Paths, app_version: &str) -> Result<()> {
    settings::Settings::ensure(paths)?;
    trust::TrustStore::ensure(paths)?;
    version::VersionFile::ensure(paths, app_version)?;
    bundled::BundledManifest::ensure(paths, app_version)?;
    project::ensure(paths)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_creates_full_layout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("config");
        let data = tmp.path().join("data");
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(config, data, project);

        ensure_with_paths(&paths, "0.0.10-test").await.expect("ensure layout");

        assert!(paths.settings_path().exists());
        assert!(paths.trust_path().exists());
        assert!(paths.version_path().exists());
        assert!(paths.bundled_manifest_path().exists());

        datastore::ensure(&paths).await.expect("ensure datastore");
        assert!(paths.metadata_db_path().exists());
        assert!(paths.memory_db_path().exists());
        assert!(paths.project_gitignore_path().exists());
        assert!(paths.bundled_dir().join("agents").is_dir());
        assert!(paths.bundled_dir().join("personas").is_dir());
        assert!(paths.bundled_dir().join("skills").is_dir());
        assert!(paths.bundled_dir().join("user-guide").is_dir());
        assert!(paths.prompts_dir().is_dir());
        assert!(paths.providers_dir().is_dir());
        assert!(paths.sessions_dir().is_dir());
        assert!(paths.skills_dir().is_dir());
        assert!(paths.worktrees_dir().is_dir());
        assert!(paths.attachments_dir().is_dir());
        assert!(paths.downloads_dir().is_dir());
        assert!(paths.logs_dir().is_dir());
        assert!(paths.vendor_dir().is_dir());
    }
}
