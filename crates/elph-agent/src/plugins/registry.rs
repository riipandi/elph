//! Extension registry — discovery, load, slash dispatch.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use parking_lot::RwLock;

use super::discovery::{discover_manifests, extension_roots};
use super::host::LoadedExtension;
use super::types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult, ExtensionsSettings};

#[derive(Default)]
pub struct ExtensionRegistry {
    inner: RwLock<RegistryState>,
}

#[derive(Default)]
struct RegistryState {
    extensions: Vec<LoadedExtension>,
    commands: Vec<ExtensionCommand>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(
        &self,
        config_dir: &Path,
        project_elph_dir: &Path,
        settings: &ExtensionsSettings,
        include_project: bool,
    ) -> Result<()> {
        let roots = extension_roots(config_dir, project_elph_dir, settings, include_project);
        let manifests = discover_manifests(&roots)?;
        let mut extensions = Vec::new();
        let mut commands = Vec::new();

        for (root, manifest) in manifests {
            if !manifest.enabled || !settings.is_enabled(&manifest.name) {
                continue;
            }
            let loaded =
                LoadedExtension::load(&root, manifest).with_context(|| format!("load extension {}", root.display()))?;
            let mut ext_commands = loaded
                .list_commands()
                .with_context(|| format!("list commands for extension {}", loaded.manifest.name))?;
            commands.append(&mut ext_commands);
            extensions.push(loaded);
        }

        commands.sort_by(|a, b| a.name.cmp(&b.name));
        *self.inner.write() = RegistryState { extensions, commands };
        Ok(())
    }

    pub fn commands(&self) -> Vec<ExtensionCommand> {
        self.inner.read().commands.clone()
    }

    pub fn extensions(&self) -> Vec<ExtensionManifest> {
        self.inner
            .read()
            .extensions
            .iter()
            .map(|e| e.manifest.clone())
            .collect()
    }

    pub fn dispatch_slash(&self, name: &str, args: &str) -> Option<Result<ExtensionSlashResult>> {
        let state = self.inner.read();
        let owner = state.commands.iter().find(|cmd| cmd.name.eq_ignore_ascii_case(name))?;
        let extension = state
            .extensions
            .iter()
            .find(|ext| ext.manifest.name == owner.extension)?;
        Some(
            extension
                .execute_command(&owner.name, args)
                .with_context(|| format!("extension /{} from {}", name, owner.extension)),
        )
    }

    pub fn install_bundle(&self, source_dir: &Path, config_dir: &Path, force: bool) -> Result<PathBuf> {
        let manifest_path = source_dir.join("extension.toml");
        let manifest = super::discovery::load_manifest(&manifest_path)?;
        let dest = config_dir.join("extensions").join(&manifest.name);
        if dest.exists() && !force {
            anyhow::bail!("extension '{}' already installed at {}", manifest.name, dest.display());
        }
        std::fs::create_dir_all(config_dir.join("extensions")).context("create extensions dir")?;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).with_context(|| format!("remove {}", dest.display()))?;
        }
        copy_dir_recursive(source_dir, &dest)?;
        Ok(dest)
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("read dir {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copy {} to {}", entry.path().display(), target.display()))?;
        }
    }
    Ok(())
}
