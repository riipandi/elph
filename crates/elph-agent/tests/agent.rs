//! Agent integration tests.

mod common;

use std::sync::Arc;

use parking_lot::Mutex as ParkingMutex;

use elph_agent::{
    Agent, AgentEvent, AgentMessage, AgentOptions, AgentThinkingLevel, AgentTool, AgentToolResult, PartialAgentState,
    QueueMode, ToolResultContent, llm_message_to_agent, simple_tool,
};
use elph_ai::{
    FauxResponseStep, Message, StopReason, Tool, UserContent, api::common::wrap_on_payload, builtin_models,
    faux_assistant_message, faux_provider, faux_text, faux_tool_call,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, oneshot};

use common::{error_stream_fn, faux_stream_fn, hanging_until_abort_stream_fn};

#[tokio::test(flavor = "multi_thread")]
async fn agent_wires_on_payload_to_stream_options() {
    use elph_agent::runtime::try_block_on;

    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    let final_payload = Arc::new(ParkingMutex::new(None));
    let final_payload_clone = final_payload.clone();

    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        move |_context, options, _, model| {
            let payload = if let Some(on_payload) = options.and_then(|o| o.on_payload.clone()) {
                try_block_on(on_payload(json!({ "source": "provider" }), model.clone()))
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| json!({ "source": "provider" }))
            } else {
                json!({ "source": "provider" })
            };
            *final_payload_clone.lock() = Some(payload);
            faux_assistant_message(vec![faux_text("ok")], None)
        },
    ))]);

    let on_payload = wrap_on_payload(|payload, _model| {
        Box::pin(async move {
            let mut mutated = payload;
            mutated["mutated"] = json!(true);
            Some(mutated)
        })
    });

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        on_payload: Some(on_payload),
        ..Default::default()
    });

    agent.prompt_text("hello", None).await.expect("prompt");

    assert_eq!(
        final_payload.lock().clone(),
        Some(json!({ "source": "provider", "mutated": true }))
    );
}

#[tokio::test]
async fn agent_creates_with_default_state() {
    let agent = Agent::new(AgentOptions::default());
    let state = agent.state().await;

    assert_eq!(state.system_prompt, "");
    assert_eq!(state.thinking_level, AgentThinkingLevel::Off);
    assert!(state.tools.is_empty());
    assert!(state.messages.is_empty());
    assert!(!state.is_streaming);
    assert!(state.streaming_message.is_none());
    assert!(state.pending_tool_calls.is_empty());
    assert!(state.error_message.is_none());
}

#[tokio::test]
async fn agent_creates_with_custom_initial_state() {
    let models = builtin_models(None);
    let model = models.get_model("openai", "gpt-4o-mini").expect("model");

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model.clone()),
            thinking_level: Some(AgentThinkingLevel::Low),
            ..Default::default()
        }),
        ..Default::default()
    });

    let state = agent.state().await;
    assert_eq!(state.system_prompt, "You are a helpful assistant.");
    assert_eq!(state.model.id, model.id);
    assert_eq!(state.thinking_level, AgentThinkingLevel::Low);
}

#[tokio::test]
async fn agent_prompt_updates_state() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    });

    agent.prompt_text("hello", None).await.expect("prompt");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert_eq!(state.messages.len(), 2);
    assert!(matches!(state.messages[0], AgentMessage::Llm(_)));
    assert_eq!(state.messages[0].role(), "user");
}

#[tokio::test]
async fn agent_steering_queue_drains_one_at_a_time() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("first")], None)),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("second")], None)),
    ]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    });

    agent.steer(llm_message_to_agent(Message::User {
        content: UserContent::Text("steer-one".into()),
        timestamp: 1,
    }));
    agent.steer(llm_message_to_agent(Message::User {
        content: UserContent::Text("steer-two".into()),
        timestamp: 2,
    }));
    assert!(agent.has_queued_messages());

    agent.prompt_text("start", None).await.expect("prompt");
    agent.wait_for_idle().await;

    assert!(!agent.has_queued_messages());
}

#[tokio::test]
async fn agent_follow_up_queue_does_not_affect_messages_until_drained() {
    let agent = Agent::new(AgentOptions::default());
    agent.follow_up(llm_message_to_agent(Message::User {
        content: UserContent::Text("Follow-up message".into()),
        timestamp: 1,
    }));

    let state = agent.state().await;
    assert!(state.messages.is_empty());
    assert!(agent.has_queued_messages());
}

#[tokio::test]
async fn agent_steering_queue_does_not_affect_messages_until_drained() {
    let agent = Agent::new(AgentOptions::default());
    agent.steer(llm_message_to_agent(Message::User {
        content: UserContent::Text("Steering message".into()),
        timestamp: 1,
    }));

    let state = agent.state().await;
    assert!(state.messages.is_empty());
    assert!(agent.has_queued_messages());
}

#[tokio::test]
async fn agent_abort_does_not_panic_when_idle() {
    let agent = Agent::new(AgentOptions::default());
    agent.abort().await;
}

#[tokio::test]
async fn agent_emits_lifecycle_on_provider_failure() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(error_stream_fn("provider exploded")),
        ..Default::default()
    });

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_capture = events.clone();
    agent
        .subscribe(Arc::new(move |event, _| {
            let events_capture = events_capture.clone();
            Box::pin(async move {
                let name = match &event {
                    AgentEvent::AgentStart => "agent_start",
                    AgentEvent::TurnStart => "turn_start",
                    AgentEvent::MessageStart { .. } => "message_start",
                    AgentEvent::MessageEnd { .. } => "message_end",
                    AgentEvent::TurnEnd { .. } => "turn_end",
                    AgentEvent::AgentEnd { .. } => "agent_end",
                    _ => "other",
                };
                events_capture.lock().await.push(name.to_string());
            })
        }))
        .await;

    agent.prompt_text("hello", None).await.expect("prompt");
    agent.wait_for_idle().await;

    let recorded = events.lock().await.clone();
    assert_eq!(
        recorded,
        vec![
            "agent_start",
            "turn_start",
            "message_start",
            "message_end",
            "message_start",
            "message_end",
            "turn_end",
            "agent_end"
        ]
    );

    let state = agent.state().await;
    let last = state.messages.last().expect("assistant message");
    assert_eq!(last.role(), "assistant");
    let Some(Message::Assistant(assistant)) = last.as_llm() else {
        panic!("expected assistant llm message");
    };
    assert_eq!(assistant.stop_reason, StopReason::Error);
    assert_eq!(assistant.error_message.as_deref(), Some("provider exploded"));
    assert_eq!(state.error_message.as_deref(), Some("provider exploded"));
}

#[tokio::test]
async fn agent_awaits_async_subscribers_before_settling() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let (tx, rx) = oneshot::channel::<()>();
    let barrier = Arc::new(Mutex::new(Some(rx)));
    let barrier_capture = barrier.clone();

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    }));

    let listener_finished = Arc::new(Mutex::new(false));
    let listener_finished_capture = listener_finished.clone();
    agent
        .subscribe(Arc::new(move |event, _| {
            let barrier_capture = barrier_capture.clone();
            let listener_finished = listener_finished_capture.clone();
            Box::pin(async move {
                if matches!(event, AgentEvent::AgentEnd { .. }) {
                    if let Some(rx) = barrier_capture.lock().await.take() {
                        let _ = rx.await;
                    }
                    *listener_finished.lock().await = true;
                }
            })
        }))
        .await;

    let prompt_done = Arc::new(Mutex::new(false));
    let prompt_done_capture = prompt_done.clone();
    let agent_for_prompt = agent.clone();
    let prompt_task = tokio::spawn(async move {
        agent_for_prompt.prompt_text("hello", None).await.expect("prompt");
        *prompt_done_capture.lock().await = true;
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert!(!*prompt_done.lock().await);
    assert!(!*listener_finished.lock().await);
    assert!(agent.state().await.is_streaming);

    let _ = tx.send(());
    prompt_task.await.expect("prompt task");

    assert!(*listener_finished.lock().await);
    assert!(*prompt_done.lock().await);
    assert!(!agent.state().await.is_streaming);
}

#[tokio::test]
async fn agent_rejects_second_prompt_while_streaming() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model.clone()),
            ..Default::default()
        }),
        stream_fn: Some(hanging_until_abort_stream_fn(&model)),
        ..Default::default()
    }));

    let agent_for_first = agent.clone();
    let first = tokio::spawn(async move {
        let _ = agent_for_first.prompt_text("first", None).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(agent.state().await.is_streaming);

    let second = agent.prompt_text("second", None).await;
    assert!(second.is_err());
    assert!(
        second
            .unwrap_err()
            .to_string()
            .contains("Agent is already processing a prompt")
    );

    agent.abort().await;
    let _ = first.await;
}

#[tokio::test]
async fn agent_rejects_continue_while_streaming() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model.clone()),
            ..Default::default()
        }),
        stream_fn: Some(hanging_until_abort_stream_fn(&model)),
        ..Default::default()
    }));

    let agent_for_first = agent.clone();
    let first = tokio::spawn(async move {
        let _ = agent_for_first.prompt_text("first", None).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let second = agent.continue_run().await;
    assert!(second.is_err());
    assert!(second.unwrap_err().to_string().contains("Agent is already processing"));

    agent.abort().await;
    let _ = first.await;
}

#[tokio::test]
async fn agent_continue_processes_follow_up_from_assistant_tail() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Processed")],
        None,
    ))]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            messages: Some(vec![
                llm_message_to_agent(Message::User {
                    content: UserContent::Text("Initial".into()),
                    timestamp: 1,
                }),
                llm_message_to_agent(Message::Assistant(faux_assistant_message(
                    vec![faux_text("Initial response")],
                    None,
                ))),
            ]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    });

    agent.follow_up(llm_message_to_agent(Message::User {
        content: UserContent::Text("Queued follow-up".into()),
        timestamp: 2,
    }));

    agent.continue_run().await.expect("continue");

    let state = agent.state().await;
    let has_follow_up = state.messages.iter().any(|message| {
        matches!(
            message.as_llm(),
            Some(Message::User {
                content: UserContent::Text(text),
                ..
            }) if text == "Queued follow-up"
        )
    });
    assert!(has_follow_up);
    assert_eq!(state.messages.last().map(|m| m.role()), Some("assistant"));
}

#[tokio::test]
async fn agent_forwards_session_id_to_stream_fn() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    let captured = Arc::new(ParkingMutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let provider = faux.provider.clone();

    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| {
        captured_clone
            .lock()
            .push(opts.as_ref().and_then(|o| o.base.session_id.clone()));
        provider.stream_simple(m, ctx, opts)
    });

    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        session_id: Some("session-abc".into()),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    agent.prompt_text("hello", None).await.expect("prompt");
    assert_eq!(captured.lock().first().cloned().flatten(), Some("session-abc".into()));
}

#[tokio::test]
async fn agent_exposes_queue_modes() {
    let agent = Agent::new(AgentOptions {
        steering_mode: QueueMode::All,
        follow_up_mode: QueueMode::All,
        ..Default::default()
    });

    assert_eq!(agent.steering_mode(), QueueMode::All);
    assert_eq!(agent.follow_up_mode(), QueueMode::All);

    agent.set_steering_mode(QueueMode::OneAtATime);
    agent.set_follow_up_mode(QueueMode::OneAtATime);
    assert_eq!(agent.steering_mode(), QueueMode::OneAtATime);
    assert_eq!(agent.follow_up_mode(), QueueMode::OneAtATime);

    agent.clear_all_queues();
    assert!(!agent.has_queued_messages());
}

#[tokio::test]
async fn agent_subscribe_can_unsubscribe() {
    let agent = Agent::new(AgentOptions::default());
    let count = Arc::new(Mutex::new(0));
    let count_capture = count.clone();
    let subscription = agent
        .subscribe(Arc::new(move |_, _| {
            let count_capture = count_capture.clone();
            Box::pin(async move {
                *count_capture.lock().await += 1;
            })
        }))
        .await;

    agent.set_system_prompt("Test prompt").await;
    assert_eq!(*count.lock().await, 0);

    subscription.unsubscribe().await;
    agent.set_system_prompt("Another prompt").await;
    assert_eq!(*count.lock().await, 0);
}

#[tokio::test]
async fn agent_wait_for_idle_waits_for_async_subscribers() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let (tx, rx) = oneshot::channel::<()>();
    let barrier = Arc::new(Mutex::new(Some(rx)));

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    }));

    let barrier_capture = barrier.clone();
    agent
        .subscribe(Arc::new(move |event, _| {
            let barrier_capture = barrier_capture.clone();
            Box::pin(async move {
                if matches!(event, AgentEvent::AgentEnd { .. })
                    && let Some(rx) = barrier_capture.lock().await.take()
                {
                    let _ = rx.await;
                }
            })
        }))
        .await;

    let idle_done = Arc::new(Mutex::new(false));
    let idle_done_capture = idle_done.clone();
    let agent_for_prompt = agent.clone();
    let agent_for_idle = agent.clone();
    let prompt_task = tokio::spawn(async move {
        agent_for_prompt.prompt_text("hello", None).await.expect("prompt");
    });
    let idle_task = tokio::spawn(async move {
        agent_for_idle.wait_for_idle().await;
        *idle_done_capture.lock().await = true;
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(!*idle_done.lock().await);
    assert!(agent.state().await.is_streaming);

    let _ = tx.send(());
    let _ = tokio::join!(prompt_task, idle_task);
    assert!(*idle_done.lock().await);
    assert!(!agent.state().await.is_streaming);
}

#[tokio::test]
async fn agent_passes_abort_signal_to_subscribers() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model.clone()),
            ..Default::default()
        }),
        stream_fn: Some(hanging_until_abort_stream_fn(&model)),
        ..Default::default()
    }));

    let received_aborted = Arc::new(Mutex::new(None));
    let received_capture = received_aborted.clone();
    agent
        .subscribe(Arc::new(move |event, signal| {
            let received_capture = received_capture.clone();
            Box::pin(async move {
                if matches!(event, AgentEvent::AgentStart) {
                    *received_capture.lock().await = Some(signal.is_cancelled());
                }
            })
        }))
        .await;

    let prompt_task = {
        let agent = agent.clone();
        tokio::spawn(async move {
            let _ = agent.prompt_text("hello", None).await;
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(*received_aborted.lock().await, Some(false));

    agent.abort().await;
    let _ = prompt_task.await;
    if let Some(signal) = agent.signal().await {
        assert!(signal.is_cancelled());
    }
}

#[tokio::test]
async fn agent_updates_state_with_mutators() {
    let models = builtin_models(None);
    let model = models.get_model("openai", "gpt-4o-mini").expect("model");
    let agent = Agent::new(AgentOptions::default());

    agent.set_system_prompt("Custom prompt").await;
    assert_eq!(agent.state().await.system_prompt, "Custom prompt");

    agent.set_model(model.clone()).await;
    assert_eq!(agent.state().await.model.id, model.id);

    agent.set_thinking_level(AgentThinkingLevel::High).await;
    assert_eq!(agent.state().await.thinking_level, AgentThinkingLevel::High);

    let tools = vec![noop_tool()];
    agent.set_tools(tools.clone()).await;
    assert_eq!(agent.state().await.tools.len(), 1);
    assert_eq!(agent.state().await.tools[0].tool.name, "noop");

    let messages = vec![llm_message_to_agent(Message::User {
        content: UserContent::Text("Hello".into()),
        timestamp: 1,
    })];
    agent.set_messages(messages.clone()).await;
    assert_eq!(agent.state().await.messages.len(), 1);

    let assistant = llm_message_to_agent(Message::Assistant(faux_assistant_message(vec![faux_text("Hi")], None)));
    agent.append_message(assistant).await;
    assert_eq!(agent.state().await.messages.len(), 2);

    agent.clear_messages().await;
    assert!(agent.state().await.messages.is_empty());
}

fn noop_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "noop".into(),
            description: "Noop".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        "Noop",
        |_, _| Box::pin(async { Ok(AgentToolResult::text("ok")) }),
    )
}

#[tokio::test]
async fn agent_ignores_late_tool_updates_after_settlement() {
    let update_tx: Arc<tokio::sync::Mutex<Option<elph_agent::ToolUpdateCallback>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let update_tx_capture = update_tx.clone();
    let tool = AgentTool {
        tool: Tool {
            name: "delayed_tool".into(),
            description: "Captures progress callbacks".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        label: "Delayed Tool".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: Arc::new(move |_id, _args, _signal, on_update| {
            let update_tx_capture = update_tx_capture.clone();
            Box::pin(async move {
                *update_tx_capture.lock().await = on_update.clone();
                if let Some(cb) = on_update.as_ref() {
                    cb(AgentToolResult {
                        content: vec![ToolResultContent::Text(elph_ai::TextContent::new("running"))],
                        details: json!({ "status": "running" }),
                        terminate: None,
                    });
                }
                Ok(AgentToolResult {
                    content: vec![ToolResultContent::Text(elph_ai::TextContent::new("ok"))],
                    details: json!({ "status": "done" }),
                    terminate: Some(true),
                })
            })
        }),
    };

    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_tool_call("delayed_tool", json!({}), Some("call-1".into()))],
        Some(StopReason::ToolUse),
    ))]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            tools: Some(vec![tool]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    });

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_capture = events.clone();
    agent
        .subscribe(Arc::new(move |event, _| {
            let events_capture = events_capture.clone();
            Box::pin(async move {
                if matches!(event, AgentEvent::ToolExecutionUpdate { .. }) {
                    events_capture.lock().await.push("update");
                }
            })
        }))
        .await;

    agent.prompt_text("run tool", None).await.expect("prompt");
    let count_after_prompt = events.lock().await.len();

    if let Some(cb) = update_tx.lock().await.clone() {
        cb(AgentToolResult {
            content: vec![ToolResultContent::Text(elph_ai::TextContent::new("late"))],
            details: json!({ "status": "late" }),
            terminate: None,
        });
    }
    tokio::task::yield_now().await;

    assert_eq!(events.lock().await.len(), count_after_prompt);
}

#[tokio::test]
async fn agent_continue_keeps_one_at_a_time_steering_from_assistant_tail() {
    let response_count = Arc::new(AtomicUsize::new(0));
    let response_count_capture = response_count.clone();
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let response_count_capture = response_count_capture.clone();
            Arc::new(move |_, _, _, _| {
                let n = response_count_capture.fetch_add(1, Ordering::SeqCst) + 1;
                faux_assistant_message(vec![faux_text(format!("Processed {n}"))], None)
            })
        }),
        FauxResponseStep::Factory({
            let response_count_capture = response_count_capture.clone();
            Arc::new(move |_, _, _, _| {
                let n = response_count_capture.fetch_add(1, Ordering::SeqCst) + 1;
                faux_assistant_message(vec![faux_text(format!("Processed {n}"))], None)
            })
        }),
    ]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            messages: Some(vec![
                llm_message_to_agent(Message::User {
                    content: UserContent::Text("Initial".into()),
                    timestamp: 1,
                }),
                llm_message_to_agent(Message::Assistant(faux_assistant_message(
                    vec![faux_text("Initial response")],
                    None,
                ))),
            ]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    });

    agent.steer(llm_message_to_agent(Message::User {
        content: UserContent::Text("Steering 1".into()),
        timestamp: 2,
    }));
    agent.steer(llm_message_to_agent(Message::User {
        content: UserContent::Text("Steering 2".into()),
        timestamp: 3,
    }));

    agent.continue_run().await.expect("continue");

    let roles: Vec<_> = agent
        .state()
        .await
        .messages
        .iter()
        .map(|m| m.role().to_string())
        .collect();
    assert!(roles.len() >= 4);
    assert_eq!(&roles[roles.len() - 4..], &["user", "assistant", "user", "assistant"]);
    assert_eq!(response_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn agent_prepare_next_turn_runs_between_tool_and_followup_turn() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("noop", json!({}), Some("tool-1".into()))],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let noop = simple_tool(
        Tool {
            name: "noop".into(),
            description: "Noop".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        "Noop",
        |_, _| Box::pin(async { Ok(AgentToolResult::text("ok")) }),
    );

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            tools: Some(vec![noop]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        prepare_next_turn: Some(Arc::new(|_context| Box::pin(async { None }))),
        ..Default::default()
    });

    agent.prompt_text("start", None).await.expect("prompt");

    let roles: Vec<_> = agent
        .state()
        .await
        .messages
        .iter()
        .map(|m| m.role().to_string())
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "toolResult", "assistant"]);
}

#[tokio::test]
async fn agent_session_id_setter_updates_stream_fn() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    let captured = Arc::new(ParkingMutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let provider = faux.provider.clone();

    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| {
        captured_clone
            .lock()
            .push(opts.as_ref().and_then(|o| o.base.session_id.clone()));
        provider.stream_simple(m, ctx, opts)
    });

    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("ok")], None)),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("ok again")], None)),
    ]);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        session_id: Some("session-abc".into()),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    agent.prompt_text("hello", None).await.expect("prompt");
    agent.set_session_id(Some("session-def".into()));
    agent.prompt_text("hello again", None).await.expect("prompt");

    let ids = captured.lock().clone();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], Some("session-abc".into()));
    assert_eq!(ids[1], Some("session-def".into()));
}

#[tokio::test]
async fn agent_ignores_parallel_settled_tool_update_while_another_tool_runs() {
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    let slow_started = Arc::new(AtomicBool::new(false));
    let settled_tool_ended = Arc::new(AtomicBool::new(false));
    let release_slow = Arc::new(AtomicBool::new(false));
    let settled_update_tx: Arc<tokio::sync::Mutex<Option<elph_agent::ToolUpdateCallback>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    let settled_update_capture = settled_update_tx.clone();
    let settled_tool = AgentTool {
        tool: Tool {
            name: "settled_tool".into(),
            description: "Captures progress callbacks".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        label: "Settled Tool".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: Arc::new(move |_id, _args, _signal, on_update| {
            let settled_update_capture = settled_update_capture.clone();
            Box::pin(async move {
                *settled_update_capture.lock().await = on_update.clone();
                Ok(AgentToolResult {
                    content: vec![ToolResultContent::Text(elph_ai::TextContent::new("done"))],
                    details: json!({ "status": "done" }),
                    terminate: Some(true),
                })
            })
        }),
    };

    let slow_started_capture = slow_started.clone();
    let release_slow_capture = release_slow.clone();
    let slow_tool = AgentTool {
        tool: Tool {
            name: "slow_tool".into(),
            description: "Keeps the agent run active".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        label: "Slow Tool".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: Arc::new(move |_id, _args, _signal, _on_update| {
            let slow_started_capture = slow_started_capture.clone();
            let release_slow_capture = release_slow_capture.clone();
            Box::pin(async move {
                slow_started_capture.store(true, Ordering::SeqCst);
                while !release_slow_capture.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok(AgentToolResult {
                    content: vec![ToolResultContent::Text(elph_ai::TextContent::new("done"))],
                    details: json!({ "status": "done" }),
                    terminate: Some(true),
                })
            })
        }),
    };

    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![
            faux_tool_call("settled_tool", json!({}), Some("call-1".into())),
            faux_tool_call("slow_tool", json!({}), Some("call-2".into())),
        ],
        Some(StopReason::ToolUse),
    ))]);

    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            tools: Some(vec![settled_tool, slow_tool]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        ..Default::default()
    }));

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_capture = events.clone();
    let settled_tool_ended_capture = settled_tool_ended.clone();
    agent
        .subscribe(Arc::new(move |event, _| {
            let events_capture = events_capture.clone();
            let settled_tool_ended_capture = settled_tool_ended_capture.clone();
            Box::pin(async move {
                match &event {
                    AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                        if tool_call_id == "call-1" {
                            settled_tool_ended_capture.store(true, Ordering::SeqCst);
                        }
                        events_capture.lock().await.push(event);
                    }
                    AgentEvent::ToolExecutionUpdate { .. } => {
                        events_capture.lock().await.push(event);
                    }
                    _ => {}
                }
            })
        }))
        .await;

    let prompt_task = tokio::spawn({
        let agent = agent.clone();
        async move { agent.prompt_text("run tools", None).await }
    });

    while !slow_started.load(Ordering::SeqCst) || !settled_tool_ended.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }
    let count_before_late_update = events.lock().await.len();

    if let Some(cb) = settled_update_tx.lock().await.clone() {
        cb(AgentToolResult {
            content: vec![ToolResultContent::Text(elph_ai::TextContent::new("late"))],
            details: json!({ "status": "late" }),
            terminate: None,
        });
    }
    tokio::task::yield_now().await;
    assert_eq!(events.lock().await.len(), count_before_late_update);

    release_slow.store(true, Ordering::SeqCst);
    prompt_task.await.expect("prompt task").expect("prompt");
    assert!(
        events
            .lock()
            .await
            .iter()
            .all(|event| !matches!(event, AgentEvent::ToolExecutionUpdate { .. }))
    );
}

#[tokio::test]
async fn agent_legacy_prepare_next_turn_receives_abort_signal() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("noop", json!({}), Some("tool-1".into()))],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let saw_abort_signal = Arc::new(tokio::sync::Mutex::new(false));
    let saw_abort_signal_capture = saw_abort_signal.clone();
    let prepare_next_turn_legacy: elph_agent::PrepareNextTurnLegacyFn = Arc::new(move |signal| {
        let saw_abort_signal_capture = saw_abort_signal_capture.clone();
        Box::pin(async move {
            *saw_abort_signal_capture.lock().await = signal.is_some();
            None
        })
    });

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            tools: Some(vec![noop_tool()]),
            ..Default::default()
        }),
        stream_fn: Some(faux_stream_fn(&faux)),
        prepare_next_turn_legacy: Some(prepare_next_turn_legacy),
        ..Default::default()
    });

    agent.prompt_text("start", None).await.expect("prompt");
    assert!(*saw_abort_signal.lock().await);
}
