/// Result of handling Ctrl+C / SIGINT against the current prompt text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptInterrupt {
    /// Non-empty prompt was cleared; shell should stay open.
    Cleared,
    /// Empty prompt or no action required.
    Ignored,
}

/// Ctrl+C / SIGINT clears a non-empty prompt; empty prompt is ignored (no exit).
pub fn handle_prompt_interrupt_text(value: &str) -> PromptInterrupt {
    if value.is_empty() {
        PromptInterrupt::Ignored
    } else {
        PromptInterrupt::Cleared
    }
}

/// Returns `true` when the prompt should be cleared (never requests exit).
pub fn handle_prompt_interrupt(prompt_text: &str) -> bool {
    matches!(handle_prompt_interrupt_text(prompt_text), PromptInterrupt::Cleared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_prompt_clears_without_exit() {
        assert_eq!(handle_prompt_interrupt_text("draft"), PromptInterrupt::Cleared);
    }

    #[test]
    fn empty_prompt_is_ignored() {
        assert_eq!(handle_prompt_interrupt_text(""), PromptInterrupt::Ignored);
    }
}
