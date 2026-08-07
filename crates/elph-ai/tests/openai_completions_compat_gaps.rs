//! Coverage for safe, additive Pi gaps ported into elph-ai's OpenAI-completions adapter:
//! sampled finish-reason inference, `sampling_params` merge, and `thinking_token_budget`.

mod common;

use std::collections::HashMap;

use common::{completions_proxy_model, sample_user_context};
use elph_ai::api::openai_completions::OpenAICompletionsOptions;
use elph_ai::api::openai_completions::build_openai_completions_params;
use elph_ai::types::OpenAICompletionsCompat;
use elph_ai::types::StreamOptions;
use serde_json::json;

#[test]
fn merges_model_default_sampling_params_into_payload() {
    let model = completions_proxy_model(Some(OpenAICompletionsCompat {
        sampling_params: Some(HashMap::from([
            ("top_p".to_string(), json!(0.9)),
            ("repetition_penalty".to_string(), json!(1.1)),
        ])),
        ..Default::default()
    }));

    let params =
        build_openai_completions_params(&model, &sample_user_context(None), &OpenAICompletionsOptions::default())
            .expect("params");

    assert_eq!(params["top_p"], json!(0.9));
    assert_eq!(params["repetition_penalty"], json!(1.1));
}

#[test]
fn per_request_sampling_params_override_model_defaults() {
    let model = completions_proxy_model(Some(OpenAICompletionsCompat {
        sampling_params: Some(HashMap::from([
            ("top_p".to_string(), json!(0.9)),
            ("top_k".to_string(), json!(40)),
        ])),
        ..Default::default()
    }));

    let options = OpenAICompletionsOptions {
        base: StreamOptions {
            sampling_params: Some(HashMap::from([
                ("top_p".to_string(), json!(0.5)),  // override
                ("min_p".to_string(), json!(0.05)), // new
            ])),
            ..Default::default()
        },
        ..Default::default()
    };

    let params = build_openai_completions_params(&model, &sample_user_context(None), &options).expect("params");

    // Per-request wins over model default; model-only key preserved.
    assert_eq!(params["top_p"], json!(0.5));
    assert_eq!(params["min_p"], json!(0.05));
    assert_eq!(params["top_k"], json!(40));
}

#[test]
fn sampling_params_never_clobber_explicit_options_like_temperature() {
    let model = completions_proxy_model(Some(OpenAICompletionsCompat {
        sampling_params: Some(HashMap::from([
            ("temperature".to_string(), json!(0.1)),
            ("top_p".to_string(), json!(0.9)),
        ])),
        ..Default::default()
    }));

    let options = OpenAICompletionsOptions {
        base: StreamOptions {
            temperature: Some(0.7),
            ..Default::default()
        },
        ..Default::default()
    };
    let params = build_openai_completions_params(&model, &sample_user_context(None), &options).expect("params");

    // Explicit option wins; unrelated sampling key still applied.
    assert_eq!(params["temperature"], json!(0.7));
    assert_eq!(params["top_p"], json!(0.9));
}

#[test]
fn thinking_token_budget_emitted_when_compat_opted_in() {
    let model = completions_proxy_model(Some(OpenAICompletionsCompat {
        supports_thinking_token_budget: Some(true),
        ..Default::default()
    }));

    let options = OpenAICompletionsOptions {
        base: StreamOptions {
            max_tokens: Some(8000),
            ..Default::default()
        },
        ..Default::default()
    };
    let params = build_openai_completions_params(&model, &sample_user_context(None), &options).expect("params");

    assert_eq!(params["thinking_token_budget"], json!(2000));
    // max_tokens must remain untouched.
    assert_eq!(params["max_completion_tokens"], json!(8000));
}

#[test]
fn thinking_token_budget_omitted_by_default() {
    let model = completions_proxy_model(None);
    let options = OpenAICompletionsOptions {
        base: StreamOptions {
            max_tokens: Some(8000),
            ..Default::default()
        },
        ..Default::default()
    };

    let params = build_openai_completions_params(&model, &sample_user_context(None), &options).expect("params");

    assert!(params.get("thinking_token_budget").is_none());
}

#[test]
fn supports_finish_reason_defaults_to_true() {
    let model = completions_proxy_model(None);
    let compat = elph_ai::api::openai_compat::get_compat(&model);
    assert!(
        compat.supports_finish_reason,
        "finish-reason inference must remain opt-out to preserve existing behavior"
    );
}
