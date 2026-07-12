//! TOON encode helpers with JSON fallback.

use serde_json::Value;
use toon_format::{EncodeOptions, encode};

use super::config::PromptEncodingConfig;
use super::fence::format_toon_block;
use super::heuristic::{meets_savings_gate, should_encode};

/// Encode a JSON value when config and heuristics allow it.
pub fn encode_value(value: &Value, config: &PromptEncodingConfig) -> Option<String> {
    if !config.is_enabled() {
        return None;
    }

    let json = serde_json::to_string(value).ok()?;
    if json.len() < config.min_bytes {
        return None;
    }

    if !should_encode(value, config.mode) {
        return None;
    }

    let delimiter = config.delimiter_for_value(value);
    let options = EncodeOptions::new().with_delimiter(delimiter.as_toon_delimiter());
    let encoded = encode(value, &options).ok()?;
    if !meets_savings_gate(json.len(), encoded.len(), config.min_savings_ratio) {
        return None;
    }

    Some(format_toon_block(&encoded, config.preamble.as_deref(), delimiter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use toon_format::decode_default;

    use super::super::config::{PromptEncodingDelimiter, PromptEncodingMode};
    use super::super::fence::parse_toon_fence;

    fn enabled_toon() -> PromptEncodingConfig {
        PromptEncodingConfig {
            mode: PromptEncodingMode::Toon,
            min_bytes: 1,
            min_savings_ratio: 1.05,
            ..PromptEncodingConfig::default()
        }
    }

    #[test]
    fn roundtrip_through_toon() {
        let value = json!([{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }]);
        let encoded = encode_value(&value, &enabled_toon()).expect("encoded");
        let body = parse_toon_fence(&encoded).expect("fence");
        let decoded: Value = decode_default(body).expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn auto_skips_non_tabular() {
        let value = json!({ "meta": { "count": 2 }, "note": "not tabular" });
        let config = PromptEncodingConfig {
            mode: PromptEncodingMode::Auto,
            min_bytes: 1,
            min_savings_ratio: 1.05,
            ..PromptEncodingConfig::default()
        };
        assert!(encode_value(&value, &config).is_none());
    }

    #[test]
    fn tabular_uses_tab_delimiter_by_default() {
        let value = json!([{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }]);
        let encoded = encode_value(&value, &enabled_toon()).expect("encoded");
        assert!(encoded.contains("tab-separated"));
        let body = parse_toon_fence(&encoded).expect("fence");
        assert!(body.contains('\t') || body.contains("{id,name}"));
    }

    #[test]
    fn off_mode_returns_none() {
        let value = json!([{ "id": 1 }, { "id": 2 }]);
        assert!(encode_value(&value, &PromptEncodingConfig::default()).is_none());
    }

    #[test]
    fn savings_gate_blocks_expansion() {
        let value = json!({ "x": "short" });
        let config = PromptEncodingConfig {
            mode: PromptEncodingMode::Toon,
            min_bytes: 1,
            min_savings_ratio: 0.5,
            delimiter: PromptEncodingDelimiter::Comma,
            tabular_delimiter: Some(PromptEncodingDelimiter::Comma),
            ..PromptEncodingConfig::default()
        };
        assert!(encode_value(&value, &config).is_none());
    }
}
