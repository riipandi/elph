use elph_agent::Migration;
use floppy::migrations::{V1_NAME, V1_UP, V2_NAME, V2_UP, V3_NAME, V3_UP};

pub fn metadata_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 1,
            name: "create_sessions_table",
            up: "CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    work_dir TEXT,
                    provider_id TEXT,
                    model_id TEXT,
                    agent_mode TEXT DEFAULT 'build',
                    system_prompt TEXT,
                    metadata TEXT
                ) STRICT;
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
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                ) STRICT;
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
                    completed INTEGER NOT NULL DEFAULT 0,
                    position INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                ) STRICT;
                CREATE INDEX IF NOT EXISTS idx_todos_session_id ON todos(session_id);
                CREATE INDEX IF NOT EXISTS idx_todos_position ON todos(session_id, position);",
        },
        Migration {
            version: 4,
            name: "create_goals_table",
            up: "CREATE TABLE IF NOT EXISTS goals (
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
                    completed_at TEXT,
                    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
                ) STRICT;
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
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    expires_at TEXT,
                    UNIQUE(skill_name, skill_hash)
                ) STRICT;
                CREATE INDEX IF NOT EXISTS idx_skill_cache_name ON skill_cache(skill_name);
                CREATE INDEX IF NOT EXISTS idx_skill_cache_expires ON skill_cache(expires_at);",
        },
        // v6 (add_goal_id_column) intentionally deleted — goal_id is no longer a separate column.
        // The prefixed Kalid `goal_<16>` now serves as the primary key `id`.
        Migration {
            version: 7,
            name: "create_agent_spawn_edges_table",
            up: "CREATE TABLE IF NOT EXISTS agent_spawn_edges (
                    parent_session_id TEXT NOT NULL,
                    child_session_id TEXT NOT NULL,
                    agent_path TEXT NOT NULL,
                    depth INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'open',
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (parent_session_id, child_session_id)
                ) STRICT;
                CREATE INDEX IF NOT EXISTS idx_agent_spawn_parent ON agent_spawn_edges(parent_session_id);
                CREATE INDEX IF NOT EXISTS idx_agent_spawn_path ON agent_spawn_edges(agent_path);",
        },
        // Pi-aligned session tree (sqlite-node). Goals table (v4) and spawn edges (v7)
        // are unchanged — only session index/tree tables evolve here.
        Migration {
            version: 8,
            name: "session_tree_pi_schema",
            up: r#"
                -- Projected columns for list UI / leaf pointer (no-op if already present after wipe).
                ALTER TABLE sessions ADD COLUMN cwd TEXT;
                ALTER TABLE sessions ADD COLUMN parent_session_id TEXT;
                ALTER TABLE sessions ADD COLUMN active_leaf_id TEXT;
                ALTER TABLE sessions ADD COLUMN name TEXT;

                UPDATE sessions SET cwd = work_dir WHERE cwd IS NULL AND work_dir IS NOT NULL;

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

                -- Dual-model chat log never had writers; tree is source of truth.
                DROP TABLE IF EXISTS messages;
            "#,
        },
    ]
}

/// Project-local memory store (`.elph/store.db`).
///
/// Composed from floppy schema migrations (ported from
/// [memelord](https://github.com/glommer/memelord)); append Elph-specific entries with
/// `version > migrations::LAST_VERSION`.
#[allow(dead_code)]
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
    use elph_agent::{GoalStore, TursoSessionRepo, TursoSessionRepoCreateOptions, ensure_database};
    use floppy::migrations;

    #[test]
    fn memory_migrations_track_floppy_versions() {
        assert_eq!(memory_migrations().len(), migrations::MIGRATIONS.len());
        assert_eq!(memory_migrations().last().map(|m| m.version), Some(migrations::LAST_VERSION));
    }

    #[test]
    fn platform_migrations_end_at_session_tree() {
        let last = metadata_migrations().last().expect("migrations");
        assert_eq!(last.version, 8);
        assert_eq!(last.name, "session_tree_pi_schema");
        // Goals migration still present and unchanged in sequence.
        let goals = metadata_migrations().iter().find(|m| m.version == 4).expect("goals");
        assert_eq!(goals.name, "create_goals_table");
        assert!(goals.up.contains("CREATE TABLE IF NOT EXISTS goals"));
    }

    #[tokio::test]
    async fn platform_metadata_db_supports_sessions_and_goals() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("metadata.db");
        ensure_database(&db, metadata_migrations()).await.expect("migrate");

        let repo = TursoSessionRepo::new(&db);
        let session = repo
            .create(TursoSessionRepoCreateOptions {
                cwd: "/tmp/proj".into(),
                id: Some("sess_platform".into()),
                ..Default::default()
            })
            .await
            .expect("create session");
        assert_eq!(session.metadata().await.id, "sess_platform");

        let goals = GoalStore::new(&db);
        let goal = goals
            .create_goal("sess_platform", "keep goals working", None, 0, 0, 0)
            .await
            .expect("create goal");
        assert!(goal.id.starts_with("goal_"));
        assert!(goals.get_active_goal("sess_platform").await.expect("get").is_some());
    }
}
