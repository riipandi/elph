//! Live integration tests against the public DeepWiki MCP server.
//!
//! DeepWiki: free remote MCP, no authentication.
//! - Streamable HTTP: `https://mcp.deepwiki.com/mcp`
//! - Docs: https://docs.devin.ai/work-with-devin/deepwiki-mcp
//!
//! These tests hit the network. Run explicitly:
//!
//! ```bash
//! ELPH_MCP_LIVE=1 cargo test -p elph-agent --features mcp --test mcp_deepwiki -- --nocapture
//! ```
//!
//! Without `ELPH_MCP_LIVE=1` the suite is skipped (CI-friendly).

#![cfg(feature = "mcp")]

use std::time::Duration;

use elph_agent::{
    McpConfig, McpHttpConfig, McpLoadOptions, McpServerConfig, McpToolRegistry, parse_and_validate_mcp_config,
};
use serde_json::json;

const DEEPWIKI_URL: &str = "https://mcp.deepwiki.com/mcp";
/// Small, stable public repo used for smoke tests.
const TEST_REPO: &str = "modelcontextprotocol/rust-sdk";

fn live_enabled() -> bool {
    matches!(std::env::var("ELPH_MCP_LIVE").as_deref(), Ok("1") | Ok("true") | Ok("yes"))
}

fn deepwiki_config() -> McpConfig {
    let mut config = McpConfig::default();
    let mut http = McpHttpConfig::new(DEEPWIKI_URL);
    http.timeout_ms = Some(90_000);
    config.servers.insert("deepwiki".into(), McpServerConfig::Http(http));
    config
}

fn load_options() -> McpLoadOptions {
    McpLoadOptions {
        continue_on_error: false,
        discovery_timeout: Some(Duration::from_secs(45)),
        discover_resources_and_prompts: true,
        enable_list_changed: false,
        ..McpLoadOptions::default()
    }
}

#[test]
fn deepwiki_mcp_json_validates() {
    let config = deepwiki_config();
    let raw = serde_json::to_string_pretty(&config).expect("serialize");
    let parsed = parse_and_validate_mcp_config(&raw).expect("schema+semantic");
    assert_eq!(parsed.server_count(), 1);
    assert!(matches!(parsed.servers.get("deepwiki"), Some(McpServerConfig::Http(_))));
}

#[tokio::test]
async fn deepwiki_discover_and_list_tools() {
    if !live_enabled() {
        eprintln!("skip: set ELPH_MCP_LIVE=1 to run DeepWiki live tests");
        return;
    }

    let registry = McpToolRegistry::load_with_options(deepwiki_config(), load_options())
        .await
        .expect("load DeepWiki MCP");
    let report = registry.load_report();
    assert_eq!(report.servers_ok, 1, "report={report:?}");
    assert!(
        report.tools_loaded >= 3,
        "expected at least 3 DeepWiki tools, got {}",
        report.tools_loaded
    );

    let names: Vec<_> = registry
        .descriptors()
        .into_iter()
        .map(|d| d.tool_name.clone())
        .collect();
    for expected in ["read_wiki_structure", "read_wiki_contents", "ask_question"] {
        assert!(names.iter().any(|n| n == expected), "missing tool {expected}; have {names:?}");
    }

    // Exposed naming: mcp_deepwiki__read_wiki_structure
    assert!(
        registry
            .descriptors()
            .iter()
            .any(|d| d.exposed_name == "mcp_deepwiki__read_wiki_structure")
    );

    registry.shutdown().await;
}

#[tokio::test]
async fn deepwiki_call_read_wiki_structure() {
    if !live_enabled() {
        eprintln!("skip: set ELPH_MCP_LIVE=1 to run DeepWiki live tests");
        return;
    }

    let registry = McpToolRegistry::load_with_options(deepwiki_config(), load_options())
        .await
        .expect("load DeepWiki MCP");

    let result = registry
        .call_tool("deepwiki", "read_wiki_structure", json!({ "repoName": TEST_REPO }))
        .await
        .expect("call read_wiki_structure");

    let text = result
        .content
        .iter()
        .filter_map(|c| match c {
            elph_agent::ToolResultContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!text.trim().is_empty(), "expected non-empty structure for {TEST_REPO}");
    // Structure responses typically mention sections / pages / the repo name.
    let lower = text.to_ascii_lowercase();
    assert!(
        lower.contains("rust") || lower.contains("mcp") || lower.contains("sdk") || lower.contains("#"),
        "unexpected structure payload (first 500 chars): {}",
        text.chars().take(500).collect::<String>()
    );

    registry.shutdown().await;
}

#[tokio::test]
async fn deepwiki_agent_tools_bridge() {
    if !live_enabled() {
        eprintln!("skip: set ELPH_MCP_LIVE=1 to run DeepWiki live tests");
        return;
    }

    let registry = std::sync::Arc::new(
        McpToolRegistry::load_with_options(deepwiki_config(), load_options())
            .await
            .expect("load DeepWiki MCP"),
    );

    let tools = registry.create_agent_tools();
    assert!(
        tools.iter().any(|t| t.name() == "mcp_deepwiki__read_wiki_structure"),
        "agent tools missing exposed DeepWiki tool"
    );

    registry.shutdown().await;
}
