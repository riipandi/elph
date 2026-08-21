//! WASM extension host (wasmi interpreter, core Wasm ABI).
//!
//! Pi-compatible discovery:
//! - `~/.elph/extensions/<name>/extension.toml` + core wasm
//! - `<project>/.elph/extensions/<name>/...` (after project trust)

mod abi;
mod discovery;
mod host;
mod registry;
mod types;
mod ui;

pub use discovery::{
    discover_manifests, extension_roots, global_extensions_dir, load_manifest, project_extensions_dir,
};
pub use registry::ExtensionRegistry;
pub use types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult, ExtensionToolSpec, ExtensionsSettings};
pub use ui::{DenyUi, ExtensionUi};
