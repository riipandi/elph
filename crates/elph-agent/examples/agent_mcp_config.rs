//! MCP server configuration — build, inspect, serialize, and apply tool policies.
//!
//! Demonstrates: `McpConfig`, `McpServerConfig`, `McpStdioConfig`, `McpHttpConfig`,
//! `McpPolicyConfig`, `McpPolicyAction`, `enabled_servers`, `effective_policy`,
//! `is_disabled`, `operation_timeout`, `kind_label`, `remote_url`, merge configs,
//! `mcp_tool_requires_approval`, `pattern_matches`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_mcp_config --features mcp
//! ```

use std::collections::BTreeMap;

use elph_agent::tools::mcp::{
    McpConfig, McpHttpConfig, McpPolicyAction, McpPolicyConfig, McpServerConfig, McpStdioConfig,
    mcp_tool_requires_approval, pattern_matches,
};

fn main() {
    // ── 1. Build MCP config using constructors ──
    println!("=== MCP Config (constructors) ===");

    let mut servers = BTreeMap::new();

    // Stdio (local MCP process)
    servers.insert(
        "deepwiki".into(),
        McpServerConfig::stdio("npx", vec!["-y".into(), "@opencode/deepwiki-mcp".into()]),
    );

    // Streamable HTTP (remote)
    servers.insert("fetch".into(), McpServerConfig::http("https://mcp.example.com/fetch"));

    // Legacy SSE
    servers.insert("legacy-sse".into(), McpServerConfig::sse("https://old-mcp.example.com/sse"));

    println!("  added 3 servers via constructors");

    // ── 2. Build with McpStdioConfig directly ──
    println!("\n=== Stdio with Custom Config ===");

    let _custom = McpServerConfig::Stdio(McpStdioConfig {
        command: "uvx".into(),
        args: vec!["mcp-server-fetch".into()],
        env: BTreeMap::from([("DEBUG".into(), "1".into())]),
        cwd: Some("/tmp".into()),
        timeout_ms: Some(120_000),
        disabled: false,
        policy: None,
    });
    println!("  custom stdio: command=uvx, env=1 var, timeout=120s");

    // ── 3. Disabled server ──
    println!("\n=== Disabled Server ===");
    let disabled = McpServerConfig::Stdio(McpStdioConfig {
        command: "old-tool".into(),
        args: vec![],
        env: BTreeMap::new(),
        cwd: None,
        timeout_ms: None,
        disabled: true,
        policy: None,
    });
    println!("  is_disabled: {}", disabled.is_disabled());
    servers.insert("deprecated".into(), disabled);

    // ── 4. Global tool policy ──
    println!("\n=== Tool Policy ===");
    let policy = McpPolicyConfig {
        default: McpPolicyAction::Allow,
        require_approval: vec!["filesystem__*".into(), "edit_file".into()],
        deny: vec!["dangerous__*".into()],
        allow: vec!["read_file".into(), "grep".into()],
    };

    println!("  default:        {:?}", policy.default);
    println!("  allow:          {:?}", policy.allow);
    println!("  deny:           {:?}", policy.deny);
    println!("  require_approval: {:?}", policy.require_approval);

    // ── 5. Assemble config ──
    let config = McpConfig { servers, policy };

    println!("\n  total servers:   {}", config.server_count());
    println!("  enabled:         {}", config.enabled_count());
    println!("  is_empty:        {}", config.is_empty());

    // ── 6. Enumerate enabled servers ──
    println!("\n=== Enabled Servers ===");
    for (name, server) in config.enabled_servers() {
        println!(
            "  {name}: {} — disabled={}, remote_url={:?}",
            server.kind_label(),
            server.is_disabled(),
            server.remote_url(),
        );
    }

    // ── 7. Effective policy ──
    println!("\n=== Effective Policy ===");
    for (name, server) in config.enabled_servers() {
        let eff = config.effective_policy(server);
        println!(
            "  {name}: default={:?}, {} allowed patterns, {} denied, {} require_approval",
            eff.default,
            eff.allow.len(),
            eff.deny.len(),
            eff.require_approval.len(),
        );
    }

    // ── 8. Operation timeout ──
    println!("\n=== Operation Timeout ===");
    let stdio_server = McpServerConfig::Stdio(McpStdioConfig {
        command: "test".into(),
        args: vec![],
        env: BTreeMap::new(),
        cwd: None,
        timeout_ms: Some(60_000),
        disabled: false,
        policy: None,
    });
    let http_server = McpServerConfig::http("https://example.com/mcp");
    println!("  stdio timeout: {:?}", stdio_server.operation_timeout());
    println!("  http timeout:  {:?}", http_server.operation_timeout());

    // ── 9. OAuth ──
    println!("\n=== OAuth ===");
    let oauth_server = McpServerConfig::Http(McpHttpConfig::new("https://mcp.example.com/oauth"));
    println!("  wants_oauth: {}", oauth_server.wants_oauth());
    println!("  oauth_scopes: {:?}", oauth_server.oauth_scopes());

    // ── 10. pattern_matches ──
    println!("\n=== Pattern Matching ===");
    println!(
        "  'filesystem__*'      'filesystem__write':  {}",
        pattern_matches("filesystem__*", "filesystem__write")
    );
    println!(
        "  'filesystem__*'      'fetch__get':         {}",
        pattern_matches("filesystem__*", "fetch__get")
    );
    println!(
        "  'read'               'read':               {}",
        pattern_matches("read", "read")
    );
    println!(
        "  'read'               'write':              {}",
        pattern_matches("read", "write")
    );

    // ── 11. mcp_tool_requires_approval ──
    println!("\n=== Requires Approval ===");
    println!(
        "  filesystem__write: {}",
        mcp_tool_requires_approval(&config.policy, "filesystem__write")
    );
    println!("  read:              {}", mcp_tool_requires_approval(&config.policy, "read"));
    println!(
        "  dangerous__delete: {}",
        mcp_tool_requires_approval(&config.policy, "dangerous__delete")
    );

    // ── 12. Serialize to JSON ──
    println!("\n=== JSON Serialization ===");
    let json = serde_json::to_string_pretty(&config).unwrap();
    for line in json.lines().take(25) {
        println!("  {line}");
    }
    if json.lines().count() > 25 {
        println!("  ... ({} more lines)", json.lines().count() - 25);
    }

    // ── 13. Round-trip ──
    println!("\n=== Round-Trip ===");
    let parsed: McpConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.server_count(), parsed.server_count());
    assert_eq!(config.enabled_count(), parsed.enabled_count());
    println!("  OK: {} servers, {} enabled", parsed.server_count(), parsed.enabled_count());

    // ── 14. Merge configs ──
    println!("\n=== Merge Config ===");
    let base = McpConfig::default();
    let merged = base.merge_with(&config);
    println!("  merged server count: {}", merged.server_count());

    println!("\nDone.");
}
