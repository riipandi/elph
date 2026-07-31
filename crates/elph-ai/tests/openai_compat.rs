use elph_ai::api::openai_compat::{detect_compat, get_compat};
use elph_ai::api::openai_completions::{OpenAICompletionsOptions, build_openai_completions_params};
use elph_ai::get_builtin_model;
use elph_ai::types::{Context, StreamOptions};

#[test]
fn detect_compat_matches_elph_ai_defaults() {
    let model = get_builtin_model("deepseek", "deepseek-v4-flash").expect("model exists");
    let compat = detect_compat(&model);
    assert_eq!(compat.thinking_format, "deepseek");
    assert!(!compat.supports_store);
    assert!(compat.requires_reasoning_content_on_assistant_messages);

    let openrouter = get_builtin_model("openrouter", "anthropic/claude-3-haiku").expect("model exists");
    let or_compat = get_compat(&openrouter);
    assert_eq!(or_compat.thinking_format, "openrouter");
    assert_eq!(or_compat.cache_control_format.as_deref(), Some("anthropic"));
}

#[test]
fn tokenrouter_gateway_avoids_openai_only_fields() {
    let model = get_builtin_model("tokenrouter", "moonshotai/kimi-k3-free").expect("model exists");
    let compat = detect_compat(&model);
    assert!(!compat.supports_store, "gateways must not send store");
    assert!(!compat.supports_developer_role, "gateways must keep system role");
    assert!(!compat.supports_strict_mode, "gateways must omit tool strict");
    assert_eq!(compat.max_tokens_field, "max_tokens");
    assert!(compat.supports_reasoning_effort, "moonshotai/* may use reasoning_effort");
    assert!(compat.requires_reasoning_content_on_assistant_messages);

    let context = Context {
        system_prompt: Some("You are helpful.".into()),
        messages: vec![],
        tools: None,
    };
    let options = OpenAICompletionsOptions {
        base: StreamOptions {
            max_tokens: Some(1024),
            ..Default::default()
        },
        reasoning_effort: Some("medium".into()),
        ..Default::default()
    };
    let params = build_openai_completions_params(&model, &context, &options).expect("params");
    assert!(params.get("store").is_none(), "must not send store: {:?}", params.get("store"));
    assert!(params.get("max_completion_tokens").is_none());
    assert_eq!(params["max_tokens"], 1024);
    let messages = params["messages"].as_array().expect("messages");
    assert_eq!(messages[0]["role"], "system");
}

#[test]
fn responses_tools_strip_root_anyof_required_unions_for_xai() {
    use elph_ai::api::openai_responses_shared::convert_responses_tools;
    use elph_ai::types::Tool;

    let tool = Tool {
        name: "ask_user_question".into(),
        description: "ask".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string" },
                "questions": { "type": "array" }
            },
            "anyOf": [
                { "required": ["question"] },
                { "required": ["questions"] }
            ]
        }),
        constrained_sampling: None,
    };
    let converted = convert_responses_tools(&[tool], Some(false));
    assert_eq!(converted.len(), 1);
    let params = &converted[0]["parameters"];
    assert_eq!(params["type"], "object");
    assert!(
        params.get("anyOf").is_none(),
        "xAI rejects root anyOf with required-only branches: {params}"
    );
}

#[test]
fn opengateway_and_baseten_are_non_standard_gateways() {
    if let Some(model) = get_builtin_model("opengateway", "auto") {
        let compat = detect_compat(&model);
        assert!(!compat.supports_store);
        assert!(!compat.supports_developer_role);
        assert_eq!(compat.max_tokens_field, "max_tokens");
    }
    // baseten sample
    let models = elph_ai::get_builtin_models("baseten");
    if let Some(model) = models.into_iter().next() {
        let compat = detect_compat(&model);
        assert!(!compat.supports_store);
        assert!(!compat.supports_developer_role);
        assert_eq!(compat.max_tokens_field, "max_tokens");
    }
}
