//! Plan mode system prompt and implementation prompts.

/// Extra appendix when Plan is re-entered in the same session.
pub fn plan_mode_reentry_prompt() -> &'static str {
    "\n\n# Returning to Plan mode\n\
     You are entering Plan mode again after having previously exited it.\n\
     A previous plan may exist under `.elph/plans/`.\n\
     End the turn with `<proposed_plan>...</proposed_plan>` or `ask_user_question`."
}

/// System prompt appendix for Plan mode.
pub fn plan_mode_system_prompt() -> &'static str {
    "\n\n# Plan mode\n\
     You are in **Plan mode**: design a plan. Exploration tools run freely.\n\
     Workspace mutating tools need one-shot user approval and are for investigation only.\n\
     They do not start implementation. Do not spawn subagents.\n\
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
pub fn implement_prompt(plan_text: &str, plan_file: Option<&str>, review_notes: Option<&str>) -> String {
    let mut body = if let Some(file_path) = plan_file {
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
    };
    if let Some(notes) = review_notes.map(str::trim).filter(|s| !s.is_empty()) {
        body.push_str("\n\nThe user approved the plan with the following review comments:\n\n");
        body.push_str(notes);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implement_prompt_omits_empty_review_notes() {
        let text = implement_prompt("Do X", None, None);
        assert!(text.contains("Implement this plan"));
        assert!(!text.contains("review comments"));
        let text = implement_prompt("Do X", None, Some("   "));
        assert!(!text.contains("review comments"));
    }

    #[test]
    fn implement_prompt_appends_review_notes() {
        let text = implement_prompt("Do X", None, Some("prefer helper Y"));
        assert!(text.contains("Implement this plan"));
        assert!(text.contains("prefer helper Y"));
        assert!(text.contains("review comments"));
    }
}
