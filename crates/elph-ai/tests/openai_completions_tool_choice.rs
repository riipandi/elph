mod common;

use common::{completions_proxy_model, sample_user_context};
use elph_ai::api::openai_completions::{OpenAICompletionsOptions, build_openai_completions_params};
use elph_ai::get_builtin_model;
use elph_ai::models::{clamp_thinking_level, thinking_level_to_str};
use elph_ai::types::{Context, Message, OpenAICompletionsCompat, ThinkingLevel, Tool, UserContent};
use serde_json::json;

fn reasoning_effort_for_model(model: &elph_ai::types::Model, level: ThinkingLevel) -> String {
    let clamped = clamp_thinking_level(model, level);
    if clamped == ThinkingLevel::Minimal {
        "minimal".to_string()
    } else {
        thinking_level_to_str(clamped).to_string()
    }
}

fn sample_tool() -> Tool {
    Tool {
        name: "ping".to_string(),
        description: "Ping tool".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"]
        }),
    }
}

#[test]
fn forwards_tool_choice_to_payload() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let mut context = sample_user_context(None);
    context.tools = Some(vec![sample_tool()]);
    let options = OpenAICompletionsOptions {
        tool_choice: Some(json!("required")),
        ..Default::default()
    };

    let params = build_openai_completions_params(&model, &context, &options).expect("params");

    assert_eq!(params["tool_choice"], json!("required"));
    assert!(params["tools"].as_array().is_some_and(|tools| !tools.is_empty()));
}

#[test]
fn omits_strict_when_compat_disables_strict_mode() {
    let model = completions_proxy_model(Some(OpenAICompletionsCompat {
        supports_strict_mode: Some(false),
        ..Default::default()
    }));
    let mut context = sample_user_context(None);
    context.tools = Some(vec![sample_tool()]);

    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");

    let tool = &params["tools"][0]["function"];
    assert!(tool.get("strict").is_none());
}

#[test]
fn maps_groq_qwen3_reasoning_levels_to_default_reasoning_effort() {
    let model = get_builtin_model("groq", "qwen/qwen3-32b").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };
    let options = OpenAICompletionsOptions {
        reasoning_effort: Some(reasoning_effort_for_model(&model, ThinkingLevel::Medium)),
        ..Default::default()
    };

    let params = build_openai_completions_params(&model, &context, &options).expect("params");

    assert_eq!(params["reasoning_effort"], json!("default"));
}

#[test]
fn keeps_normal_reasoning_effort_for_groq_models_without_compat_mapping() {
    let model = get_builtin_model("groq", "openai/gpt-oss-20b").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };
    let options = OpenAICompletionsOptions {
        reasoning_effort: Some(reasoning_effort_for_model(&model, ThinkingLevel::Medium)),
        ..Default::default()
    };

    let params = build_openai_completions_params(&model, &context, &options).expect("params");

    assert_eq!(params["reasoning_effort"], json!("medium"));
}

#[test]
fn enables_tool_stream_for_supported_zai_models_with_tools() {
    let model = get_builtin_model("zai", "glm-5.1").expect("model");
    let mut context = sample_user_context(None);
    context.tools = Some(vec![sample_tool()]);

    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");

    assert_eq!(params["tool_stream"], json!(true));
}

#[test]
fn stores_zai_tool_stream_support_in_model_compat_metadata() {
    let glm_5_1 = get_builtin_model("zai", "glm-5.1").expect("model");
    let glm_4_7 = get_builtin_model("zai", "glm-4.7").expect("model");
    let glm_5_turbo = get_builtin_model("zai", "glm-5-turbo").expect("model");
    let glm_4_5_air = get_builtin_model("zai", "glm-4.5-air").expect("model");

    assert_eq!(
        glm_5_1
            .openai_completions_compat
            .as_ref()
            .and_then(|c| c.zai_tool_stream),
        Some(true)
    );
    assert_eq!(
        glm_4_7
            .openai_completions_compat
            .as_ref()
            .and_then(|c| c.zai_tool_stream),
        Some(true)
    );
    assert_eq!(
        glm_5_turbo
            .openai_completions_compat
            .as_ref()
            .and_then(|c| c.zai_tool_stream),
        Some(true)
    );
    assert!(
        glm_4_5_air
            .openai_completions_compat
            .as_ref()
            .and_then(|c| c.zai_tool_stream)
            .is_none()
    );
}

#[test]
fn stores_zai_glm_5_2_effort_metadata() {
    let model = get_builtin_model("zai", "glm-5.2").expect("model");

    assert_eq!(
        model
            .openai_completions_compat
            .as_ref()
            .and_then(|c| c.supports_reasoning_effort),
        Some(true)
    );
    assert_eq!(
        model.thinking_level_map,
        Some(
            [
                ("minimal".to_string(), None),
                ("low".to_string(), Some("high".to_string())),
                ("medium".to_string(), Some("high".to_string())),
                ("high".to_string(), Some("high".to_string())),
                ("xhigh".to_string(), Some("max".to_string())),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn maps_zai_glm_5_2_thinking_levels_to_reasoning_effort() {
    let model = get_builtin_model("zai", "glm-5.2").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };

    for (level, expected_effort) in [
        (ThinkingLevel::Low, "high"),
        (ThinkingLevel::Medium, "high"),
        (ThinkingLevel::High, "high"),
        (ThinkingLevel::Xhigh, "max"),
    ] {
        let options = OpenAICompletionsOptions {
            reasoning_effort: Some(reasoning_effort_for_model(&model, level)),
            ..Default::default()
        };
        let params = build_openai_completions_params(&model, &context, &options).expect("params");
        assert_eq!(
            params["thinking"],
            json!({ "type": "enabled", "clear_thinking": false })
        );
        assert_eq!(params["reasoning_effort"], json!(expected_effort));
    }
}

#[test]
fn omits_zai_glm_5_2_reasoning_effort_when_thinking_is_off() {
    let model = get_builtin_model("zai", "glm-5.2").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };

    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");

    assert_eq!(params["thinking"], json!({ "type": "disabled" }));
    assert!(params.get("reasoning_effort").is_none());
}

#[test]
fn omits_tool_stream_for_unsupported_zai_models() {
    let model = get_builtin_model("zai", "glm-4.5-air").expect("model");
    let mut context = sample_user_context(None);
    context.tools = Some(vec![sample_tool()]);

    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");

    assert!(params.get("tool_stream").is_none());
}

#[test]
fn respects_explicit_zai_tool_stream_compat_override() {
    let mut model = get_builtin_model("zai", "glm-4.5-air").expect("model");
    model.openai_completions_compat = Some(OpenAICompletionsCompat {
        zai_tool_stream: Some(true),
        ..Default::default()
    });
    let mut context = sample_user_context(None);
    context.tools = Some(vec![sample_tool()]);

    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");

    assert_eq!(params["tool_stream"], json!(true));
}

#[test]
fn omits_tool_stream_when_no_tools_are_provided() {
    let model = get_builtin_model("zai", "glm-5.1").expect("model");

    let params =
        build_openai_completions_params(&model, &sample_user_context(None), &OpenAICompletionsOptions::default())
            .expect("params");

    assert!(params.get("tool_stream").is_none());
}
