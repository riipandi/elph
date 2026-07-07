use std::collections::HashMap;

use serde_json::{Value, json};

use crate::types::{CacheRetention, Model, ProviderEnv, ThinkingBudgets, ThinkingLevel};
use crate::utils::provider_env::get_provider_env_value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BedrockRuntimeConfig {
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub profile: Option<String>,
}

pub struct BedrockThinkingOptions<'a> {
    pub region: Option<&'a str>,
    pub profile: Option<&'a str>,
    pub ambient_profile: Option<&'a str>,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<&'a ThinkingBudgets>,
    pub thinking_display: Option<&'a str>,
    pub interleaved_thinking: bool,
    pub env: Option<&'a ProviderEnv>,
}

const RESERVED_BEDROCK_HEADERS: &[&str] = &["authorization", "host", "x-amz-date"];

pub fn is_reserved_bedrock_header(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.starts_with("x-amz-") || RESERVED_BEDROCK_HEADERS.contains(&lower.as_str())
}

pub fn merge_bedrock_custom_headers(
    existing: &HashMap<String, String>,
    custom: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = existing.clone();
    for (key, value) in custom {
        if !is_reserved_bedrock_header(key) {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

pub fn get_standard_bedrock_endpoint_region(base_url: &str) -> Option<String> {
    let host = url::Url::parse(base_url.trim_end_matches('/'))
        .ok()?
        .host_str()?
        .to_ascii_lowercase();
    let re = regex::Regex::new(r"^bedrock-runtime(?:-fips)?\.([a-z0-9-]+)\.amazonaws\.com(?:\.cn)?$").ok()?;
    re.captures(&host)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

pub fn get_configured_bedrock_region(region: Option<&str>, env: Option<&ProviderEnv>) -> Option<String> {
    region
        .map(str::to_string)
        .or_else(|| get_provider_env_value("AWS_REGION", env))
        .or_else(|| get_provider_env_value("AWS_DEFAULT_REGION", env))
}

pub fn should_use_explicit_bedrock_endpoint(
    base_url: &str,
    configured_region: Option<&str>,
    has_ambient_profile: bool,
) -> bool {
    match get_standard_bedrock_endpoint_region(base_url) {
        Some(_) => configured_region.is_none() && !has_ambient_profile,
        None => true,
    }
}

pub fn extract_bedrock_arn_region(model_id: &str) -> Option<String> {
    let re = regex::Regex::new(r"^arn:aws(?:-[a-z0-9-]+)?:bedrock:([a-z0-9-]+):").ok()?;
    re.captures(model_id)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

pub fn resolve_bedrock_runtime_config(model: &Model, options: &BedrockThinkingOptions<'_>) -> BedrockRuntimeConfig {
    let configured_region = get_configured_bedrock_region(options.region, options.env);
    let profile = options
        .profile
        .map(str::to_string)
        .or_else(|| get_provider_env_value("AWS_PROFILE", options.env))
        .or_else(|| options.ambient_profile.map(str::to_string));
    let has_ambient_profile = options.ambient_profile.is_some();
    let use_explicit_endpoint =
        should_use_explicit_bedrock_endpoint(&model.base_url, configured_region.as_deref(), has_ambient_profile);

    let region = extract_bedrock_arn_region(&model.id).or_else(|| {
        if let Some(region) = configured_region.clone() {
            return Some(region);
        }
        if use_explicit_endpoint {
            return get_standard_bedrock_endpoint_region(&model.base_url);
        }
        if !has_ambient_profile {
            return Some("us-east-1".to_string());
        }
        None
    });

    let endpoint = if use_explicit_endpoint {
        Some(model.base_url.trim_end_matches('/').to_string())
    } else {
        None
    };

    BedrockRuntimeConfig {
        region,
        endpoint,
        profile,
    }
}

fn model_match_candidates(model_id: &str, model_name: &str) -> Vec<String> {
    [model_id, model_name]
        .iter()
        .flat_map(|value| {
            let lower = value.to_ascii_lowercase();
            [lower.clone(), lower.replace([' ', '_', '.', ':'], "-")]
        })
        .collect()
}

pub fn supports_adaptive_thinking(model_id: &str, model_name: &str) -> bool {
    model_match_candidates(model_id, model_name).iter().any(|candidate| {
        candidate.contains("opus-4-6")
            || candidate.contains("opus-4-7")
            || candidate.contains("opus-4-8")
            || candidate.contains("sonnet-4-6")
            || candidate.contains("sonnet-5")
            || candidate.contains("fable-5")
    })
}

pub fn supports_native_xhigh_effort(model_id: &str, model_name: &str) -> bool {
    model_match_candidates(model_id, model_name).iter().any(|candidate| {
        candidate.contains("opus-4-7") || candidate.contains("opus-4-8") || candidate.contains("fable-5")
    })
}

pub fn is_anthropic_claude_model(model_id: &str, model_name: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    let name = model_name.to_ascii_lowercase();
    id.contains("anthropic.claude")
        || id.contains("anthropic/claude")
        || name.contains("anthropic.claude")
        || name.contains("anthropic/claude")
        || name.contains("claude")
}

pub fn is_govcloud_bedrock_target(model_id: &str, region: Option<&str>) -> bool {
    if region.is_some_and(|r| r.to_ascii_lowercase().starts_with("us-gov-")) {
        return true;
    }
    let id = model_id.to_ascii_lowercase();
    id.starts_with("us-gov.") || id.starts_with("arn:aws-us-gov:")
}

pub fn resolve_cache_retention(cache_retention: Option<CacheRetention>, env: Option<&ProviderEnv>) -> CacheRetention {
    if let Some(retention) = cache_retention {
        return retention;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", env).as_deref() == Some("long") {
        CacheRetention::Long
    } else {
        CacheRetention::Short
    }
}

pub fn supports_prompt_caching(model_id: &str, model_name: &str, env: Option<&ProviderEnv>) -> bool {
    let candidates = model_match_candidates(model_id, model_name);
    let has_claude_ref = candidates.iter().any(|s| s.contains("claude"));
    if !has_claude_ref {
        return get_provider_env_value("AWS_BEDROCK_FORCE_CACHE", env).as_deref() == Some("1");
    }
    if candidates
        .iter()
        .any(|s| s.contains("fable-5") || s.contains("sonnet-5"))
    {
        return true;
    }
    if candidates.iter().any(|s| s.contains("-4-")) {
        return true;
    }
    if candidates.iter().any(|s| s.contains("claude-3-7-sonnet")) {
        return true;
    }
    candidates.iter().any(|s| s.contains("claude-3-5-haiku"))
}

fn map_thinking_level_to_effort(model: &Model, level: ThinkingLevel) -> String {
    if level == ThinkingLevel::Xhigh && supports_native_xhigh_effort(&model.id, &model.name) {
        return "xhigh".to_string();
    }
    if let Some(map) = &model.thinking_level_map {
        let key = match level {
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::Xhigh => "xhigh",
        };
        if let Some(Some(mapped)) = map.get(key) {
            return mapped.clone();
        }
    }
    match level {
        ThinkingLevel::Minimal | ThinkingLevel::Low => "low".to_string(),
        ThinkingLevel::Medium => "medium".to_string(),
        ThinkingLevel::High | ThinkingLevel::Xhigh => "high".to_string(),
    }
}

pub fn build_additional_model_request_fields(model: &Model, options: &BedrockThinkingOptions<'_>) -> Option<Value> {
    let reasoning = options.reasoning?;
    if !model.reasoning {
        return None;
    }
    if !is_anthropic_claude_model(&model.id, &model.name) {
        return None;
    }

    let display = if is_govcloud_bedrock_target(&model.id, options.region) {
        None
    } else {
        Some(options.thinking_display.unwrap_or("summarized").to_string())
    };

    let mut result = if supports_adaptive_thinking(&model.id, &model.name) {
        let mut thinking = json!({ "type": "adaptive" });
        if let Some(display) = &display {
            thinking["display"] = json!(display);
        }
        json!({
            "thinking": thinking,
            "output_config": { "effort": map_thinking_level_to_effort(model, reasoning) },
        })
    } else {
        let level = if reasoning == ThinkingLevel::Xhigh {
            ThinkingLevel::High
        } else {
            reasoning
        };
        let default_budgets = [
            (ThinkingLevel::Minimal, 1024),
            (ThinkingLevel::Low, 2048),
            (ThinkingLevel::Medium, 8192),
            (ThinkingLevel::High, 16384),
        ];
        let budget = options
            .thinking_budgets
            .and_then(|budgets| match level {
                ThinkingLevel::Minimal => budgets.minimal,
                ThinkingLevel::Low => budgets.low,
                ThinkingLevel::Medium => budgets.medium,
                ThinkingLevel::High | ThinkingLevel::Xhigh => budgets.high,
            })
            .unwrap_or_else(|| {
                default_budgets
                    .iter()
                    .find_map(|(lvl, tokens)| (*lvl == level).then_some(*tokens))
                    .unwrap_or(16384)
            });
        let mut thinking = json!({ "type": "enabled", "budget_tokens": budget });
        if let Some(display) = &display {
            thinking["display"] = json!(display);
        }
        json!({ "thinking": thinking })
    };

    if !supports_adaptive_thinking(&model.id, &model.name) && options.interleaved_thinking {
        result["anthropic_beta"] = json!(["interleaved-thinking-2025-05-14"]);
    }

    Some(result)
}

pub fn cache_point_block(cache_retention: CacheRetention) -> Value {
    if cache_retention == CacheRetention::Long {
        json!({ "cachePoint": { "type": "default", "ttl": "ONE_HOUR" } })
    } else {
        json!({ "cachePoint": { "type": "default" } })
    }
}

pub fn build_bedrock_system_blocks(
    system_prompt: Option<&str>,
    model: &Model,
    cache_retention: CacheRetention,
    env: Option<&ProviderEnv>,
    sanitize: impl Fn(&str) -> String,
) -> Option<Vec<Value>> {
    let prompt = system_prompt?;
    let mut blocks = vec![json!({ "text": sanitize(prompt) })];
    if cache_retention != CacheRetention::None && supports_prompt_caching(&model.id, &model.name, env) {
        blocks.push(cache_point_block(cache_retention));
    }
    Some(blocks)
}

pub fn append_cache_point_to_last_user_message(messages: &mut [Value], cache_retention: CacheRetention) {
    if cache_retention == CacheRetention::None {
        return;
    }
    let Some(last) = messages.last_mut() else {
        return;
    };
    if last.get("role").and_then(|v| v.as_str()) != Some("user") {
        return;
    }
    if let Some(content) = last.get_mut("content").and_then(|v| v.as_array_mut()) {
        content.push(cache_point_block(cache_retention));
    }
}
