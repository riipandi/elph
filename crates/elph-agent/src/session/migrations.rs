//! Session tree schema migrations for Turso backends.

use crate::migration::Migration;

pub const SESSION_TREE_MIGRATIONS: [Migration; 1] = [Migration {
    version: 1,
    name: "create_session_tree",
    up: "CREATE TABLE IF NOT EXISTS session_meta (
            session_id TEXT PRIMARY KEY,
            leaf_id TEXT,
            created_at TEXT NOT NULL,
            metadata TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS session_tree_entries (
            session_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            id TEXT NOT NULL,
            data TEXT NOT NULL,
            PRIMARY KEY (session_id, seq)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_session_tree_entries_id
            ON session_tree_entries(session_id, id);",
}];
