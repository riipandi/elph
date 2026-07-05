use anyhow::Result;

use super::{
    migrations,
    paths::{AppPaths, Paths},
};
use elph_agent::{DatabaseSpec, InitProgress, ensure_databases_once, try_block_on};

const DATASTORE_STEPS: u64 = 1;

/// Lazily initialize local databases on first use.
pub async fn ensure(paths: &Paths) -> Result<()> {
    let metadata_db = paths.metadata_db_path();
    let memory_db = paths.memory_db_path();
    let specs = [
        DatabaseSpec {
            path: &metadata_db,
            migrations: migrations::metadata_migrations(),
        },
        DatabaseSpec {
            path: &memory_db,
            migrations: migrations::memory_migrations(),
        },
    ];

    let progress = InitProgress::new(DATASTORE_STEPS).with_quiet_env("ECLAW_QUIET");
    progress.advance("Initializing databases");
    ensure_databases_once(&specs).await?;
    progress.finish();
    Ok(())
}

/// Blocking wrapper for CLI commands that need persistence.
pub fn ensure_blocking(paths: &Paths) -> Result<()> {
    try_block_on(ensure(paths))?
}
