//! WASM extension host (wasmtime + Component Model).
//!
//! Pi-compatible discovery:
//! - `~/.elph/extensions/<name>/extension.toml` + component wasm
//! - `<project>/.elph/extensions/<name>/...` (after project trust)

mod discovery;
mod host;
mod registry;
mod types;

pub use discovery::{
    discover_manifests, extension_roots, global_extensions_dir, load_manifest, project_extensions_dir,
};
pub use registry::ExtensionRegistry;
pub use types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult, ExtensionsSettings};
