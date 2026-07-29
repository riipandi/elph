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
        "payment.?required",
        "account.?suspended",
        "access.?denied",
        "invalid.?api.?key",
        "unauthorized",
        "403",
        "401",
    ])
});
static RETRYABLE: LazyLock<Regex> = LazyLock::new(|| {
    build_provider_error_pattern(&[
        // HTTP status codes
        "429",
        "500",
        "502",
        "503",
        "504",
        "408",
        "524",
        // Rate limiting
        "overloaded",
        "rate.?limit",
        "too many requests",
        // Server errors
        "service.?unavailable",
        "server.?error",
        "internal.?error",
        "provider.?returned.?error",
        "temporarily.?unavailable",
        "try.?again.?later",
        // Network / connection errors
        "network.?error",
        "connection.?error",
        "connection.?refused",
        "connection.?lost",
        "connection.?reset",
        "other side closed",
        "fetch failed",
        "upstream.?connect",
        "upstream.?closed",
        "reset before headers",
        "socket hang up",
        "socket.?closed",
        "socket.?connection.?was.?closed",
        // DNS errors (pi #6946)
        "getaddrinfo",
        "enotfound",
        "eai_again",
        "dns.?resolution",
        "dns.?lookup.?failed",
        // Timeouts
        "timed? out",
        "timeout",
        "deadline.?exceeded",
        // WebSocket errors
        "websocket.?closed",
        "websocket.?error",
        "ended without",
        "stream ended before message_stop",
        "previous_response_not_found",
        // HTTP/2 errors
        "http2 request did not get a response",
        "http2.?goaway",
        "http2.?protocol.?error",
        // Retry hints
        "retry delay",
        "you can retry your request",
        "try your request again",
        "please retry your request",
        // Stream errors
        "error.?decoding.?response.?body",
        "transport.?error",
        "stream.?error",
        "stream.?ended.?unexpectedly",
        // gRPC / server errors (pi #6449)
        "resource.?exhausted",
        "resourceed.?exhausted",
        "unavailable",
        // Abort / interrupt
        "cancelled",
        "aborted",
        // Generic transient
        "transient.?error",
        "temporary.?failure",
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

/// Check if an error message looks like a transient provider error that might
/// succeed after a retry. More lenient than `is_retryable` — checks for any
/// retryable pattern without requiring an `AssistantMessage`.
pub fn is_transient_error(text: &str) -> bool {
    if NON_RETRYABLE.is_match(text) {
        return false;
    }
    RETRYABLE.is_match(text)
}
