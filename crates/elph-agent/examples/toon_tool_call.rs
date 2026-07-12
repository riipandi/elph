//! TOON with tool calling — custom tool returns JSON, agent encodes results as TOON.
//!
//! Pair with `default_tool_call` using the same flags to compare token usage.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example toon_tool_call
//! cargo run -p elph-agent --example default_tool_call
//! ```

#[path = "support/toon_common.rs"]
mod common;

use std::sync::Arc;

use common::{
    DEFAULT_ROWS, RunMeta, TOOL_CALL_PROMPT, TOOL_CALL_SYSTEM, build_agent, build_stream_fn, parse_delimiter,
    print_encoding_preview, print_model_banner, report_tool_result, require_opencode_key, resolve_model,
    run_agent_prompt, sample_catalog, toon_prompt_encoding_with_delimiter,
};
use elph_agent::{AgentEvent, AgentToolResult, PromptEncodingDelimiter, PromptEncodingMode, simple_tool};
use elph_ai::Tool;
use serde_json::json;

struct Args {
    prompt: String,
    rows: usize,
    mode: PromptEncodingMode,
    delimiter: Option<PromptEncodingDelimiter>,
    tabular_delimiter: Option<PromptEncodingDelimiter>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    require_opencode_key()?;

    let prompt_encoding = toon_prompt_encoding_with_delimiter(args.mode, args.delimiter, args.tabular_delimiter);
    let catalog = sample_catalog(args.rows);
    print_encoding_preview("list_inventory tool output", &catalog, &prompt_encoding);
    print_model_banner(Some(&prompt_encoding));

    let model = resolve_model().await?;
    let stream_fn = build_stream_fn().await?;

    let list_inventory = simple_tool(
        Tool {
            name: "list_inventory".into(),
            description: "Return the full product inventory as structured JSON.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        "List inventory",
        move |_id, _args| {
            let catalog = catalog.clone();
            Box::pin(async move { Ok(AgentToolResult::text(catalog.to_string())) })
        },
    );

    let agent = build_agent(
        model,
        stream_fn,
        TOOL_CALL_SYSTEM,
        vec![list_inventory],
        Some(prompt_encoding.clone()),
    );

    agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                if let AgentEvent::ToolExecutionEnd { tool_name, result, .. } = event {
                    report_tool_result(&tool_name, &result);
                }
            })
        }))
        .await;

    let meta = RunMeta {
        example: "toon_tool_call",
        encoding: common::encoding_label(prompt_encoding.mode),
        prompt_bytes: Some(args.prompt.len()),
    };
    run_agent_prompt(&agent, &args.prompt, &meta).await?;
    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut prompt = TOOL_CALL_PROMPT.to_string();
    let mut rows = DEFAULT_ROWS;
    let mut mode = PromptEncodingMode::Toon;
    let mut delimiter = None;
    let mut tabular_delimiter = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--prompt" => {
                prompt = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--prompt requires a value"))?;
            }
            "--rows" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--rows requires a number"))?;
                rows = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--rows must be a positive integer"))?;
            }
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--mode requires toon|auto|off"))?;
                mode = common::parse_encoding_mode(&value)?;
            }
            "--delimiter" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--delimiter requires comma|tab|pipe"))?;
                delimiter = Some(parse_delimiter(&value)?);
            }
            "--tabular-delimiter" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--tabular-delimiter requires comma|tab|pipe"))?;
                tabular_delimiter = Some(parse_delimiter(&value)?);
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help."),
        }
    }

    Ok(Args {
        prompt,
        rows,
        mode,
        delimiter,
        tabular_delimiter,
    })
}

fn print_help() {
    println!("TOON on tool results — pair with default_tool_call");
    println!();
    println!("Options:");
    println!("  --prompt <text>         Same prompt as default_tool_call");
    println!("  --rows <n>              Inventory rows (default: {DEFAULT_ROWS})");
    println!("  --mode <toon|auto|off>          Encoding mode (default: toon)");
    println!("  --delimiter <comma|tab|pipe>    General delimiter (default: comma)");
    println!("  --tabular-delimiter <...>       Tabular delimiter (default: tab)");
    println!("  -h, --help                      Show help");
}
