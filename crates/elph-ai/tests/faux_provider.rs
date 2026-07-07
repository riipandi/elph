use elph_ai::api::faux::{FauxModelDefinition, RegisterFauxProviderOptions};
use elph_ai::{
    AssistantContentBlock, Context, FauxResponseStep, Message, ProviderStreams, StopReason, UserContent,
    faux_assistant_message, faux_provider, faux_text, faux_thinking, faux_tool_call,
};
use elph_ai::{CreateModelsOptions, create_models};
use serde_json::json;

#[tokio::test]
async fn registers_custom_provider_and_estimates_usage() {
    let faux = faux_provider(Default::default());
    let mut models = create_models(None);
    models.set_provider(faux.provider.clone());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("hello world")],
        None,
    ))]);

    let context = Context {
        system_prompt: Some("Be concise.".to_string()),
        messages: vec![Message::User {
            content: UserContent::Text("hi there".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };

    let response = models.complete(&model, &context, None).await;
    assert_eq!(response.content.len(), 1);
    if let AssistantContentBlock::Text(t) = &response.content[0] {
        assert_eq!(t.text, "hello world");
    } else {
        panic!("expected text block");
    }
    assert!(response.usage.input > 0);
    assert!(response.usage.output > 0);
    assert_eq!(
        response.usage.total_tokens,
        response.usage.input + response.usage.output
    );
    assert_eq!(faux.core.state.lock().unwrap().call_count, 1);
}

#[tokio::test]
async fn supports_text_thinking_and_tool_call_blocks() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![
            faux_thinking("think"),
            faux_tool_call("echo", json!({ "text": "hi" }), None),
            faux_text("done"),
        ],
        Some(StopReason::ToolUse),
    ))]);

    let response = faux
        .provider
        .stream_simple(
            &model,
            &Context {
                system_prompt: None,
                messages: vec![Message::User {
                    content: UserContent::Text("hi".to_string()),
                    timestamp: 0,
                }],
                tools: None,
            },
            None,
        )
        .result()
        .await;

    assert!(matches!(response.content[0], AssistantContentBlock::Thinking(_)));
    assert!(matches!(response.content[1], AssistantContentBlock::ToolCall(_)));
    assert!(matches!(response.content[2], AssistantContentBlock::Text(_)));
    assert_eq!(response.stop_reason, StopReason::ToolUse);
}

#[tokio::test]
async fn supports_multiple_models_with_per_model_reasoning() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        models: Some(vec![
            FauxModelDefinition {
                id: "faux-fast".to_string(),
                name: Some("Faux Fast".to_string()),
                reasoning: Some(false),
                input: None,
                context_window: None,
                max_tokens: None,
            },
            FauxModelDefinition {
                id: "faux-thinker".to_string(),
                name: Some("Faux Thinker".to_string()),
                reasoning: Some(true),
                input: None,
                context_window: None,
                max_tokens: None,
            },
        ]),
        ..Default::default()
    });
    faux.set_responses(vec![
        FauxResponseStep::Factory(std::sync::Arc::new(|_, _, _, model| {
            faux_assistant_message(vec![faux_text(format!("{}:{}", model.id, model.reasoning))], None)
        })),
        FauxResponseStep::Factory(std::sync::Arc::new(|_, _, _, model| {
            faux_assistant_message(vec![faux_text(format!("{}:{}", model.id, model.reasoning))], None)
        })),
    ]);

    let fast = faux.provider.get_models().iter().find(|m| m.id == "faux-fast").unwrap();
    let thinker = faux
        .provider
        .get_models()
        .iter()
        .find(|m| m.id == "faux-thinker")
        .unwrap();
    let ctx = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };

    let fast_resp = faux.provider.stream_simple(fast, &ctx, None).result().await;
    let thinker_resp = faux.provider.stream_simple(thinker, &ctx, None).result().await;
    if let AssistantContentBlock::Text(t) = &fast_resp.content[0] {
        assert_eq!(t.text, "faux-fast:false");
    }
    if let AssistantContentBlock::Text(t) = &thinker_resp.content[0] {
        assert_eq!(t.text, "faux-thinker:true");
    }
}

#[tokio::test]
async fn consumes_queued_responses_in_order() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("first")], None)),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("second")], None)),
    ]);
    let ctx = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("hi".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };
    let first = faux.provider.stream_simple(&model, &ctx, None).result().await;
    let second = faux.provider.stream_simple(&model, &ctx, None).result().await;
    if let AssistantContentBlock::Text(t) = &first.content[0] {
        assert_eq!(t.text, "first");
    }
    if let AssistantContentBlock::Text(t) = &second.content[0] {
        assert_eq!(t.text, "second");
    }
    assert_eq!(faux.pending_count(), 0);
}
