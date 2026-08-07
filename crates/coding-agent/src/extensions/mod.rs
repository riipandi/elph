//! Extension host wiring for the Elph CLI (wasmtime + Component Model).

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use elph_agent::{ExtensionCommand, ExtensionRegistry, ExtensionSlashResult, ExtensionsSettings};
use elph_agent::{global_extensions_dir, project_extensions_dir, write_json_file};
use parking_lot::RwLock;

use crate::platform::{AppPaths, Paths};

const EXTENSIONS_SETTINGS_FILE: &str = "extensions.json";

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

    pub fn settings_path(paths: &Paths) -> std::path::PathBuf {
        paths.config_dir().join(EXTENSIONS_SETTINGS_FILE)
    }

    pub fn load_settings(paths: &Paths) -> ExtensionsSettings {
        let path = Self::settings_path(paths);
        if !path.is_file() {
            return ExtensionsSettings::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(paths: &Paths, settings: &ExtensionsSettings) -> Result<()> {
        write_json_file(&Self::settings_path(paths), settings)
    }

    pub fn reload(&self, paths: &Paths, include_project: bool) -> Result<()> {
        let settings = Self::load_settings(paths);
        *self.settings.write() = settings.clone();
        self.registry
            .read()
            .load(paths.config_dir(), &paths.project_elph_dir(), &settings, include_project)
    }

    pub fn commands(&self) -> Vec<ExtensionCommand> {
        self.registry.read().commands()
    }

    pub fn dispatch_slash(&self, name: &str, args: &str) -> Option<Result<ExtensionSlashResult>> {
        self.registry.read().dispatch_slash(name, args)
    }

    pub fn ensure_dirs(paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(global_extensions_dir(paths.config_dir()))?;
        std::fs::create_dir_all(project_extensions_dir(&paths.project_elph_dir()))?;
        Ok(())
    }

    pub fn install_bundle(&self, source: &Path, paths: &Paths, force: bool) -> Result<std::path::PathBuf> {
        let dest = self.registry.read().install_bundle(source, paths.config_dir(), force)?;
        self.reload(paths, false)?;
        Ok(dest)
    }
}
