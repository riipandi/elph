mod common;

use elph_agent::compaction::{
    CompactionPreparation, CompactionSettings, DEFAULT_COMPACTION_SETTINGS, calculate_context_tokens, compact,
    compute_file_lists, create_file_ops, estimate_context_tokens, estimate_tokens, extract_file_ops_from_message,
    find_cut_point, find_turn_start_index, format_file_operations, get_last_assistant_usage, prepare_compaction,
    serialize_conversation, should_compact,
};
use elph_agent::session::SessionTreeEntry;
use elph_agent::types::AgentMessage;
use elph_ai::{
    AssistantContentBlock, ContentBlock, FauxResponseStep, Message, StopReason, ToolCall, Usage, UserContent,
    create_models, faux_assistant_message, faux_provider, faux_text,
};
use serde_json::json;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::User {
        content: UserContent::Text(text.to_string()),
        timestamp: 0,
    }))
}

fn assistant_message(text: &str, usage: Option<Usage>) -> AgentMessage {
    let mut assistant = faux_assistant_message(vec![faux_text(text)], None);
    if let Some(usage) = usage {
        assistant.usage = usage;
    }
    AgentMessage::Llm(Box::new(Message::Assistant(assistant)))
}

fn assistant_with_tool(name: &str, path: &str) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::Assistant(faux_assistant_message(
        vec![AssistantContentBlock::ToolCall(ToolCall::new(
            "tc1",
            name,
            json!({ "path": path }),
        ))],
        None,
    ))))
}

fn message_entry(id: &str, parent_id: Option<&str>, message: AgentMessage) -> SessionTreeEntry {
    SessionTreeEntry::Message {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        message,
    }
}

#[test]
fn estimate_tokens_uses_char_heuristic() {
    assert_eq!(estimate_tokens(&user_message("12345678")), 2);
    assert_eq!(estimate_tokens(&assistant_message("1234", None)), 1);
}

#[test]
fn calculate_context_tokens_prefers_total_tokens() {
    let usage = Usage {
        input: 10,
        output: 5,
        total_tokens: 42,
        ..Usage::default()
    };
    assert_eq!(calculate_context_tokens(&usage), 42);
}

#[test]
fn estimate_context_tokens_uses_assistant_usage_when_present() {
    let usage = Usage {
        input: 100,
        output: 20,
        total_tokens: 120,
        ..Usage::default()
    };
    let messages = vec![
        user_message("hello"),
        assistant_message("world", Some(usage)),
        user_message("after"),
    ];
    let estimate = estimate_context_tokens(&messages);
    assert_eq!(estimate.usage_tokens, 120);
    assert_eq!(estimate.trailing_tokens, estimate_tokens(&user_message("after")));
    assert_eq!(estimate.tokens, 120 + estimate.trailing_tokens);
}

#[test]
fn should_compact_respects_settings() {
    let settings = DEFAULT_COMPACTION_SETTINGS;
    assert!(!should_compact(100_000, 128_000, settings));
    assert!(should_compact(120_000, 128_000, settings));
    assert!(!should_compact(
        120_000,
        128_000,
        CompactionSettings {
            enabled: false,
            ..settings
        }
    ));
}

#[test]
fn find_cut_point_keeps_recent_tokens() {
    let entries = vec![
        message_entry("u1", None, user_message(&"a".repeat(400))),
        message_entry("a1", Some("u1"), assistant_message("short", None)),
        message_entry("u2", Some("a1"), user_message(&"b".repeat(400))),
        message_entry("a2", Some("u2"), assistant_message("tail", None)),
    ];
    let cut = find_cut_point(&entries, 0, entries.len(), 50);
    assert!(cut.first_kept_entry_index >= 2);
}

#[test]
fn find_turn_start_index_finds_user_turn() {
    let entries = vec![
        message_entry("u1", None, user_message("start")),
        message_entry("a1", Some("u1"), assistant_message("middle", None)),
        message_entry("a2", Some("a1"), assistant_message("end", None)),
    ];
    assert_eq!(find_turn_start_index(&entries, 2, 0), Some(0));
}

#[test]
fn prepare_compaction_returns_none_for_empty_or_compacted_leaf() {
    let settings = DEFAULT_COMPACTION_SETTINGS;
    assert!(prepare_compaction(&[], settings).unwrap().is_none());
    let entries = vec![SessionTreeEntry::Compaction {
        id: "c1".to_string(),
        parent_id: None,
        timestamp: "t".to_string(),
        summary: "summary".to_string(),
        first_kept_entry_id: "u1".to_string(),
        tokens_before: 10,
        details: None,
        from_hook: None,
    }];
    assert!(prepare_compaction(&entries, settings).unwrap().is_none());
}

#[test]
fn prepare_compaction_selects_history_to_summarize() {
    let entries = vec![
        message_entry("u1", None, user_message(&"old ".repeat(200))),
        message_entry("a1", Some("u1"), assistant_message("old reply", None)),
        message_entry("u2", Some("a1"), user_message(&"recent ".repeat(200))),
        message_entry("a2", Some("u2"), assistant_message("recent reply", None)),
    ];
    let preparation = prepare_compaction(
        &entries,
        CompactionSettings {
            keep_recent_tokens: 50,
            ..DEFAULT_COMPACTION_SETTINGS
        },
    )
    .unwrap()
    .expect("preparation");
    assert_eq!(preparation.first_kept_entry_id, "u2");
    assert_eq!(preparation.messages_to_summarize.len(), 2);
    assert!(preparation.tokens_before > 0);
}

#[test]
fn serialize_conversation_formats_roles() {
    let messages = vec![
        Message::User {
            content: UserContent::Text("hello".to_string()),
            timestamp: 0,
        },
        Message::Assistant(faux_assistant_message(vec![faux_text("hi there")], None)),
        Message::ToolResult {
            tool_call_id: "tc1".to_string(),
            tool_name: "echo".to_string(),
            content: vec![ContentBlock::Text {
                text: "done".to_string(),
            }],
            details: None,
            is_error: false,
            timestamp: 0,
        },
    ];
    let text = serialize_conversation(&messages);
    assert!(text.contains("[User]: hello"));
    assert!(text.contains("[Assistant]: hi there"));
    assert!(text.contains("[Tool result]: done"));
}

#[test]
fn file_ops_tracking_and_formatting() {
    let mut file_ops = create_file_ops();
    extract_file_ops_from_message(&assistant_with_tool("read", "/a.rs"), &mut file_ops);
    extract_file_ops_from_message(&assistant_with_tool("edit", "/b.rs"), &mut file_ops);
    extract_file_ops_from_message(&assistant_with_tool("write", "/b.rs"), &mut file_ops);
    let (read_files, modified_files) = compute_file_lists(&file_ops);
    assert_eq!(read_files, vec!["/a.rs".to_string()]);
    assert_eq!(modified_files, vec!["/b.rs".to_string()]);
    let formatted = format_file_operations(&read_files, &modified_files);
    assert!(formatted.contains("<read-files>"));
    assert!(formatted.contains("<modified-files>"));
}

#[test]
fn get_last_assistant_usage_skips_aborted_messages() {
    let mut aborted = faux_assistant_message(vec![faux_text("x")], Some(StopReason::Aborted));
    aborted.usage.total_tokens = 99;
    let entries = vec![
        message_entry("u1", None, user_message("hi")),
        message_entry(
            "a1",
            Some("u1"),
            AgentMessage::Llm(Box::new(Message::Assistant(aborted))),
        ),
        message_entry(
            "a2",
            Some("a1"),
            assistant_message(
                "ok",
                Some(Usage {
                    total_tokens: 12,
                    ..Usage::default()
                }),
            ),
        ),
    ];
    assert_eq!(
        get_last_assistant_usage(&entries).map(|usage| usage.total_tokens),
        Some(12)
    );
}

#[tokio::test]
async fn compact_generates_summary_with_faux_provider() {
    let faux = faux_provider(Default::default());
    let mut models = create_models(None);
    models.set_provider(faux.provider.clone());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Goal\nShip feature")],
        None,
    ))]);

    let preparation = CompactionPreparation {
        first_kept_entry_id: "u2".to_string(),
        messages_to_summarize: vec![user_message("old question"), assistant_message("old answer", None)],
        turn_prefix_messages: Vec::new(),
        is_split_turn: false,
        tokens_before: 500,
        previous_summary: None,
        file_ops: create_file_ops(),
        settings: DEFAULT_COMPACTION_SETTINGS,
    };

    let result = compact(preparation, &models, &model, None, None, None)
        .await
        .expect("compact");
    assert!(result.summary.contains("## Goal"));
    assert_eq!(result.first_kept_entry_id, "u2");
    assert_eq!(result.tokens_before, 500);
}
