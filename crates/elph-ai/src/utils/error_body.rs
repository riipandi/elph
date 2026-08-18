use serde_json::Value;

pub const MAX_PROVIDER_ERROR_BODY_CHARS: usize = 4000;
/// Max chars for a 4xx snippet written to logs (not the caller-facing body).
pub const MAX_HTTP_ERROR_LOG_CHARS: usize = 160;

#[derive(Debug, Clone)]
pub struct NormalizedProviderError {
    pub status: Option<u16>,
    pub body: Option<String>,
    pub message: String,
    pub message_carries_body: bool,
}

/// SDK-shaped HTTP error used by provider catch blocks and tests.
#[derive(Debug)]
pub struct ProviderSdkError {
    pub message: String,
    pub status_code: Option<u16>,
    pub status: Option<u16>,
    pub body: Option<String>,
    pub parsed_error: Option<Value>,
    pub bedrock_metadata_http_status: Option<u16>,
    pub bedrock_response_status_code: Option<u16>,
    pub bedrock_response_body: Option<String>,
}

impl std::fmt::Display for ProviderSdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderSdkError {}

/// Non-`Error` thrown value serialized into the normalized message.
#[derive(Debug)]
pub struct ThrownValue(pub Value);

impl std::fmt::Display for ThrownValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", safe_json_stringify(&self.0))
    }
}

impl std::error::Error for ThrownValue {}

pub fn normalize_provider_error(error: &anyhow::Error) -> NormalizedProviderError {
    if let Some(thrown) = error.downcast_ref::<ThrownValue>() {
        return NormalizedProviderError {
            status: None,
            body: None,
            message: safe_json_stringify(&thrown.0),
            message_carries_body: false,
        };
    }

    if let Some(sdk) = error.downcast_ref::<ProviderSdkError>() {
        let status = extract_status(sdk);
        let body = extract_body(sdk);
        let message_carries_body = body.as_ref().is_none_or(|b| sdk.message.contains(b));
        return NormalizedProviderError {
            status,
            body,
            message: sdk.message.clone(),
            message_carries_body,
        };
    }

    let message = error.to_string();
    let status = error
        .downcast_ref::<reqwest::Error>()
        .and_then(|e| e.status())
        .map(|s| s.as_u16());

    NormalizedProviderError {
        status,
        body: None,
        message,
        message_carries_body: false,
    }
}

fn extract_status(error: &ProviderSdkError) -> Option<u16> {
    error
        .status_code
        .or(error.status)
        .or(error.bedrock_metadata_http_status)
        .or(error.bedrock_response_status_code)
}

fn extract_body(error: &ProviderSdkError) -> Option<String> {
    let body_text = pick_body_text(error)?;
    let trimmed = body_text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_error_text(trimmed, MAX_PROVIDER_ERROR_BODY_CHARS))
}

fn pick_body_text(error: &ProviderSdkError) -> Option<String> {
    if let Some(body) = &error.body {
        return Some(body.clone());
    }
    if is_non_empty_object(error.parsed_error.as_ref()) {
        return Some(safe_json_stringify(error.parsed_error.as_ref().unwrap()));
    }
    if let Some(body) = &error.bedrock_response_body {
        return Some(body.clone());
    }
    None
}

fn is_non_empty_object(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Object(map)) => !map.is_empty(),
        _ => false,
    }
}

pub fn format_provider_error(norm: &NormalizedProviderError, prefix: Option<&str>) -> String {
    if norm.message_carries_body || norm.status.is_none() || norm.body.is_none() {
        if let (Some(prefix), Some(status)) = (prefix, norm.status) {
            return format!("{prefix} ({status}): {}", norm.message);
        }
        return norm.message.clone();
    }
    if let Some(prefix) = prefix {
        format!(
            "{prefix} ({}): {}",
            norm.status.unwrap_or(0),
            norm.body.as_deref().unwrap_or("")
        )
    } else {
        format!("{}: {}", norm.status.unwrap_or(0), norm.body.as_deref().unwrap_or(""))
    }
}

/// Compact 4xx log snippet: JSON `error` / `message` / `code` / `type` only.
/// Never logs the raw body (prompts and request echoes stay out of the file).
pub fn http_error_log_snippet(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return json_error_snippet(&value)
            .map(|s| truncate_error_text(&s, MAX_HTTP_ERROR_LOG_CHARS))
            .unwrap_or_default();
    }
    String::from("(non-json error body)")
}

fn json_error_snippet(value: &Value) -> Option<String> {
    let obj = match value {
        Value::Object(map) => map,
        _ => return None,
    };
    let inner = match obj.get("error") {
        Some(Value::String(s)) if !s.is_empty() => {
            return Some(s.clone());
        }
        Some(Value::Object(err)) => err,
        _ => obj,
    };
    let message = inner
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| obj.get("message").and_then(Value::as_str))
        .unwrap_or("")
        .trim();
    let code = inner.get("code").or_else(|| obj.get("code")).map(value_as_short);
    let kind = inner
        .get("type")
        .or_else(|| inner.get("status"))
        .or_else(|| obj.get("type"))
        .map(value_as_short);
    let mut parts = Vec::new();
    if let Some(code) = code.filter(|s| !s.is_empty()) {
        parts.push(code);
    }
    if let Some(kind) = kind.filter(|s| !s.is_empty()) {
        parts.push(kind);
    }
    if !message.is_empty() {
        parts.push(message.to_string());
    }
    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

fn value_as_short(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

pub fn truncate_error_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    format!("{}... [truncated {} chars]", &text[..max_chars], text.len() - max_chars)
}

pub fn safe_json_stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub async fn error_body_from_response(response: reqwest::Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if text.trim().is_empty() {
        format!("{status}")
    } else {
        truncate_error_text(&text, MAX_PROVIDER_ERROR_BODY_CHARS)
    }
}
