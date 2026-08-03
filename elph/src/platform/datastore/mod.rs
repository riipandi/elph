use anyhow::Result;

use super::migrations;
use super::paths::{AppPaths, Paths};
use elph_agent::{DatabaseSpec, InitProgress};
use elph_agent::{ensure_databases_once, try_block_on};

const DATASTORE_STEPS: u64 = 2;

/// Lazily initialize local databases on first use.
///
/// Only the metadata DB is initialized here. The memory DB (`.elph/store.db`)
/// is opened by `floppy::MemoryStore::init()` which handles its own schema.
pub async fn ensure(paths: &Paths) -> Result<()> {
    let metadata_db = paths.metadata_db_path();
    let specs = [DatabaseSpec {
        path: &metadata_db,
        migrations: migrations::metadata_migrations(),
    }];

    let progress = InitProgress::new(DATASTORE_STEPS).with_quiet_env("ELPH_QUIET");
    // Two observable phases instead of a bar that is instantly full: opening the
    // SQLite connection (incl. stale shared-memory cleanup) and applying
    // migrations. The elapsed-time ticker shows the step is alive while blocked.
    progress.advance("Opening metadata database");

    match ensure_databases_once(&specs).await {
        Ok(_) => {
            progress.advance("Databases ready");
            progress.finish();
            Ok(())
        }
        Err(e) => {
            progress.finish();
            let error_msg = format!("Database initialization failed: {e}");

            // Provide helpful hints for common errors
            if error_msg.contains("timeout") {
                anyhow::bail!(
                    "{}\n\nThis may be caused by:\n\
                    • Stale shared memory files from a previous crash\n\
                    • Another process holding the database lock\n\
                    • Corrupted database file\n\n\
                    Try: rm ~/.local/share/elph/metadata.db*",
                    error_msg
                );
            } else if error_msg.contains("locked") || error_msg.contains("busy") {
                anyhow::bail!(
                    "{}\n\nAnother process may be using the database.\n\
                    If no other elph instance is running, try:\n\
                    rm ~/.local/share/elph/metadata.db*",
                    error_msg
                );
            } else {
                anyhow::bail!("{}", error_msg);
            }
        }
    }
}

/// Blocking wrapper for CLI commands that need persistence.
pub fn ensure_blocking(paths: &Paths) -> Result<()> {
    try_block_on(ensure(paths))?
}
