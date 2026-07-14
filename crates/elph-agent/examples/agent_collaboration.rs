//! Collaboration mode, plan-mode tool filtering, and planning helpers.
//!
//! Demonstrates: `CollaborationMode`, `PlanConfirmationChoice`,
//! `plan_mode_system_prompt`, `plan_mode_block_reason`, `plan_mode_blocks_tool`,
//! `filter_active_tools`, `extract_proposed_plan`, `assistant_message_text`,
//! `implement_prompt`, `is_mcp_tool`, `is_collaboration_tool`, `is_mutating_tool`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_collaboration
//! ```

use elph_agent::collaboration::{
    CollaborationMode, PlanConfirmationChoice, assistant_message_text, extract_proposed_plan, filter_active_tools,
    implement_prompt, is_collaboration_tool, is_mcp_read_only_bridge_tool, is_mcp_tool, is_mutating_tool,
    plan_mode_block_reason, plan_mode_blocks_tool, plan_mode_system_prompt,
};
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

    // ── 3. plan_mode_system_prompt ──
    println!("\n=== Plan Mode System Prompt ===");
    let plan_prompt = plan_mode_system_prompt();
    let preview: String = plan_prompt.lines().take(6).collect::<Vec<_>>().join("\n");
    println!("  first 6 lines:\n{preview}");
    println!("  (total: {} lines, {} chars)", plan_prompt.lines().count(), plan_prompt.len());

    // ── 4. Tool filtering ──
    println!("\n=== Tool Filtering ===");
    let tool_names: Vec<String> = vec![
        "read_file".into(),
        "bash".into(),
        "edit_file".into(),
        "write_file".into(),
        "mcp_server__search".into(),
        "multi_agent__delegate".into(),
        "create_goal".into(),
        "grep".into(),
    ];

    let active_names = filter_active_tools(CollaborationMode::Plan, &tool_names);
    println!("  plan mode filtered tools:");
    for name in &active_names {
        println!("    - {name}");
    }

    let all_default = filter_active_tools(CollaborationMode::Default, &tool_names);
    println!("  default mode tools count: {}", all_default.len());

    // ── 5. Tool policy helpers ──
    println!("\n=== Tool Policy Helpers ===");
    for name in &tool_names {
        println!(
            "  {name:30} mcp={:5} mutating={:5} collaboration={:5} plan_blocks={:5} mcp_ro={:5}",
            is_mcp_tool(name),
            is_mutating_tool(name),
            is_collaboration_tool(name),
            plan_mode_blocks_tool(CollaborationMode::Plan, name),
            is_mcp_read_only_bridge_tool(name),
        );
    }

    // ── 6. plan_mode_block_reason ──
    println!("\n=== Block Reasons ===");
    println!("  bash: {:?}", plan_mode_block_reason("bash"));
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
        usage: elph_ai::Usage::default(),
        stop_reason: elph_ai::StopReason::Stop,
        error_message: None,
        timestamp: 1_700_000_000_000,
    };
    let text = assistant_message_text(&msg.content);
    println!("  combined text:\n{text}");

    // ── 9. implement_prompt ──
    println!("\n=== Implement Prompt ===");
    let implement = implement_prompt("Implement the changes described in the plan above");
    println!("  implement prompt:\n{implement}");

    println!("\nDone.");
}

/// Wrapper that calls extract_proposed_plan with a simple &str.
fn extract_proposed_plann(text: &str) -> Option<String> {
    extract_proposed_plan(text)
}
