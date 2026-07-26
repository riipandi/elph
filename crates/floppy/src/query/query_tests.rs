use super::super::*;
use crate::types::{MemoryCategory, MemoryReportInput, ReportCorrectionInput, TaskEndInput, UserInputSource};
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

struct TestCtx {
    _dir: tempfile::TempDir,
    store: MemoryStore,
}

impl TestCtx {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("store.db").to_string_lossy().into_owned();
        let store = create_memory_store(FloppyConfig::new(db_path, "test").top_k(3).dimensions(4), mock_embed());
        Self { _dir: dir, store }
    }
}

#[tokio::test]
async fn get_status_includes_categories() {
    let ctx = TestCtx::new();
    ctx.store
        .report_user_input(crate::ReportUserInput {
            lesson: "use pnpm".into(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("report");

    let status = ctx.store.get_status().await.expect("status");
    assert_eq!(status.total_memories, 1);
    assert_eq!(status.categories.len(), 1);
    assert_eq!(status.categories[0].category, MemoryCategory::User);
}

#[tokio::test]
async fn list_memories_filters_category() {
    let ctx = TestCtx::new();
    ctx.store
        .insert_raw_memory("insight note", MemoryCategory::Insight, 1.0)
        .await
        .expect("insight");
    ctx.store
        .report_user_input(crate::ReportUserInput {
            lesson: "user note".into(),
            source: UserInputSource::UserInput,
        })
        .await
        .expect("user");

    let all = ctx.store.list_memories(None).await.expect("all");
    assert_eq!(all.len(), 2);

    let users = ctx
        .store
        .list_memories(Some(MemoryCategory::User))
        .await
        .expect("users");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].category, MemoryCategory::User);
}

#[tokio::test]
async fn search_memories_is_read_only() {
    let ctx = TestCtx::new();
    let mem_id = ctx
        .store
        .report_user_input(crate::ReportUserInput {
            lesson: "auth middleware path".into(),
            source: UserInputSource::UserCorrection,
        })
        .await
        .expect("report");

    let hits = ctx.store.search_memories("auth middleware").await.expect("search");
    assert!(hits.iter().any(|m| m.id == mem_id));

    let tasks = ctx.store.list_tasks(10).await.expect("tasks");
    assert!(tasks.is_empty(), "search_memories must not create tasks");
}

#[tokio::test]
async fn report_unified_insight_and_end_with_decay() {
    let ctx = TestCtx::new();
    let id = ctx
        .store
        .report(MemoryReportInput::insight("VDBE architecture"))
        .await
        .expect("insight");
    assert!(!id.is_empty());

    let start = ctx.store.start_task("explore vm").await.expect("start");
    let result = ctx
        .store
        .end_task_with_decay(
            &start.task_id,
            TaskEndInput {
                tokens_used: 100,
                tool_calls: 1,
                errors: 0,
                user_corrections: 0,
                completed: true,
                self_report: None,
            },
        )
        .await
        .expect("end+decay");
    assert!(result.decay.decayed >= 1);

    let timeline = ctx.store.get_timeline(10).await.expect("timeline");
    assert!(!timeline.is_empty());
}

#[tokio::test]
async fn contradict_wrapper_returns_struct() {
    let ctx = TestCtx::new();
    let id = ctx
        .store
        .report_correction(ReportCorrectionInput {
            lesson: "old".into(),
            what_failed: "a".into(),
            what_worked: "b".into(),
            tokens_wasted: None,
            tools_wasted: None,
        })
        .await
        .expect("report");

    let result = ctx.store.contradict(&id, Some("corrected")).await.expect("contradict");
    assert!(result.deleted);
    assert!(result.correction_id.is_some());
}

#[test]
fn floppy_paths_project_local() {
    let paths = FloppyPaths::project_local();
    assert!(paths.db_path().ends_with(".floppy/store.db"));
    let cfg = paths.config("sess");
    assert_eq!(cfg.session_id, "sess");
}
