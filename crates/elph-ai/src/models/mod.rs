pub mod catalog;
pub mod catalog_json;
pub mod embedded;
pub mod provider_dir;
pub mod provider_update;

mod collection;

pub use catalog::{builtin_catalog, builtin_provider_ids, invalidate_catalog_cache, merge_model_lists};
pub use catalog::{provider_catalog_dir, set_provider_catalog_dir};
pub use catalog_json::parse_provider_catalog_json;
pub use collection::*;
pub use embedded::{embedded_provider_ids, embedded_provider_json};
pub use provider_dir::{all_provider_ids, custom_provider_catalogs, custom_provider_ids, install_provider_catalog_dir};
pub use provider_update::{
    ProviderUpdatePlan, ProviderUpdatePlanEntry, ProviderUpdateReport, ProviderUpdateStatus, UpdatePolicy,
    apply_provider_update, plan_provider_update,
};

/// Model lookup: embedded seed with `CONFIG_DIR/providers` merged over it (see [`catalog`]).
pub fn get_builtin_model(provider: &str, id: &str) -> Option<crate::types::Model> {
    catalog::get_builtin_model(provider, id)
}

/// Models for a provider: embedded seed with the disk catalog merged over it.
pub fn get_builtin_models(provider: &str) -> Vec<crate::types::Model> {
    catalog::get_builtin_models(provider)
}

/// Provider ids from embedded catalogs ∪ disk-only providers (sorted).
pub fn get_builtin_providers() -> Vec<String> {
    provider_dir::all_provider_ids()
}

use crate::types::{Model, ModelCostRates, ThinkingLevel, Usage};

/// Select rate table for this usage (base rates or highest matching tier).
pub fn resolve_cost_rates(model: &Model, usage: &Usage) -> ModelCostRates {
    let input_tokens = usage.input + usage.cache_read + usage.cache_write;
    let mut rates = model.cost.rates();
    let mut matched_threshold: i64 = -1;
    if let Some(tiers) = &model.cost.tiers {
        for tier in tiers {
            let threshold = tier.input_tokens_above as i64;
            if input_tokens as i64 > threshold && threshold > matched_threshold {
                rates = ModelCostRates {
                    input: tier.input,
                    output: tier.output,
                    cache_read: tier.cache_read,
                    cache_write: tier.cache_write,
                };
                matched_threshold = threshold;
            }
        }
    }
    rates
}

pub fn calculate_cost(model: &Model, usage: &mut Usage) {
    let rates = resolve_cost_rates(model, usage);
    let long_write = usage.cache_write_1h.unwrap_or(0);
    let short_write = usage.cache_write.saturating_sub(long_write);
    let m = 1_000_000.0;
    usage.cost.input = (rates.input / m) * usage.input as f64;
    usage.cost.output = (rates.output / m) * usage.output as f64;
    usage.cost.cache_read = (rates.cache_read / m) * usage.cache_read as f64;
    // Anthropic charges 2x base input for 1h cache writes.
    usage.cost.cache_write = (rates.cache_write * short_write as f64 + rates.input * 2.0 * long_write as f64) / m;
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

pub fn thinking_level_to_str(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
        ThinkingLevel::Xhigh => "xhigh",
        ThinkingLevel::Max => "max",
    }
}

/// Map a UI/agent thinking level to the wire string for this model.
///
/// Uses [`clamp_thinking_level`] then `thinkingLevelMap` when present so providers
/// that only accept a subset of values (e.g. xAI: `low` | `high` | `max`) never
/// receive unsupported effort names like `medium` or `minimal`.
pub fn map_thinking_level_for_api(model: &Model, level: ThinkingLevel) -> Option<String> {
    let clamped = clamp_thinking_level(model, level);
    let key = thinking_level_to_str(clamped);
    if let Some(map) = &model.thinking_level_map {
        return map.get(key).cloned().flatten();
    }
    Some(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Model, ModelCost, ModelCostTier, Usage};

    fn model_with_tier() -> Model {
        Model {
            id: "gpt-5.6-sol".into(),
            name: "GPT-5.6 Sol".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            reasoning: true,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: ModelCost {
                input: 5.0,
                output: 30.0,
                cache_read: 0.5,
                cache_write: 6.25,
                tiers: Some(vec![ModelCostTier {
                    input_tokens_above: 272_000,
                    input: 10.0,
                    output: 45.0,
                    cache_read: 1.0,
                    cache_write: 12.5,
                }]),
            },
            context_window: 272_000,
            max_tokens: 128_000,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }
    }

    #[test]
    fn calculate_cost_selects_highest_matching_tier() {
        let model = model_with_tier();
        let mut usage = Usage {
            input: 300_000,
            output: 1_000,
            cache_read: 0,
            cache_write: 0,
            ..Default::default()
        };
        calculate_cost(&model, &mut usage);
        // 10$/M * 300k + 45$/M * 1k
        assert!((usage.cost.input - 3.0).abs() < 1e-9);
        assert!((usage.cost.output - 0.045).abs() < 1e-9);
    }

    #[test]
    fn thinking_level_max_stringifies() {
        assert_eq!(thinking_level_to_str(ThinkingLevel::Max), "max");
    }

    #[test]
    fn map_thinking_level_for_api_uses_catalog_map() {
        use crate::get_builtin_model;
        let Some(model) = get_builtin_model("xai", "grok-4.5") else {
            return;
        };
        // xAI grok-4.5 supports low / medium / high (from models.dev reasoning_options)
        assert_eq!(
            map_thinking_level_for_api(&model, ThinkingLevel::Medium).as_deref(),
            Some("medium"),
            "medium is a supported xAI effort per models.dev"
        );
        assert_eq!(map_thinking_level_for_api(&model, ThinkingLevel::Low).as_deref(), Some("low"));
        // xAI does not support max — clamps to high
        assert_eq!(map_thinking_level_for_api(&model, ThinkingLevel::Max).as_deref(), Some("high"));
        let wire = map_thinking_level_for_api(&model, ThinkingLevel::High).unwrap();
        assert!(
            matches!(wire.as_str(), "low" | "medium" | "high"),
            "wire effort must be xAI-legal: {wire}"
        );
    }
}
