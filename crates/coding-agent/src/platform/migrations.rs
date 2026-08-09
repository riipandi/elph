use elph_agent::Migration;
use elph_agent::session::migrations::CANONICAL_SESSION_SCHEMA_SQL;

/// Platform schema migrations, applied into the shared `.elph/store.db` ledger.
///
/// Version bands: floppy memory 1–99, **platform/session 201**, floppy codegraph 500–599.
/// All bands share one `app_migrations` table.
///
/// Clean break at v201: hybrid session tree + turns + todos + goals with FK + indexes.
/// No data migration — delete `store.db` if upgrading from an experimental build.
pub fn metadata_migrations() -> &'static [Migration] {
    &[Migration {
        version: 201,
        name: "elph_session_schema_v2_relational",
        // Shared with elph-agent `SESSION_TREE_MIGRATIONS` / `CANONICAL_SESSION_SCHEMA_SQL`.
        up: CANONICAL_SESSION_SCHEMA_SQL,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::{GoalStore, TodoStore, TursoSessionRepo, TursoSessionRepoCreateOptions, ensure_database};

    #[test]
    fn platform_migrations_are_session_schema_v2_relational() {
        let last = metadata_migrations().last().expect("migrations");
        assert_eq!(last.version, 201);
        assert_eq!(last.name, "elph_session_schema_v2_relational");
        assert!(last.up.contains("FOREIGN KEY (session_id) REFERENCES sessions(id)"));
        assert!(last.up.contains("CREATE TABLE session_turns"));
        assert!(last.up.contains("CREATE TABLE session_todos"));
        assert!(!last.up.contains("transcript_snapshot"));
        assert!(!last.up.contains("skill_cache"));
    }

    #[tokio::test]
    async fn platform_metadata_db_supports_sessions_goals_todos_and_fk() {
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

        let todos = TodoStore::new(&db);
        let items = todos
            .replace(
                "sess_platform",
                vec![elph_agent::TodoUpdate {
                    id: Some("todo_aaaaaaaaaaaaaaaa".into()),
                    content: Some("item".into()),
                    status: Some(elph_agent::TodoStatus::Pending),
                }],
            )
            .await
            .expect("todos");
        assert_eq!(items.len(), 1);

        // FK: cannot insert a goal for a missing session.
        let orphan = goals.create_goal("sess_missing_zzzz", "orphan", None, 0, 0, 0).await;
        assert!(orphan.is_err(), "FK should reject orphan session_id");
    }
}
