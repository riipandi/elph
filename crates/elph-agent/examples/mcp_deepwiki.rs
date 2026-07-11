//! Connect to the public DeepWiki MCP server (Streamable HTTP, no auth).
//!
//! DeepWiki tools: `read_wiki_structure`, `read_wiki_contents`, `ask_question`  
//! Endpoint: <https://mcp.deepwiki.com/mcp>
//!
//! ```bash
//! cargo run -p elph-agent --features mcp --example mcp_deepwiki
//!
//! cargo run -p elph-agent --features mcp --example mcp_deepwiki -- \
//!   --repo rust-lang/rust --tool read_wiki_structure
//!
//! cargo run -p elph-agent --features mcp --example mcp_deepwiki -- \
//!   --tool ask_question \
//!   --repo modelcontextprotocol/rust-sdk \
//!   --question "How does Streamable HTTP transport work?"
//!
//! # Extra debug (config JSON, raw details)
//! cargo run -p elph-agent --features mcp --example mcp_deepwiki -- --verbose
//! ```

use std::time::Duration;

use elph_agent::{
    McpConfig, McpHttpConfig, McpLoadOptions, McpServerConfig, McpToolRegistry, ToolResultContent,
    parse_and_validate_mcp_config,
};
use serde_json::{Value, json};

const DEEPWIKI_MCP_URL: &str = "https://mcp.deepwiki.com/mcp";
const DEFAULT_REPO: &str = "modelcontextprotocol/rust-sdk";
const DEFAULT_TOOL: &str = "read_wiki_structure";
const DEFAULT_MAX_CHARS: usize = 6_000;

struct Args {
    repo: String,
    tool: String,
    question: Option<String>,
    url: String,
    dry_run: bool,
    verbose: bool,
    max_chars: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    banner("DeepWiki MCP");
    kv("endpoint", &args.url);
    kv("tool", &args.tool);
    kv("repo", &args.repo);
    if let Some(q) = &args.question {
        kv("question", q);
    }
    println!();

    let mut config = McpConfig::default();
    let mut http = McpHttpConfig::new(&args.url);
    http.timeout_ms = Some(120_000);
    config.servers.insert("deepwiki".into(), McpServerConfig::Http(http));
    parse_and_validate_mcp_config(&serde_json::to_string(&config)?)?;

    if args.verbose || args.dry_run {
        section("Config");
        println!("{}", serde_json::to_string_pretty(&config)?);
        println!();
    } else {
        println!("Config: deepwiki → {} (validated)", args.url);
        println!();
    }

    if args.dry_run {
        println!("Dry-run only — no network call.");
        return Ok(());
    }

    let load_opts = McpLoadOptions {
        continue_on_error: false,
        discovery_timeout: Some(Duration::from_secs(60)),
        discover_resources_and_prompts: true,
        enable_list_changed: false,
        ..McpLoadOptions::default()
    };

    section("Connecting");
    let registry = McpToolRegistry::load_with_options(config, load_opts).await?;
    let report = registry.load_report();

    let status = if report.servers_failed == 0 && report.servers_ok > 0 {
        "ok"
    } else {
        "partial/fail"
    };
    println!(
        "  status    {status}  (servers ok={}, failed={})",
        report.servers_ok, report.servers_failed
    );
    println!(
        "  catalog   {} tools, {} resources, {} prompts",
        report.tools_loaded, report.resources_loaded, report.prompts_loaded
    );
    for s in &report.servers {
        let mark = if s.ok { "✓" } else { "✗" };
        println!("  {mark} {:<12} {:<8}  {}", s.name, s.transport, s.message);
    }
    println!();

    section("Tools");
    for desc in registry.descriptors() {
        let summary = desc
            .description
            .strip_prefix(&format!("[MCP:{}] ", desc.server_name))
            .unwrap_or(desc.description.as_str())
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        println!("  • {:<28} {}", desc.tool_name, summary);
        if args.verbose {
            println!("      exposed as {}", desc.exposed_name);
        }
    }
    println!();

    let call_args = match args.tool.as_str() {
        "ask_question" => {
            let q = args
                .question
                .clone()
                .unwrap_or_else(|| format!("What is the main purpose of the {} repository?", args.repo));
            json!({ "repoName": args.repo, "question": q })
        }
        "read_wiki_contents" | "read_wiki_structure" => json!({ "repoName": args.repo }),
        other => {
            println!("Note: unknown tool \"{other}\"; sending {{\"repoName\": \"{}\"}}", args.repo);
            json!({ "repoName": args.repo })
        }
    };

    section(&format!("Call  deepwiki / {}", args.tool));
    if args.verbose {
        println!("  args: {call_args}");
        println!();
    }

    let result = registry.call_tool("deepwiki", &args.tool, call_args).await?;
    let is_error = result
        .details
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let body = extract_readable_body(&result.content, &result.details);
    print_body(&body, args.max_chars);

    if is_error {
        println!();
        println!("Status: error");
    } else {
        println!();
        println!("Status: ok  ({} chars)", body.chars().count());
    }

    if args.verbose && result.details != Value::Null {
        println!();
        section("Raw details");
        println!("{}", pretty_truncate(&result.details, args.max_chars));
    }

    registry.shutdown().await;
    Ok(())
}

/// Prefer plain text; fall back to structured_content without re-printing duplicates.
fn extract_readable_body(content: &[ToolResultContent], details: &Value) -> String {
    let mut texts = Vec::new();
    for block in content {
        match block {
            ToolResultContent::Text(t) if !t.text.trim().is_empty() => texts.push(t.text.clone()),
            ToolResultContent::Image(_) => texts.push("[image]".into()),
            ToolResultContent::Text(_) => {}
        }
    }
    let from_content = texts.join("\n\n");

    // If content is empty, lift structured_content.result (DeepWiki often puts the body there).
    if from_content.trim().is_empty() {
        return structured_body(details).unwrap_or_else(|| "(empty result)".into());
    }

    // If structured result is identical (or nearly) to text, keep text only.
    if let Some(structured) = structured_body(details)
        && normalize_ws(&structured) != normalize_ws(&from_content)
        && !from_content.contains(structured.trim())
    {
        return format!("{from_content}\n\n---\n{structured}");
    }

    from_content
}

fn structured_body(details: &Value) -> Option<String> {
    let sc = details.get("structured_content")?;
    if let Some(s) = sc.as_str() {
        return Some(s.to_string());
    }
    if let Some(s) = sc.get("result").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    // Pretty-print small objects; skip huge nested dumps when result field missing.
    if sc.is_object() || sc.is_array() {
        return serde_json::to_string_pretty(sc).ok();
    }
    None
}

fn print_body(body: &str, max_chars: usize) {
    let trimmed = body.trim_end();
    if trimmed.is_empty() {
        println!("  (no content)");
        return;
    }
    // Indent multi-line body slightly for separation from headers.
    let display = if trimmed.chars().count() > max_chars {
        let cut: String = trimmed.chars().take(max_chars).collect();
        // Prefer cutting at last newline for cleaner edge.
        let cut = cut.rsplit_once('\n').map(|(a, _)| a).unwrap_or(&cut);
        format!(
            "{cut}\n\n  … truncated ({} more characters; use --max-chars N or --verbose)",
            trimmed.chars().count().saturating_sub(cut.chars().count())
        )
    } else {
        trimmed.to_string()
    };
    println!("{display}");
}

fn pretty_truncate(value: &Value, max_chars: usize) -> String {
    let s = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    if s.chars().count() <= max_chars {
        return s;
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}\n… ({} more chars)", s.chars().count() - max_chars)
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn banner(title: &str) {
    let line = "─".repeat(title.len().saturating_add(4).max(40));
    println!("{line}");
    println!("  {title}");
    println!("{line}");
}

fn section(title: &str) {
    println!("── {title} ──");
}

fn kv(key: &str, value: &str) {
    println!("  {key:<10} {value}");
}

fn parse_args() -> anyhow::Result<Args> {
    let mut repo = DEFAULT_REPO.to_string();
    let mut tool = DEFAULT_TOOL.to_string();
    let mut question = None;
    let mut url = DEEPWIKI_MCP_URL.to_string();
    let mut dry_run = false;
    let mut verbose = false;
    let mut max_chars = DEFAULT_MAX_CHARS;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--repo" => {
                repo = args.next().ok_or_else(|| anyhow::anyhow!("--repo needs a value"))?;
            }
            "--tool" => {
                tool = args.next().ok_or_else(|| anyhow::anyhow!("--tool needs a value"))?;
            }
            "--question" => {
                question = Some(args.next().ok_or_else(|| anyhow::anyhow!("--question needs a value"))?);
            }
            "--url" => {
                url = args.next().ok_or_else(|| anyhow::anyhow!("--url needs a value"))?;
            }
            "--max-chars" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--max-chars needs a number"))?;
                max_chars = v
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--max-chars must be a positive integer"))?;
            }
            "--dry-run" => dry_run = true,
            "-v" | "--verbose" => verbose = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown arg: {other}\n\n{}", help_text()),
        }
    }

    Ok(Args {
        repo,
        tool,
        question,
        url,
        dry_run,
        verbose,
        max_chars,
    })
}

fn help_text() -> String {
    format!(
        "Usage: cargo run -p elph-agent --features mcp --example mcp_deepwiki -- [OPTIONS]\n\
         \n\
         Options:\n\
           --repo <owner/name>   GitHub repo (default: {DEFAULT_REPO})\n\
           --tool <name>         read_wiki_structure | read_wiki_contents | ask_question\n\
           --question <text>     for ask_question\n\
           --url <url>           MCP endpoint (default: {DEEPWIKI_MCP_URL})\n\
           --max-chars <n>       truncate body after n chars (default: {DEFAULT_MAX_CHARS})\n\
           --dry-run             validate config only, no network\n\
           -v, --verbose         show config JSON and raw details\n\
           -h, --help            show this help\n"
    )
}

fn print_help() {
    print!("{}", help_text());
}
