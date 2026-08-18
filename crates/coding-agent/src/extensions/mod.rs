//! Extension host wiring for the Elph CLI (wasmtime + Component Model).

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use elph_agent::plugins::global_extensions_dir;
use elph_agent::plugins::{ExtensionCommand, ExtensionRegistry, ExtensionSlashResult, ExtensionsSettings};
use parking_lot::RwLock;

use crate::platform::{AppPaths, Paths};

/// Shared extension registry for slash dispatch and `/reload`.
#[derive(Clone, Default)]
pub struct ExtensionHost {
    registry: Arc<RwLock<ExtensionRegistry>>,
    settings: Arc<RwLock<ExtensionsSettings>>,
}

impl ExtensionHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn registry(&self) -> Arc<RwLock<ExtensionRegistry>> {
        self.registry.clone()
    }

    pub fn load_settings(paths: &Paths) -> ExtensionsSettings {
        crate::platform::Settings::load(paths)
            .map(|s| s.extensions_settings())
            .unwrap_or_default()
    }

    pub fn save_settings(paths: &Paths, ext: &ExtensionsSettings) -> Result<()> {
        let mut settings =
            crate::platform::Settings::load_home(paths).unwrap_or_else(|_| crate::platform::Settings::defaults());
        settings.resources.disabled_extensions = ext.disabled.clone();
        settings.resources.extensions = ext
            .extra_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        crate::platform::Settings::save(paths, &settings)
    }

    pub fn reload(&self, paths: &Paths, host_settings: &crate::platform::Settings) -> Result<()> {
        let settings = host_settings.extensions_settings();
        *self.settings.write() = settings.clone();
        self.registry.read().load(
            paths.config_dir(),
            &paths.project_elph_dir(),
            &settings,
            host_settings.include_project_extensions(paths),
        )
    }

    pub fn commands(&self) -> Vec<ExtensionCommand> {
        self.registry.read().commands()
    }

    pub fn dispatch_slash(&self, name: &str, args: &str) -> Option<Result<ExtensionSlashResult>> {
        self.registry.read().dispatch_slash(name, args)
    }

    pub fn ensure_dirs(paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(global_extensions_dir(paths.config_dir()))?;
        Ok(())
    }

    pub fn install_bundle(&self, source: &Path, paths: &Paths, force: bool) -> Result<std::path::PathBuf> {
        let dest = self.registry.read().install_bundle(source, paths.config_dir(), force)?;
        let host = crate::platform::Settings::load(paths).unwrap_or_else(|_| crate::platform::Settings::defaults());
        self.reload(paths, &host)?;
        Ok(dest)
    }
}
