//! User-facing formatting for provider / HTTP API errors (401, 409, …).

/// Build a clear **transcript** line from a raw provider or harness error string.
///
/// Includes a short title + status code + cleaned detail (never raw JSON dumps when a
/// `message` field can be extracted).
pub fn format_user_facing_api_error(raw: &str) -> String {
    let parsed = parse_api_error(raw);
    match (parsed.status, parsed.detail.as_str()) {
        (Some(code), "") => format!("{} ({code})", parsed.title),
        (Some(code), d) => format!("{} ({code}): {d}", parsed.title),
        (None, "") => parsed.title,
        (None, d) => {
            if d.eq_ignore_ascii_case(&parsed.title) {
                parsed.title
            } else {
                format!("{}: {d}", parsed.title)
            }
        }
    }
}

/// Shorter, friendlier line for the **ephemeral banner** (no raw payloads).
///
/// Prefers provider `message` / similar fields; falls back to a human title + code.
pub fn format_ephemeral_api_error(raw: &str) -> String {
    let parsed = parse_api_error(raw);
    // Toast: lead with the human message when we have one; code as a light suffix.
    match (parsed.detail.as_str(), parsed.status) {
        ("", Some(code)) => format!("{} ({code})", parsed.title),
        ("", None) => parsed.title,
        (d, Some(code)) => {
            // Avoid "Authentication failed (401): Authentication failed"
            if d.eq_ignore_ascii_case(parsed.title.as_str()) {
                format!("{} ({code})", parsed.title)
            } else {
                format!("{d} ({code})")
            }
        }
        (d, None) => d.to_string(),
    }
}

/// True when a status line should use error transcript chrome / ephemeral error banner.
pub fn is_user_facing_api_error_line(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("authentication failed")
        || lower.starts_with("permission denied")
        || lower.starts_with("rate limited")
        || lower.starts_with("request conflict")
        || lower.starts_with("api request failed")
        || lower.starts_with("provider server error")
        || lower.starts_with("model or endpoint not found")
        || lower.starts_with("invalid request")
        || lower.starts_with("request timed out")
        || lower.starts_with("request cancelled")
        || lower.starts_with("request canceled")
        || lower.starts_with("payment required")
        || lower.starts_with("something went wrong")
        || lower.starts_with("request too large")
    {
        return true;
    }
    if lower.starts_with("error:") || lower.starts_with("api error") || lower.contains("api error") {
        return true;
    }
    extract_status_code(line).is_some_and(|code| (400..600).contains(&code))
}

#[derive(Debug, Clone)]
struct ParsedApiError {
    status: Option<u16>,
    title: String,
    /// Human-readable detail only (no JSON braces, no stack dumps).
    detail: String,
}

fn parse_api_error(raw: &str) -> ParsedApiError {
    let raw = strip_error_prefixes(raw.trim());
    if raw.is_empty() {
        return ParsedApiError {
            status: None,
            title: "Something went wrong".to_string(),
            detail: String::new(),
        };
    }
    if is_abort_message(raw) {
        return ParsedApiError {
            status: None,
            title: "Request cancelled".to_string(),
            detail: String::new(),
        };
    }

    let status = extract_status_code(raw);
    let title = status.map(status_title).unwrap_or("Something went wrong").to_string();

    // 1) Prefer structured message fields from any JSON blob in the string.
    if let Some(msg) = extract_message_from_any_json(raw) {
        return ParsedApiError {
            status,
            title,
            detail: humanize_detail(&msg),
        };
    }

    // 2) Strip codes/prefixes; reject leftover raw JSON.
    let mut detail = extract_detail_body(raw, status);
    if looks_like_raw_json(&detail) {
        detail = String::new();
    } else {
        detail = humanize_detail(&detail);
    }

    ParsedApiError { status, title, detail }
}

fn status_title(code: u16) -> &'static str {
    match code {
        400 => "Invalid request",
        401 => "Authentication failed",
        402 => "Payment required",
        403 => "Permission denied",
        404 => "Model or endpoint not found",
        408 => "Request timed out",
        409 => "Request conflict",
        413 => "Request too large",
        422 => "Invalid request",
        429 => "Rate limited — try again shortly",
        500..=599 => "Provider server error",
        _ if (400..500).contains(&code) => "API request failed",
        _ => "Something went wrong",
    }
}

fn strip_error_prefixes(raw: &str) -> &str {
    let mut s = raw;
    for prefix in [
        "Error: ",
        "error: ",
        "API error: ",
        "Api error: ",
        "Provider error: ",
        "provider error: ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim();
        }
    }
    s
}

fn is_abort_message(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("aborted") || lower.contains("cancelled") || lower.contains("canceled") || lower == "request aborted"
}

fn extract_status_code(raw: &str) -> Option<u16> {
    if let Some(open) = raw.find('(') {
        let rest = &raw[open + 1..];
        if let Some(close) = rest.find(')') {
            let inner = rest[..close].trim();
            if let Ok(code) = inner.parse::<u16>()
                && (100..600).contains(&code)
            {
                return Some(code);
            }
        }
    }
    let mut chars = raw.chars().peekable();
    let mut digits = String::new();
    while let Some(c) = chars.peek().copied() {
        if c.is_ascii_digit() {
            digits.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if digits.len() == 3
        && let Ok(code) = digits.parse::<u16>()
        && (100..600).contains(&code)
    {
        let rest: String = chars.collect();
        let rest = rest.trim_start();
        if rest.starts_with(':') || rest.starts_with("status") || rest.is_empty() || rest.starts_with(' ') {
            return Some(code);
        }
    }
    for token in raw.split(|c: char| !c.is_ascii_digit()) {
        if token.len() == 3
            && let Ok(code) = token.parse::<u16>()
            && (400..600).contains(&code)
        {
            return Some(code);
        }
    }
    None
}

fn extract_detail_body(raw: &str, status: Option<u16>) -> String {
    let mut s = raw.trim();
    if let Some(code) = status {
        let code_s = code.to_string();
        if let Some(rest) = s.strip_prefix(&code_s) {
            s = rest.trim_start_matches([':', ' ']).trim();
        }
        let marker = format!("({code})");
        if let Some(idx) = s.find(&marker) {
            s = s[idx + marker.len()..].trim_start_matches([':', ' ']).trim();
        }
        let lower = s.to_ascii_lowercase();
        // Provider placeholders with no useful body.
        if lower.contains("status code") && lower.contains("no body") {
            s = "";
        } else if lower == "status code" || lower.starts_with("status code ") {
            s = "";
        } else {
            let status_phrase = format!("{code} status code");
            if let Some(idx) = lower.find(&status_phrase) {
                let after = s[idx + status_phrase.len()..].trim_start();
                if after.to_ascii_lowercase().contains("no body") || after.is_empty() {
                    s = "";
                }
            }
        }
    }
    for prefix in [
        "OpenAI API error",
        "Anthropic API error",
        "Google API error",
        "Provider error",
        "API error",
        "Authentication failed",
        "Permission denied",
        "Rate limited",
        "Request conflict",
        "Invalid request",
        "Provider server error",
        "Something went wrong",
        "API request failed",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start_matches([':', ' ', '(']).trim();
            if let Some(close) = s.find(')') {
                let after = s[close + 1..].trim_start_matches([':', ' ']).trim();
                if !after.is_empty() {
                    s = after;
                }
            }
            break;
        }
    }
    if let Some(msg) = extract_message_from_any_json(s) {
        return msg;
    }
    if looks_like_raw_json(s) {
        return String::new();
    }
    s.to_string()
}

/// Find a JSON object anywhere in `s` and pull a human message field.
fn extract_message_from_any_json(s: &str) -> Option<String> {
    if let Some(msg) = try_extract_json_error_message(s) {
        return Some(msg);
    }
    // Embedded JSON after a prefix: `401: {"error":...}`
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    try_extract_json_error_message(&s[start..=end])
}

fn try_extract_json_error_message(s: &str) -> Option<String> {
    let s = s.trim();
    if !(s.starts_with('{') && s.ends_with('}')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(s).ok()?;
    extract_message_from_value(&value)
}

fn extract_message_from_value(value: &serde_json::Value) -> Option<String> {
    // Prefer nested message fields used by OpenAI / Anthropic / gateways.
    const POINTERS: &[&str] = &[
        "/error/message",
        "/error/error/message",
        "/error/error",
        "/message",
        "/error_description",
        "/error/msg",
        "/msg",
        "/detail",
        "/error/detail",
        "/error/details",
        "/errors/0/message",
        "/error/errors/0/message",
    ];
    for pointer in POINTERS {
        if let Some(msg) = value
            .pointer(pointer)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            return Some(msg.to_string());
        }
    }
    // { "error": "plain string" }
    if let Some(msg) = value
        .get("error")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        return Some(msg.to_string());
    }
    // { "error": { ... } } — recurse once
    if let Some(obj) = value.get("error")
        && obj.is_object()
        && let Some(msg) = extract_message_from_value(obj)
    {
        return Some(msg);
    }
    None
}

fn looks_like_raw_json(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with('{') && s.ends_with('}')) || (s.starts_with('[') && s.ends_with(']'))
}

/// Turn `invalid_api_key` / snake_case codes into slightly friendlier phrases.
fn humanize_detail(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    // Already a sentence or has spaces — keep (truncate for UI).
    if s.contains(' ') || s.contains('.') {
        return truncate_detail(s);
    }
    // snake_case / kebab-case error codes
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        let words: Vec<&str> = s.split(['_', '-']).filter(|w| !w.is_empty()).collect();
        if words.len() >= 2 {
            let mut out = words.join(" ");
            if let Some(first) = out.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            return truncate_detail(&out);
        }
    }
    truncate_detail(s)
}

fn truncate_detail(s: &str) -> String {
    const MAX: usize = 160;
    let s = s.trim();
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_401_clearly() {
        let line = format_user_facing_api_error("401: invalid_api_key");
        assert!(line.contains("Authentication failed"), "{line}");
        assert!(line.contains("401"), "{line}");
        assert!(line.to_ascii_lowercase().contains("invalid"), "{line}");
        assert!(!line.contains('{'), "{line}");
    }

    #[test]
    fn formats_409_conflict() {
        let line = format_user_facing_api_error("409: resource version conflict");
        assert!(line.contains("Request conflict"), "{line}");
        assert!(line.contains("409"), "{line}");
    }

    #[test]
    fn formats_429_rate_limit() {
        let line = format_user_facing_api_error("OpenAI API error (429): rate limit exceeded");
        assert!(line.to_ascii_lowercase().contains("rate"), "{line}");
        assert!(line.contains("429"), "{line}");
    }

    #[test]
    fn ephemeral_prefers_payload_message_not_raw_json() {
        let raw = r#"401: {"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#;
        let toast = format_ephemeral_api_error(raw);
        assert!(toast.contains("Incorrect API key provided"), "{toast}");
        assert!(toast.contains("401"), "{toast}");
        assert!(!toast.contains('{'), "{toast}");
        assert!(!toast.contains("invalid_api_key"), "{toast}");
        assert!(!toast.contains("invalid_request_error"), "{toast}");
    }

    #[test]
    fn ephemeral_without_payload_uses_friendly_title() {
        let toast = format_ephemeral_api_error("409 status code (no body)");
        assert!(toast.contains("Request conflict"), "{toast}");
        assert!(toast.contains("409"), "{toast}");
        assert!(!toast.to_ascii_lowercase().contains("no body"), "{toast}");
    }

    #[test]
    fn extracts_nested_error_description() {
        let raw = r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#;
        let toast = format_ephemeral_api_error(raw);
        assert!(toast.contains("Token has been expired or revoked"), "{toast}");
        assert!(!toast.contains("invalid_grant"), "{toast}");
    }

    #[test]
    fn abort_is_friendly() {
        assert_eq!(format_user_facing_api_error("Request aborted"), "Request cancelled");
        assert_eq!(format_ephemeral_api_error("Request aborted"), "Request cancelled");
    }

    #[test]
    fn detects_user_facing_lines() {
        assert!(is_user_facing_api_error_line("Authentication failed (401): bad key"));
        assert!(is_user_facing_api_error_line("Error: 500 internal"));
        assert!(!is_user_facing_api_error_line("History compacted."));
    }
}
