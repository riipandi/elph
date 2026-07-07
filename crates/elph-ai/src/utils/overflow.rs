use crate::types::{AssistantMessage, StopReason};

/// Heuristic context-overflow detection from elph-ai.
pub fn is_context_overflow(message: &AssistantMessage) -> bool {
    if message.stop_reason != StopReason::Error {
        return false;
    }
    let Some(text) = &message.error_message else {
        return false;
    };
    let lower = text.to_lowercase();
    lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("maximum context")
        || lower.contains("prompt is too long")
        || lower.contains("input is too long")
        || lower.contains("token limit")
        || lower.contains("too many tokens")
        || lower.contains("context_length_exceeded")
}
