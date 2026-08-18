//! Provider catalog JSON → [`Model`] conversion.
//!
//! Shared by the embedded seed ([`super::embedded`]) and user files under
//! `CONFIG_DIR/providers/*.json`; both use the same shape.

use std::collections::HashMap;

use serde::Deserialize;

use crate::types::{AnthropicMessagesCompat, Model, ModelCost, ModelCostTier};
use crate::types::{OpenAICompletionsCompat, OpenAIResponsesCompat, ThinkingLevelMap};

/// Parse a provider catalog JSON body into models.
///
/// Accepts:
/// - map of `modelId → model` (seed / unpacked shape)
/// - schema wrapper `{ "baseUrl"?, "headers"?, "models": { … } }` (stamps baseUrl/headers onto models that omit them)
pub fn parse_provider_catalog_json(json: &str) -> Result<Vec<Model>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid provider catalog JSON: {e}"))?;

    let wrapper_base = value.get("baseUrl").and_then(|v| v.as_str()).map(str::to_string);
    let wrapper_headers = value
        .get("headers")
        .cloned()
        .and_then(|h| serde_json::from_value::<HashMap<String, String>>(h).ok());

    let map_value = if let Some(models) = value.get("models") {
        models.clone()
    } else if let serde_json::Value::Object(mut map) = value {
        for key in ["$schema", "name", "baseUrl", "api", "headers", "auth", "models"] {
            map.remove(key);
        }
        serde_json::Value::Object(map)
    } else {
        value
    };
    let raw: HashMap<String, RawModel> =
        serde_json::from_value(map_value).map_err(|e| format!("invalid provider model map: {e}"))?;
    Ok(raw
        .into_values()
        .map(|mut m| {
            if m.base_url.is_empty()
                && let Some(ref base) = wrapper_base
            {
                m.base_url = base.clone();
            }
            if m.headers.is_none() {
                m.headers = wrapper_headers.clone();
            }
            convert_model(m)
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct RawModel {
    id: String,
    name: String,
    api: String,
    provider: String,
    #[serde(rename = "baseUrl")]
    base_url: String,
    reasoning: bool,
    #[serde(default)]
    #[serde(rename = "thinkingLevelMap")]
    thinking_level_map: Option<ThinkingLevelMap>,
    input: Vec<String>,
    cost: RawCost,
    #[serde(rename = "contextWindow")]
    context_window: u32,
    #[serde(rename = "maxTokens")]
    max_tokens: u32,
    #[serde(default)]
    headers: Option<HashMap<String, String>>,
    #[serde(default)]
    compat: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawCost {
    input: f64,
    output: f64,
    #[serde(rename = "cacheRead")]
    cache_read: f64,
    #[serde(rename = "cacheWrite")]
    cache_write: f64,
    #[serde(default)]
    tiers: Option<Vec<RawCostTier>>,
}

#[derive(Debug, Deserialize)]
struct RawCostTier {
    #[serde(rename = "inputTokensAbove")]
    input_tokens_above: u64,
    input: f64,
    output: f64,
    #[serde(rename = "cacheRead")]
    cache_read: f64,
    #[serde(rename = "cacheWrite")]
    cache_write: f64,
}

fn convert_model(raw: RawModel) -> Model {
    let (openai_completions_compat, openai_responses_compat, anthropic_compat) =
        parse_compat(&raw.api, raw.compat.as_ref());

    Model {
        id: raw.id,
        name: raw.name,
        api: raw.api,
        provider: raw.provider,
        base_url: raw.base_url,
        reasoning: raw.reasoning,
        thinking_level_map: raw.thinking_level_map,
        input: raw.input,
        cost: ModelCost {
            input: raw.cost.input,
            output: raw.cost.output,
            cache_read: raw.cost.cache_read,
            cache_write: raw.cost.cache_write,
            tiers: raw.cost.tiers.map(|tiers| {
                tiers
                    .into_iter()
                    .map(|t| ModelCostTier {
                        input_tokens_above: t.input_tokens_above,
                        input: t.input,
                        output: t.output,
                        cache_read: t.cache_read,
                        cache_write: t.cache_write,
                    })
                    .collect()
            }),
        },
        context_window: raw.context_window,
        max_tokens: raw.max_tokens,
        headers: raw.headers,
        openai_completions_compat,
        openai_responses_compat,
        anthropic_compat,
    }
}

fn parse_compat(
    api: &str,
    compat: Option<&serde_json::Value>,
) -> (
    Option<OpenAICompletionsCompat>,
    Option<OpenAIResponsesCompat>,
    Option<AnthropicMessagesCompat>,
) {
    let Some(compat) = compat else {
        return (None, None, None);
    };
    match api {
        "openai-completions" => (serde_json::from_value(compat.clone()).ok(), None, None),
        "openai-responses" | "azure-openai-responses" | "openai-codex-responses" => {
            (None, serde_json::from_value(compat.clone()).ok(), None)
        }
        "anthropic-messages" => (None, None, serde_json::from_value(compat.clone()).ok()),
        _ => (None, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_map_catalog_ignores_schema_key() {
        let json = r#"{
          "$schema": "https://elph.space/provider-schema.json",
          "m1": {
            "id": "m1",
            "name": "M1",
            "api": "openai-completions",
            "provider": "custom",
            "baseUrl": "https://example.com",
            "reasoning": false,
            "input": ["text"],
            "cost": {"input":1,"output":1,"cacheRead":0,"cacheWrite":0},
            "contextWindow": 8000,
            "maxTokens": 1024
          }
        }"#;
        let models = parse_provider_catalog_json(json).expect("parse");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "m1");
    }

    #[test]
    fn parse_map_catalog() {
        let json = r#"{
          "m1": {
            "id": "m1",
            "name": "M1",
            "api": "openai-completions",
            "provider": "custom",
            "baseUrl": "https://example.com",
            "reasoning": false,
            "input": ["text"],
            "cost": {"input":1,"output":1,"cacheRead":0,"cacheWrite":0},
            "contextWindow": 8000,
            "maxTokens": 1024
          }
        }"#;
        let models = parse_provider_catalog_json(json).expect("parse");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "m1");
        assert_eq!(models[0].provider, "custom");
    }

    #[test]
    fn parse_schema_wrapper_stamps_base_url_and_headers() {
        let json = r#"{
          "baseUrl": "https://gateway.example",
          "headers": {"X-Custom": "1"},
          "models": {
            "m1": {
              "id": "m1",
              "name": "M1",
              "api": "openai-completions",
              "provider": "custom",
              "baseUrl": "",
              "reasoning": false,
              "input": ["text"],
              "cost": {"input":1,"output":1,"cacheRead":0,"cacheWrite":0},
              "contextWindow": 8000,
              "maxTokens": 1024
            },
            "m2": {
              "id": "m2",
              "name": "M2",
              "api": "openai-completions",
              "provider": "custom",
              "baseUrl": "https://model-specific",
              "reasoning": false,
              "input": ["text"],
              "cost": {"input":1,"output":1,"cacheRead":0,"cacheWrite":0},
              "contextWindow": 8000,
              "maxTokens": 1024,
              "headers": {"X-Keep": "yes"}
            }
          }
        }"#;
        let mut models = parse_provider_catalog_json(json).expect("parse");
        models.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].base_url, "https://gateway.example");
        assert_eq!(
            models[0]
                .headers
                .as_ref()
                .and_then(|h| h.get("X-Custom"))
                .map(String::as_str),
            Some("1")
        );
        // Per-model baseUrl/headers win over wrapper.
        assert_eq!(models[1].base_url, "https://model-specific");
        assert_eq!(
            models[1]
                .headers
                .as_ref()
                .and_then(|h| h.get("X-Keep"))
                .map(String::as_str),
            Some("yes")
        );
    }
}
