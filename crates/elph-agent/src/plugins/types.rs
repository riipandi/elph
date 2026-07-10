//! Extension manifest and command descriptors.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// On-disk manifest for a WASM component extension (`extension.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Path to the component `.wasm` file, relative to the manifest directory.
    pub component: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub trusted: bool,
}

fn default_true() -> bool {
    true
}

impl ExtensionManifest {
    pub fn component_path(&self, root: &std::path::Path) -> PathBuf {
        root.join(&self.component)
    }
}

/// A slash command contributed by an extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCommand {
    pub extension: String,
    pub name: String,
    pub description: String,
}

/// Result of executing an extension slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionSlashResult {
    pub message: String,
    pub is_error: bool,
}

/// Persisted enable/disable state (`~/.elph/extensions.json`).
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
