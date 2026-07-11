//! Default encoding (off) with MCP DeepWiki — same prompt as `toon_mcp_deepwiki`.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --features mcp --example default_mcp_deepwiki
//! cargo run -p elph-agent --features mcp --example toon_mcp_deepwiki
//! ```

#[path = "support/toon_common.rs"]
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    DEEPWIKI_MCP_URL, MCP_DEFAULT_REPO, MCP_DEFAULT_TOOL, MCP_SYSTEM, RunMeta, baseline_prompt_encoding, build_agent,
    build_mcp_prompt, build_stream_fn, print_model_banner, report_tool_result, require_opencode_key, resolve_model,
    run_agent_prompt,
};
use elph_agent::{
    AgentEvent, McpConfig, McpHttpConfig, McpLoadOptions, McpServerConfig, McpToolRegistry, expose_tool_name,
    parse_and_validate_mcp_config,
};

struct Args {
    repo: String,
    tool: String,
    question: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    require_opencode_key()?;

    print_model_banner(Some(&baseline_prompt_encoding()));

    println!("MCP:      DeepWiki ({DEEPWIKI_MCP_URL})");
    println!("Repo:     {}", args.repo);
    println!("Tool:     {}", args.tool);
    println!();

    let mut config = McpConfig::default();
    let mut http = McpHttpConfig::new(DEEPWIKI_MCP_URL);
    http.timeout_ms = Some(120_000);
    config.servers.insert("deepwiki".into(), McpServerConfig::Http(http));
    parse_and_validate_mcp_config(&serde_json::to_string(&config)?)?;

    let load_opts = McpLoadOptions {
        continue_on_error: false,
        discovery_timeout: Some(Duration::from_secs(60)),
        discover_resources_and_prompts: true,
        enable_list_changed: false,
        ..McpLoadOptions::default()
    };

    println!("Connecting to DeepWiki MCP...");
    let registry = Arc::new(McpToolRegistry::load_with_options(config, load_opts).await?);
    let report = registry.load_report();
    println!(
        "Catalog:  {} tools (servers ok={}, failed={})",
        report.tools_loaded, report.servers_ok, report.servers_failed
    );
    println!();

    let tools = registry.create_agent_tools();
    let exposed = expose_tool_name("deepwiki", &args.tool);
    if !tools.iter().any(|t| t.name() == exposed) {
        anyhow::bail!("tool not found after load: {exposed}");
    }

    let model = resolve_model().await?;
    let stream_fn = build_stream_fn().await?;
    let agent = build_agent(model, stream_fn, MCP_SYSTEM, tools, Some(baseline_prompt_encoding()));

    agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                if let AgentEvent::ToolExecutionEnd { tool_name, result, .. } = event {
                    report_tool_result(&tool_name, &result);
                }
            })
        }))
        .await;

    let prompt = build_mcp_prompt(&args.repo, &args.tool, args.question.as_deref(), &exposed);
    let meta = RunMeta {
        example: "default_mcp_deepwiki",
        encoding: "off (default)",
        prompt_bytes: Some(prompt.len()),
    };
    run_agent_prompt(&agent, &prompt, &meta).await?;

    registry.shutdown().await;
    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut repo = MCP_DEFAULT_REPO.to_string();
    let mut tool = MCP_DEFAULT_TOOL.to_string();
    let mut question = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                repo = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--repo requires owner/name"))?;
            }
            "--tool" => {
                tool = args.next().ok_or_else(|| anyhow::anyhow!("--tool requires a value"))?;
            }
            "--question" => {
                question = Some(args.next().ok_or_else(|| anyhow::anyhow!("--question requires text"))?);
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help."),
        }
    }

    Ok(Args { repo, tool, question })
}

fn print_help() {
    println!("Default encoding baseline (MCP DeepWiki) — pair with toon_mcp_deepwiki");
    println!();
    println!("Options:");
    println!("  --repo <owner/name>   GitHub repo (default: {MCP_DEFAULT_REPO})");
    println!("  --tool <name>         read_wiki_structure | read_wiki_contents | ask_question");
    println!("  --question <text>     For ask_question");
    println!("  -h, --help            Show help");
}
