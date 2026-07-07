use elph_ai::api::faux::RegisterFauxProviderOptions;
use elph_ai::{
    AssistantContentBlock, AssistantMessageEvent, Context, FauxResponseStep, Message, SimpleStreamOptions, StopReason,
    UserContent, faux_assistant_message, faux_provider, faux_text,
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

#[tokio::test]
async fn faux_aborts_mid_stream_after_some_text_deltas() {
    let token = CancellationToken::new();
    let faux = faux_provider(RegisterFauxProviderOptions {
        tokens_per_second: Some(1.0),
        ..Default::default()
    });
    let model = faux.provider.get_models()[0].clone();
    let long_text = "word ".repeat(80);
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text(long_text)],
        None,
    ))]);

    let mut stream = faux.provider.stream_simple(
        &model,
        &sample_context(),
        Some(SimpleStreamOptions {
            base: elph_ai::StreamOptions {
                signal: Some(token.clone()),
                ..Default::default()
            },
            reasoning: None,
            thinking_budgets: None,
        }),
    );

    let mut saw_text_delta = false;
    let mut event_count = 0usize;
    while let Some(event) = stream.next_event().await {
        event_count += 1;
        if matches!(event, AssistantMessageEvent::TextDelta { .. }) {
            saw_text_delta = true;
            token.cancel();
        }
        if event_count > 200 {
            break;
        }
    }

    let response = stream.result().await;
    assert!(saw_text_delta, "expected at least one text delta before abort");
    assert_eq!(response.stop_reason, StopReason::Aborted);
    if let Some(AssistantContentBlock::Text(t)) = response.content.first() {
        assert!(!t.text.is_empty(), "expected partial text before abort");
        assert!(t.text.len() < "word ".repeat(80).len(), "expected truncated stream");
    } else {
        panic!("expected partial text block");
    }
}
