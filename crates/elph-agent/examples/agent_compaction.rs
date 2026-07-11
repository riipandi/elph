//! Context compaction — manage long conversations by summarizing old turns.
//!
//! Demonstrates: `estimate_context_tokens`, `should_compact`, `CompactionSettings`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_compaction
//! ```

use elph_agent::{
    AgentMessage,
    compaction::{CompactionSettings, estimate_context_tokens, should_compact},
    llm_message_to_agent,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Create a simulated conversation with many turns.
fn create_long_conversation(turns: usize) -> Vec<AgentMessage> {
    let mut messages = Vec::new();

    for i in 0..turns {
        // User message
        messages.push(llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text(format!("User message {i}: What is Rust?")),
            timestamp: now_ms(),
        }));

        // Assistant message with usage info
        let response = format!(
            "Assistant response {i}: Rust is a systems programming language focused on \
             safety, speed, and concurrency. This is turn {i} of {turns}."
        );
        messages.push(llm_message_to_agent(elph_ai::Message::Assistant(elph_ai::AssistantMessage {
            role: "assistant".into(),
            content: vec![elph_ai::AssistantContentBlock::Text(elph_ai::TextContent::new(
                &response,
            ))],
            api: "faux".into(),
            provider: "faux".into(),
            model: "faux-1".into(),
            response_model: None,
            response_id: None,
            diagnostics: None,
            usage: elph_ai::Usage {
                input: 100 + i as u64 * 50,
                output: 200 + i as u64 * 100,
                cache_read: 0,
                cache_write: 0,
                cache_write_1h: None,
                reasoning: None,
                total_tokens: 300 + i as u64 * 150,
                cost: elph_ai::UsageCost::default(),
            },
            stop_reason: elph_ai::StopReason::Stop,
            error_message: None,
            timestamp: now_ms(),
        })));
    }

    messages
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Create a long conversation ──
    let turns = 50;
    let messages = create_long_conversation(turns);
    println!("Created conversation with {turns} turns ({} messages)", messages.len());

    // ── Estimate context usage ──
    let estimate = estimate_context_tokens(&messages);
    println!("\n=== Context Estimate ===");
    println!("Total tokens: {}", estimate.tokens);
    println!("Usage tokens: {}", estimate.usage_tokens);
    println!("Trailing tokens: {}", estimate.trailing_tokens);
    if let Some(idx) = estimate.last_usage_index {
        println!("Last usage at message: {idx}");
    }

    // ── Check if compaction is needed ──
    let context_window: u64 = 128_000;
    let settings = CompactionSettings {
        enabled: true,
        reserve_tokens: 16384,
        keep_recent_tokens: 20000,
    };

    let should = should_compact(estimate.tokens, context_window, settings);
    println!("\n=== Compaction Decision ===");
    println!("Context window: {context_window}");
    println!("Current tokens: {}", estimate.tokens);
    println!("Reserve tokens: {}", settings.reserve_tokens);
    println!("Keep recent: {}", settings.keep_recent_tokens);
    println!("Should compact: {should}");

    // ── Show token growth pattern ──
    println!("\n=== Token Growth Pattern ===");
    for step in [10, 20, 30, 40, 50] {
        let msgs = create_long_conversation(step);
        let est = estimate_context_tokens(&msgs);
        let should = should_compact(est.tokens, context_window, settings);
        println!("  {step:3} turns: {:6} tokens, compact={should}", est.tokens);
    }

    // ── Token calculation utility ──
    println!("\n=== Token Calculation ===");
    let single_msg = &messages[0];
    if let Some(_llm) = single_msg.as_llm() {
        let tokens = elph_agent::compaction::estimate_tokens(single_msg);
        println!("Single message estimate: {tokens} tokens");
    }

    println!("\nDone.");
    Ok(())
}
