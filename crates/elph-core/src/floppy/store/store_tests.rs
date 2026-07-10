use super::*;
use crate::floppy::create_memory_store;
use crate::floppy::scoring::{compute_credit, update_weight};
use crate::floppy::types::{
    FloppyConfig, MemoryCategory, ReportCorrectionInput, ReportUserInput, SelfReportEntry, TaskEndInput,
    UserInputSource,
};
use std::sync::Arc;

fn mock_embed() -> EmbedFn {
    Arc::new(|text: &str| {
        let text = text.to_string();
        Box::pin(async move {
            let mut vec = vec![0.0f32; 4];
            for (i, b) in text.bytes().enumerate() {
                vec[i % 4] += b as f32;
            }
            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut vec {
                    *x /= norm;
                }
            }
            Ok(vec)
        })
    })
}

fn test_config(db_path: &str) -> FloppyConfig {
    FloppyConfig {
        db_path: db_path.to_string(),
        session_id: "test-session".to_string(),
        vector_type: None,
        dimensions: Some(4),
        top_k: Some(3),
        learning_rate: Some(0.1),
        decay_rate: Some(0.995),
        apply_migrations: None,
    }
}

/// Holds a `tempfile::TempDir` so the DB path stays valid for the whole test.
struct TestCtx {
    _dir: tempfile::TempDir,
    db_path: String,
}

impl TestCtx {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("memory.db").to_string_lossy().into_owned();
        Self { _dir: dir, db_path }
    }

    fn store(&self) -> MemoryStore {
        create_memory_store(test_config(&self.db_path), mock_embed())
    }

    fn store_with(&self, mut config: FloppyConfig) -> MemoryStore {
        config.db_path = self.db_path.clone();
        create_memory_store(config, mock_embed())
    }
}

fn assert_tsid(id: &str) {
    assert_eq!(id.len(), 13);
    assert!(tsid::TSID::try_from(id).is_ok());
}

#[tokio::test]
async fn init_creates_schema() {
    let ctx = TestCtx::new();
    let store = ctx.store();
    store.init().await.expect("init");

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.total_memories, 0);
    assert_eq!(stats.task_count, 0);
}

#[tokio::test]
async fn ids_use_tsid() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let mem_id = store
        .report_user_input(ReportUserInput {
            lesson: "tsid id check".to_string(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("report");
    assert_tsid(&mem_id);

    let start = store.start_task("tsid task").await.expect("start");
    assert_tsid(&start.task_id);
}

#[tokio::test]
async fn full_task_lifecycle_with_retrieval_and_weight_update() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let mem_id = store
        .report_user_input(ReportUserInput {
            lesson: "Always use Result for fallible ops".to_string(),
            source: UserInputSource::UserCorrection,
        })
        .await
        .expect("report user input");

    let start = store
        .start_task("implement error handling in parser")
        .await
        .expect("start task");
    assert!(!start.task_id.is_empty());
    assert!(
        start.memories.iter().any(|m| m.id == mem_id),
        "relevant memory should be retrieved"
    );

    let mem = start.memories.iter().find(|m| m.id == mem_id).expect("memory");
    let weight_before = mem.weight;

    store
        .end_task(
            &start.task_id,
            TaskEndInput {
                tokens_used: 500,
                tool_calls: 3,
                errors: 0,
                user_corrections: 0,
                completed: true,
                self_report: Some(vec![SelfReportEntry {
                    memory_id: mem_id.clone(),
                    score: 3,
                }]),
            },
        )
        .await
        .expect("end task");

    let top = store.get_top_by_weight(5).await.expect("top");
    let updated = top.iter().find(|m| m.id == mem_id).expect("updated memory");
    let expected = update_weight(weight_before, compute_credit(1.0, 3.0, 1), 0.1);
    assert!(
        (updated.weight - expected).abs() < 1e-9,
        "weight should be updated via EMA: got {}, expected {}",
        updated.weight,
        expected
    );

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.task_count, 1);
    assert_eq!(stats.total_memories, 1);
    assert!(stats.avg_task_score > 0.0);
}

#[tokio::test]
async fn report_correction_inserts_without_prior_task() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let id = store
        .report_correction(ReportCorrectionInput {
            lesson: "Use bcrypt".to_string(),
            what_failed: "md5".to_string(),
            what_worked: "bcrypt".to_string(),
            tokens_wasted: Some(1000),
            tools_wasted: None,
        })
        .await
        .expect("correction");

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.total_memories, 1, "correction insert should persist (id={id})");

    let user_id = store
        .report_user_input(ReportUserInput {
            lesson: "user lesson".to_string(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("user input");
    let stats2 = store.get_stats().await.expect("stats2");
    assert_eq!(
        stats2.total_memories, 2,
        "user insert should work alongside correction (user_id={user_id})"
    );

    let top = store.get_top_by_weight(2).await.expect("top");
    assert!(top.iter().any(|m| m.id == id));
}

#[tokio::test]
async fn report_correction_sets_weight_from_tokens_wasted() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let task = store.start_task("fix auth bug").await.expect("start");
    store
        .end_task(
            &task.task_id,
            TaskEndInput {
                tokens_used: 10_000,
                tool_calls: 5,
                errors: 0,
                user_corrections: 0,
                completed: true,
                self_report: None,
            },
        )
        .await
        .expect("end");

    let id = store
        .report_correction(ReportCorrectionInput {
            lesson: "Use bcrypt not md5".to_string(),
            what_failed: "md5 hash".to_string(),
            what_worked: "bcrypt".to_string(),
            tokens_wasted: Some(5000),
            tools_wasted: Some(2),
        })
        .await
        .expect("correction");

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.total_memories, 1, "correction memory should be stored");

    let top = store.get_top_by_weight(1).await.expect("top");
    assert_eq!(top[0].id, id);
    assert!((top[0].weight - 1.5).abs() < f64::EPSILON);
    assert!(top[0].content.contains("Failed approach"));
}

#[tokio::test]
async fn insert_raw_memory_and_embed_pending() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let id = store
        .insert_raw_memory("raw discovery note", MemoryCategory::Discovery, 1.5)
        .await
        .expect("insert raw");

    let n = store.embed_pending().await.expect("embed pending");
    assert_eq!(n, 1);

    let start = store.start_task("discovery task").await.expect("start");
    assert!(start.memories.iter().any(|m| m.id == id));
}

#[tokio::test]
async fn purge_cleans_orphan_memory_retrievals() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let mem_id = store
        .report_user_input(ReportUserInput {
            lesson: "orphan retrieval test".to_string(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("report");

    let start = store.start_task("task with retrieval").await.expect("start");
    assert!(start.memories.iter().any(|m| m.id == mem_id));

    store
        .insert_raw_memory("purge me", MemoryCategory::Insight, 0.05)
        .await
        .expect("insert");

    let purged = store.purge(0.1).await.expect("purge");
    assert_eq!(purged, 1);

    let orphans: i64 = store
        .with_db(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT COUNT(*) FROM memory_retrievals WHERE memory_id NOT IN (SELECT id FROM memories)",
                    (),
                )
                .await
                .map_err(anyhow::Error::from)?;
            let row = rows
                .next()
                .await
                .map_err(anyhow::Error::from)?
                .ok_or_else(|| anyhow::anyhow!("no row"))?;
            let count: i64 = row.get(0).map_err(anyhow::Error::from)?;
            drain_rows(&mut rows).await?;
            Ok(count)
        })
        .await
        .expect("orphan count");
    assert_eq!(orphans, 0, "purge should remove orphan retrieval rows");
}

#[tokio::test]
async fn decay_and_purge_maintenance() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    store
        .insert_raw_memory("low weight memory", MemoryCategory::Insight, 0.1)
        .await
        .expect("insert");

    let decayed = store.decay().await.expect("decay");
    assert_eq!(decayed.decayed, 1);

    let purged = store.purge(0.2).await.expect("purge");
    assert_eq!(purged, 1);

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.total_memories, 0);
}

#[tokio::test]
async fn contradict_memory_deletes_and_optionally_replaces() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let id = store
        .report_user_input(ReportUserInput {
            lesson: "old fact".to_string(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("report");

    let (deleted, correction_id) = store
        .contradict_memory(&id, Some("corrected fact"))
        .await
        .expect("contradict");
    assert!(deleted);
    assert!(correction_id.is_some());

    let stats = store.get_stats().await.expect("stats");
    assert_eq!(stats.total_memories, 1);
}

#[tokio::test]
async fn penalize_memory_reduces_weight_with_floor() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let id = store
        .insert_raw_memory("penalized", MemoryCategory::User, 2.0)
        .await
        .expect("insert");

    store.penalize_memory(&id, 0.25).await.expect("penalize");

    let top = store.get_top_by_weight(1).await.expect("top");
    assert!((top[0].weight - 0.5).abs() < f64::EPSILON);

    store.penalize_memory(&id, 0.01).await.expect("penalize again");
    let top = store.get_top_by_weight(1).await.expect("top");
    assert!((top[0].weight - 0.1).abs() < f64::EPSILON);
}

#[tokio::test]
async fn baseline_persists_across_store_instances() {
    let ctx = TestCtx::new();

    let store1 = ctx.store();
    let task = store1.start_task("first task").await.expect("start");
    store1
        .end_task(
            &task.task_id,
            TaskEndInput {
                tokens_used: 1000,
                tool_calls: 2,
                errors: 1,
                user_corrections: 0,
                completed: true,
                self_report: None,
            },
        )
        .await
        .expect("end");
    store1.close().await.expect("close");

    let store2 = ctx.store();
    store2.init().await.expect("re-init");
    let task2 = store2.start_task("second task").await.expect("start");
    store2
        .end_task(
            &task2.task_id,
            TaskEndInput {
                tokens_used: 800,
                tool_calls: 1,
                errors: 0,
                user_corrections: 0,
                completed: true,
                self_report: None,
            },
        )
        .await
        .expect("end");

    let stats = store2.get_stats().await.expect("stats");
    assert_eq!(stats.task_count, 2);
}

#[tokio::test]
async fn start_task_with_no_memories_returns_empty() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let start = store.start_task("fresh task with no memories").await.expect("start");
    assert!(start.memories.is_empty());
}

#[tokio::test]
async fn top_k_limits_retrieved_memories() {
    let ctx = TestCtx::new();
    let mut config = test_config(&ctx.db_path);
    config.top_k = Some(2);
    let store = ctx.store_with(config);

    for i in 0..5 {
        store
            .insert_raw_memory(&format!("memory number {i}"), MemoryCategory::Insight, 1.0)
            .await
            .expect("insert");
    }
    store.embed_pending().await.expect("embed");

    let start = store.start_task("memory number").await.expect("start");
    assert_eq!(start.memories.len(), 2);
}

#[tokio::test]
async fn end_task_clears_current_task_id() {
    let ctx = TestCtx::new();
    let store = ctx.store();

    let task = store.start_task("task").await.expect("start");
    store
        .end_task(
            &task.task_id,
            TaskEndInput {
                tokens_used: 100,
                tool_calls: 0,
                errors: 0,
                user_corrections: 0,
                completed: true,
                self_report: None,
            },
        )
        .await
        .expect("end");

    let id = store
        .report_user_input(ReportUserInput {
            lesson: "after end".to_string(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("report");
    assert!(!id.is_empty());
}

#[test]
fn is_lock_err_detects_lock_messages() {
    assert!(is_lock_err("database is locked"));
    assert!(is_lock_err("Locking error"));
    assert!(!is_lock_err("syntax error"));
}
