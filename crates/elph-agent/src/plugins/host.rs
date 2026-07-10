//! wasmtime Component Model host for extension guests.

use std::path::Path;

use anyhow::{Context, Result};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

use super::types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult};

wasmtime::component::bindgen!({
    path: "wit",
    world: "guest",
});

struct ExtensionState;

impl elph::extension::types::Host for ExtensionState {}

pub struct LoadedExtension {
    pub manifest: ExtensionManifest,
    #[allow(dead_code)]
    pub root: std::path::PathBuf,
    engine: Engine,
    component: Component,
    linker: Linker<ExtensionState>,
}

impl LoadedExtension {
    pub fn load(root: &Path, manifest: ExtensionManifest) -> Result<Self> {
        let wasm_path = manifest.component_path(root);
        anyhow::ensure!(wasm_path.is_file(), "component not found: {}", wasm_path.display());

        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        let engine = Engine::new(&config).context("create wasmtime engine")?;

        let component = Component::from_file(&engine, &wasm_path)
            .with_context(|| format!("load component {}", wasm_path.display()))?;

        let mut linker = Linker::new(&engine);
        Guest::add_to_linker(&mut linker, |state: &mut ExtensionState| state).context("link extension world")?;

        Ok(Self {
            manifest,
            root: root.to_path_buf(),
            engine,
            component,
            linker,
        })
    }

    pub fn list_commands(&self) -> Result<Vec<ExtensionCommand>> {
        let mut store = Store::new(&self.engine, ExtensionState);
        let bindings =
            Guest::instantiate(&mut store, &self.component, &self.linker).context("instantiate extension component")?;
        let instance = bindings.elph_extension_extension();
        let commands = instance
            .call_list_commands(&mut store)
            .context("extension list-commands")?;
        Ok(commands
            .into_iter()
            .map(|cmd| ExtensionCommand {
                extension: self.manifest.name.clone(),
                name: cmd.name,
                description: cmd.description,
            })
            .collect())
    }

    pub fn execute_command(&self, name: &str, args: &str) -> Result<ExtensionSlashResult> {
        let mut store = Store::new(&self.engine, ExtensionState);
        let bindings =
            Guest::instantiate(&mut store, &self.component, &self.linker).context("instantiate extension component")?;
        let instance = bindings.elph_extension_extension();
        match instance
            .call_execute_command(&mut store, name, args)
            .context("extension execute-command")?
        {
            Ok(result) => Ok(ExtensionSlashResult {
                message: result.message,
                is_error: result.is_error,
            }),
            Err(message) => Ok(ExtensionSlashResult {
                message,
                is_error: true,
            }),
        }
    }
}
