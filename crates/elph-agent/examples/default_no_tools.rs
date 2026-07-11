//! Default encoding (off) without tool calling — same prompt as `toon_no_tools`, raw JSON payload.
//!
//! Run alongside `toon_no_tools` with identical flags to compare token usage.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example default_no_tools
//! cargo run -p elph-agent --example toon_no_tools
//! ```

#[path = "support/toon_common.rs"]
mod common;

use common::{
    DEFAULT_ROWS, NO_TOOLS_SYSTEM, NO_TOOLS_TASK, RunMeta, baseline_prompt_encoding, build_agent, build_stream_fn,
    embed_json_prompt, print_json_preview, print_model_banner, require_opencode_key, resolve_model, run_agent_prompt,
    sample_catalog,
};

struct Args {
    task: String,
    rows: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    require_opencode_key()?;

    let catalog = sample_catalog(args.rows);
    print_json_preview("user prompt payload", &catalog);
    print_model_banner(Some(&baseline_prompt_encoding()));

    let model = resolve_model().await?;
    let stream_fn = build_stream_fn().await?;
    let agent = build_agent(model, stream_fn, NO_TOOLS_SYSTEM, vec![], Some(baseline_prompt_encoding()));

    let prompt = embed_json_prompt(&args.task, &catalog)?;
    println!("User prompt bytes: {}", prompt.len());
    println!();

    let meta = RunMeta {
        example: "default_no_tools",
        encoding: "off (default)",
        prompt_bytes: Some(prompt.len()),
    };
    run_agent_prompt(&agent, &prompt, &meta).await?;
    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut task = NO_TOOLS_TASK.to_string();
    let mut rows = DEFAULT_ROWS;
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
                    anyhow::bail!("--rows must be at least 2");
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help."),
        }
    }

    Ok(Args { task, rows })
}

fn print_help() {
    println!("Default encoding baseline (no tools) — pair with toon_no_tools");
    println!();
    println!("Options:");
    println!("  --task <text>   Same task as toon_no_tools (default: shared NO_TOOLS_TASK)");
    println!("  --rows <n>      Inventory rows (default: {DEFAULT_ROWS})");
    println!("  -h, --help      Show help");
}
