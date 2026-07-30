//! Collaboration mode, plan-mode tool filtering, and planning helpers.
//!
//! Demonstrates: `CollaborationMode`, `PlanConfirmationChoice`,
//! `plan_mode_block_reason`, `plan_mode_blocks_tool`,
//! `filter_active_tools`, `extract_proposed_plan`, `assistant_message_text`,
//! `implement_prompt`, `is_mcp_tool`, `is_collaboration_tool`, `is_mutating_tool`.
//!
//! ```sh
//! cargo run -p elph-agent --example agent_collaboration
//! ```

use elph_agent::collaboration::assistant_message_text;
use elph_agent::collaboration::extract_proposed_plan;
use elph_agent::collaboration::filter_active_tools;
use elph_agent::collaboration::implement_prompt;
use elph_agent::collaboration::is_collaboration_tool;
use elph_agent::collaboration::is_mcp_read_only_bridge_tool;
use elph_agent::collaboration::is_mcp_tool;
use elph_agent::collaboration::is_mutating_tool;
use elph_agent::collaboration::plan_mode_block_reason;
use elph_agent::collaboration::plan_mode_blocks_tool;
use elph_agent::collaboration::{CollaborationMode, PlanConfirmationChoice};
use elph_ai::{AssistantContentBlock, AssistantMessage, TextContent};

fn main() {
    // ── 1. CollaborationMode ──
    println!("=== CollaborationMode ===");

    let default_mode = CollaborationMode::default();
    println!("  default:  {default_mode:?} (as_str: {})", default_mode.as_str());

    let plan_mode = CollaborationMode::Plan;
    println!("  plan:     {plan_mode:?} (as_str: {})", plan_mode.as_str());

    let parsed: CollaborationMode = "plan".parse().unwrap();
    assert_eq!(parsed, CollaborationMode::Plan);
    println!("  parsed 'plan': {:?}", parsed);

    let parsed_default: CollaborationMode = "anything_else".parse().unwrap();
    assert_eq!(parsed_default, CollaborationMode::Default);
    println!("  parsed 'anything_else': {:?}", parsed_default);

    // ── 2. PlanConfirmationChoice ──
    println!("\n=== PlanConfirmationChoice ===");
    println!("  {:?}", PlanConfirmationChoice::Implement);
    println!("  {:?}", PlanConfirmationChoice::ImplementFresh);
    println!("  {:?}", PlanConfirmationChoice::StayInPlan);

    // ── 3. Tool filtering ──
    println!("\n=== Tool Filtering ===");
    let tool_names: Vec<String> = vec![
        "read_file".into(),
        "shell_exec".into(),
        "edit_file".into(),
        "write_file".into(),
        "mcp_server__search".into(),
        "multi_agent__delegate".into(),
        "create_goal".into(),
        "grep".into(),
    ];

    let active_names = filter_active_tools(CollaborationMode::Plan, &tool_names, None);
    println!("  plan mode filtered tools:");
    for name in &active_names {
        println!("    - {name}");
    }

    let all_default = filter_active_tools(CollaborationMode::Default, &tool_names, None);
    println!("  default mode tools count: {}", all_default.len());

    // ── 5. Tool policy helpers ──
    println!("\n=== Tool Policy Helpers ===");
    for name in &tool_names {
        println!(
            "  {name:30} mcp={:5} mutating={:5} collaboration={:5} plan_blocks={:5} mcp_ro={:5}",
            is_mcp_tool(name),
            is_mutating_tool(name, None),
            is_collaboration_tool(name, None),
            plan_mode_blocks_tool(CollaborationMode::Plan, name, None),
            is_mcp_read_only_bridge_tool(name),
        );
    }

    // ── 6. plan_mode_block_reason ──
    println!("\n=== Block Reasons ===");
    println!("  shell_exec: {:?}", plan_mode_block_reason("shell_exec"));
    println!("  edit_file: {:?}", plan_mode_block_reason("edit_file"));
    println!("  read_file: {:?}", plan_mode_block_reason("read_file"));
    println!("  write_file: {:?}", plan_mode_block_reason("write_file"));

    // ── 7. extract_proposed_plan ──
    println!("\n=== Extract Proposed Plan ===");
    let plan_text = "Let me outline the approach:

<proposed_plan>
1. Read src/main.rs to understand current structure
2. Add error handling with anyhow
3. Write unit tests
</proposed_plan>

Let me start with step 1.";
    let extracted = extract_proposed_plan(plan_text);
    match extracted {
        Some(plan) => println!("  extracted plan:\n{plan}"),
        None => println!("  no plan found"),
    }

    // Empty plan
    let empty = extract_proposed_plann("Let me think about this... <proposed_plan></proposed_plan>");
    println!("  empty plan: {empty:?}");

    // No tags
    let none = extract_proposed_plann("No tags here");
    println!("  no tags: {none:?}");

    // ── 8. assistant_message_text ──
    println!("\n=== Assistant Message Text ===");
    let msg = AssistantMessage {
        role: "assistant".into(),
        content: vec![
            AssistantContentBlock::Text(TextContent::new("Here's my analysis:\n\n")),
            AssistantContentBlock::Text(TextContent::new(
                "1. The issue is in the retry logic\n2. Missing timeout config",
            )),
        ],
        api: "faux".into(),
        provider: "faux".into(),
        model: "faux-1".into(),
        response_model: None,
        response_id: None,
        diagnostics: None,
        pending_stop_reason: None,
        usage: elph_ai::Usage::default(),
        stop_reason: elph_ai::StopReason::Stop,
        error_message: None,
        timestamp: 1_700_000_000_000,
    };
    let text = assistant_message_text(&msg.content);
    println!("  combined text:\n{text}");

    // ── 9. implement_prompt ──
    println!("\n=== Implement Prompt ===");
    let implement = implement_prompt("Implement the changes described in the plan above", None);
    println!("  implement prompt:\n{implement}");

    println!("\nDone.");
}

/// Wrapper that calls extract_proposed_plan with a simple &str.
fn extract_proposed_plann(text: &str) -> Option<String> {
    extract_proposed_plan(text)
}
