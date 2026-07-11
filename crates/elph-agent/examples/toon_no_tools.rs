//! TOON without tool calling — embed tabular JSON as TOON in the user prompt.
//!
//! Pair with `default_no_tools` using the same flags to compare token usage.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example toon_no_tools
//! cargo run -p elph-agent --example default_no_tools
//! ```

#[path = "support/toon_common.rs"]
mod common;

use common::{
    DEFAULT_ROWS, NO_TOOLS_SYSTEM, NO_TOOLS_TASK, RunMeta, build_agent, build_stream_fn, embed_toon_prompt,
    print_encoding_preview, print_model_banner, require_opencode_key, resolve_model, run_agent_prompt, sample_catalog,
    toon_prompt_encoding,
};
use elph_agent::PromptEncodingMode;

struct Args {
    task: String,
    rows: usize,
    mode: PromptEncodingMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    require_opencode_key()?;

    let encoding = toon_prompt_encoding(args.mode);
    let catalog = sample_catalog(args.rows);
    print_encoding_preview("user prompt payload", &catalog, &encoding);
    print_model_banner(Some(&encoding));

    let model = resolve_model().await?;
    let stream_fn = build_stream_fn().await?;
    let agent = build_agent(model, stream_fn, NO_TOOLS_SYSTEM, vec![], Some(encoding.clone()));

    let prompt = embed_toon_prompt(&args.task, &catalog, &encoding)?;
    println!("User prompt bytes: {}", prompt.len());
    println!();

    let meta = RunMeta {
        example: "toon_no_tools",
        encoding: common::encoding_label(encoding.mode),
        prompt_bytes: Some(prompt.len()),
    };
    run_agent_prompt(&agent, &prompt, &meta).await?;
    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut task = NO_TOOLS_TASK.to_string();
    let mut rows = DEFAULT_ROWS;
    let mut mode = PromptEncodingMode::Toon;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--task" => {
                task = args.next().ok_or_else(|| anyhow::anyhow!("--task requires a value"))?;
            }
            "--rows" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--rows requires a number"))?;
                rows = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--rows must be a positive integer"))?;
                if rows < 2 {
                    anyhow::bail!("--rows must be at least 2 for tabular TOON");
                }
            }
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--mode requires toon|auto|off"))?;
                mode = common::parse_encoding_mode(&value)?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help."),
        }
    }

    Ok(Args { task, rows, mode })
}

fn print_help() {
    println!("TOON in user prompt (no tool calling) — pair with default_no_tools");
    println!();
    println!("Options:");
    println!("  --task <text>           Same task as default_no_tools");
    println!("  --rows <n>              Inventory rows (default: {DEFAULT_ROWS})");
    println!("  --mode <toon|auto|off>  Encoding mode (default: toon)");
    println!("  -h, --help              Show help");
}
