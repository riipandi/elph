//! Agent harness integration tests — ported from pi-agent `agent-harness.test.ts`.

use std::sync::{Arc, Mutex as StdMutex};

use elph_agent::session::types::SessionTreeEntry;
use elph_agent::{
    AgentHarness, AgentHarnessEvent, AgentHarnessOptions, AgentHarnessOwnEvent, AgentHarnessResources,
    AgentThinkingLevel, AgentTool, InMemorySessionStorage, LocalExecutionEnv, QueueMode, Session, SystemPrompt,
    ToolResultPatch, llm_message_to_agent, simple_tool,
};
use elph_ai::{
    ContentBlock, FauxResponseStep, Message, Models, StopReason, Tool, UserContent, builtin_models,
    faux_assistant_message, faux_provider, faux_text, faux_tool_call,
};
use serde_json::json;
use tempfile::TempDir;

fn test_env() -> (TempDir, Arc<LocalExecutionEnv>) {
    let temp = TempDir::new().expect("temp dir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    (temp, env)
}

fn faux_models(faux: &elph_ai::FauxProviderHandle) -> Arc<Models> {
    let mut models = builtin_models(None);
    models.set_provider(faux.provider.clone());
    models.into_arc()
}

fn calculate_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "calculate".into(),
            description: "Evaluate arithmetic".into(),
            parameters: json!({
                "type": "object",
                "properties": { "expression": { "type": "string" } },
                "required": ["expression"]
            }),
        },
        "calculate",
        |_, args| {
            let expression = args
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Box::pin(async move {
                let value = if expression.contains('+') {
                    let parts: Vec<i64> = expression
                        .split('+')
                        .filter_map(|part| part.trim().parse().ok())
                        .collect();
                    parts.iter().sum::<i64>().to_string()
                } else {
                    "0".to_string()
                };
                Ok(elph_agent::AgentToolResult::text(value))
            })
        },
    )
}

fn user_texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.role() == "user")
        .filter_map(|message| match message {
            Message::User { content, .. } => match content {
                UserContent::Text(text) => Some(vec![text.clone()]),
                UserContent::Blocks(blocks) => Some(
                    blocks
                        .iter()
                        .filter_map(|block| match block {
                            elph_ai::ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect(),
                ),
            },
            _ => None,
        })
        .flatten()
        .collect()
}

fn make_harness(
    faux: &elph_ai::FauxProviderHandle,
    models: Arc<Models>,
    env: Arc<LocalExecutionEnv>,
    options: HarnessOptions,
) -> AgentHarness<InMemorySessionStorage> {
    let model = faux.provider.get_models()[0].clone();
    AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: options.tools,
        resources: options.resources,
        system_prompt: options.system_prompt,
        stream_options: Default::default(),
        model,
        thinking_level: options.thinking_level,
        active_tool_names: options.active_tool_names,
        steering_mode: options.steering_mode,
        follow_up_mode: options.follow_up_mode,
    })
    .expect("harness")
}

struct HarnessOptions {
    tools: Vec<AgentTool>,
    resources: AgentHarnessResources,
    system_prompt: SystemPrompt<InMemorySessionStorage>,
    thinking_level: AgentThinkingLevel,
    active_tool_names: Vec<String>,
    steering_mode: QueueMode,
    follow_up_mode: QueueMode,
}

impl Default for HarnessOptions {
    fn default() -> Self {
        Self {
            tools: Vec::new(),
            resources: AgentHarnessResources::default(),
            system_prompt: SystemPrompt::Static("You are helpful.".into()),
            thinking_level: AgentThinkingLevel::Off,
            active_tool_names: Vec::new(),
            steering_mode: QueueMode::OneAtATime,
            follow_up_mode: QueueMode::OneAtATime,
        }
    }
}

#[tokio::test]
async fn harness_exposes_queue_modes() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    let model = faux.provider.get_models()[0].clone();
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model: model.clone(),
        thinking_level: AgentThinkingLevel::High,
        active_tool_names: vec![],
        steering_mode: QueueMode::All,
        follow_up_mode: QueueMode::All,
    })
    .expect("harness");

    assert_eq!(harness.get_model().await.id, model.id);
    assert_eq!(harness.get_thinking_level().await, AgentThinkingLevel::High);
    assert_eq!(harness.get_steering_mode().await, QueueMode::All);
    harness.set_steering_mode(QueueMode::OneAtATime).await;
    harness.set_follow_up_mode(QueueMode::OneAtATime).await;
    assert_eq!(harness.get_steering_mode().await, QueueMode::OneAtATime);
}

#[tokio::test]
async fn harness_drains_steering_one_at_a_time() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    let user_counts = Arc::new(StdMutex::new(Vec::new()));
    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("first")], None)
            })
        }),
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("second")], None)
            })
        }),
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("third")], None)
            })
        }),
    ]);

    let harness = Arc::new(make_harness(
        &faux,
        models,
        env,
        HarnessOptions {
            steering_mode: QueueMode::OneAtATime,
            ..Default::default()
        },
    ));

    let steer_lengths = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let queued = Arc::new(tokio::sync::Mutex::new(false));
    let steer_lengths_clone = steer_lengths.clone();
    let queued_clone = queued.clone();
    let harness_for_sub = harness.clone();
    harness
        .subscribe(move |event, _| {
            let steer_lengths = steer_lengths_clone.clone();
            let queued = queued_clone.clone();
            let harness = harness_for_sub.clone();
            async move {
                match event {
                    AgentHarnessEvent::Own(AgentHarnessOwnEvent::QueueUpdate(update)) => {
                        steer_lengths.lock().await.push(update.steer.len());
                    }
                    AgentHarnessEvent::Agent(elph_agent::AgentEvent::MessageStart { message })
                        if message.role() == "assistant" =>
                    {
                        let mut guard = queued.lock().await;
                        if !*guard {
                            *guard = true;
                            harness.steer("one", None).await.ok();
                            harness.steer("two", None).await.ok();
                        }
                    }
                    _ => {}
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");
    let lengths = steer_lengths.lock().await.clone();
    let counts = user_counts.lock().expect("user counts lock").clone();
    assert_eq!(counts, vec![1, 2, 3]);
    assert!(lengths.contains(&1));
    assert!(lengths.contains(&2));
}

#[tokio::test]
async fn harness_before_agent_start_appends_messages() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    let captured = Arc::new(StdMutex::new(Vec::new()));
    let captured_clone = captured.clone();
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(move |context, _, _, _| {
        captured_clone
            .lock()
            .expect("captured lock")
            .extend(user_texts(&context.messages));
        faux_assistant_message(vec![faux_text("ok")], None)
    }))]);

    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let model = faux.provider.get_models()[0].clone();
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    })
    .expect("harness");

    harness
        .on_before_agent_start(|_| async {
            Some(elph_agent::BeforeAgentStartResult {
                messages: Some(vec![llm_message_to_agent(Message::User {
                    content: UserContent::Text("hook".into()),
                    timestamp: 1,
                })]),
                system_prompt: None,
            })
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");
    let request_text = captured.lock().expect("captured lock").clone();
    assert_eq!(request_text, vec!["hello".to_string(), "hook".to_string()]);
}

#[tokio::test]
async fn harness_tool_result_hook_patches_output() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_tool_call(
            "calculate",
            json!({ "expression": "2 + 2" }),
            Some("call-1".into()),
        )],
        Some(StopReason::ToolUse),
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: vec![calculate_tool()],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec!["calculate".into()],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    })
    .expect("harness");

    let seen_tool_calls = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let seen_tool_calls_clone = seen_tool_calls.clone();
    harness
        .on_tool_call(move |event| {
            let seen_tool_calls = seen_tool_calls_clone.clone();
            let tool_call_id = event.tool_call_id.clone();
            let tool_name = event.tool_name.clone();
            let expression = event.input.get("expression").cloned();
            async move {
                seen_tool_calls.lock().await.push((tool_call_id, tool_name, expression));
                None
            }
        })
        .await;

    harness
        .on_tool_result(|event| {
            let tool_call_id = event.tool_call_id.clone();
            let tool_name = event.tool_name.clone();
            async move {
                assert_eq!(tool_call_id, "call-1");
                assert_eq!(tool_name, "calculate");
                Some(ToolResultPatch {
                    content: Some(vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(
                        "patched result",
                    ))]),
                    details: Some(json!({ "patched": true })),
                    is_error: None,
                    terminate: Some(true),
                })
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let seen = seen_tool_calls.lock().await.clone();
    assert_eq!(
        seen,
        vec![("call-1".to_string(), "calculate".to_string(), Some(json!("2 + 2")))]
    );

    let tool_result = harness
        .session_entries()
        .await
        .into_iter()
        .find_map(|entry| match entry {
            SessionTreeEntry::Message { message, .. } if message.role() == "toolResult" => message.as_llm().cloned(),
            _ => None,
        })
        .expect("tool result entry");

    let Message::ToolResult { content, details, .. } = tool_result else {
        panic!("expected tool result message");
    };
    let text = content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text, "patched result");
    assert_eq!(details, Some(json!({ "patched": true })));
}

#[tokio::test]
async fn harness_drains_follow_up_one_at_a_time() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    let user_counts = Arc::new(StdMutex::new(Vec::new()));
    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("first")], None)
            })
        }),
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("second")], None)
            })
        }),
        FauxResponseStep::Factory({
            let user_counts = user_counts.clone();
            Arc::new(move |context, _, _, _| {
                user_counts
                    .lock()
                    .expect("user counts lock")
                    .push(context.messages.iter().filter(|m| m.role() == "user").count());
                faux_assistant_message(vec![faux_text("third")], None)
            })
        }),
    ]);

    let harness = Arc::new(make_harness(
        &faux,
        models,
        env,
        HarnessOptions {
            follow_up_mode: QueueMode::OneAtATime,
            ..Default::default()
        },
    ));

    let follow_up_lengths = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let queued = Arc::new(tokio::sync::Mutex::new(false));
    let follow_up_lengths_clone = follow_up_lengths.clone();
    let queued_clone = queued.clone();
    let harness_for_sub = harness.clone();
    harness
        .subscribe(move |event, _| {
            let follow_up_lengths = follow_up_lengths_clone.clone();
            let queued = queued_clone.clone();
            let harness = harness_for_sub.clone();
            async move {
                match event {
                    AgentHarnessEvent::Own(AgentHarnessOwnEvent::QueueUpdate(update)) => {
                        follow_up_lengths.lock().await.push(update.follow_up.len());
                    }
                    AgentHarnessEvent::Agent(elph_agent::AgentEvent::MessageStart { message })
                        if message.role() == "assistant" =>
                    {
                        let mut guard = queued.lock().await;
                        if !*guard {
                            *guard = true;
                            harness.follow_up("one", None).await.ok();
                            harness.follow_up("two", None).await.ok();
                        }
                    }
                    _ => {}
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");
    let lengths = follow_up_lengths.lock().await.clone();
    let counts = user_counts.lock().expect("user counts lock").clone();
    assert_eq!(counts, vec![1, 2, 3]);
    assert!(lengths.contains(&1));
    assert!(lengths.contains(&2));
}

#[tokio::test]
async fn harness_settles_context_hook_failures() {
    let (_temp, env) = test_env();
    let faux = faux_provider(Default::default());
    let models = faux_models(&faux);
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("should not be used")],
        None,
    ))]);

    let model = faux.provider.get_models()[0].clone();
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    })
    .expect("harness");

    harness
        .on_context(|_| async {
            Err(elph_agent::AgentHarnessError::new(
                elph_agent::AgentHarnessErrorCode::Hook,
                "context exploded",
            ))
        })
        .await;

    let response = harness.prompt("hello", None).await.expect("prompt");
    assert_eq!(response.stop_reason, StopReason::Error);
    assert_eq!(response.error_message.as_deref(), Some("context exploded"));

    harness.prompt("after failure", None).await.expect("second prompt");

    let roles: Vec<_> = harness
        .session_entries()
        .await
        .into_iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message { message, .. } => Some(message.role().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "user", "assistant"]);
}
