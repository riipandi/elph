mod common;

use std::sync::Arc;

use parking_lot::Mutex;

use common::new_faux_with_options;
use elph_agent::CompactionErrorCode;
use elph_agent::build_session_context;
use elph_agent::compaction::{
    CompactionPreparation, CompactionSettings, DEFAULT_COMPACTION_SETTINGS, calculate_context_tokens, compact,
    compute_file_lists, create_file_ops, estimate_context_tokens, estimate_tokens, extract_file_ops_from_message,
    find_cut_point, find_turn_start_index, format_file_operations, generate_summary, get_last_assistant_usage,
    prepare_compaction, serialize_conversation, should_compact,
};
use elph_agent::session::SessionTreeEntry;
use elph_agent::types::{AgentMessage, CustomAgentMessage};
use elph_ai::api::faux::{FauxModelDefinition, RegisterFauxProviderOptions};
use elph_ai::models::{CreateProviderOptions, ProviderApi, ProviderStreamsDyn, create_models, create_provider};
use elph_ai::providers::adapter::faux_api;
use elph_ai::{
    AssistantContentBlock, ContentBlock, FauxResponseStep, Message, SimpleStreamOptions, StopReason, ThinkingLevel,
    ToolCall, Usage, UserContent, faux_assistant_message, faux_text, faux_thinking,
};
use serde_json::json;

struct CapturingFauxStreams {
    inner: Arc<dyn ProviderStreamsDyn>,
    captured: Arc<Mutex<Vec<SimpleStreamOptions>>>,
}

impl ProviderStreamsDyn for CapturingFauxStreams {
    fn stream(
        &self,
        model: &elph_ai::Model,
        context: &elph_ai::Context,
        options: Option<elph_ai::StreamOptions>,
    ) -> elph_ai::utils::event_stream::AssistantMessageEventStream {
        self.inner.stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: &elph_ai::Model,
        context: &elph_ai::Context,
        options: Option<SimpleStreamOptions>,
    ) -> elph_ai::utils::event_stream::AssistantMessageEventStream {
        if let Some(opts) = options.clone() {
            self.captured.lock().push(opts);
        }
        self.inner.stream_simple(model, context, options)
    }
}

fn faux_models_with_capture(
    options: RegisterFauxProviderOptions,
) -> (
    elph_ai::FauxProviderHandle,
    Arc<elph_ai::Models>,
    Arc<Mutex<Vec<SimpleStreamOptions>>>,
) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let faux = new_faux_with_options(options);
    let inner = faux_api(faux.core.api());
    let wrapper = Arc::new(CapturingFauxStreams {
        inner,
        captured: captured.clone(),
    });
    let provider = create_provider(CreateProviderOptions {
        id: faux.provider.id.clone(),
        name: Some(faux.provider.name.clone()),
        base_url: faux.provider.base_url.clone(),
        headers: faux.provider.headers.clone(),
        auth: faux.provider.auth.clone(),
        models: faux.provider.get_models().to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(wrapper),
    });
    let mut models = create_models(None);
    models.set_provider(provider);
    (faux, models.into_arc(), captured)
}

fn faux_model_options(reasoning: bool, max_tokens: u32) -> RegisterFauxProviderOptions {
    RegisterFauxProviderOptions {
        models: Some(vec![FauxModelDefinition {
            id: if reasoning {
                "reasoning-model".to_string()
            } else {
                "non-reasoning-model".to_string()
            },
            name: None,
            reasoning: Some(reasoning),
            input: None,
            context_window: Some(200_000),
            max_tokens: Some(max_tokens),
        }]),
        ..Default::default()
    }
}

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

fn error_assistant_message(message: &str, stop_reason: StopReason) -> elph_ai::AssistantMessage {
    let mut assistant = faux_assistant_message(vec![], Some(stop_reason));
    assistant.error_message = Some(message.to_string());
    assistant
}

fn message_entry(id: &str, parent_id: Option<&str>, message: AgentMessage) -> SessionTreeEntry {
    SessionTreeEntry::Message {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        message,
    }
}

fn thinking_level_entry(id: &str, parent_id: Option<&str>, level: &str) -> SessionTreeEntry {
    SessionTreeEntry::ThinkingLevelChange {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        thinking_level: level.to_string(),
    }
}

fn model_change_entry(id: &str, parent_id: Option<&str>, provider: &str, model_id: &str) -> SessionTreeEntry {
    SessionTreeEntry::ModelChange {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
    }
}

fn branch_summary_entry(id: &str, parent_id: Option<&str>, from_id: &str, summary: &str) -> SessionTreeEntry {
    SessionTreeEntry::BranchSummary {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        from_id: from_id.to_string(),
        summary: summary.to_string(),
        details: None,
        from_hook: None,
    }
}

fn custom_message_entry(id: &str, parent_id: Option<&str>, content: &str) -> SessionTreeEntry {
    SessionTreeEntry::CustomMessage {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        custom_type: "note".to_string(),
        content: elph_agent::session::types::CustomMessageEntryContent::Text(content.to_string()),
        display: true,
        details: None,
    }
}

fn compaction_entry(
    id: &str,
    parent_id: Option<&str>,
    summary: &str,
    first_kept_entry_id: &str,
    details: Option<serde_json::Value>,
) -> SessionTreeEntry {
    SessionTreeEntry::Compaction {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        summary: summary.to_string(),
        first_kept_entry_id: first_kept_entry_id.to_string(),
        tokens_before: 1234,
        details,
        from_hook: None,
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
fn calculate_context_tokens_sums_usage_fields_when_total_missing() {
    let usage = Usage {
        input: 1000,
        output: 500,
        cache_read: 200,
        cache_write: 100,
        total_tokens: 0,
        ..Usage::default()
    };
    assert_eq!(calculate_context_tokens(&usage), 1800);
    assert_eq!(calculate_context_tokens(&Usage::default()), 0);
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
    let settings = CompactionSettings {
        enabled: true,
        reserve_tokens: 10_000,
        keep_recent_tokens: 20_000,
    };
    assert!(should_compact(95_000, 100_000, settings));
    assert!(!should_compact(89_000, 100_000, settings));
    assert!(!should_compact(
        95_000,
        100_000,
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
fn find_cut_point_uses_token_differences() {
    let mut entries = Vec::new();
    for i in 0..10 {
        let parent_id = if i == 0 { None } else { Some(format!("a{}", i - 1)) };
        entries.push(message_entry(
            &format!("u{i}"),
            parent_id.as_deref(),
            user_message(&format!("User {i}")),
        ));
        entries.push(message_entry(
            &format!("a{i}"),
            Some(&format!("u{i}")),
            assistant_message(&"x".repeat((i + 1) * 1000), None),
        ));
    }

    let cut = find_cut_point(&entries, 0, entries.len(), 2500);
    assert!(matches!(
        entries[cut.first_kept_entry_index],
        SessionTreeEntry::Message { .. }
    ));
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
fn find_cut_point_and_turn_start_edge_cases() {
    let thinking = thinking_level_entry("thinking", None, "high");
    let model_change = model_change_entry("model", Some("thinking"), "openai", "gpt-4");
    assert_eq!(
        find_cut_point(&[thinking.clone(), model_change.clone()], 0, 2, 1),
        elph_agent::compaction::CutPointResult {
            first_kept_entry_index: 0,
            turn_start_index: None,
            is_split_turn: false,
        }
    );

    let branch_summary = branch_summary_entry("branch", Some("model"), "branch", "branch summary");
    let custom_message = custom_message_entry("custom", Some("branch"), "custom content");
    assert_eq!(
        find_turn_start_index(&[thinking.clone(), branch_summary.clone()], 1, 0),
        Some(1)
    );
    assert_eq!(
        find_turn_start_index(&[thinking.clone(), custom_message.clone()], 1, 0),
        Some(1)
    );
    assert_eq!(find_turn_start_index(&[thinking, model_change], 1, 0), None);

    let cut = find_cut_point(
        &[
            branch_summary_entry("branch2", None, "branch", "branch summary"),
            custom_message_entry("custom2", Some("branch2"), "custom content"),
            message_entry("keep", Some("custom2"), user_message("keep")),
        ],
        0,
        3,
        1,
    );
    assert_eq!(cut.first_kept_entry_index, 0);

    let tool_result = message_entry(
        "tool",
        None,
        AgentMessage::Llm(Box::new(Message::ToolResult {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            content: vec![ContentBlock::Text {
                text: "tool output".to_string(),
            }],
            details: None,
            added_tool_names: None,
            is_error: false,
            timestamp: 0,
        })),
    );
    assert_eq!(
        find_cut_point(&[tool_result], 0, 1, 1),
        elph_agent::compaction::CutPointResult {
            first_kept_entry_index: 0,
            turn_start_index: None,
            is_split_turn: false,
        }
    );

    let user = message_entry("user", None, user_message("user"));
    let compaction = compaction_entry("compact", Some("user"), "summary", "user", None);
    let assistant = message_entry("assistant", Some("compact"), assistant_message("assistant", None));
    assert_eq!(
        find_cut_point(&[user, compaction, assistant], 0, 3, 1).first_kept_entry_index,
        2
    );
}

#[test]
fn estimate_tokens_across_supported_message_roles() {
    let usage = Usage {
        input: 10,
        output: 5,
        cache_read: 3,
        cache_write: 2,
        total_tokens: 20,
        ..Usage::default()
    };
    let assistant = assistant_message("assistant", Some(usage.clone()));
    let assistant_with_thinking_and_tool = AgentMessage::Llm(Box::new(Message::Assistant(faux_assistant_message(
        vec![
            faux_thinking("thinking"),
            AssistantContentBlock::ToolCall(ToolCall::new("call-1", "read", json!({ "path": "file.ts" }))),
        ],
        None,
    ))));
    let custom_string = AgentMessage::Custom(CustomAgentMessage::Custom {
        kind: "note".to_string(),
        content: json!("custom text"),
        display: true,
        details: None,
        timestamp: 0,
    });
    let tool_result_with_image = AgentMessage::Llm(Box::new(Message::ToolResult {
        tool_call_id: "call-1".to_string(),
        tool_name: "read".to_string(),
        content: vec![
            ContentBlock::Text {
                text: "tool text".to_string(),
            },
            ContentBlock::Image {
                data: "abc".to_string(),
                mime_type: "image/png".to_string(),
            },
        ],
        details: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 0,
    }));
    let bash_execution = AgentMessage::Custom(CustomAgentMessage::BashExecution {
        command: "npm run check".to_string(),
        output: Some("ok".to_string()),
        exit_code: None,
        cancelled: false,
        truncated: false,
        full_output_path: None,
        timestamp: 0,
        exclude_from_context: false,
    });
    let branch_summary = AgentMessage::Custom(CustomAgentMessage::BranchSummary {
        summary: "branch".to_string(),
        from_id: "x".to_string(),
        timestamp: 0,
    });
    let compaction_summary = AgentMessage::Custom(CustomAgentMessage::CompactionSummary {
        summary: "compact".to_string(),
        tokens_before: 123,
        timestamp: 0,
    });

    assert!(estimate_tokens(&user_message("plain user")) > 0);
    assert!(estimate_tokens(&assistant_with_thinking_and_tool) > 0);
    assert!(estimate_tokens(&custom_string) > 0);
    assert!(estimate_tokens(&tool_result_with_image) > 1000);
    assert!(estimate_tokens(&bash_execution) > 0);
    assert!(estimate_tokens(&branch_summary) > 0);
    assert!(estimate_tokens(&compaction_summary) > 0);

    assert_eq!(
        get_last_assistant_usage(&[
            message_entry("u1", None, user_message("user")),
            message_entry("a1", Some("u1"), assistant.clone()),
        ])
        .map(|u| u.total_tokens),
        Some(20)
    );

    let mut aborted = faux_assistant_message(vec![faux_text("x")], Some(StopReason::Aborted));
    aborted.usage = usage.clone();
    let mut errored = faux_assistant_message(vec![faux_text("x")], Some(StopReason::Error));
    errored.usage = usage;
    assert!(
        get_last_assistant_usage(&[
            message_entry("a1", None, AgentMessage::Llm(Box::new(Message::Assistant(aborted)))),
            message_entry("a2", None, AgentMessage::Llm(Box::new(Message::Assistant(errored)))),
        ])
        .is_none()
    );

    assert_eq!(
        get_last_assistant_usage(&[
            message_entry("u1", None, user_message("user")),
            message_entry("a1", Some("u1"), assistant.clone()),
            message_entry("a2", Some("a1"), assistant_message("partial", Some(Usage::default())),),
        ])
        .map(|u| u.total_tokens),
        Some(20)
    );

    assert_eq!(
        estimate_context_tokens(&[user_message("no usage")]).last_usage_index,
        None
    );
    let estimate = estimate_context_tokens(&[assistant, user_message("tail")]);
    assert_eq!(estimate.usage_tokens, 20);
    assert_eq!(estimate.last_usage_index, Some(0));

    let partial_estimate = estimate_context_tokens(&[
        user_message("Hello"),
        assistant_message(
            "world",
            Some(Usage {
                input: 10,
                output: 10,
                total_tokens: 20,
                ..Usage::default()
            }),
        ),
        user_message("continue"),
        assistant_message("Partial thinking", Some(Usage::default())),
    ]);
    assert_eq!(partial_estimate.usage_tokens, 20);
    assert_eq!(partial_estimate.last_usage_index, Some(1));
    assert!(partial_estimate.trailing_tokens > 0);
    assert_eq!(partial_estimate.tokens, 20 + partial_estimate.trailing_tokens);
}

#[test]
fn build_session_context_includes_compaction_summary() {
    let entries = vec![
        message_entry("u1", None, user_message("1")),
        message_entry("a1", Some("u1"), assistant_message("a", None)),
        message_entry("u2", Some("a1"), user_message("2")),
        message_entry("a2", Some("u2"), assistant_message("b", None)),
        compaction_entry("c1", Some("a2"), "Summary of 1,a,2,b", "u2", None),
        message_entry("u3", Some("c1"), user_message("3")),
        message_entry("a3", Some("u3"), assistant_message("c", None)),
    ];
    let loaded = build_session_context(&entries);
    assert_eq!(loaded.messages.len(), 5);
    assert_eq!(loaded.messages[0].role(), "compactionSummary");
}

#[test]
fn build_session_context_tracks_model_and_thinking_level() {
    let mut assistant = faux_assistant_message(vec![faux_text("a")], None);
    assistant.provider = "anthropic".into();
    assistant.model = "claude-sonnet-4-5".to_string();
    let entries = vec![
        message_entry("u1", None, user_message("1")),
        model_change_entry("model", Some("u1"), "openai", "gpt-4"),
        message_entry(
            "a1",
            Some("model"),
            AgentMessage::Llm(Box::new(Message::Assistant(assistant))),
        ),
        thinking_level_entry("thinking", Some("a1"), "high"),
    ];
    let loaded = build_session_context(&entries);
    assert_eq!(
        loaded.model,
        Some(elph_agent::session::SessionModelRef {
            provider: "anthropic".to_string(),
            model_id: "claude-sonnet-4-5".to_string(),
        })
    );
    assert_eq!(loaded.thinking_level, "high");
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
fn prepare_compaction_uses_previous_summary() {
    let entries = vec![
        message_entry("u1", None, user_message("user msg 1")),
        message_entry("a1", Some("u1"), assistant_message("assistant msg 1", None)),
        message_entry("u2", Some("a1"), user_message("user msg 2")),
        message_entry(
            "a2",
            Some("u2"),
            assistant_message(
                "assistant msg 2",
                Some(Usage {
                    input: 5000,
                    output: 1000,
                    total_tokens: 6000,
                    ..Usage::default()
                }),
            ),
        ),
        compaction_entry("c1", Some("a2"), "First summary", "u2", None),
        message_entry("u3", Some("c1"), user_message("user msg 3")),
        message_entry(
            "a3",
            Some("u3"),
            assistant_message(
                "assistant msg 3",
                Some(Usage {
                    input: 8000,
                    output: 2000,
                    total_tokens: 10_000,
                    ..Usage::default()
                }),
            ),
        ),
    ];
    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS)
        .unwrap()
        .expect("preparation");
    assert_eq!(preparation.previous_summary.as_deref(), Some("First summary"));
    assert!(!preparation.first_kept_entry_id.is_empty());
    assert_eq!(
        preparation.tokens_before,
        estimate_context_tokens(&build_session_context(&entries).messages).tokens
    );
}

#[test]
fn prepare_compaction_split_turn_includes_prior_file_ops() {
    let entries = vec![
        message_entry("u1", None, user_message("user msg 1")),
        message_entry("a1", Some("u1"), assistant_with_tool("write", "written.ts")),
        compaction_entry(
            "c1",
            Some("a1"),
            "First summary",
            "u1",
            Some(json!({
                "readFiles": ["old-read.ts"],
                "modifiedFiles": ["old-edit.ts"]
            })),
        ),
        message_entry("u2", Some("c1"), user_message(&"large turn ".repeat(200))),
        message_entry("a2", Some("u2"), assistant_message("large assistant message", None)),
    ];
    let preparation = prepare_compaction(
        &entries,
        CompactionSettings {
            enabled: true,
            reserve_tokens: 100,
            keep_recent_tokens: 1,
        },
    )
    .unwrap()
    .expect("preparation");

    assert_eq!(preparation.previous_summary.as_deref(), Some("First summary"));
    assert!(preparation.is_split_turn);
    assert_eq!(
        preparation
            .turn_prefix_messages
            .iter()
            .map(AgentMessage::role)
            .collect::<Vec<_>>(),
        vec!["user"]
    );
    assert!(preparation.file_ops.read.contains("old-read.ts"));
    assert!(preparation.file_ops.edited.contains("old-edit.ts"));
    assert!(preparation.file_ops.written.contains("written.ts"));
}

#[test]
fn prepare_compaction_includes_branch_and_custom_entries() {
    let entries = vec![
        branch_summary_entry("branch", None, "branch", "branch summary"),
        custom_message_entry("custom", Some("branch"), "custom content"),
        message_entry("user", Some("custom"), user_message("keep")),
        message_entry("assistant", Some("user"), assistant_message("assistant", None)),
    ];
    let preparation = prepare_compaction(
        &entries,
        CompactionSettings {
            enabled: true,
            reserve_tokens: 100,
            keep_recent_tokens: 1,
        },
    )
    .unwrap()
    .expect("preparation");
    assert_eq!(
        preparation
            .messages_to_summarize
            .iter()
            .map(AgentMessage::role)
            .collect::<Vec<_>>(),
        vec!["branchSummary", "custom"]
    );
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
            added_tool_names: None,
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
fn serialize_conversation_truncates_long_tool_results() {
    let long_content = "x".repeat(5000);
    let messages = vec![Message::ToolResult {
        tool_call_id: "tc1".to_string(),
        tool_name: "read".to_string(),
        content: vec![ContentBlock::Text { text: long_content }],
        details: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 0,
    }];
    let result = serialize_conversation(&messages);
    assert!(result.contains("[Tool result]:"));
    assert!(result.contains("[... 3000 more characters truncated]"));
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
async fn generate_summary_passes_reasoning_for_reasoning_models() {
    let messages = vec![user_message("Summarize this.")];

    let (faux_reasoning, models_reasoning, options_capture) = faux_models_with_capture(faux_model_options(true, 8192));
    let reasoning_model = faux_reasoning.provider.get_models()[0].clone();
    faux_reasoning.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Goal\nTest summary")],
        None,
    ))]);
    generate_summary(
        &messages,
        &models_reasoning,
        &reasoning_model,
        2000,
        None,
        None,
        None,
        Some(ThinkingLevel::Medium),
    )
    .await
    .expect("summary");
    assert_eq!(options_capture.lock()[0].reasoning, Some(ThinkingLevel::Medium));

    let (faux_off, models_off, options_capture_off) = faux_models_with_capture(faux_model_options(true, 8192));
    let off_model = faux_off.provider.get_models()[0].clone();
    faux_off.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Goal\nTest summary")],
        None,
    ))]);
    generate_summary(&messages, &models_off, &off_model, 2000, None, None, None, None)
        .await
        .expect("summary");
    assert_eq!(options_capture_off.lock()[0].reasoning, None);

    let (faux_non_reasoning, models_non_reasoning, options_capture_non) =
        faux_models_with_capture(faux_model_options(false, 8192));
    let non_reasoning_model = faux_non_reasoning.provider.get_models()[0].clone();
    faux_non_reasoning.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Goal\nTest summary")],
        None,
    ))]);
    generate_summary(
        &messages,
        &models_non_reasoning,
        &non_reasoning_model,
        2000,
        None,
        None,
        None,
        Some(ThinkingLevel::Medium),
    )
    .await
    .expect("summary");
    assert_eq!(options_capture_non.lock()[0].reasoning, None);
}

#[tokio::test]
async fn generate_summary_includes_previous_summary_and_instructions() {
    let messages = vec![user_message("Summarize this.")];
    let prompt_text = Arc::new(Mutex::new(String::new()));
    let prompt_capture = prompt_text.clone();

    let (faux, models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(move |context, _, _, _| {
        if let Some(Message::User {
            content: UserContent::Text(text),
            ..
        }) = context.messages.first()
        {
            *prompt_capture.lock() = text.clone();
        }
        faux_assistant_message(vec![faux_text("## Goal\nTest summary")], None)
    }))]);

    let summary = generate_summary(
        &messages,
        &models,
        &model,
        2000,
        None,
        Some("focus"),
        Some("old summary"),
        None,
    )
    .await
    .expect("summary");

    let prompt = prompt_text.lock().clone();
    assert!(summary.contains("Test summary"));
    assert!(prompt.contains("<previous-summary>\nold summary\n</previous-summary>"));
    assert!(prompt.contains("Additional focus: focus"));
}

#[tokio::test]
async fn generate_summary_returns_errors_for_failed_generations() {
    let messages = vec![user_message("Summarize this.")];

    let (error_faux, error_models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let error_model = error_faux.provider.get_models()[0].clone();
    error_faux.set_responses(vec![FauxResponseStep::Static(error_assistant_message(
        "boom",
        StopReason::Error,
    ))]);
    let error_result = generate_summary(&messages, &error_models, &error_model, 2000, None, None, None, None)
        .await
        .unwrap_err();
    assert_eq!(error_result.code, CompactionErrorCode::SummarizationFailed);
    assert_eq!(error_result.message, "Summarization failed: boom");

    let (aborted_faux, aborted_models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let aborted_model = aborted_faux.provider.get_models()[0].clone();
    aborted_faux.set_responses(vec![FauxResponseStep::Static(error_assistant_message(
        "stopped",
        StopReason::Aborted,
    ))]);
    let aborted_result = generate_summary(&messages, &aborted_models, &aborted_model, 2000, None, None, None, None)
        .await
        .unwrap_err();
    assert_eq!(aborted_result.code, CompactionErrorCode::Aborted);
    assert_eq!(aborted_result.message, "stopped");
}

#[tokio::test]
async fn compact_clamps_summary_max_tokens_to_model_cap() {
    let messages = vec![user_message("Summarize this.")];
    let (faux, models, captured) = faux_models_with_capture(faux_model_options(false, 128_000));
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("## Goal\nTest summary")], None)),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("## Goal\nTest summary")], None)),
    ]);

    let preparation = CompactionPreparation {
        first_kept_entry_id: "entry-keep".to_string(),
        messages_to_summarize: messages.clone(),
        turn_prefix_messages: messages,
        is_split_turn: true,
        tokens_before: 600_000,
        previous_summary: None,
        file_ops: create_file_ops(),
        settings: CompactionSettings {
            enabled: true,
            reserve_tokens: 500_000,
            keep_recent_tokens: 20_000,
        },
    };

    compact(preparation, &models, &model, None, None, None)
        .await
        .expect("compact");

    let max_tokens: Vec<_> = captured.lock().iter().map(|options| options.base.max_tokens).collect();
    assert_eq!(max_tokens, vec![Some(128_000), Some(128_000)]);
}

#[tokio::test]
async fn compact_returns_errors_without_panicking() {
    let messages = vec![user_message("Summarize this.")];
    let preparation = CompactionPreparation {
        first_kept_entry_id: "entry-keep".to_string(),
        messages_to_summarize: messages.clone(),
        turn_prefix_messages: Vec::new(),
        is_split_turn: false,
        tokens_before: 100,
        previous_summary: None,
        file_ops: create_file_ops(),
        settings: CompactionSettings {
            enabled: true,
            reserve_tokens: 2000,
            keep_recent_tokens: 20,
        },
    };

    let (history_faux, history_models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let history_model = history_faux.provider.get_models()[0].clone();
    history_faux.set_responses(vec![FauxResponseStep::Static(error_assistant_message(
        "history failed",
        StopReason::Error,
    ))]);
    let history_result = compact(preparation.clone(), &history_models, &history_model, None, None, None)
        .await
        .unwrap_err();
    assert_eq!(history_result.code, CompactionErrorCode::SummarizationFailed);
    assert_eq!(history_result.message, "Summarization failed: history failed");

    let (invalid_faux, invalid_models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let invalid_model = invalid_faux.provider.get_models()[0].clone();
    let invalid_result = compact(
        CompactionPreparation {
            first_kept_entry_id: String::new(),
            messages_to_summarize: Vec::new(),
            ..preparation
        },
        &invalid_models,
        &invalid_model,
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_result.code, CompactionErrorCode::InvalidSession);
}

#[tokio::test]
async fn compact_passes_reasoning_for_turn_prefix_summaries() {
    let messages = vec![user_message("Summarize this.")];
    let (faux, models, captured) = faux_models_with_capture(faux_model_options(true, 8192));
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Original Request\nTest summary")],
        None,
    ))]);

    let preparation = CompactionPreparation {
        first_kept_entry_id: "entry-keep".to_string(),
        messages_to_summarize: Vec::new(),
        turn_prefix_messages: messages,
        is_split_turn: true,
        tokens_before: 100,
        previous_summary: None,
        file_ops: create_file_ops(),
        settings: CompactionSettings {
            enabled: true,
            reserve_tokens: 2000,
            keep_recent_tokens: 20,
        },
    };

    compact(preparation, &models, &model, None, None, Some(ThinkingLevel::High))
        .await
        .expect("compact");

    assert_eq!(captured.lock()[0].reasoning, Some(ThinkingLevel::High));
}

#[tokio::test]
async fn compact_returns_turn_prefix_errors_without_panicking() {
    let messages = vec![user_message("Summarize this.")];
    let preparation = CompactionPreparation {
        first_kept_entry_id: "entry-keep".to_string(),
        messages_to_summarize: Vec::new(),
        turn_prefix_messages: messages.clone(),
        is_split_turn: true,
        tokens_before: 100,
        previous_summary: None,
        file_ops: create_file_ops(),
        settings: CompactionSettings {
            enabled: true,
            reserve_tokens: 2000,
            keep_recent_tokens: 20,
        },
    };

    let (faux, models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(error_assistant_message(
        "prefix failed",
        StopReason::Error,
    ))]);
    let error_result = compact(preparation.clone(), &models, &model, None, None, None)
        .await
        .unwrap_err();
    assert_eq!(error_result.code, CompactionErrorCode::SummarizationFailed);
    assert_eq!(error_result.message, "Turn prefix summarization failed: prefix failed");

    let (aborted_faux, aborted_models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let aborted_model = aborted_faux.provider.get_models()[0].clone();
    aborted_faux.set_responses(vec![FauxResponseStep::Static(error_assistant_message(
        "prefix stopped",
        StopReason::Aborted,
    ))]);
    let aborted_result = compact(preparation, &aborted_models, &aborted_model, None, None, None)
        .await
        .unwrap_err();
    assert_eq!(aborted_result.code, CompactionErrorCode::Aborted);
    assert_eq!(aborted_result.message, "prefix stopped");
}

#[tokio::test]
async fn compact_generates_summary_with_faux_provider() {
    let (faux, models, _) = faux_models_with_capture(Default::default());
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

#[tokio::test]
async fn compact_returns_result_with_file_details() {
    let entries = vec![
        message_entry("u1", None, user_message("read a file")),
        message_entry("a1", Some("u1"), assistant_with_tool("read", "src/index.ts")),
        message_entry("u2", Some("a1"), user_message("continue")),
        message_entry(
            "a2",
            Some("u2"),
            assistant_message(
                "done",
                Some(Usage {
                    input: 4000,
                    output: 500,
                    total_tokens: 4500,
                    ..Usage::default()
                }),
            ),
        ),
    ];
    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS)
        .unwrap()
        .expect("preparation");

    let (faux, models, _) = faux_models_with_capture(faux_model_options(false, 8192));
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("## Goal\nTest summary")],
        None,
    ))]);

    let result = compact(preparation, &models, &model, None, None, None)
        .await
        .expect("compact");
    assert!(!result.summary.is_empty());
    assert!(!result.first_kept_entry_id.is_empty());
    assert!(result.details.is_some());
}
