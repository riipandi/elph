use anyhow::Result;
use elph_agent::datastore::{connect, open_local_with};
use elph_agent::try_block_on;
use elph_tui::CliSpinner;
use turso::Database;

use super::migrations;
use super::paths::Paths;
use floppy::memory::migrations as memory_migrations;

/// Open the shared project database, apply all migration bands, and return the
/// live handle.
///
/// The store DB (`.elph/store.db`) hosts the platform schema band via
/// `metadata_migrations()`, plus the floppy memory band (v1–4) so all
/// tables exist immediately — not lazily on first use.
///
/// All migrations are applied through a single connection to avoid WAL lock
/// contention from opening and closing multiple connections in sequence.
///
/// The returned [`Database`] is meant to be wrapped in an [`Arc`] and shared
/// with every store (`TursoSessionRepo`, `GoalStore`, `AgentGraphStore`,
/// floppy's `MemoryStore`) so they all connect from one
/// open handle instead of each opening the file on every operation.
pub async fn ensure_database(paths: &Paths) -> Result<Database> {
    let store_db = paths.memory_db_path();

    let spinner = CliSpinner::new("Opening store database");
    log::info!("Opening store database");

    // Open the database and apply all migration bands through one connection.
    let db = open_local_with(&store_db, |b| {
        b.experimental_multiprocess_wal(true).experimental_index_method(true)
    })
    .await?;
    let conn = connect(&db).await?;

    // Platform band (v101–107).
    elph_agent::datastore::run_migrations(&conn, migrations::metadata_migrations()).await?;

    // Floppy memory (v1–4).
    memory_migrations::apply(&conn).await?;

    log::info!("Databases ready");
    spinner.finish_and_clear();
    Ok(db)
}

/// Lazily initialize the shared project database on first use.
///
/// Opens the store DB, applies all migration bands, then drops the handle.
/// Use [`ensure_database`] when you need to share the open handle with stores.
pub async fn ensure(paths: &Paths) -> Result<()> {
    ensure_database(paths).await?;
    Ok(())
}

/// Blocking wrapper for CLI commands that need persistence.
pub fn ensure_blocking(paths: &Paths) -> Result<()> {
    try_block_on(ensure(paths))?
}

/// Blocking wrapper that returns the open database handle.
pub fn ensure_database_blocking(paths: &Paths) -> Result<Database> {
    try_block_on(ensure_database(paths))?
}
