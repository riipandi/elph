//! Default home-directory files scaffolded on first run.
//!
//! Each type writes a minimal placeholder file when missing so `elph` and
//! Downstream apps can bootstrap their config/data trees before app-specific setup.

mod assets;
mod bundled;
mod changelog;
mod providers;
mod trust;
mod version;

pub use assets::{BundledAssets, BundledAssetsReport};
pub use bundled::BundledManifest;
pub use changelog::{ChangelogEntry, ChangelogFile, ChangelogScaffold};
pub use providers::{ProvidersUnpack, ProvidersUnpackReport};
pub use trust::TrustStore;
pub use version::VersionFile;
