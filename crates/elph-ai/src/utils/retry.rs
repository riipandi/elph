use std::sync::LazyLock;

use regex::Regex;

use crate::types::AssistantMessage;

fn build_provider_error_pattern(patterns: &[&str]) -> Regex {
    let joined = patterns.join("|");
    Regex::new(&format!("(?i){joined}")).expect("valid retry regex")
}

static NON_RETRYABLE: LazyLock<Regex> = LazyLock::new(|| {
    build_provider_error_pattern(&[
        "GoUsageLimitError",
        "FreeUsageLimitError",
        "Monthly usage limit reached",
        "available balance",
        "insufficient_quota",
        "out of budget",
        "quota exceeded",
        "billing",
    ])
});
static RETRYABLE: LazyLock<Regex> = LazyLock::new(|| {
    build_provider_error_pattern(&[
        "overloaded",
        "rate.?limit",
        "too many requests",
        "429",
        "500",
        "502",
        "503",
        "504",
        "524",
        "service.?unavailable",
        "server.?error",
        "internal.?error",
        "provider.?returned.?error",
        "network.?error",
        "connection.?error",
        "connection.?refused",
        "connection.?lost",
        "other side closed",
        "fetch failed",
        "upstream.?connect",
        "reset before headers",
        "socket hang up",
        "timed? out",
        "timeout",
        "terminated",
        "websocket.?closed",
        "websocket.?error",
        "ended without",
        "stream ended before message_stop",
        "http2 request did not get a response",
        "retry delay",
        "you can retry your request",
        "try your request again",
        "please retry your request",
    ])
});

/// Whether an assistant error message is likely retryable (elph-ai).
pub fn is_retryable(message: &AssistantMessage) -> bool {
    let Some(text) = &message.error_message else {
        return false;
    };
    if NON_RETRYABLE.is_match(text) {
        return false;
    }
    RETRYABLE.is_match(text)
}
