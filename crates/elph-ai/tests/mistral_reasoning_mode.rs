use elph_ai::api::mistral_conversations::{
    MistralOptions, build_mistral_conversations_payload, mistral_options_from_simple,
};
use elph_ai::get_builtin_model;
use elph_ai::types::{
    CacheRetention, Context, Message, SimpleStreamOptions, StreamOptions, ThinkingLevel, UserContent,
};

fn sample_context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: None,
    }
}

fn payload_field(model_id: &str, options: MistralOptions) -> serde_json::Value {
    let model = get_builtin_model("mistral", model_id).expect("model");
    let context = sample_context();
    build_mistral_conversations_payload(&model, &context, &context.messages, &options).expect("payload")
}

#[test]
fn uses_reasoning_effort_for_mistral_small_4() {
    let payload = payload_field(
        "mistral-small-2603",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-small-2603").expect("model"),
            &sample_context(),
            Some(&SimpleStreamOptions {
                base: StreamOptions::default(),
                reasoning: Some(ThinkingLevel::Medium),
                thinking_budgets: None,
            }),
        ),
    );
    assert_eq!(payload["reasoning_effort"], "high");
    assert!(payload.get("prompt_mode").is_none());
}

#[test]
fn omits_reasoning_controls_for_mistral_small_4_when_thinking_is_off() {
    let payload = payload_field(
        "mistral-small-2603",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-small-2603").expect("model"),
            &sample_context(),
            None,
        ),
    );
    assert!(payload.get("reasoning_effort").is_none());
    assert!(payload.get("prompt_mode").is_none());
}

#[test]
fn uses_prompt_mode_for_magistral_reasoning_models() {
    let payload = payload_field(
        "magistral-medium-latest",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "magistral-medium-latest").expect("model"),
            &sample_context(),
            Some(&SimpleStreamOptions {
                base: StreamOptions::default(),
                reasoning: Some(ThinkingLevel::Medium),
                thinking_budgets: None,
            }),
        ),
    );
    assert_eq!(payload["prompt_mode"], "reasoning");
    assert!(payload.get("reasoning_effort").is_none());
}

#[test]
fn uses_reasoning_effort_for_mistral_medium_3_5() {
    let payload = payload_field(
        "mistral-medium-3.5",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-medium-3.5").expect("model"),
            &sample_context(),
            Some(&SimpleStreamOptions {
                base: StreamOptions::default(),
                reasoning: Some(ThinkingLevel::Medium),
                thinking_budgets: None,
            }),
        ),
    );
    assert_eq!(payload["reasoning_effort"], "high");
    assert!(payload.get("prompt_mode").is_none());
}

#[test]
fn omits_reasoning_controls_for_mistral_medium_3_5_when_thinking_is_off() {
    let payload = payload_field(
        "mistral-medium-3.5",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-medium-3.5").expect("model"),
            &sample_context(),
            None,
        ),
    );
    assert!(payload.get("reasoning_effort").is_none());
    assert!(payload.get("prompt_mode").is_none());
}

#[test]
fn uses_session_id_as_prompt_cache_key() {
    let payload = payload_field(
        "mistral-large-latest",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-large-latest").expect("model"),
            &sample_context(),
            Some(&SimpleStreamOptions {
                base: StreamOptions {
                    session_id: Some("session-123".to_string()),
                    ..Default::default()
                },
                reasoning: None,
                thinking_budgets: None,
            }),
        ),
    );
    assert_eq!(payload["prompt_cache_key"], "session-123");
}

#[test]
fn omits_prompt_cache_key_when_cache_retention_is_disabled() {
    let payload = payload_field(
        "mistral-large-latest",
        mistral_options_from_simple(
            &get_builtin_model("mistral", "mistral-large-latest").expect("model"),
            &sample_context(),
            Some(&SimpleStreamOptions {
                base: StreamOptions {
                    session_id: Some("session-123".to_string()),
                    cache_retention: Some(CacheRetention::None),
                    ..Default::default()
                },
                reasoning: None,
                thinking_budgets: None,
            }),
        ),
    );
    assert!(payload.get("prompt_cache_key").is_none());
}
