mod common;

use std::sync::Arc;

use elph_agent::tools::echo_tool;
use elph_agent::{
    AgentContext, AgentEvent, AgentLoopConfig, ToolExecutionMode, default_convert_to_llm_fn, llm_message_to_agent,
    run_agent_loop,
};
use elph_ai::{
    FauxResponseStep, Message, SimpleStreamOptions, StopReason, UserContent, faux_assistant_message, faux_provider,
    faux_text, faux_tool_call,
};
use serde_json::json;
use tokio::sync::Mutex;

fn faux_stream_fn(faux: &elph_ai::FauxProviderHandle) -> elph_agent::StreamFn {
    let provider = faux.provider.clone();
    Arc::new(move |model, context, options| provider.stream_simple(model, context, options))
}

fn base_config(model: elph_ai::Model, stream_fn: elph_agent::StreamFn) -> AgentLoopConfig {
    AgentLoopConfig {
        model,
        stream_options: SimpleStreamOptions {
            base: Default::default(),
            reasoning: None,
            thinking_budgets: None,
        },
        convert_to_llm: default_convert_to_llm_fn(),
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        tool_execution: ToolExecutionMode::Parallel,
        before_tool_call: None,
        after_tool_call: None,
        stream_fn: Some(stream_fn),
    }
}

#[tokio::test]
async fn run_agent_loop_completes_text_response() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("hello from faux")],
        None,
    ))]);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_capture = events.clone();
    let emit: elph_agent::agent_loop::AgentEventCallback = Arc::new(move |event| {
        let events_capture = events_capture.clone();
        Box::pin(async move {
            events_capture.lock().await.push(event);
        })
    });

    let prompts = vec![llm_message_to_agent(Message::User {
        content: UserContent::Text("hi".into()),
        timestamp: 0,
    })];
    let context = AgentContext {
        system_prompt: "test".into(),
        messages: Vec::new(),
        tools: Vec::new(),
    };

    let new_messages = run_agent_loop(prompts, context, base_config(model, faux_stream_fn(&faux)), emit, None)
        .await
        .expect("agent loop");

    assert_eq!(new_messages.len(), 2);
    let recorded = events.lock().await;
    assert!(recorded.iter().any(|e| matches!(e, AgentEvent::AgentStart)));
    assert!(recorded.iter().any(|e| matches!(e, AgentEvent::AgentEnd { .. })));
}

#[tokio::test]
async fn run_agent_loop_executes_tool_and_continues() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("echo", json!({ "text": "ping" }), None)],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let flags = Arc::new(Mutex::new((false, false)));
    let flags_capture = flags.clone();
    let emit: elph_agent::agent_loop::AgentEventCallback = Arc::new(move |event| {
        let flags_capture = flags_capture.clone();
        Box::pin(async move {
            let mut flags = flags_capture.lock().await;
            match &event {
                AgentEvent::ToolExecutionStart { tool_name, .. } if tool_name == "echo" => flags.0 = true,
                AgentEvent::ToolExecutionEnd { tool_name, .. } if tool_name == "echo" => flags.1 = true,
                _ => {}
            }
        })
    });

    let prompts = vec![llm_message_to_agent(Message::User {
        content: UserContent::Text("echo".into()),
        timestamp: 0,
    })];
    let mut config = base_config(model, faux_stream_fn(&faux));
    config.tool_execution = ToolExecutionMode::Sequential;
    let context = AgentContext {
        system_prompt: String::new(),
        messages: Vec::new(),
        tools: vec![echo_tool()],
    };

    let _ = run_agent_loop(prompts, context, config, emit, None)
        .await
        .expect("agent loop");
    let flags = flags.lock().await;
    assert!(flags.0);
    assert!(flags.1);
}

#[tokio::test]
async fn fails_truncated_tool_calls_on_length_stop() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("echo", json!({ "text": "partial" }), None)],
            Some(StopReason::Length),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("recovered")], None)),
    ]);

    let tool_error = Arc::new(Mutex::new(false));
    let tool_error_capture = tool_error.clone();
    let emit: elph_agent::agent_loop::AgentEventCallback = Arc::new(move |event| {
        let tool_error_capture = tool_error_capture.clone();
        Box::pin(async move {
            if let AgentEvent::ToolExecutionEnd { is_error: true, .. } = event {
                *tool_error_capture.lock().await = true;
            }
        })
    });

    let context = AgentContext {
        system_prompt: String::new(),
        messages: vec![llm_message_to_agent(Message::User {
            content: UserContent::Text("go".into()),
            timestamp: 0,
        })],
        tools: vec![echo_tool()],
    };

    let _ = run_agent_loop(
        Vec::new(),
        context,
        base_config(model, faux_stream_fn(&faux)),
        emit,
        None,
    )
    .await
    .expect("agent loop");
    assert!(*tool_error.lock().await);
}
