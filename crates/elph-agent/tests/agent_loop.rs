mod common;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex as ParkingMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use elph_agent::{
    AgentContext, AgentEvent, AgentMessage, AgentTool, AgentToolResult, BeforeToolCallResult, CustomAgentMessage,
    ToolExecutionMode, llm_message_to_agent, run_agent_loop, run_agent_loop_continue, simple_tool,
};
use elph_ai::{
    FauxResponseStep, Message, StopReason, Tool, UserContent, faux_assistant_message, faux_provider, faux_text,
    faux_tool_call,
};
use serde_json::json;
use tokio::sync::{Mutex, Notify};

use common::{base_loop_config, capture_events, empty_context, faux_stream_fn, test_context, user_agent_message};

fn event_type_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::AgentStart => "agent_start",
        AgentEvent::AgentEnd { .. } => "agent_end",
        AgentEvent::TurnStart => "turn_start",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::MessageStart { .. } => "message_start",
        AgentEvent::MessageUpdate { .. } => "message_update",
        AgentEvent::MessageEnd { .. } => "message_end",
        AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        AgentEvent::PlanProposed { .. } => "plan_proposed",
        AgentEvent::PlanConfirmationRequired { .. } => "plan_confirmation_required",
    }
}

fn value_echo_tool<F, Fut>(execute: F) -> AgentTool
where
    F: Fn(String, serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<AgentToolResult>> + Send + 'static,
{
    let execute_fn = move |id: String,
                           args: serde_json::Value|
          -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
        Box::pin(execute(id, args))
    };
    simple_tool(
        Tool {
            name: "echo".into(),
            description: "Echo tool".into(),
            parameters: json!({
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"]
            }),
        },
        "Echo",
        execute_fn,
    )
}

fn value_echo_tool_with_mode(
    execution_mode: Option<ToolExecutionMode>,
    execute: impl Fn(String, serde_json::Value) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
    + Send
    + Sync
    + 'static,
) -> AgentTool {
    let mut tool = value_echo_tool(execute);
    tool.execution_mode = execution_mode;
    tool
}

fn two_step_tool_then_text_responses(
    faux: &elph_ai::FauxProviderHandle,
    tool_calls: Vec<elph_ai::AssistantContentBlock>,
) {
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(tool_calls, Some(StopReason::ToolUse))),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);
}

#[tokio::test]
async fn run_agent_loop_completes_text_response() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("hello from faux")],
        None,
    ))]);

    let (emit, events) = capture_events();
    let prompts = vec![user_agent_message("hi")];
    let context = test_context("test");

    let new_messages = run_agent_loop(prompts, context, base_loop_config(model, faux_stream_fn(&faux)), emit, None)
        .await
        .expect("agent loop");

    assert_eq!(new_messages.len(), 2);
    assert_eq!(new_messages[0].role(), "user");
    assert_eq!(new_messages[1].role(), "assistant");

    let recorded = events.lock().await;
    let event_types: Vec<_> = recorded.iter().map(event_type_name).collect();
    for expected in [
        "agent_start",
        "turn_start",
        "message_start",
        "message_end",
        "message_start",
        "message_end",
        "turn_end",
        "agent_end",
    ] {
        assert!(event_types.contains(&expected), "missing event {expected}");
    }
}

#[tokio::test]
async fn run_agent_loop_handles_custom_message_types_via_convert_to_llm() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Response")],
        None,
    ))]);

    let notification = AgentMessage::Custom(CustomAgentMessage::Custom {
        kind: "notification".into(),
        content: json!("This is a notification"),
        display: false,
        details: None,
        timestamp: 0,
    });

    let converted = Arc::new(Mutex::new(Vec::new()));
    let converted_capture = converted.clone();
    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.convert_to_llm = Arc::new(move |messages| {
        let converted_capture = converted_capture.clone();
        Box::pin(async move {
            let filtered: Vec<Message> = messages
                .into_iter()
                .filter(|message| {
                    !matches!(
                        message,
                        AgentMessage::Custom(CustomAgentMessage::Custom { kind, .. }) if kind == "notification"
                    )
                })
                .filter_map(|message| message.into_llm())
                .filter(|message| matches!(message.role(), "user" | "assistant" | "toolResult"))
                .collect();
            *converted_capture.lock().await = filtered.clone();
            filtered
        })
    });

    let mut context = empty_context();
    context.messages = vec![notification];

    let _ = run_agent_loop(vec![user_agent_message("Hello")], context, config, capture_events().0, None)
        .await
        .expect("agent loop");

    let converted = converted.lock().await;
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role(), "user");
}

#[tokio::test]
async fn run_agent_loop_applies_transform_context_before_convert_to_llm() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Response")],
        None,
    ))]);

    let transformed = Arc::new(Mutex::new(Vec::new()));
    let converted = Arc::new(Mutex::new(Vec::new()));
    let transformed_capture = transformed.clone();
    let converted_capture = converted.clone();

    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.transform_context = Some(Arc::new(move |messages, _| {
        let transformed_capture = transformed_capture.clone();
        Box::pin(async move {
            let pruned: Vec<AgentMessage> = messages
                .into_iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            *transformed_capture.lock().await = pruned.clone();
            Ok(pruned)
        })
    }));
    config.convert_to_llm = Arc::new(move |messages| {
        let converted_capture = converted_capture.clone();
        Box::pin(async move {
            let llm: Vec<Message> = messages
                .into_iter()
                .filter_map(|message| message.into_llm())
                .filter(|message| matches!(message.role(), "user" | "assistant" | "toolResult"))
                .collect();
            *converted_capture.lock().await = llm.clone();
            llm
        })
    });

    let mut context = empty_context();
    context.messages = vec![
        user_agent_message("old message 1"),
        llm_message_to_agent(elph_ai::Message::Assistant(faux_assistant_message(
            vec![faux_text("old response 1")],
            None,
        ))),
        user_agent_message("old message 2"),
        llm_message_to_agent(elph_ai::Message::Assistant(faux_assistant_message(
            vec![faux_text("old response 2")],
            None,
        ))),
    ];

    let _ = run_agent_loop(
        vec![user_agent_message("new message")],
        context,
        config,
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(transformed.lock().await.len(), 2);
    assert_eq!(converted.lock().await.len(), 2);
}

#[tokio::test]
async fn run_agent_loop_executes_tool_and_continues() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    two_step_tool_then_text_responses(&faux, vec![faux_tool_call("echo", json!({ "value": "ping" }), None)]);

    let executed = Arc::new(Mutex::new(Vec::new()));
    let executed_capture = executed.clone();
    let tool = value_echo_tool_with_mode(None, move |_id, args| {
        let executed_capture = executed_capture.clone();
        Box::pin(async move {
            let value = args
                .get("value")
                .or_else(|| args.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            executed_capture.lock().await.push(value.clone());
            Ok(AgentToolResult::text(format!("echoed: {value}")))
        })
    });

    let (emit, events) = capture_events();
    let prompts = vec![user_agent_message("echo something")];
    let config = base_loop_config(model, faux_stream_fn(&faux));
    let mut context = empty_context();
    context.tools = vec![tool];

    let _ = run_agent_loop(prompts, context, config, emit, None)
        .await
        .expect("agent loop");

    assert_eq!(*executed.lock().await, vec!["ping".to_string()]);

    let recorded = events.lock().await;
    assert!(
        recorded
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolExecutionStart { .. }))
    );
    let tool_end = recorded
        .iter()
        .find(|e| matches!(e, AgentEvent::ToolExecutionEnd { .. }))
        .expect("tool end");
    if let AgentEvent::ToolExecutionEnd { is_error, .. } = tool_end {
        assert!(!is_error);
    }
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

    let executed = Arc::new(Mutex::new(Vec::new()));
    let executed_capture = executed.clone();
    let tool = value_echo_tool(move |_id, args| {
        let executed_capture = executed_capture.clone();
        Box::pin(async move {
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            executed_capture.lock().await.push(value);
            Ok(AgentToolResult::text("ok"))
        })
    });

    let (emit, events) = capture_events();
    let mut context = empty_context();
    context.messages = vec![user_agent_message("go")];
    context.tools = vec![tool];

    let new_messages = run_agent_loop(Vec::new(), context, base_loop_config(model, faux_stream_fn(&faux)), emit, None)
        .await
        .expect("agent loop");

    assert!(executed.lock().await.is_empty());

    let recorded = events.lock().await;
    let tool_end = recorded
        .iter()
        .find(|e| matches!(e, AgentEvent::ToolExecutionEnd { .. }))
        .expect("tool end");
    if let AgentEvent::ToolExecutionEnd {
        is_error: true, result, ..
    } = tool_end
    {
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                elph_agent::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("output token limit"));
    } else {
        panic!("expected error tool end");
    }

    assert_eq!(new_messages.last().map(|m| m.role()), Some("assistant"));
}

#[tokio::test]
async fn run_agent_loop_executes_mutated_before_tool_call_args_without_revalidation() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    two_step_tool_then_text_responses(
        &faux,
        vec![faux_tool_call(
            "echo",
            json!({ "value": "hello" }),
            Some("tool-1".into()),
        )],
    );

    let executed = Arc::new(Mutex::new(Vec::new()));
    let executed_capture = executed.clone();
    let tool = value_echo_tool(move |_id, args| {
        let executed_capture = executed_capture.clone();
        Box::pin(async move {
            let value = args.get("value").cloned().unwrap_or(json!(null));
            executed_capture.lock().await.push(value);
            Ok(AgentToolResult::text("ok"))
        })
    });

    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.before_tool_call = Some(Arc::new(move |ctx, _| {
        let mut args = ctx.args.clone();
        if let Some(obj) = args.as_object_mut() {
            obj.insert("value".into(), json!(123));
        }
        Box::pin(async move {
            Some(BeforeToolCallResult {
                block: false,
                reason: None,
                args: Some(args),
            })
        })
    }));

    let mut context = empty_context();
    context.tools = vec![tool];

    let _ = run_agent_loop(
        vec![user_agent_message("echo something")],
        context,
        config,
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(*executed.lock().await, vec![json!(123)]);
}

#[tokio::test]
async fn run_agent_loop_prepares_tool_arguments_for_validation() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    two_step_tool_then_text_responses(
        &faux,
        vec![faux_tool_call(
            "edit",
            json!({ "oldText": "before", "newText": "after" }),
            Some("tool-1".into()),
        )],
    );

    let executed = Arc::new(Mutex::new(Vec::new()));
    let executed_capture = executed.clone();
    let tool = AgentTool {
        tool: Tool {
            name: "edit".into(),
            description: "Edit tool".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "oldText": { "type": "string" },
                                "newText": { "type": "string" }
                            },
                            "required": ["oldText", "newText"]
                        }
                    }
                },
                "required": ["edits"]
            }),
        },
        label: "Edit".into(),
        execution_mode: None,
        prepare_arguments: Some(Arc::new(|args| {
            if let (Some(old_text), Some(new_text)) = (
                args.get("oldText").and_then(|v| v.as_str()),
                args.get("newText").and_then(|v| v.as_str()),
            ) {
                let mut edits = args
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                edits.push(json!({ "oldText": old_text, "newText": new_text }));
                return json!({ "edits": edits });
            }
            args
        })),
        execute: Arc::new({
            let executed_capture = executed_capture.clone();
            move |_id, args, _signal, _on_update| {
                let executed_capture = executed_capture.clone();
                Box::pin(async move {
                    executed_capture
                        .lock()
                        .await
                        .push(args.get("edits").cloned().unwrap_or_default());
                    let count = args
                        .get("edits")
                        .and_then(|v| v.as_array())
                        .map(|edits| edits.len())
                        .unwrap_or(0);
                    Ok(AgentToolResult::text(format!("edited {count}")))
                })
            }
        }),
    };

    let mut context = empty_context();
    context.tools = vec![tool];

    let _ = run_agent_loop(
        vec![user_agent_message("edit something")],
        context,
        base_loop_config(model, faux_stream_fn(&faux)),
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(
        *executed.lock().await,
        vec![json!([{ "oldText": "before", "newText": "after" }])]
    );
}

#[tokio::test]
async fn run_agent_loop_emits_tool_execution_end_in_completion_order_but_persists_results_in_source_order() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("echo", json!({ "value": "first" }), Some("tool-1".into())),
                faux_tool_call("echo", json!({ "value": "second" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let release_first = Arc::new(Notify::new());
    let release = release_first.clone();
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));
    let first_resolved_capture = first_resolved.clone();
    let parallel_observed_capture = parallel_observed.clone();

    let tool = value_echo_tool(move |_id, args| {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let release_first = release_first.clone();
        let first_resolved_capture = first_resolved_capture.clone();
        let parallel_observed_capture = parallel_observed_capture.clone();
        Box::pin(async move {
            if value == "first" {
                release_first.notified().await;
                first_resolved_capture.store(true, Ordering::SeqCst);
            }
            if value == "second" && !first_resolved_capture.load(Ordering::SeqCst) {
                parallel_observed_capture.store(true, Ordering::SeqCst);
            }
            Ok(AgentToolResult::text(format!("echoed: {value}")))
        })
    });

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        release.notify_one();
    });

    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.tool_execution = ToolExecutionMode::Parallel;
    let mut context = empty_context();
    context.tools = vec![tool];

    let (emit, events) = capture_events();
    let _ = run_agent_loop(vec![user_agent_message("echo both")], context, config, emit, None)
        .await
        .expect("agent loop");

    let recorded = events.lock().await;
    let tool_execution_end_ids: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolExecutionEnd { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();
    let tool_result_ids: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } if message.role() == "toolResult" => {
                message.as_llm().and_then(|message| match message {
                    Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    let turn_tool_result_ids: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TurnEnd { tool_results, .. } => Some(
                tool_results
                    .iter()
                    .filter_map(|message| match message {
                        Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();

    assert!(parallel_observed.load(Ordering::SeqCst));
    assert_eq!(tool_execution_end_ids, vec!["tool-2", "tool-1"]);
    assert_eq!(tool_result_ids, vec!["tool-1", "tool-2"]);
    assert_eq!(turn_tool_result_ids, vec!["tool-1", "tool-2"]);
}

#[tokio::test]
async fn run_agent_loop_injects_queued_steering_after_tools() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("echo", json!({ "value": "first" }), Some("tool-1".into())),
                faux_tool_call("echo", json!({ "value": "second" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let executed = Arc::new(Mutex::new(Vec::new()));
    let executed_capture = executed.clone();
    let queued_delivered = Arc::new(AtomicBool::new(false));
    let saw_interrupt = Arc::new(AtomicBool::new(false));
    let llm_calls = Arc::new(AtomicUsize::new(0));

    let tool = value_echo_tool({
        let executed_capture = executed_capture.clone();
        move |_id, args| {
            let executed_capture = executed_capture.clone();
            Box::pin(async move {
                let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
                executed_capture.lock().await.push(value);
                Ok(AgentToolResult::text("ok"))
            })
        }
    });

    let executed_for_steering = executed.clone();
    let queued_delivered_capture = queued_delivered.clone();
    let mut config = base_loop_config(model.clone(), {
        let faux = faux.provider.clone();
        let llm_calls = llm_calls.clone();
        let saw_interrupt = saw_interrupt.clone();
        Arc::new(move |model, context, options| {
            let call = llm_calls.fetch_add(1, Ordering::SeqCst);
            if call == 1 {
                let has_interrupt = context
                    .messages
                    .iter()
                    .any(|message| matches!(message, Message::User { content: UserContent::Text(text), .. } if text == "interrupt"));
                saw_interrupt.store(has_interrupt, Ordering::SeqCst);
            }
            faux.stream_simple(model, context, options)
        })
    });
    config.tool_execution = ToolExecutionMode::Sequential;
    config.get_steering_messages = Some(Arc::new(move || {
        let executed_for_steering = executed_for_steering.clone();
        let queued_delivered_capture = queued_delivered_capture.clone();
        Box::pin(async move {
            if !executed_for_steering.lock().await.is_empty() && !queued_delivered_capture.load(Ordering::SeqCst) {
                queued_delivered_capture.store(true, Ordering::SeqCst);
                return vec![user_agent_message("interrupt")];
            }
            Vec::new()
        })
    }));

    let mut context = empty_context();
    context.tools = vec![tool];

    let (emit, events) = capture_events();
    let _ = run_agent_loop(vec![user_agent_message("start")], context, config, emit, None)
        .await
        .expect("agent loop");

    assert_eq!(*executed.lock().await, vec!["first", "second"]);

    let recorded = events.lock().await;
    let tool_ends: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolExecutionEnd { is_error, .. } => Some(*is_error),
            _ => None,
        })
        .collect();
    assert_eq!(tool_ends, vec![false, false]);

    let message_sequence: Vec<String> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageStart { message } if message.role() == "toolResult" => {
                message.as_llm().and_then(|message| match message {
                    Message::ToolResult { tool_call_id, .. } => Some(format!("tool:{tool_call_id}")),
                    _ => None,
                })
            }
            AgentEvent::MessageStart { message } => message.as_llm().and_then(|message| match message {
                Message::User {
                    content: UserContent::Text(text),
                    ..
                } => Some(text.clone()),
                _ => None,
            }),
            _ => None,
        })
        .collect();

    assert!(message_sequence.contains(&"interrupt".to_string()));
    let interrupt_index = message_sequence.iter().position(|item| item == "interrupt").unwrap();
    let tool1_index = message_sequence.iter().position(|item| item == "tool:tool-1").unwrap();
    let tool2_index = message_sequence.iter().position(|item| item == "tool:tool-2").unwrap();
    assert!(tool1_index < interrupt_index);
    assert!(tool2_index < interrupt_index);
    assert!(saw_interrupt.load(Ordering::SeqCst));
}

#[tokio::test]
async fn run_agent_loop_forces_sequential_when_tool_has_execution_mode_sequential() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("slow", json!({ "value": "first" }), Some("tool-1".into())),
                faux_tool_call("slow", json!({ "value": "second" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let release_first = Arc::new(Notify::new());
    let release = release_first.clone();
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));

    let tool = value_echo_tool_with_mode(Some(ToolExecutionMode::Sequential), {
        let release_first = release_first.clone();
        let first_resolved = first_resolved.clone();
        let parallel_observed = parallel_observed.clone();
        move |_id, args| {
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let release_first = release_first.clone();
            let first_resolved = first_resolved.clone();
            let parallel_observed = parallel_observed.clone();
            Box::pin(async move {
                if value == "first" {
                    release_first.notified().await;
                    first_resolved.store(true, Ordering::SeqCst);
                }
                if value == "second" && !first_resolved.load(Ordering::SeqCst) {
                    parallel_observed.store(true, Ordering::SeqCst);
                }
                Ok(AgentToolResult::text(format!("slow: {value}")))
            })
        }
    });
    let mut tool = tool;
    tool.tool.name = "slow".into();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        release.notify_one();
    });

    let mut context = empty_context();
    context.tools = vec![tool];

    let (emit, events) = capture_events();
    let _ = run_agent_loop(
        vec![user_agent_message("run both")],
        context,
        base_loop_config(model, faux_stream_fn(&faux)),
        emit,
        None,
    )
    .await
    .expect("agent loop");

    assert!(!parallel_observed.load(Ordering::SeqCst));

    let recorded = events.lock().await;
    let tool_result_ids: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } if message.role() == "toolResult" => {
                message.as_llm().and_then(|message| match message {
                    Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(tool_result_ids, vec!["tool-1", "tool-2"]);
}

#[tokio::test]
async fn run_agent_loop_forces_sequential_when_one_tool_is_sequential() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("slow", json!({ "value": "a" }), Some("tool-1".into())),
                faux_tool_call("fast", json!({ "value": "b" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let release_slow = Arc::new(Notify::new());
    let release = release_slow.clone();
    let execution_order = Arc::new(Mutex::new(Vec::new()));

    let slow_tool = value_echo_tool_with_mode(Some(ToolExecutionMode::Sequential), {
        let release_slow = release_slow.clone();
        let execution_order = execution_order.clone();
        move |_id, args| {
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let release_slow = release_slow.clone();
            let execution_order = execution_order.clone();
            Box::pin(async move {
                execution_order.lock().await.push(format!("slow:{value}"));
                if value == "a" {
                    release_slow.notified().await;
                }
                Ok(AgentToolResult::text(format!("slow: {value}")))
            })
        }
    });
    let mut slow_tool = slow_tool;
    slow_tool.tool.name = "slow".into();

    let fast_tool = value_echo_tool_with_mode(None, {
        let execution_order = execution_order.clone();
        move |_id, args| {
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let execution_order = execution_order.clone();
            Box::pin(async move {
                execution_order.lock().await.push(format!("fast:{value}"));
                Ok(AgentToolResult::text(format!("fast: {value}")))
            })
        }
    });
    let mut fast_tool = fast_tool;
    fast_tool.tool.name = "fast".into();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        release.notify_one();
    });

    let mut context = empty_context();
    context.tools = vec![slow_tool, fast_tool];

    let _ = run_agent_loop(
        vec![user_agent_message("run both")],
        context,
        base_loop_config(model, faux_stream_fn(&faux)),
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    let order = execution_order.lock().await;
    assert_eq!(order.first().map(String::as_str), Some("slow:a"));
    assert!(order.iter().any(|entry| entry == "fast:b"));
}

#[tokio::test]
async fn run_agent_loop_allows_parallel_when_all_tools_have_execution_mode_parallel() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("echo", json!({ "value": "first" }), Some("tool-1".into())),
                faux_tool_call("echo", json!({ "value": "second" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let release_first = Arc::new(Notify::new());
    let release = release_first.clone();
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));

    let tool = value_echo_tool_with_mode(Some(ToolExecutionMode::Parallel), {
        let release_first = release_first.clone();
        let first_resolved = first_resolved.clone();
        let parallel_observed = parallel_observed.clone();
        move |_id, args| {
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let release_first = release_first.clone();
            let first_resolved = first_resolved.clone();
            let parallel_observed = parallel_observed.clone();
            Box::pin(async move {
                if value == "first" {
                    release_first.notified().await;
                    first_resolved.store(true, Ordering::SeqCst);
                }
                if value == "second" && !first_resolved.load(Ordering::SeqCst) {
                    parallel_observed.store(true, Ordering::SeqCst);
                }
                Ok(AgentToolResult::text(format!("echoed: {value}")))
            })
        }
    });

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        release.notify_one();
    });

    let mut context = empty_context();
    context.tools = vec![tool];

    let _ = run_agent_loop(
        vec![user_agent_message("echo both")],
        context,
        base_loop_config(model, faux_stream_fn(&faux)),
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert!(parallel_observed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn run_agent_loop_uses_prepare_next_turn_snapshot_before_continuing() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "echo",
                json!({ "value": "hello" }),
                Some("tool-1".into()),
            )],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let llm_calls = Arc::new(AtomicUsize::new(0));
    let second_turn_prompt = Arc::new(ParkingMutex::new(String::new()));
    let prepared = Arc::new(AtomicBool::new(false));

    let tool = value_echo_tool(|_id, args| {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
        Box::pin(async move { Ok(AgentToolResult::text(format!("echoed: {value}"))) })
    });

    let mut config = base_loop_config(model.clone(), {
        let faux = faux.provider.clone();
        let llm_calls = llm_calls.clone();
        let second_turn_prompt = second_turn_prompt.clone();
        Arc::new(move |model, context, options| {
            let call = llm_calls.fetch_add(1, Ordering::SeqCst);
            if call == 1 {
                *second_turn_prompt.lock() = context.system_prompt.clone().unwrap_or_default();
            }
            faux.stream_simple(model, context, options)
        })
    });
    let prepared_capture = prepared.clone();
    config.prepare_next_turn = Some(Arc::new(move |ctx| {
        let prepared_capture = prepared_capture.clone();
        Box::pin(async move {
            if prepared_capture.swap(true, Ordering::SeqCst) {
                return None;
            }
            Some(elph_agent::AgentLoopTurnUpdate {
                context: Some(AgentContext {
                    system_prompt: "second prompt".into(),
                    messages: ctx.context.messages.clone(),
                    tools: ctx.context.tools.clone(),
                }),
                model: None,
                thinking_level: None,
            })
        })
    }));

    let mut context = empty_context();
    context.system_prompt = "first prompt".into();
    context.tools = vec![tool];

    let _ = run_agent_loop(
        vec![user_agent_message("echo something")],
        context,
        config,
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(llm_calls.load(Ordering::SeqCst), 2);
    assert_eq!(*second_turn_prompt.lock(), "second prompt");
}

#[tokio::test]
async fn run_agent_loop_stops_after_turn_when_should_stop_after_turn_returns_true() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "echo",
                json!({ "value": "hello" }),
                Some("tool-1".into()),
            )],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("should not run")], None)),
    ]);

    let executed = Arc::new(Mutex::new(Vec::new()));
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let steering_polls = Arc::new(AtomicUsize::new(0));
    let follow_up_polls = Arc::new(AtomicUsize::new(0));
    let callback_tool_result_ids = Arc::new(Mutex::new(Vec::new()));
    let callback_context_roles = Arc::new(Mutex::new(Vec::new()));

    let tool = value_echo_tool({
        let executed = executed.clone();
        move |_id, args| {
            let executed = executed.clone();
            Box::pin(async move {
                let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
                executed.lock().await.push(value);
                Ok(AgentToolResult::text("ok"))
            })
        }
    });

    let steering_polls_capture = steering_polls.clone();
    let follow_up_polls_capture = follow_up_polls.clone();
    let callback_tool_result_ids_capture = callback_tool_result_ids.clone();
    let callback_context_roles_capture = callback_context_roles.clone();
    let mut config = base_loop_config(model.clone(), {
        let faux = faux.provider.clone();
        let llm_calls = llm_calls.clone();
        Arc::new(move |model, context, options| {
            llm_calls.fetch_add(1, Ordering::SeqCst);
            faux.stream_simple(model, context, options)
        })
    });
    config.get_steering_messages = Some(Arc::new(move || {
        let steering_polls_capture = steering_polls_capture.clone();
        Box::pin(async move {
            steering_polls_capture.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        })
    }));
    config.get_follow_up_messages = Some(Arc::new(move || {
        let follow_up_polls_capture = follow_up_polls_capture.clone();
        Box::pin(async move {
            follow_up_polls_capture.fetch_add(1, Ordering::SeqCst);
            vec![user_agent_message("follow up should stay queued")]
        })
    }));
    config.should_stop_after_turn = Some(Arc::new(move |ctx| {
        let callback_tool_result_ids_capture = callback_tool_result_ids_capture.clone();
        let callback_context_roles_capture = callback_context_roles_capture.clone();
        Box::pin(async move {
            *callback_tool_result_ids_capture.lock().await = ctx
                .tool_results
                .iter()
                .filter_map(|message| match message {
                    Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
                    _ => None,
                })
                .collect();
            *callback_context_roles_capture.lock().await = ctx
                .context
                .messages
                .iter()
                .map(|message| message.role().to_string())
                .collect();
            true
        })
    }));

    let mut context = empty_context();
    context.tools = vec![tool];

    let (emit, events) = capture_events();
    let new_messages = run_agent_loop(vec![user_agent_message("echo something")], context, config, emit, None)
        .await
        .expect("agent loop");

    let recorded = events.lock().await;
    let event_types: Vec<_> = recorded.iter().map(event_type_name).collect();

    assert_eq!(llm_calls.load(Ordering::SeqCst), 1);
    assert_eq!(*executed.lock().await, vec!["hello"]);
    assert_eq!(steering_polls.load(Ordering::SeqCst), 1);
    assert_eq!(follow_up_polls.load(Ordering::SeqCst), 0);
    assert_eq!(*callback_tool_result_ids.lock().await, vec!["tool-1"]);
    assert_eq!(*callback_context_roles.lock().await, vec!["user", "assistant", "toolResult"]);
    assert_eq!(
        new_messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult"]
    );
    let expected_without_updates = [
        "agent_start",
        "turn_start",
        "message_start",
        "message_end",
        "message_start",
        "message_end",
        "tool_execution_start",
        "tool_execution_end",
        "message_start",
        "message_end",
        "turn_end",
        "agent_end",
    ];
    let filtered: Vec<_> = event_types
        .iter()
        .copied()
        .filter(|event| *event != "message_update")
        .collect();
    assert_eq!(filtered, expected_without_updates);
}

#[tokio::test]
async fn run_agent_loop_stops_after_tool_batch_when_all_tools_terminate() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_tool_call(
            "echo",
            json!({ "value": "hello" }),
            Some("tool-1".into()),
        )],
        Some(StopReason::ToolUse),
    ))]);

    let tool = value_echo_tool(|_id, args| {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(format!(
                    "echoed: {value}"
                )))],
                details: json!({ "value": value }),
                added_tool_names: None,
                terminate: Some(true),
            })
        })
    });

    let mut context = empty_context();
    context.tools = vec![tool];

    let (emit, events) = capture_events();
    let new_messages = run_agent_loop(
        vec![user_agent_message("echo something")],
        context,
        base_loop_config(model, faux_stream_fn(&faux)),
        emit,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(
        new_messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult"]
    );
    let turn_ends = events
        .lock()
        .await
        .iter()
        .filter(|event| matches!(event, AgentEvent::TurnEnd { .. }))
        .count();
    assert_eq!(turn_ends, 1);
}

#[tokio::test]
async fn run_agent_loop_continues_after_parallel_tools_when_not_all_terminate() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("echo", json!({ "value": "first" }), Some("tool-1".into())),
                faux_tool_call("echo", json!({ "value": "second" }), Some("tool-2".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("done")], None)),
    ]);

    let tool = value_echo_tool(|_id, args| {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(format!(
                    "echoed: {value}"
                )))],
                details: json!({ "value": value }),
                added_tool_names: None,
                terminate: Some(value == "first"),
            })
        })
    });

    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.tool_execution = ToolExecutionMode::Parallel;
    let mut context = empty_context();
    context.tools = vec![tool];

    let new_messages = run_agent_loop(vec![user_agent_message("echo both")], context, config, capture_events().0, None)
        .await
        .expect("agent loop");

    assert_eq!(
        new_messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult", "toolResult", "assistant"]
    );
}

#[tokio::test]
async fn run_agent_loop_allows_after_tool_call_to_mark_batch_terminating() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_tool_call(
            "echo",
            json!({ "value": "hello" }),
            Some("tool-1".into()),
        )],
        Some(StopReason::ToolUse),
    ))]);

    let llm_calls = Arc::new(AtomicUsize::new(0));
    let tool = value_echo_tool(|_id, args| {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
        Box::pin(async move { Ok(AgentToolResult::text(format!("echoed: {value}"))) })
    });

    let mut config = base_loop_config(model.clone(), {
        let faux = faux.provider.clone();
        let llm_calls = llm_calls.clone();
        Arc::new(move |model, context, options| {
            llm_calls.fetch_add(1, Ordering::SeqCst);
            faux.stream_simple(model, context, options)
        })
    });
    config.after_tool_call = Some(Arc::new(|_ctx, _| {
        Box::pin(async move {
            Some(elph_agent::AfterToolCallResult {
                content: None,
                details: None,
                is_error: None,
                added_tool_names: None,
                terminate: Some(true),
            })
        })
    }));

    let mut context = empty_context();
    context.tools = vec![tool];

    let _ = run_agent_loop(
        vec![user_agent_message("echo something")],
        context,
        config,
        capture_events().0,
        None,
    )
    .await
    .expect("agent loop");

    assert_eq!(llm_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
#[should_panic(expected = "Cannot continue: no messages in context")]
async fn run_agent_loop_continue_panics_when_no_messages() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    let _ = run_agent_loop_continue(
        empty_context(),
        base_loop_config(model, faux_stream_fn(&faux)),
        capture_events().0,
        None,
    )
    .await;
}

#[tokio::test]
async fn run_agent_loop_continue_without_user_message_events() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Response")],
        None,
    ))]);

    let mut context = empty_context();
    context.messages = vec![user_agent_message("Hello")];

    let (emit, events) = capture_events();
    let new_messages = run_agent_loop_continue(context, base_loop_config(model, faux_stream_fn(&faux)), emit, None)
        .await
        .expect("continue");

    assert_eq!(new_messages.len(), 1);
    assert_eq!(new_messages[0].role(), "assistant");

    let message_end_roles: Vec<_> = events
        .lock()
        .await
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } => Some(message.role().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(message_end_roles, vec!["assistant"]);
}

#[tokio::test]
async fn run_agent_loop_continue_with_custom_message_types() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Response to custom message")],
        None,
    ))]);

    let custom = AgentMessage::Custom(CustomAgentMessage::Custom {
        kind: "custom".into(),
        content: json!("Hook content"),
        display: false,
        details: None,
        timestamp: 0,
    });

    let mut context = empty_context();
    context.messages = vec![custom];

    let mut config = base_loop_config(model, faux_stream_fn(&faux));
    config.convert_to_llm = Arc::new(|messages| {
        Box::pin(async move {
            messages
                .into_iter()
                .map(|message| match message {
                    AgentMessage::Custom(CustomAgentMessage::Custom { content, timestamp, .. }) => {
                        let text = content
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| content.to_string());
                        Message::User {
                            content: UserContent::Text(text),
                            timestamp,
                        }
                    }
                    AgentMessage::Llm(message) => *message,
                    _ => Message::User {
                        content: UserContent::Text(String::new()),
                        timestamp: 0,
                    },
                })
                .filter(|message| matches!(message.role(), "user" | "assistant" | "toolResult"))
                .collect()
        })
    });

    let new_messages = run_agent_loop_continue(context, config, capture_events().0, None)
        .await
        .expect("continue");

    assert_eq!(new_messages.len(), 1);
    assert_eq!(new_messages[0].role(), "assistant");
}
