use elph_agent::Migration;

/// Platform schema migrations, applied into the shared `.elph/store.db` ledger.
///
/// Version bands: floppy memory 1–99, elph-agent session tree 100, **platform
/// 101–199**, floppy codegraph 500–599. All bands share one `app_migrations`
/// table, so platform versions must not collide with other bands.
///
/// The legacy user-level `metadata.db` (versions 1–8) is orphaned: sessions are
/// project-scoped now, and the platform schema below is renumbered and reshaped
/// to fully idempotent DDL (`CREATE ... IF NOT EXISTS`, no `ALTER TABLE`). That
/// makes application order irrelevant: the session-tree migration (v100) may
/// create `sessions`/`session_entries`/`session_sequences` first and these
/// migrations become no-ops, or vice versa. The dual-model `messages` chat log
/// (old v2, dropped by old v8) is not recreated.
pub fn metadata_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 101,
            name: "create_sessions_table",
            // Mirrors elph-agent `SESSION_TREE_SCHEMA_SQL` so v100 and v101 agree.
            up: "CREATE TABLE IF NOT EXISTS sessions (
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
                CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);",
        },
        Migration {
            version: 102,
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
            version: 103,
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
            version: 104,
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
        // Old v6 (add_goal_id_column) intentionally not renumbered — goal_id is
        // no longer a separate column. The prefixed Kalid `goal_<16>` is the PK.
        Migration {
            version: 105,
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
        // Pi-aligned session tree (sqlite-node): entries + sequences only.
        // Idempotent: mirrors elph-agent `SESSION_TREE_SCHEMA_SQL`; no ALTERs,
        // no DROP of the legacy `messages` table (never recreated).
        Migration {
            version: 106,
            name: "session_tree_pi_schema",
            up: "CREATE TABLE IF NOT EXISTS session_entries (
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
                ) STRICT;",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::{GoalStore, TursoSessionRepo, TursoSessionRepoCreateOptions, ensure_database};

    #[test]
    fn platform_migrations_end_at_session_tree() {
        let last = metadata_migrations().last().expect("migrations");
        assert_eq!(last.version, 106);
        assert_eq!(last.name, "session_tree_pi_schema");
        // Goals migration still present and unchanged in sequence.
        let goals = metadata_migrations().iter().find(|m| m.version == 103).expect("goals");
        assert_eq!(goals.name, "create_goals_table");
        assert!(goals.up.contains("CREATE TABLE IF NOT EXISTS goals"));
    }

    #[tokio::test]
    async fn platform_metadata_db_supports_sessions_and_goals() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("store.db");
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
