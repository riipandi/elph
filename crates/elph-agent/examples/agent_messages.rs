//! Custom messages, summaries, and LLM conversion — message bridge layer.
//!
//! Demonstrates: `CustomMessageContent`, `create_branch_summary_message`,
//! `create_compaction_summary_message`, `create_custom_message`, `shell_exec_execution_to_text`,
//! `default_convert_to_llm`, `default_convert_to_llm_fn`, `now_iso_timestamp`.
//!
//! ```sh
//! cargo run -p elph-agent --example agent_messages
//! ```

use elph_agent::llm_message_to_agent;
use elph_agent::messages::create_branch_summary_message;
use elph_agent::messages::create_compaction_summary_message;
use elph_agent::messages::create_custom_message;
use elph_agent::messages::default_convert_to_llm;
use elph_agent::messages::default_convert_to_llm_fn;
use elph_agent::messages::now_iso_timestamp;
use elph_agent::messages::shell_exec_execution_to_text;
use elph_agent::messages::{CustomMessageBlock, CustomMessageContent};
use elph_agent::{AgentMessage, CustomAgentMessage};
use elph_ai::{ImageContent, TextContent};

fn main() {
    let ts = || now_iso_timestamp();

    // ── 1. Branch summary message ──
    println!("=== Branch Summary ===");
    let branch = create_branch_summary_message(
        "Explored two approaches: (1) async streams with tokio, (2) sync channels with crossbeam. \
         Chose option 1 for composability.",
        "branch_abc123",
        &ts(),
    );
    if let AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, from_id, .. }) = &branch {
        println!("  from_id:  {from_id}");
        println!("  summary:  {summary}");
    }

    // ── 2. Compaction summary message ──
    println!("\n=== Compaction Summary ===");
    let compaction = create_compaction_summary_message(
        "User asked to fix a concurrency bug. Root cause: missing Mutex guard on shared state. \
         Applied fix: wrapped Vec in Arc<Mutex<>>. Added regression test.",
        12_340,
        &ts(),
    );
    if let AgentMessage::Custom(CustomAgentMessage::CompactionSummary {
        summary, tokens_before, ..
    }) = &compaction
    {
        println!("  tokens_before: {tokens_before}");
        println!("  summary:       {summary}");
    }

    // ── 3. Custom message (text) ──
    println!("\n=== Custom Text Message ===");
    let custom_text = create_custom_message(
        "user_input",
        CustomMessageContent::Text("Remember to check the CHANGELOG before release.".into()),
        true,
        None,
        &ts(),
    );
    if let AgentMessage::Custom(CustomAgentMessage::Custom {
        kind, content, display, ..
    }) = &custom_text
    {
        println!("  kind:    {kind}");
        println!("  content: {content}");
        println!("  display: {display}");
    }

    // ── 4. Custom message (blocks) ──
    println!("\n=== Custom Blocks Message ===");
    let blocks_msg = create_custom_message(
        "diagnostic",
        CustomMessageContent::Blocks(vec![
            CustomMessageBlock::Text(TextContent::new("Warning: high memory usage")),
            CustomMessageBlock::Image(ImageContent::new(
                "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",
                "image/gif",
            )),
        ]),
        false,
        Some(serde_json::json!({"severity": "info", "source": "memory_monitor"})),
        &ts(),
    );
    if let AgentMessage::Custom(CustomAgentMessage::Custom {
        kind,
        content: _,
        display,
        details,
        ..
    }) = &blocks_msg
    {
        println!("  kind:    {kind}");
        println!("  display: {display}");
        println!("  details: {details:?}");
    }

    // ── 5. shell_exec_execution_to_text formatting ──
    println!("\n=== Shell Exec Execution Formatting ===");

    // Full output, success
    let shell_exec_success = AgentMessage::Custom(CustomAgentMessage::ShellExecExecution {
        command: "cargo check".into(),
        output: Some("    Checking elph-core v0.0.15\n    Finished".into()),
        exit_code: Some(0),
        cancelled: false,
        truncated: false,
        full_output_path: None,
        exclude_from_context: false,
        timestamp: 1_700_000_000_000,
    });
    println!(
        "  success: {}",
        shell_exec_execution_to_text(&extract_custom(&shell_exec_success)).unwrap()
    );

    // Non-zero exit, truncated
    let shell_exec_fail = AgentMessage::Custom(CustomAgentMessage::ShellExecExecution {
        command: "make test".into(),
        output: Some("error[E0308]: mismatched types".into()),
        exit_code: Some(1),
        cancelled: false,
        truncated: true,
        full_output_path: Some("/tmp/full-output.log".into()),
        exclude_from_context: false,
        timestamp: 1_700_000_000_001,
    });
    println!(
        "  failure: {}",
        shell_exec_execution_to_text(&extract_custom(&shell_exec_fail)).unwrap()
    );

    // Cancelled
    let shell_exec_cancelled = AgentMessage::Custom(CustomAgentMessage::ShellExecExecution {
        command: "cargo test -- --ignored".into(),
        output: None,
        exit_code: None,
        cancelled: true,
        truncated: false,
        full_output_path: None,
        exclude_from_context: false,
        timestamp: 1_700_000_000_002,
    });
    println!(
        "  cancelled: {}",
        shell_exec_execution_to_text(&extract_custom(&shell_exec_cancelled)).unwrap()
    );

    // Excluded from context
    let shell_exec_excluded = AgentMessage::Custom(CustomAgentMessage::ShellExecExecution {
        command: "echo secret".into(),
        output: Some("secret-value".into()),
        exit_code: Some(0),
        cancelled: false,
        truncated: false,
        full_output_path: None,
        exclude_from_context: true,
        timestamp: 1_700_000_000_003,
    });
    let text = shell_exec_execution_to_text(&extract_custom(&shell_exec_excluded));
    println!("  excluded: {text:?} (returns None)");

    // ── 6. default_convert_to_llm: filter non-LLM messages ──
    println!("\n=== LLM Conversion ===");
    let mixed_messages = vec![
        llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("Hello".into()),
            timestamp: 1_700_000_000_000,
        }),
        compaction,
        llm_message_to_agent(elph_ai::Message::Assistant(elph_ai::AssistantMessage {
            role: "assistant".into(),
            content: vec![elph_ai::AssistantContentBlock::Text(TextContent::new("Hi there!"))],
            api: "faux".into(),
            provider: "faux".into(),
            model: "faux-1".into(),
            response_model: None,
            response_id: None,
            diagnostics: None,
            usage: elph_ai::Usage::default(),
            stop_reason: elph_ai::StopReason::Stop,
            error_message: None,
            timestamp: 1_700_000_000_001,
        })),
        shell_exec_success,
        branch,
    ];
    let converted = default_convert_to_llm(mixed_messages);
    println!("  input messages:  5 (user + assistant + compaction + shell_exec + branch)");
    println!("  output messages: {}", converted.len());
    for (i, msg) in converted.iter().enumerate() {
        match msg {
            elph_ai::Message::User { content, .. } => {
                let preview = match content {
                    elph_ai::UserContent::Text(t) => t.chars().take(40).collect::<String>(),
                    _ => "(tool result)".into(),
                };
                println!("    [{i}] user: \"{preview}...\"");
            }
            elph_ai::Message::Assistant(m) => {
                let preview = m
                    .content
                    .first()
                    .map(|b| match b {
                        elph_ai::AssistantContentBlock::Text(t) => t.text.chars().take(40).collect::<String>(),
                        _ => "(non-text)".into(),
                    })
                    .unwrap_or_default();
                println!("    [{i}] assistant: \"{preview}...\"");
            }
            _ => println!("    [{i}] other"),
        }
    }

    // ── 7. convert_to_llm_sync / default_convert_to_llm_fn ──
    println!("\n=== ConvertToLlmFn (trait object) ===");
    let convert_fn = default_convert_to_llm_fn();
    let single = vec![llm_message_to_agent(elph_ai::Message::User {
        content: elph_ai::UserContent::Text("test".into()),
        timestamp: 1_700_000_000_000,
    })];
    let result = tokio::runtime::Runtime::new().unwrap().block_on(convert_fn(single));
    println!("  fn output messages: {}", result.len());

    // ── 8. now_iso_timestamp ──
    println!("\n=== Timestamp ===");
    let ts_now = now_iso_timestamp();
    println!("  ISO 8601 now: {ts_now}");

    println!("\nDone.");
}

fn extract_custom(msg: &AgentMessage) -> CustomAgentMessage {
    match msg {
        AgentMessage::Custom(c) => c.clone(),
        _ => unreachable!(),
    }
}
