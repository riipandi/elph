//! Load and merge provider model catalogs from disk (`CONFIG_DIR/providers/*.json`).
//!
//! File shape matches embedded catalogs: a JSON object map of `modelId → model`.
//! Also accepts a schema-style wrapper `{ "models": { ... } }`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

use crate::types::{AnthropicMessagesCompat, Model, ModelCost, ModelCostTier};
use crate::types::{OpenAICompletionsCompat, OpenAIResponsesCompat, ThinkingLevelMap};

use super::catalog::{
    all_builtin_models, get_builtin_model as embedded_get_model, get_builtin_models as embedded_get_models,
};

static DISK_OVERRIDES: OnceLock<RwLock<HashMap<String, Vec<Model>>>> = OnceLock::new();

fn overrides() -> &'static RwLock<HashMap<String, Vec<Model>>> {
    DISK_OVERRIDES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Install process-wide disk overrides used by [`crate::get_builtin_model`] and friends.
///
/// Call after reading `CONFIG_DIR/providers/`. Pass an empty map to clear.
pub fn set_disk_catalog_overrides(map: HashMap<String, Vec<Model>>) {
    if let Ok(mut guard) = overrides().write() {
        *guard = map;
    }
}

/// Snapshot of currently installed disk overrides (provider_id → models).
pub fn disk_catalog_overrides() -> HashMap<String, Vec<Model>> {
    overrides().read().map(|g| g.clone()).unwrap_or_default()
}

/// Load all `*.json` files from a providers directory into a map keyed by file stem (kebab-case id).
pub fn load_provider_catalogs_dir(dir: &Path) -> Result<HashMap<String, Vec<Model>>, String> {
    let mut out = HashMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.eq_ignore_ascii_case("index") {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        match parse_provider_catalog_json(&raw) {
            Ok(models) => {
                out.insert(stem.to_string(), models);
            }
            Err(err) => {
                log::warn!("skip provider catalog {}: {err}", path.display());
            }
        }
    }
    Ok(out)
}

/// Parse a provider catalog JSON body into models.
pub fn parse_provider_catalog_json(json: &str) -> Result<Vec<Model>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid provider catalog JSON: {e}"))?;
    let map_value = if let Some(models) = value.get("models") {
        models.clone()
    } else {
        value
    };
    let raw: HashMap<String, RawModel> =
        serde_json::from_value(map_value).map_err(|e| format!("invalid provider model map: {e}"))?;
    Ok(raw.into_values().map(convert_model).collect())
}

/// Merge overlay models over base by model `id` (overlay wins; extras append).
pub fn merge_model_lists(base: &[Model], overlay: &[Model]) -> Vec<Model> {
    let mut by_id: HashMap<String, Model> = HashMap::new();
    for m in base {
        by_id.insert(m.id.clone(), m.clone());
    }
    for m in overlay {
        by_id.insert(m.id.clone(), m.clone());
    }
    let mut models: Vec<Model> = by_id.into_values().collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

/// Merged model list for a provider: disk overlay over embedded (or disk-only custom provider).
pub fn merged_models_for_provider(provider: &str) -> Vec<Model> {
    let embedded = embedded_get_models(provider);
    let disk = overrides()
        .read()
        .ok()
        .and_then(|g| g.get(provider).cloned())
        .unwrap_or_default();
    if disk.is_empty() {
        return embedded;
    }
    if embedded.is_empty() {
        return disk;
    }
    merge_model_lists(&embedded, &disk)
}

/// Lookup one model using disk overrides when present.
pub fn merged_get_model(provider: &str, id: &str) -> Option<Model> {
    merged_models_for_provider(provider)
        .into_iter()
        .find(|m| m.id == id)
        .or_else(|| embedded_get_model(provider, id))
}

/// Provider ids from embedded ∪ disk overrides, sorted.
pub fn merged_providers() -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = all_builtin_models().keys().map(|k| (*k).to_string()).collect();
    if let Ok(guard) = overrides().read() {
        for k in guard.keys() {
            set.insert(k.clone());
        }
    }
    set.into_iter().collect()
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
    fn merge_overlay_replaces_by_id() {
        let base = vec![Model {
            id: "a".into(),
            name: "A".into(),
            api: "openai-completions".into(),
            provider: "x".into(),
            base_url: "https://x".into(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: ModelCost {
                input: 1.0,
                output: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                tiers: None,
            },
            context_window: 1000,
            max_tokens: 100,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }];
        let mut overlay = base.clone();
        overlay[0].name = "A-override".into();
        let merged = merge_model_lists(&base, &overlay);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "A-override");
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
}
