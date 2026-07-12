//! Decode TOON fenced blocks from model-visible text.

use serde_json::Value;
use toon_format::{ToonError, decode_strict};

use super::fence::parse_toon_fence;

/// Error decoding a TOON fenced block.
#[derive(Debug, thiserror::Error)]
pub enum ToonDecodeError {
    #[error("no ```toon fenced block found")]
    MissingFence,
    #[error(transparent)]
    Toon(#[from] ToonError),
}

/// Extract and strictly decode a ```toon fenced block from text.
pub fn decode_toon_fence(text: &str) -> Result<Value, ToonDecodeError> {
    let body = parse_toon_fence(text).ok_or(ToonDecodeError::MissingFence)?;
    let decoded = decode_strict(body)?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use super::super::config::PromptEncodingConfig;
    use super::super::encode::encode_value;

    #[test]
    fn decodes_formatted_block() {
        let value = json!([{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }]);
        let config = PromptEncodingConfig {
            mode: super::super::config::PromptEncodingMode::Toon,
            min_bytes: 1,
            min_savings_ratio: 1.05,
            ..PromptEncodingConfig::default()
        };
        let block = encode_value(&value, &config).expect("encoded");
        let decoded = decode_toon_fence(&block).expect("decoded");
        assert_eq!(decoded, value);
    }

    #[test]
    fn missing_fence_errors() {
        let err = decode_toon_fence("not toon").unwrap_err();
        assert!(matches!(err, ToonDecodeError::MissingFence));
    }
}
