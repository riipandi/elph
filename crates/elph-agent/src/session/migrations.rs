//! Session tree schema migrations for Turso backends (Pi sqlite-node aligned).
//!
//! Uses a high version number so it can apply after platform migrations that share
//! the same `app_migrations` ledger. All statements are `IF NOT EXISTS` / additive.

use crate::datastore::Migration;

/// SQL shared with platform migration `session_tree_pi_schema` (elph host).
pub const SESSION_TREE_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    cwd TEXT,
    parent_session_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    agent_mode TEXT DEFAULT 'build',
    name TEXT,
    system_prompt TEXT,
    metadata TEXT,
    active_leaf_id TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_sessions_cwd ON sessions(cwd);
CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);

CREATE TABLE IF NOT EXISTS session_entries (
    session_id TEXT NOT NULL,
    id TEXT NOT NULL,
    entry_seq INTEGER NOT NULL,
    parent_id TEXT,
    type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (session_id, id)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_session_entries_session_seq
    ON session_entries(session_id, entry_seq);
CREATE INDEX IF NOT EXISTS idx_session_entries_session_parent
    ON session_entries(session_id, parent_id);
CREATE INDEX IF NOT EXISTS idx_session_entries_session_type
    ON session_entries(session_id, type);

CREATE TABLE IF NOT EXISTS session_sequences (
    session_id TEXT PRIMARY KEY,
    next_seq INTEGER NOT NULL
) STRICT;
"#;

/// Standalone / library migration for Turso session backends.
///
/// Version 100 sits above historical platform migrations (1–8) so a host DB that
/// already applied platform schema still gets a no-op `IF NOT EXISTS` pass here.
pub const SESSION_TREE_MIGRATIONS: [Migration; 1] = [Migration {
    version: 100,
    name: "session_tree_pi_schema",
    up: SESSION_TREE_SCHEMA_SQL,
}];
