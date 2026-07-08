//! Session persistence — save and restore agent conversation history.
//!
//! Demonstrates: `InMemorySessionRepo`, `JsonlSessionRepo`, `Session`,
//! `append_message`, `entries`, `build_context`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_session
//! ```

use std::sync::Arc;

use elph_agent::{
    InMemorySessionCreateOptions, InMemorySessionRepo, JsonlSessionRepo, JsonlSessionRepoCreateOptions,
    LocalExecutionEnv, llm_message_to_agent,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── In-memory session ──
    println!("=== In-Memory Session ===");
    let mut repo = InMemorySessionRepo::new();
    let mut session = repo.create(InMemorySessionCreateOptions { id: None }).await?;

    // Append user message
    session
        .append_message(llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("Hello session!".into()),
            timestamp: now_ms(),
        }))
        .await?;

    // Append assistant message
    session
        .append_message(llm_message_to_agent(elph_ai::Message::Assistant(
            elph_ai::AssistantMessage {
                role: "assistant".into(),
                content: vec![elph_ai::AssistantContentBlock::Text(elph_ai::TextContent::new(
                    "Session response",
                ))],
                api: "faux".into(),
                provider: "faux".into(),
                model: "faux-1".into(),
                response_model: None,
                response_id: None,
                usage: elph_ai::Usage::default(),
                stop_reason: elph_ai::StopReason::Stop,
                error_message: None,
                timestamp: now_ms(),
            },
        )))
        .await?;

    // Read back entries
    let entries = session.entries().await;
    println!("Entries: {}", entries.len());
    for e in &entries {
        println!("  - {}: {}", e.id(), e.entry_type());
    }

    // Build context from session
    let ctx = session.build_context().await?;
    println!("Context messages: {}", ctx.messages.len());
    println!("Thinking level: {}", ctx.thinking_level);

    // ── JSONL session (file-based) ──
    println!("\n=== JSONL Session ===");
    let tmp = tempfile::tempdir()?;
    let sessions_root = tmp.path().to_str().unwrap().to_string();

    let env = Arc::new(LocalExecutionEnv::new(tmp.path()));
    let jsonl_repo = JsonlSessionRepo::new(env, &sessions_root);

    let mut jsonl_session = jsonl_repo
        .create(JsonlSessionRepoCreateOptions {
            cwd: tmp.path().to_str().unwrap().to_string(),
            id: None,
            parent_session_path: None,
        })
        .await?;

    jsonl_session
        .append_message(llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("JSONL persistence".into()),
            timestamp: now_ms(),
        }))
        .await?;

    let jsonl_entries = jsonl_session.entries().await;
    println!("JSONL entries: {}", jsonl_entries.len());

    println!("\nDone.");
    Ok(())
}
