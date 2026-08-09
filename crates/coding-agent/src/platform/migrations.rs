use elph_agent::Migration;
use elph_agent::{CANONICAL_SESSION_SCHEMA_SQL, WORKERS_SCHEMA_SQL};

/// Platform schema migrations, applied into the shared `.elph/store.db` ledger.
///
/// Version bands: floppy memory 1–99, **platform/session 201–202**, floppy codegraph 500–599.
///
/// - v201: hybrid session tree + turns/todos/goals with FK + indexes (rebuild).
/// - v202: multi-worker leases, registry, mailbox, file claims (additive).
pub fn metadata_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 201,
            name: "elph_session_schema_v2_relational",
            up: CANONICAL_SESSION_SCHEMA_SQL,
        },
        Migration {
            version: 202,
            name: "elph_workers_v1",
            up: WORKERS_SCHEMA_SQL,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::{
        GoalStore, SessionLeaseStore, TodoStore, TursoSessionRepo, TursoSessionRepoCreateOptions, ensure_database,
    };

    #[test]
    fn platform_migrations_include_workers() {
        let versions: Vec<_> = metadata_migrations().iter().map(|m| m.version).collect();
        assert_eq!(versions, vec![201, 202]);
        let w = metadata_migrations().iter().find(|m| m.version == 202).unwrap();
        assert!(w.up.contains("session_leases"));
        assert!(w.up.contains("worker_messages"));
        assert!(w.up.contains("file_leases"));
    }

    #[tokio::test]
    async fn platform_db_supports_sessions_and_leases() {
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
        let _ = goals
            .create_goal("sess_platform", "keep goals working", None, 0, 0, 0)
            .await
            .expect("create goal");

        let todos = TodoStore::new(&db);
        let _ = todos
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

        let leases = SessionLeaseStore::new(&db);
        let lease = leases
            .try_acquire("sess_platform", "wrk_testworker0001", 30)
            .await
            .expect("lease");
        assert_eq!(lease.session_id, "sess_platform");
    }
}
