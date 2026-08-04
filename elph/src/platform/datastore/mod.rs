use anyhow::Result;
use elph_tui::CliSpinner;
use turso_db::{connect, open_local};

use super::migrations;
use super::paths::Paths;
use elph_agent::try_block_on;
use floppy::codegraph_migrations;
use floppy::memory::migrations as memory_migrations;

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

    let spinner = CliSpinner::new("Opening store database");
    log::info!("Opening store database");

    // Open one connection and apply all migration bands through it.
    let db = open_local(
        &store_db,
        |b| b.experimental_multiprocess_wal(true).experimental_index_method(true),
        false,
    )
    .await?;
    let conn = connect(&db).await?;

    // Platform band (v101–106).
    elph_agent::datastore::run_migrations(&conn, migrations::metadata_migrations()).await?;

    // Floppy memory (v1–4) and codegraph (v500–501).
    memory_migrations::apply(&conn).await?;
    codegraph_migrations::apply(&conn).await?;

    log::info!("Databases ready");
    spinner.finish_and_clear();
    Ok(())
}

/// Blocking wrapper for CLI commands that need persistence.
pub fn ensure_blocking(paths: &Paths) -> Result<()> {
    try_block_on(ensure(paths))?
}
