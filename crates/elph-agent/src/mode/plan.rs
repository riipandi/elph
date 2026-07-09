//! Plan proposal parsing and confirmation choices.

use serde::{Deserialize, Serialize};

/// User choice after a `<proposed_plan>` is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanConfirmationChoice {
    /// Switch to Default mode and implement the plan in context.
    Implement,
    /// Clear conversation context, then implement in Default mode.
    ImplementFresh,
    /// Stay in Plan mode for further refinement.
    StayInPlan,
}

/// Extract plan text from `<proposed_plan>...</proposed_plan>` in assistant output.
pub fn extract_proposed_plan(text: &str) -> Option<String> {
    const OPEN: &str = "<proposed_plan>";
    const CLOSE: &str = "</proposed_plan>";
    let lower = text.to_ascii_lowercase();
    let start = lower.find(OPEN)?;
    let after_open = start + OPEN.len();
    let end = lower[after_open..].find(CLOSE)? + after_open;
    let plan = text[after_open..end].trim();
    if plan.is_empty() { None } else { Some(plan.to_string()) }
}

/// Collect assistant text from message content blocks.
pub fn assistant_message_text(content: &[elph_ai::AssistantContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            elph_ai::AssistantContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn implement_prompt(plan_text: &str) -> String {
    format!("Implement this plan:\n\n{plan_text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_proposed_plan_block() {
        let text = "Here is the plan:\n<proposed_plan>\n## Step 1\nDo thing\n</proposed_plan>\n";
        assert_eq!(extract_proposed_plan(text).as_deref(), Some("## Step 1\nDo thing"));
    }

    #[test]
    fn missing_block_returns_none() {
        assert!(extract_proposed_plan("no plan here").is_none());
    }
}
