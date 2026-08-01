//! Plan mode system prompt and implementation prompts.

/// System prompt appendix for Plan mode.
pub fn plan_mode_system_prompt() -> &'static str {
    "\n\n# Plan mode\n\
     You are in **Plan mode**: read-only exploration — no file edits, shell commands, or patches.\n\
     Workflow:\n\
     1. Ground yourself in the repository and environment.\n\
     2. Ask clarifying questions when requirements are ambiguous.\n\
     3. Produce a concrete implementation plan.\n\
     Wrap the final plan in a single block:\n\
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
             The frontmatter has been auto-updated to `Status: in_progress`.\n\n\
             Read the plan file and implement it step by step. As you complete tasks,\n\
             use `edit_file` to update the plan file's frontmatter:\n\
             1. Change `Status: in_progress` → `Status: completed` (when fully done).\n\
             2. Change `Updated: <old>` → `Updated: YYYY-MM-DD HH:MM` (current datetime).\n\
             \n\
             Only edit the frontmatter lines; keep the plan body intact.\n\
             Do not create additional plan files — modify the existing one."
        )
    } else {
        format!("Implement this plan:\n\n{plan_text}")
    }
}
