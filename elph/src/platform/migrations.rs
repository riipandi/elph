use elph_agent::Migration;
use elph_core::floppy::migrations::{V1_NAME, V1_UP, V2_NAME, V2_UP, V3_NAME, V3_UP};

pub fn metadata_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 1,
            name: "create_sessions_table",
            up: "CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    work_dir TEXT,
                    provider_id TEXT,
                    model_id TEXT,
                    agent_mode TEXT DEFAULT 'build',
                    system_prompt TEXT,
                    metadata TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at);
                CREATE INDEX IF NOT EXISTS idx_sessions_work_dir ON sessions(work_dir);",
        },
        Migration {
            version: 2,
            name: "create_messages_table",
            up: "CREATE TABLE IF NOT EXISTS messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT,
                    tool_call_id TEXT,
                    tool_calls TEXT,
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
                CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);",
        },
        Migration {
            version: 3,
            name: "create_todos_table",
            up: "CREATE TABLE IF NOT EXISTS todos (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
                    content TEXT NOT NULL,
                    completed BOOLEAN NOT NULL DEFAULT 0,
                    position INTEGER NOT NULL DEFAULT 0,
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_todos_session_id ON todos(session_id);
                CREATE INDEX IF NOT EXISTS idx_todos_position ON todos(session_id, position);",
        },
        Migration {
            version: 4,
            name: "create_goals_table",
            up: "CREATE TABLE IF NOT EXISTS goals (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
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
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    completed_at DATETIME,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_goals_session_id ON goals(session_id);
                CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);",
        },
        Migration {
            version: 5,
            name: "create_skill_cache_table",
            up: "CREATE TABLE IF NOT EXISTS skill_cache (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    skill_name TEXT NOT NULL,
                    skill_hash TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    expires_at DATETIME,
                    UNIQUE(skill_name, skill_hash)
                );
                CREATE INDEX IF NOT EXISTS idx_skill_cache_name ON skill_cache(skill_name);
                CREATE INDEX IF NOT EXISTS idx_skill_cache_expires ON skill_cache(expires_at);",
        },
        Migration {
            version: 6,
            name: "add_goal_id_column",
            up: "ALTER TABLE goals ADD COLUMN goal_id TEXT;
                CREATE INDEX IF NOT EXISTS idx_goals_goal_id ON goals(goal_id);",
        },
        Migration {
            version: 7,
            name: "create_agent_spawn_edges_table",
            up: "CREATE TABLE IF NOT EXISTS agent_spawn_edges (
                    parent_session_id TEXT NOT NULL,
                    child_session_id TEXT NOT NULL,
                    agent_path TEXT NOT NULL,
                    depth INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'open',
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (parent_session_id, child_session_id)
                );
                CREATE INDEX IF NOT EXISTS idx_agent_spawn_parent ON agent_spawn_edges(parent_session_id);
                CREATE INDEX IF NOT EXISTS idx_agent_spawn_path ON agent_spawn_edges(agent_path);",
        },
    ]
}

/// Project-local memory store (`.elph/memory.db`).
///
/// Composed from floppy schema migrations (ported from
/// [memelord](https://github.com/glommer/memelord)); append Elph-specific entries with
/// `version > migrations::LAST_VERSION`.
pub fn memory_migrations() -> &'static [Migration] {
    const MIGRATIONS: &[Migration] = &[
        Migration {
            version: 1,
            name: V1_NAME,
            up: V1_UP,
        },
        Migration {
            version: 2,
            name: V2_NAME,
            up: V2_UP,
        },
        Migration {
            version: 3,
            name: V3_NAME,
            up: V3_UP,
        },
    ];
    MIGRATIONS
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_core::floppy::migrations;

    #[test]
    fn memory_migrations_track_floppy_versions() {
        assert_eq!(memory_migrations().len(), migrations::MIGRATIONS.len());
        assert_eq!(
            memory_migrations().last().map(|m| m.version),
            Some(migrations::LAST_VERSION)
        );
    }
}
