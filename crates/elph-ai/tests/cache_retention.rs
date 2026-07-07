mod common;

use std::collections::HashMap;

use common::{
    anthropic_model, completions_proxy_model, responses_model, sample_user_context, stream_options_with_cache,
};
use elph_ai::api::anthropic_messages::{AnthropicOptions, build_anthropic_messages_params};
use elph_ai::api::openai_completions::{OpenAICompletionsOptions, build_openai_completions_params};
use elph_ai::api::openai_responses::{OpenAIResponsesOptions, build_openai_responses_params};
use elph_ai::types::{AnthropicMessagesCompat, CacheRetention, OpenAICompletionsCompat, OpenAIResponsesCompat};
#[test]
fn anthropic_uses_ephemeral_cache_control_by_default() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Short, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(params["system"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn anthropic_uses_one_hour_ttl_when_cache_retention_is_long() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Long, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["ttl"], "1h");
}

#[test]
fn anthropic_omits_ttl_when_long_cache_retention_is_unsupported() {
    let params = build_anthropic_messages_params(
        &anthropic_model(
            "https://my-proxy.example.com/v1",
            Some(AnthropicMessagesCompat {
                supports_long_cache_retention: Some(false),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Long, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(params["system"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn anthropic_omits_cache_control_when_cache_retention_is_none() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::None, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params["system"][0].get("cache_control").is_none());
}

#[test]
fn openai_responses_sets_prompt_cache_retention_for_proxy_with_long_retention() {
    let params = build_openai_responses_params(
        &responses_model("https://my-proxy.example.com/v1", None),
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-2")),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["prompt_cache_key"], "session-2");
    assert_eq!(params["prompt_cache_retention"], "24h");
}

#[test]
fn openai_responses_omits_long_cache_when_compat_disables_it() {
    let params = build_openai_responses_params(
        &responses_model(
            "https://api.openai.com/v1",
            Some(OpenAIResponsesCompat {
                supports_long_cache_retention: Some(false),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-compat-false")),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn openai_completions_sets_prompt_cache_for_proxy_with_long_retention() {
    let params = build_openai_completions_params(
        &completions_proxy_model(None),
        &sample_user_context(Some("system")),
        &OpenAICompletionsOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-completions")),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["prompt_cache_key"], "session-completions");
    assert_eq!(params["prompt_cache_retention"], "24h");
}

#[test]
fn openai_completions_omits_long_cache_when_compat_disables_it() {
    let params = build_openai_completions_params(
        &completions_proxy_model(Some(OpenAICompletionsCompat {
            supports_long_cache_retention: Some(false),
            ..Default::default()
        })),
        &sample_user_context(Some("system")),
        &OpenAICompletionsOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-completions-false")),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn elph_cache_retention_env_maps_to_long_ttl() {
    let mut env = HashMap::new();
    env.insert("PI_CACHE_RETENTION".to_string(), "long".to_string());
    let mut base = stream_options_with_cache(CacheRetention::Short, None);
    base.cache_retention = None;
    base.env = Some(env);

    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base,
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["ttl"], "1h");
}
