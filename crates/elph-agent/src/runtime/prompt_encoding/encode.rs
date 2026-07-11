//! TOON encode helpers with JSON fallback.

use serde_json::Value;
use toon_format::encode_default;

use super::config::{PromptEncodingConfig, PromptEncodingMode};
use super::heuristic::is_tabular_json;

const TOON_FENCE: &str = "```toon";

/// Encode a JSON value when config and heuristics allow it.
pub fn encode_value(value: &Value, config: &PromptEncodingConfig) -> Option<String> {
    if !config.is_enabled() {
        return None;
    }

    let json = serde_json::to_string(value).ok()?;
    if json.len() < config.min_bytes {
        return None;
    }

    let should_encode = match config.mode {
        PromptEncodingMode::Off => false,
        PromptEncodingMode::Toon => true,
        PromptEncodingMode::Auto => is_tabular_json(value),
    };
    if !should_encode {
        return None;
    }

    let encoded = encode_default(value).ok()?;
    Some(format_encoded_block(&encoded, config))
}

pub(crate) fn format_encoded_block(encoded: &str, config: &PromptEncodingConfig) -> String {
    let mut out = String::new();
    if let Some(preamble) = config.preamble.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(preamble);
        out.push_str("\n\n");
    }
    out.push_str(TOON_FENCE);
    out.push('\n');
    out.push_str(encoded);
    if !encoded.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```");
    out
}

pub(crate) fn already_toon_encoded(text: &str) -> bool {
    text.contains(TOON_FENCE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use toon_format::decode_default;

    fn enabled_toon() -> PromptEncodingConfig {
        PromptEncodingConfig {
            mode: PromptEncodingMode::Toon,
            min_bytes: 1,
            ..PromptEncodingConfig::default()
        }
    }

    #[test]
    fn roundtrip_through_toon() {
        let value = json!([{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }]);
        let encoded = encode_value(&value, &enabled_toon()).expect("encoded");
        assert!(encoded.contains("```toon"));
        let start = encoded.find("```toon\n").unwrap() + "```toon\n".len();
        let end = encoded.rfind("\n```").unwrap();
        let body = &encoded[start..end];
        let decoded: Value = decode_default(body).expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn off_mode_returns_none() {
        let value = json!([{ "id": 1 }, { "id": 2 }]);
        assert!(encode_value(&value, &PromptEncodingConfig::default()).is_none());
    }
}
