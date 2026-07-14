//! Full compaction pipeline — estimate context, track file operations, prepare compaction.
//!
//! Demonstrates: `estimate_context_tokens`, `should_compact`, `create_file_ops`,
//! `compute_file_lists`, `format_file_operations`, `serialize_conversation`,
//! `FileOperations`, `CompactionSettings`, `DEFAULT_COMPACTION_SETTINGS`,
//! `CompactionResult`, `CompactionDetails`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_compaction_full
//! ```

use elph_agent::{AgentMessage, llm_message_to_agent};
use elph_ai::{AssistantMessage, Message, TextContent, UserContent};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Build a conversation for compaction demo.
fn build_conversation(turns: usize) -> Vec<AgentMessage> {
    let mut messages = Vec::new();
    for i in 0..turns {
        messages.push(llm_message_to_agent(Message::User {
            content: UserContent::Text(format!("User turn {i}: Refactor the database module")),
            timestamp: now_ms(),
        }));
        messages.push(llm_message_to_agent(Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![elph_ai::AssistantContentBlock::Text(TextContent::new(format!(
                "Assistant turn {i}: Here's the refactoring plan for iteration {i}..."
            )))],
            api: "faux".into(),
            provider: "faux".into(),
            model: "faux-1".into(),
            response_model: None,
            response_id: None,
            diagnostics: None,
            usage: elph_ai::Usage {
                input: 200 + i as u64 * 50,
                output: 400 + i as u64 * 100,
                total_tokens: 600 + i as u64 * 150,
                ..Default::default()
            },
            stop_reason: elph_ai::StopReason::Stop,
            error_message: None,
            timestamp: now_ms(),
        })));
    }
    messages
}

fn main() {
    // ── 1. Create conversation ──
    let messages = build_conversation(10);
    println!("=== Conversation ===");
    println!("  {} messages ({} turns)", messages.len(), messages.len() / 2);

    // ── 2. Estimate context ──
    println!("\n=== Context Estimation ===");
    let estimate = elph_agent::compaction::estimate_context_tokens(&messages);
    println!("  total tokens:    {}", estimate.tokens);
    println!("  usage tokens:    {}", estimate.usage_tokens);
    println!("  trailing tokens: {}", estimate.trailing_tokens);
    if let Some(idx) = estimate.last_usage_index {
        println!("  last usage at message index: {idx}");
    }

    // ── 3. Should we compact? ──
    println!("\n=== Compaction Decision ===");
    let context_window: u64 = 128_000;
    let should = elph_agent::compaction::should_compact(
        estimate.tokens,
        context_window,
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS,
    );
    println!("  context window:     {context_window}");
    println!(
        "  reserve_tokens:     {}",
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS.reserve_tokens
    );
    println!(
        "  keep_recent_tokens: {}",
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS.keep_recent_tokens
    );
    println!("  should compact:     {should}");

    // ── 4. File operations ──
    println!("\n=== File Operations ===");
    let mut file_ops = elph_agent::compaction::create_file_ops();
    file_ops.read.insert("src/main.rs".into());
    file_ops.edited.insert("src/lib.rs".into());
    file_ops.written.insert("Cargo.toml".into());
    println!(
        "  file_ops: {} reads, {} edited, {} written",
        file_ops.read.len(),
        file_ops.edited.len(),
        file_ops.written.len(),
    );

    let (read_files, modified_files) = elph_agent::compaction::compute_file_lists(&file_ops);
    println!("  computed: {} read, {} modified", read_files.len(), modified_files.len());
    let formatted = elph_agent::compaction::format_file_operations(&read_files, &modified_files);
    println!("  formatted:");
    for line in formatted.lines() {
        println!("    {line}");
    }

    // ── 5. Serialize conversation ──
    let llm_msgs: Vec<elph_ai::Message> = messages.iter().filter_map(|m| m.as_llm().cloned()).collect();
    println!("\n=== Serialize ===");
    let serialized = elph_agent::compaction::serialize_conversation(&llm_msgs);
    println!("  serialized length: {} bytes", serialized.len());
    println!("  preview:");
    for line in serialized.lines().take(8) {
        println!("    {line}");
    }

    // ── 6. CompactionResult ──
    println!("\n=== CompactionResult ===");
    let result = elph_agent::compaction::CompactionResult {
        summary: "User requested database module refactoring across 10 turns. \
                  Main changes: extracted repository trait, added migration runner, \
                  replaced raw SQL with query builder. File operations: read src/main.rs, \
                  modified src/lib.rs, created Cargo.toml."
            .into(),
        first_kept_entry_id: "tsid-keep-001".into(),
        tokens_before: 45_600,
        details: Some(elph_agent::compaction::CompactionDetails {
            read_files: vec!["src/main.rs".into(), "src/lib.rs".into()],
            modified_files: vec!["Cargo.toml".into()],
        }),
    };
    println!(
        "  summary (first 80 chars):     {}",
        &result.summary[..80.min(result.summary.len())]
    );
    println!("  first_kept_entry_id:            {}", result.first_kept_entry_id);
    println!("  tokens_before:                  {}", result.tokens_before);
    if let Some(details) = &result.details {
        println!("  read files:                     {}", details.read_files.join(", "));
        println!("  modified files:                 {}", details.modified_files.join(", "));
    }

    // ── 7. CompactionSettings ──
    println!("\n=== CompactionSettings ===");
    println!(
        "  DEFAULT: enabled={}, reserve={}, keep_recent={}",
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS.enabled,
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS.reserve_tokens,
        elph_agent::compaction::DEFAULT_COMPACTION_SETTINGS.keep_recent_tokens,
    );

    println!("\nDone.");
}
