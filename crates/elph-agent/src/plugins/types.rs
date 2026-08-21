//! Extension manifest, command descriptors, and JSON ABI payloads.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// On-disk manifest for a core Wasm extension (`extension.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Path to the core `.wasm` file, relative to the manifest directory.
    pub wasm: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub trusted: bool,
}

fn default_true() -> bool {
    true
}

impl ExtensionManifest {
    pub fn wasm_path(&self, root: &std::path::Path) -> PathBuf {
        root.join(&self.wasm)
    }
}

/// A slash command contributed by an extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionCommand {
    pub extension: String,
    pub name: String,
    pub description: String,
}

/// Result of executing an extension slash command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSlashResult {
    pub message: String,
    pub is_error: bool,
}

/// Tool advertised during `elph_init`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionToolSpec {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: Value,
}

/// Persisted enable/disable state (`~/.elph/extensions.json` / settings).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionsSettings {
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub extra_paths: Vec<PathBuf>,
}

impl ExtensionsSettings {
    pub fn is_enabled(&self, name: &str) -> bool {
        !self.disabled.iter().any(|n| n == name)
    }
}
