//! Canonical session schema for Turso backends (hybrid tree + relational metrics).
//!
//! Platform band version **201** (shared with coding-agent). Floppy memory stays 1–99;
//! codegraph 500+. Clean break: no data migration from pre-v201 tables.
//!
//! Foreign keys are declared in DDL; connections must run `PRAGMA foreign_keys = ON`
//! (see [`crate::datastore::connect`]).

use crate::datastore::Migration;

/// Full session-related DDL with PK / FK / indexes.
///
/// Child tables cascade-delete with the parent `sessions` row. Soft self-references
/// (tree `parent_id`, turn entry ids) are **not** FK-enforced so append order and
/// post-compaction prune stay simple.
pub const CANONICAL_SESSION_SCHEMA_SQL: &str = r#"
-- Rebuild session domain tables so FK definitions apply (clean break; no data migrate).
PRAGMA foreign_keys = OFF;
DROP TABLE IF EXISTS agent_spawn_edges;
DROP TABLE IF EXISTS goals;
DROP TABLE IF EXISTS session_todos;
DROP TABLE IF EXISTS session_entries;
DROP TABLE IF EXISTS session_turns;
DROP TABLE IF EXISTS session_sequences;
DROP TABLE IF EXISTS sessions;
PRAGMA foreign_keys = ON;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    cwd TEXT,
    parent_session_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    agent_mode TEXT NOT NULL DEFAULT 'build',
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
    approx_bytes INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (parent_session_id) REFERENCES sessions(id) ON DELETE SET NULL
) STRICT;

CREATE INDEX idx_sessions_created_at ON sessions(created_at);
CREATE INDEX idx_sessions_updated_at ON sessions(updated_at);
CREATE INDEX idx_sessions_cwd ON sessions(cwd);
CREATE INDEX idx_sessions_cwd_updated ON sessions(cwd, updated_at);
CREATE INDEX idx_sessions_parent ON sessions(parent_session_id);
CREATE INDEX idx_sessions_pinned ON sessions(pinned);
CREATE INDEX idx_sessions_last_turn_at ON sessions(last_turn_at);

CREATE TABLE session_sequences (
    session_id TEXT PRIMARY KEY NOT NULL,
    next_seq INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
) STRICT;

CREATE TABLE session_turns (
    id TEXT PRIMARY KEY NOT NULL,
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
    agent_mode TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cost REAL NOT NULL DEFAULT 0,
    user_entry_id TEXT,
    assistant_entry_id TEXT,
    error_message TEXT,
    UNIQUE (session_id, turn_index),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_session_turns_session ON session_turns(session_id);
CREATE INDEX idx_session_turns_session_started ON session_turns(session_id, started_at);
CREATE INDEX idx_session_turns_session_status ON session_turns(session_id, status);
CREATE INDEX idx_session_turns_session_mode ON session_turns(session_id, agent_mode);

CREATE TABLE session_entries (
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
    PRIMARY KEY (session_id, id),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (turn_id) REFERENCES session_turns(id) ON DELETE SET NULL
) STRICT;

CREATE UNIQUE INDEX idx_session_entries_session_seq
    ON session_entries(session_id, entry_seq);
CREATE INDEX idx_session_entries_session_parent
    ON session_entries(session_id, parent_id);
CREATE INDEX idx_session_entries_session_type
    ON session_entries(session_id, type);
CREATE INDEX idx_session_entries_type_ts
    ON session_entries(session_id, type, timestamp);
CREATE INDEX idx_session_entries_turn
    ON session_entries(turn_id);
CREATE INDEX idx_session_entries_role
    ON session_entries(session_id, role);

CREATE TABLE session_todos (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_session_todos_session ON session_todos(session_id);
CREATE INDEX idx_session_todos_session_position ON session_todos(session_id, position);
CREATE INDEX idx_session_todos_session_status ON session_todos(session_id, status);

CREATE TABLE goals (
    id TEXT PRIMARY KEY NOT NULL,
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
    completed_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_goals_session_id ON goals(session_id);
CREATE INDEX idx_goals_status ON goals(status);
CREATE INDEX idx_goals_session_status ON goals(session_id, status);

CREATE TABLE agent_spawn_edges (
    parent_session_id TEXT NOT NULL,
    child_session_id TEXT NOT NULL,
    agent_path TEXT NOT NULL,
    depth INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (parent_session_id, child_session_id),
    FOREIGN KEY (parent_session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (child_session_id) REFERENCES sessions(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_agent_spawn_parent ON agent_spawn_edges(parent_session_id);
CREATE INDEX idx_agent_spawn_child ON agent_spawn_edges(child_session_id);
CREATE INDEX idx_agent_spawn_path ON agent_spawn_edges(agent_path);
CREATE INDEX idx_agent_spawn_status ON agent_spawn_edges(status);
"#;

/// Alias kept for existing call sites that referenced the tree-only constant.
pub const SESSION_TREE_SCHEMA_SQL: &str = CANONICAL_SESSION_SCHEMA_SQL;

/// Standalone / library migration for Turso session backends.
///
/// Version **201**: hybrid tree + turns/todos/goals with declarative FK + indexes.
pub const SESSION_TREE_MIGRATIONS: [Migration; 1] = [Migration {
    version: 201,
    name: "elph_session_schema_v2_relational",
    up: CANONICAL_SESSION_SCHEMA_SQL,
}];
