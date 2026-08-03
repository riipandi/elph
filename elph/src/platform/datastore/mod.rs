use anyhow::Result;
use turso::Builder;

use super::migrations;
use super::paths::Paths;
use elph_agent::{InitProgress, try_block_on};
use floppy::codegraph_migrations;
use floppy::memory::migrations as memory_migrations;

const DATASTORE_STEPS: u64 = 2;

/// Lazily initialize the shared project database on first use.
///
/// The store DB (`.elph/store.db`) hosts the platform schema band via
/// `metadata_migrations()`, plus the floppy memory band (v1–4) and codegraph
/// band (v500–501) so all tables exist immediately — not lazily on first use.
///
/// All migrations are applied through a single connection to avoid WAL lock
/// contention from opening and closing multiple connections in sequence.
pub async fn ensure(paths: &Paths) -> Result<()> {
    let store_db = paths.memory_db_path();

    let progress = InitProgress::new(DATASTORE_STEPS).with_quiet_env("ELPH_QUIET");
    progress.advance("Opening store database");

    // Open one connection and apply all migration bands through it.
    let db = Builder::new_local(store_db.to_string_lossy().as_ref())
        .experimental_multiprocess_wal(true)
        .experimental_index_method(true)
        .build()
        .await?;
    let conn = db.connect()?;

    // Platform band (v101–106).
    elph_agent::datastore::run_migrations(&conn, migrations::metadata_migrations()).await?;

    // Floppy memory (v1–4) and codegraph (v500–501).
    memory_migrations::apply(&conn).await?;
    codegraph_migrations::apply(&conn).await?;

    progress.advance("Databases ready");
    progress.finish();
    Ok(())
}

/// Blocking wrapper for CLI commands that need persistence.
pub fn ensure_blocking(paths: &Paths) -> Result<()> {
    try_block_on(ensure(paths))?
}
