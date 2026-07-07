use elph_ai::{
    AssistantContentBlock, Context, FauxResponseStep, Message, SimpleStreamOptions, StopReason, UserContent,
    faux_assistant_message, faux_provider, faux_text,
};
use tokio_util::sync::CancellationToken;

fn sample_context() -> Context {
    Context {
        system_prompt: Some("You are a helpful assistant.".to_string()),
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: None,
    }
}

#[tokio::test]
async fn faux_handles_immediate_abort_via_stream_options() {
    let token = CancellationToken::new();
    token.cancel();
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("should not arrive")],
        None,
    ))]);

    let response = faux
        .provider
        .stream(
            &model,
            &sample_context(),
            Some(elph_ai::StreamOptions {
                signal: Some(token),
                ..Default::default()
            }),
        )
        .result()
        .await;

    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert!(response.content.is_empty());
    assert_eq!(faux.core.state.lock().unwrap().call_count, 0);
}

#[tokio::test]
async fn faux_handles_immediate_abort_via_simple_stream_options() {
    let token = CancellationToken::new();
    token.cancel();
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("should not arrive")],
        None,
    ))]);

    let response = faux
        .provider
        .stream_simple(
            &model,
            &sample_context(),
            Some(SimpleStreamOptions {
                base: elph_ai::StreamOptions {
                    signal: Some(token),
                    ..Default::default()
                },
                reasoning: None,
                thinking_budgets: None,
            }),
        )
        .result()
        .await;

    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert!(response.content.is_empty());
}

#[tokio::test]
async fn faux_returns_partial_content_before_immediate_abort_still_aborts_cleanly() {
    let token = CancellationToken::new();
    token.cancel();
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("partial".repeat(20))],
        None,
    ))]);

    let response = faux
        .provider
        .stream_simple(
            &model,
            &sample_context(),
            Some(SimpleStreamOptions {
                base: elph_ai::StreamOptions {
                    signal: Some(token),
                    ..Default::default()
                },
                reasoning: None,
                thinking_budgets: None,
            }),
        )
        .result()
        .await;

    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert!(response.content.is_empty() || matches!(response.content.first(), Some(AssistantContentBlock::Text(_))));
}
