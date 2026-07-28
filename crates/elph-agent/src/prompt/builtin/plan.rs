//! Plan mode system prompt and implementation prompts.

/// System prompt appendix for Plan mode.
pub fn plan_mode_system_prompt() -> &'static str {
    "\n\n# Plan mode\n\
     You are in **Plan mode**. Do not edit files, run shell commands, or apply patches.\n\
     Allowed: reading files, search, listing, web fetch/search, and asking the user clarifying questions.\n\
     Workflow:\n\
     1. Ground yourself in the repository and environment.\n\
     2. Ask clarifying questions when requirements are ambiguous.\n\
     3. Produce a concrete implementation plan.\n\
     When the plan is ready, wrap it in a single block:\n\
     <proposed_plan>\n\
     ...markdown plan...\n\
     </proposed_plan>\n\
     Do not begin implementation until the user confirms the plan."
}

/// User message sent when the user confirms a proposed plan for implementation.
///
/// When `plan_file` is `Some(path)`, the agent is instructed to read the plan
/// from the saved file and update its frontmatter fields (`Status`, `Updated`)
/// as work progresses. Otherwise the plan text is embedded inline.
pub fn implement_prompt(plan_text: &str, plan_file: Option<&str>) -> String {
    if let Some(file_path) = plan_file {
        format!(
            "The plan has been approved and saved to:\n{file_path}\n\n\
             Read the plan file and implement it step by step. \
             Update the plan file's frontmatter `Status` and `Updated` fields as you make progress."
        )
    } else {
        format!("Implement this plan:\n\n{plan_text}")
    }
}
