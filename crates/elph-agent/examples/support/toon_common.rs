//! Shared helpers for TOON vs default encoding comparison examples.

#![allow(dead_code)]

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub use elph_agent::encode_value;
use elph_agent::{
    Agent, AgentEvent, AgentOptions, AgentToolResult, PartialAgentState, PromptEncodingConfig, PromptEncodingMode,
    ToolResultContent,
};
use elph_ai::{Message, StopReason, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::{Value, json};

pub const PROVIDER: &str = "opencode";
pub const MODEL_ID: &str = "big-pickle";
pub const DEFAULT_ROWS: usize = 80;

pub const NO_TOOLS_SYSTEM: &str =
    "You are a concise assistant. Answer using only the structured data in the user message.";

pub const NO_TOOLS_TASK: &str = concat!(
    "The inventory below uses TOON format. ",
    "How many products are listed? Reply with the count and the name of the first item."
);

pub const TOOL_CALL_SYSTEM: &str =
    "You are a helpful assistant. When asked about inventory, call list_inventory first.";

pub const TOOL_CALL_PROMPT: &str = concat!(
    "Call the list_inventory tool with no arguments. ",
    "Then reply with the total number of items and the product name of the first row."
);

pub const MCP_SYSTEM: &str =
    "You are a helpful assistant with DeepWiki MCP tools. Call the requested MCP tool before answering.";

pub const DEEPWIKI_MCP_URL: &str = "https://mcp.deepwiki.com/mcp";
pub const MCP_DEFAULT_REPO: &str = "modelcontextprotocol/rust-sdk";
pub const MCP_DEFAULT_TOOL: &str = "read_wiki_structure";

pub struct RunMeta<'a> {
    pub example: &'a str,
    pub encoding: &'a str,
    pub prompt_bytes: Option<usize>,
}

pub fn require_opencode_key() -> anyhow::Result<()> {
    if std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_none()
    {
        anyhow::bail!(
            "Set OPENCODE_API_KEY to your OpenCode Zen API key.\n\
             Get one at https://opencode.ai"
        );
    }
    Ok(())
}

pub async fn resolve_model() -> anyhow::Result<elph_ai::Model> {
    let model = get_builtin_model(PROVIDER, MODEL_ID)
        .ok_or_else(|| anyhow::anyhow!("model not found: {PROVIDER}/{MODEL_ID}"))?;

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if auth.is_none() {
        anyhow::bail!("OpenCode Zen is not configured (missing OPENCODE_API_KEY?)");
    }

    Ok(model)
}

pub async fn build_stream_fn() -> anyhow::Result<elph_agent::StreamFn> {
    let models = builtin_models(None);
    let models: Arc<elph_ai::Models> = models.into_arc();
    Ok(Arc::new(move |m, ctx, opts| {
        let models = models.clone();
        models.stream_simple(m, ctx, opts)
    }))
}

pub fn toon_prompt_encoding(mode: PromptEncodingMode) -> PromptEncodingConfig {
    PromptEncodingConfig {
        mode,
        min_bytes: 512,
        ..PromptEncodingConfig::default()
    }
}

pub fn baseline_prompt_encoding() -> PromptEncodingConfig {
    PromptEncodingConfig::default()
}

pub fn encoding_label(mode: PromptEncodingMode) -> &'static str {
    match mode {
        PromptEncodingMode::Off => "off",
        PromptEncodingMode::Toon => "toon",
        PromptEncodingMode::Auto => "auto",
    }
}

pub fn sample_catalog(rows: usize) -> Value {
    let rows: Vec<Value> = (1..=rows)
        .map(|id| {
            json!({
                "sku": format!("SKU-{id:04}"),
                "name": format!("Widget {id}"),
                "category": if id % 3 == 0 { "hardware" } else { "software" },
                "price_usd": (id as f64) * 1.25,
                "in_stock": id % 5 != 0
            })
        })
        .collect();
    Value::Array(rows)
}

pub fn embed_toon_prompt(task: &str, catalog: &Value, config: &PromptEncodingConfig) -> anyhow::Result<String> {
    let toon_block = encode_value(catalog, config)
        .ok_or_else(|| anyhow::anyhow!("TOON encoding did not apply (try --mode toon or more --rows)"))?;
    Ok(format!("{task}\n\n{toon_block}"))
}

pub fn embed_json_prompt(task: &str, catalog: &Value) -> anyhow::Result<String> {
    let json_block = serde_json::to_string(catalog)?;
    Ok(format!("{task}\n\n{json_block}"))
}

pub fn build_mcp_prompt(repo: &str, tool: &str, question: Option<&str>, exposed_tool: &str) -> String {
    match tool {
        "ask_question" => {
            let question = question
                .map(str::to_string)
                .unwrap_or_else(|| format!("What is the main purpose of the {repo} repository?"));
            format!(
                "Call {exposed_tool} with repoName \"{repo}\" and question \"{question}\". \
                 Summarize the answer in three short bullet points."
            )
        }
        "read_wiki_contents" | "read_wiki_structure" => format!(
            "Call {exposed_tool} with repoName \"{repo}\". \
             List up to five main documentation sections and one-line descriptions."
        ),
        other => format!("Call {exposed_tool} for repo \"{repo}\" (tool hint: {other}). Summarize the result briefly."),
    }
}

pub fn print_encoding_preview(label: &str, value: &Value, config: &PromptEncodingConfig) {
    let raw = serde_json::to_string(value).expect("value json");
    let encoded = encode_value(value, config);

    println!("=== TOON preview: {label} ===");
    println!("Raw JSON: {} bytes", raw.len());
    if let Some(toon) = &encoded {
        println!(
            "TOON:     {} bytes ({:.0}% of raw)",
            toon.len(),
            (toon.len() as f64 / raw.len() as f64) * 100.0
        );
        println!("Fence:    {}", toon.contains("```toon"));
    } else {
        println!("TOON:     skipped (mode={:?}, min_bytes={})", config.mode, config.min_bytes);
    }
    println!();
}

pub fn print_json_preview(label: &str, value: &Value) {
    let raw = serde_json::to_string(value).expect("value json");
    println!("=== JSON preview: {label} ===");
    println!("Raw JSON: {} bytes", raw.len());
    println!("Encoding: off (default)");
    println!();
}

pub fn report_tool_result(tool_name: &str, result: &AgentToolResult) {
    let Some(text) = result.content.iter().find_map(|block| match block {
        ToolResultContent::Text(t) => Some(t.text.as_str()),
        _ => None,
    }) else {
        return;
    };

    let is_toon = text.contains("```toon");
    println!();
    println!("=== Tool result sent to model ({tool_name}) ===");
    println!("Bytes:    {}", text.len());
    println!("Format:   {}", if is_toon { "TOON" } else { "plain text / JSON" });
    if is_toon {
        let preview: String = text.chars().take(240).collect();
        println!("Preview:  {preview}...");
    }
    println!();
}

pub async fn run_agent_prompt(agent: &Agent, prompt: &str, meta: &RunMeta<'_>) -> anyhow::Result<()> {
    let generating = progress_spinner("Streaming from big-pickle...");
    let saw_delta = Arc::new(AtomicBool::new(false));

    agent
        .subscribe(Arc::new(move |event, _token| {
            let generating = generating.clone();
            let saw_delta = saw_delta.clone();
            Box::pin(async move {
                match event {
                    AgentEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event {
                            if !saw_delta.swap(true, Ordering::SeqCst) {
                                generating.finish_and_clear();
                            }
                            print!("{delta}");
                            let _ = std::io::stdout().flush();
                        }
                    }
                    AgentEvent::AgentEnd { .. } if !saw_delta.load(Ordering::SeqCst) => {
                        generating.finish_and_clear();
                    }
                    AgentEvent::AgentEnd { .. } => {}
                    _ => {}
                }
            })
        }))
        .await;

    print!("Assistant: ");
    let _ = std::io::stdout().flush();

    agent.prompt_text(prompt, None).await?;
    agent.wait_for_idle().await;
    println!();

    let state = agent.state().await;
    println!("Transcript messages: {}", state.messages.len());

    if let Some(Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
        print_comparison_summary(meta, assistant);
    }

    Ok(())
}

pub fn build_agent(
    model: elph_ai::Model,
    stream_fn: elph_agent::StreamFn,
    system_prompt: &str,
    tools: Vec<elph_agent::AgentTool>,
    prompt_encoding: Option<PromptEncodingConfig>,
) -> Agent {
    Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(system_prompt.into()),
            model: Some(model),
            tools: Some(tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        prompt_encoding,
        ..Default::default()
    })
}

pub fn parse_encoding_mode(value: &str) -> anyhow::Result<PromptEncodingMode> {
    match value.to_ascii_lowercase().as_str() {
        "toon" => Ok(PromptEncodingMode::Toon),
        "auto" => Ok(PromptEncodingMode::Auto),
        "off" => Ok(PromptEncodingMode::Off),
        other => anyhow::bail!("unknown mode: {other} (use toon, auto, or off)"),
    }
}

pub fn print_model_banner(encoding: Option<&PromptEncodingConfig>) {
    println!("Provider: OpenCode Zen");
    println!("Model:    big-pickle (opencode/big-pickle)");
    if let Some(config) = encoding {
        println!("Encoding: {:?} (min_bytes={})", config.mode, config.min_bytes);
    } else {
        println!("Encoding: embedded payload in user prompt");
    }
    println!();
}

fn print_comparison_summary(meta: &RunMeta<'_>, message: &elph_ai::AssistantMessage) {
    println!();
    println!("=== Comparison summary ===");
    println!("Example:       {}", meta.example);
    println!("Encoding:      {}", meta.encoding);
    if let Some(bytes) = meta.prompt_bytes {
        println!("Prompt bytes:  {bytes}");
    }
    println!("Stop reason:   {:?}", message.stop_reason);
    println!("Tokens in:     {}", message.usage.input);
    println!("Tokens out:    {}", message.usage.output);
    println!("Tokens total:  {}", message.usage.total_tokens);
    if let Some(reasoning) = message.usage.reasoning {
        println!("Reasoning:     {reasoning}");
    }
    if message.usage.cost.total > 0.0 {
        println!("Cost:          ${:.6}", message.usage.cost.total);
    }
    if let Some(error) = &message.error_message {
        println!("Error:         {error}");
        if message.stop_reason == StopReason::Error {
            std::process::exit(1);
        }
    }
    println!();
    println!("Pair with the matching example (toon_* vs default_*) using the same --rows/--repo flags.");
}

fn progress_spinner(message: &str) -> ProgressBar {
    if !progress_enabled() {
        eprintln!("{message}");
        return ProgressBar::hidden();
    }

    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg:.cyan}")
            .expect("valid spinner template"),
    );
    bar.set_message(message.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

fn progress_enabled() -> bool {
    if std::env::var("NO_COLOR").as_deref() == Ok("true") {
        return false;
    }
    stderr().is_terminal()
}
