//! Canonical session schema for Turso backends (hybrid tree + relational metrics).
//!
//! Platform band version **200** (shared with coding-agent). Floppy memory stays 1–99;
//! codegraph 500+. Clean break: no data migration from pre-v200 tables.
//!
//! All statements are `CREATE … IF NOT EXISTS` so re-apply is a no-op. Hosts that
//! still have pre-v200 tables should delete `store.db` (project is new; no upgrade path).

use crate::datastore::Migration;

/// Full session-related DDL (sessions tree, turns, todos, goals, host sidecars).
pub const CANONICAL_SESSION_SCHEMA_SQL: &str = r#"
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
    active_leaf_id TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    turn_count INTEGER NOT NULL DEFAULT 0,
    total_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_output_tokens INTEGER NOT NULL DEFAULT 0,
    total_cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    total_cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    total_cost REAL NOT NULL DEFAULT 0,
    last_turn_at TEXT,
    entry_count INTEGER NOT NULL DEFAULT 0,
    approx_bytes INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_sessions_updated_at ON sessions(updated_at);
CREATE INDEX IF NOT EXISTS idx_sessions_cwd ON sessions(cwd);
CREATE INDEX IF NOT EXISTS idx_sessions_cwd_updated ON sessions(cwd, updated_at);
CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_pinned ON sessions(pinned);

CREATE TABLE IF NOT EXISTS session_entries (
    session_id TEXT NOT NULL,
    id TEXT NOT NULL,
    entry_seq INTEGER NOT NULL,
    parent_id TEXT,
    type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    turn_id TEXT,
    role TEXT,
    payload_bytes INTEGER NOT NULL DEFAULT 0,
    payload TEXT NOT NULL,
    PRIMARY KEY (session_id, id)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_session_entries_session_seq
    ON session_entries(session_id, entry_seq);
CREATE INDEX IF NOT EXISTS idx_session_entries_session_parent
    ON session_entries(session_id, parent_id);
CREATE INDEX IF NOT EXISTS idx_session_entries_session_type
    ON session_entries(session_id, type);
CREATE INDEX IF NOT EXISTS idx_session_entries_turn
    ON session_entries(session_id, turn_id);

CREATE TABLE IF NOT EXISTS session_sequences (
    session_id TEXT PRIMARY KEY,
    next_seq INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS session_turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'started',
    operation_id TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    wall_clock_ms INTEGER NOT NULL DEFAULT 0,
    provider_id TEXT,
    model_id TEXT,
    thinking_level TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cost REAL NOT NULL DEFAULT 0,
    user_entry_id TEXT,
    assistant_entry_id TEXT,
    error_message TEXT,
    UNIQUE (session_id, turn_index)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_session_turns_session_started
    ON session_turns(session_id, started_at);
CREATE INDEX IF NOT EXISTS idx_session_turns_session_status
    ON session_turns(session_id, status);

CREATE TABLE IF NOT EXISTS session_todos (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS idx_session_todos_session_position
    ON session_todos(session_id, position);
CREATE INDEX IF NOT EXISTS idx_session_todos_session_status
    ON session_todos(session_id, status);

CREATE TABLE IF NOT EXISTS goals (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    objective TEXT NOT NULL,
    completion_criterion TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    turns_used INTEGER NOT NULL DEFAULT 0,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    wall_clock_ms INTEGER NOT NULL DEFAULT 0,
    wall_clock_budget_ms INTEGER NOT NULL DEFAULT 0,
    turn_budget INTEGER NOT NULL DEFAULT 0,
    token_budget INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS idx_goals_session_id ON goals(session_id);
CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);

CREATE TABLE IF NOT EXISTS skill_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    skill_hash TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT,
    UNIQUE(skill_name, skill_hash)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_skill_cache_name ON skill_cache(skill_name);
CREATE INDEX IF NOT EXISTS idx_skill_cache_expires ON skill_cache(expires_at);

CREATE TABLE IF NOT EXISTS agent_spawn_edges (
    parent_session_id TEXT NOT NULL,
    child_session_id TEXT NOT NULL,
    agent_path TEXT NOT NULL,
    depth INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (parent_session_id, child_session_id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_agent_spawn_parent ON agent_spawn_edges(parent_session_id);
CREATE INDEX IF NOT EXISTS idx_agent_spawn_path ON agent_spawn_edges(agent_path);
"#;

/// Alias kept for existing call sites that referenced the tree-only constant.
pub const SESSION_TREE_SCHEMA_SQL: &str = CANONICAL_SESSION_SCHEMA_SQL;

/// Standalone / library migration for Turso session backends.
///
/// Version **200** is the clean session schema (hybrid tree + turns + todos + goals).
pub const SESSION_TREE_MIGRATIONS: [Migration; 1] = [Migration {
    version: 200,
    name: "elph_session_schema_v2",
    up: CANONICAL_SESSION_SCHEMA_SQL,
}];
