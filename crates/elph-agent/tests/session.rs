use std::fs;

use elph_agent::{
    AgentMessage, BranchSummaryOptions, EVENTS_FILE, InMemorySessionStorage, SUMMARY_FILE, Session,
    SessionDirCreateOptions, SessionDirStorage, SessionStorage, SessionTreeEntry, TursoSessionStorage,
};
use elph_ai::{Message, UserContent, faux_assistant_message, faux_text};
use serde_json::json;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::User {
        content: UserContent::Text(text.to_string()),
        timestamp: 0,
    }))
}

fn assistant_message(text: &str) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::Assistant(faux_assistant_message(
        vec![faux_text(text)],
        None,
    ))))
}

async fn run_session_suite<S, F, Fut>(mut create_storage: F)
where
    S: SessionStorage,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = S>,
{
    let mut session = Session::new(create_storage().await);
    session.append_message(user_message("one")).await.expect("append");
    session.append_message(assistant_message("two")).await.expect("append");
    let context = session.build_context().await.expect("context");
    assert_eq!(
        context.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        vec!["user", "assistant"]
    );

    let mut session = Session::new(create_storage().await);
    session.append_message(user_message("one")).await.expect("append");
    session.append_model_change("openai", "gpt-4.1").await.expect("model");
    session.append_thinking_level_change("high").await.expect("thinking");
    let context = session.build_context().await.expect("context");
    assert_eq!(context.thinking_level, "high");
    assert_eq!(
        context
            .model
            .as_ref()
            .map(|model| (model.provider.as_str(), model.model_id.as_str())),
        Some(("openai", "gpt-4.1"))
    );

    let mut session = Session::new(create_storage().await);
    let user1 = session.append_message(user_message("one")).await.expect("user1");
    let assistant1 = session
        .append_message(assistant_message("two"))
        .await
        .expect("assistant1");
    session.append_message(user_message("three")).await.expect("user3");
    session.move_to(Some(&user1), None).await.expect("move");
    session
        .append_message(assistant_message("branched"))
        .await
        .expect("branch");
    let branch = session.branch(None).await.expect("branch");
    let branch_ids: Vec<_> = branch.iter().map(SessionTreeEntry::id).collect();
    assert!(branch_ids.contains(&user1.as_str()));
    assert!(!branch_ids.contains(&assistant1.as_str()));
    let context = session.build_context().await.expect("context");
    assert_eq!(
        context.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        vec!["user", "assistant"]
    );

    let mut session = Session::new(create_storage().await);
    session.append_message(user_message("one")).await.expect("append");
    session.move_to(None, None).await.expect("move root");
    assert!(session.leaf_id().await.expect("leaf").is_none());
    assert!(session.build_context().await.expect("context").messages.is_empty());

    let mut session = Session::new(create_storage().await);
    session.append_message(user_message("one")).await.expect("append");
    session.append_message(assistant_message("two")).await.expect("append");
    let user2 = session.append_message(user_message("three")).await.expect("user2");
    session.append_message(assistant_message("four")).await.expect("append");
    session
        .append_compaction("summary", user2, 1234, None, None)
        .await
        .expect("compact");
    session.append_message(user_message("five")).await.expect("append");
    let context = session.build_context().await.expect("context");
    assert_eq!(
        context.messages.first().map(AgentMessage::role),
        Some("compactionSummary")
    );
    assert_eq!(context.messages.len(), 4);

    let mut session = Session::new(create_storage().await);
    let user1 = session.append_message(user_message("one")).await.expect("user1");
    let summary_id = session
        .move_to(
            Some(&user1),
            Some(BranchSummaryOptions {
                summary: "summary text".to_string(),
                details: None,
                from_hook: None,
            }),
        )
        .await
        .expect("move")
        .expect("summary id");
    let summary_entry = session.entry(&summary_id).await.expect("summary entry");
    match summary_entry {
        SessionTreeEntry::BranchSummary { parent_id, from_id, .. } => {
            assert_eq!(parent_id.as_deref(), Some(user1.as_str()));
            assert_eq!(from_id, user1);
        }
        other => panic!("expected branch summary, got {other:?}"),
    }
    let context = session.build_context().await.expect("context");
    assert_eq!(context.messages.get(1).map(AgentMessage::role), Some("branchSummary"));

    let mut session = Session::new(create_storage().await);
    session.append_message(user_message("one")).await.expect("append");
    session
        .append_custom_message_entry(
            "custom",
            elph_agent::session::CustomMessageEntryContent::Text("hello".to_string()),
            true,
            Some(json!({ "ok": true })),
        )
        .await
        .expect("custom");
    let context = session.build_context().await.expect("context");
    assert_eq!(context.messages.get(1).map(AgentMessage::role), Some("custom"));

    let mut session = Session::new(create_storage().await);
    session
        .append_session_name(" hello\nworld\r\nagain ")
        .await
        .expect("name");
    assert_eq!(session.session_name().await, Some("hello world again".to_string()));

    let mut session = Session::new(create_storage().await);
    let user1 = session.append_message(user_message("one")).await.expect("user1");
    session.append_label(&user1, Some("checkpoint")).await.expect("label");
    session.append_session_name("name").await.expect("name");
    let entries = session.entries().await;
    assert!(entries.iter().any(|entry| entry.entry_type() == "label"));
    assert!(entries.iter().any(|entry| entry.entry_type() == "session_info"));
    assert_eq!(session.label(&user1).await, Some("checkpoint".to_string()));
    assert_eq!(session.session_name().await, Some("name".to_string()));
    assert_eq!(session.build_context().await.expect("context").messages.len(), 1);

    let mut session = Session::new(create_storage().await);
    let error = session.append_label("missing", Some("checkpoint")).await.unwrap_err();
    assert!(error.message.contains("Entry missing not found"));

    let storage = create_storage().await;
    let mut session = Session::new(storage);
    let user1 = session.append_message(user_message("one")).await.expect("user1");
    session.append_message(assistant_message("two")).await.expect("append");
    session.append_label(&user1, Some("checkpoint")).await.expect("label");
    session.append_session_name("name").await.expect("name");
    session.move_to(Some(&user1), None).await.expect("move");
    session
        .append_message(assistant_message("branched"))
        .await
        .expect("branch");
    let storage = session.into_storage();
    let session = Session::new(storage);
    let context = session.build_context().await.expect("context");
    assert_eq!(
        context.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        vec!["user", "assistant"]
    );
    assert_eq!(session.label(&user1).await, Some("checkpoint".to_string()));
    assert_eq!(session.session_name().await, Some("name".to_string()));
}

#[tokio::test]
async fn session_with_in_memory_storage() {
    run_session_suite(|| async { InMemorySessionStorage::new(None).expect("storage") }).await;
}

#[tokio::test]
async fn session_with_session_dir_storage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session_dir = dir.path().join("session-1");
    let cwd = dir.path().to_string_lossy().to_string();
    run_session_suite(|| async {
        SessionDirStorage::create(
            &session_dir,
            SessionDirCreateOptions {
                cwd: cwd.clone(),
                session_id: "session-1".to_string(),
                parent_session_id: None,
                system_prompt: None,
            },
        )
        .await
        .expect("create")
    })
    .await;
}

#[tokio::test]
async fn session_dir_file_layout_matches_multi_file_format() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session_dir = dir.path().join("session-1");
    let mut session = Session::new(
        SessionDirStorage::create(
            &session_dir,
            SessionDirCreateOptions {
                cwd: dir.path().to_string_lossy().to_string(),
                session_id: "session-1".to_string(),
                parent_session_id: None,
                system_prompt: None,
            },
        )
        .await
        .expect("create"),
    );
    let user1 = session.append_message(user_message("one")).await.expect("append");
    session.append_message(assistant_message("two")).await.expect("append");
    session.append_label(&user1, Some("checkpoint")).await.expect("label");
    session.move_to(Some(&user1), None).await.expect("move");
    session
        .append_message(assistant_message("branched"))
        .await
        .expect("branch");

    assert!(session_dir.join(SUMMARY_FILE).exists());
    assert!(session_dir.join("chat_history.jsonl").exists());
    assert!(session_dir.join(EVENTS_FILE).exists());

    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(session_dir.join(SUMMARY_FILE)).expect("summary")).expect("summary");
    assert_eq!(
        summary.get("info").and_then(|info| info.get("id")),
        Some(&json!("session-1"))
    );

    let events: Vec<serde_json::Value> = fs::read_to_string(session_dir.join(EVENTS_FILE))
        .expect("events")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("entry"))
        .collect();
    assert!(events.iter().any(|entry| entry.get("type") == Some(&json!("leaf"))));
    for entry in events {
        assert!(entry.get("id").and_then(serde_json::Value::as_str).is_some());
    }
}

#[tokio::test]
async fn session_with_turso_storage() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("session.db");
    let db_path = db_path.clone();
    run_session_suite(|| async {
        let session_id = format!("session-{}", SESSION_COUNTER.fetch_add(1, Ordering::SeqCst));
        TursoSessionStorage::create(&db_path, Some(session_id))
            .await
            .expect("create")
    })
    .await;
}
