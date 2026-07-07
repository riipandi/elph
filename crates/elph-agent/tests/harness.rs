//! Agent harness integration tests.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use elph_ai::api::faux::{FauxModelDefinition, RegisterFauxProviderOptions};

use elph_agent::session::types::SessionTreeEntry;
use elph_agent::{
    AgentHarness, AgentHarnessErrorCode, AgentHarnessEvent, AgentHarnessOptions, AgentHarnessOwnEvent,
    AgentHarnessPhase, AgentHarnessResources, AgentThinkingLevel, AgentTool, BranchSummarySummary,
    CustomMessageContent, InMemorySessionStorage, LocalExecutionEnv, NavigateTreeOptions, QueueMode, Session,
    SessionBeforeTreeResult, Skill, SystemPrompt, ToolResultPatch, create_custom_message, llm_message_to_agent,
    simple_tool,
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

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
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

fn get_current_time_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "get_current_time".into(),
            description: "Return the current time".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
            }),
        },
        "Get Current Time",
        |_, _| Box::pin(async { Ok(elph_agent::AgentToolResult::text("12:00")) }),
    )
}

#[derive(Clone)]
struct CapturedRequest {
    model_id: String,
    system_prompt: String,
    tools: Vec<String>,
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_abort_clears_queues_preserves_next_turn() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let release = Arc::new(AtomicBool::new(false));
    let aborted_signal = Arc::new(StdMutex::new(None::<bool>));
    let second_request_text = Arc::new(StdMutex::new(Vec::new()));

    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let release = release.clone();
            let aborted_signal = aborted_signal.clone();
            Arc::new(move |_context, options, _, _| {
                let signal = options.and_then(|o| o.signal.clone());
                loop {
                    let cancelled = signal.as_ref().is_some_and(|token| token.is_cancelled());
                    *aborted_signal.lock().expect("aborted lock") = Some(cancelled);
                    if cancelled || release.load(Ordering::SeqCst) {
                        return faux_assistant_message(vec![faux_text("aborted-ish")], None);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        }),
        FauxResponseStep::Factory({
            let second_request_text = second_request_text.clone();
            Arc::new(move |context, _, _, _| {
                second_request_text
                    .lock()
                    .expect("second request lock")
                    .extend(user_texts(&context.messages));
                faux_assistant_message(vec![faux_text("second")], None)
            })
        }),
    ]);

    let harness = Arc::new(make_harness(&faux, models, env, HarnessOptions::default()));
    let queue_updates = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let queue_updates_clone = queue_updates.clone();
    harness
        .subscribe(move |event, _| {
            let queue_updates = queue_updates_clone.clone();
            async move {
                if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::QueueUpdate(update)) = event {
                    queue_updates.lock().await.push((
                        update.steer.len(),
                        update.follow_up.len(),
                        update.next_turn.len(),
                    ));
                }
            }
        })
        .await;

    let harness_for_prompt = harness.clone();
    let first_prompt = tokio::spawn(async move { harness_for_prompt.prompt("first", None).await });
    tokio::time::sleep(Duration::from_millis(10)).await;
    harness.steer("steer", None).await.expect("steer");
    harness.follow_up("follow", None).await.expect("follow up");
    harness.next_turn("next", None).await.expect("next turn");
    let abort_result = harness.abort().await.expect("abort");
    release.store(true, Ordering::SeqCst);
    let _ = first_prompt.await.expect("join first prompt");
    harness.prompt("second", None).await.expect("second prompt");

    assert_eq!(abort_result.cleared_steer.len(), 1);
    assert_eq!(abort_result.cleared_follow_up.len(), 1);
    assert!(aborted_signal.lock().expect("aborted lock").unwrap_or(false));
    let updates = queue_updates.lock().await;
    assert!(updates.contains(&(0, 0, 1)));
    assert_eq!(
        second_request_text.lock().expect("second request lock").clone(),
        vec!["first".to_string(), "next".to_string(), "second".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_save_point_refreshes_config_at_tool_execution() {
    let (_temp, env) = test_env();
    let faux = common::new_faux_with_options(RegisterFauxProviderOptions {
        models: Some(vec![
            FauxModelDefinition {
                id: "first".into(),
                name: Some("first".into()),
                reasoning: Some(true),
                input: None,
                context_window: None,
                max_tokens: None,
            },
            FauxModelDefinition {
                id: "second".into(),
                name: Some("second".into()),
                reasoning: Some(true),
                input: None,
                context_window: None,
                max_tokens: None,
            },
        ]),
        ..Default::default()
    });
    let models = faux_models(&faux);
    let second_model = faux
        .provider
        .get_models()
        .iter()
        .find(|model| model.id == "second")
        .expect("second model");

    let captured = Arc::new(StdMutex::new(Vec::<CapturedRequest>::new()));
    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let captured = captured.clone();
            Arc::new(move |context, _, _, model| {
                captured.lock().expect("capture lock").push(CapturedRequest {
                    model_id: model.id.clone(),
                    system_prompt: context.system_prompt.clone().unwrap_or_default(),
                    tools: context
                        .tools
                        .as_ref()
                        .map(|tools| tools.iter().map(|tool| tool.name.clone()).collect())
                        .unwrap_or_default(),
                });
                faux_assistant_message(
                    vec![faux_tool_call(
                        "calculate",
                        json!({ "expression": "1 + 1" }),
                        Some("call-1".into()),
                    )],
                    Some(StopReason::ToolUse),
                )
            })
        }),
        FauxResponseStep::Factory({
            let captured = captured.clone();
            Arc::new(move |context, _, _, model| {
                captured.lock().expect("capture lock").push(CapturedRequest {
                    model_id: model.id.clone(),
                    system_prompt: context.system_prompt.clone().unwrap_or_default(),
                    tools: context
                        .tools
                        .as_ref()
                        .map(|tools| tools.iter().map(|tool| tool.name.clone()).collect())
                        .unwrap_or_default(),
                });
                faux_assistant_message(vec![faux_text("done")], None)
            })
        }),
    ]);

    let resources = AgentHarnessResources {
        skills: vec![Skill {
            name: "prompt".into(),
            description: "prompt".into(),
            content: "first prompt".into(),
            file_path: "/skills/prompt".into(),
            disable_model_invocation: false,
        }],
        ..Default::default()
    };
    let system_prompt = SystemPrompt::Dynamic(Arc::new(|ctx| {
        Box::pin(async move {
            ctx.resources
                .skills
                .first()
                .map(|skill| skill.content.clone())
                .unwrap_or_else(|| "missing prompt".to_string())
        })
    }));

    let harness = Arc::new(
        AgentHarness::new(AgentHarnessOptions {
            env,
            session: Session::new(InMemorySessionStorage::new(None).expect("session")),
            models,
            tools: vec![calculate_tool()],
            resources,
            system_prompt,
            stream_options: Default::default(),
            model: faux.provider.get_models()[0].clone(),
            thinking_level: AgentThinkingLevel::Off,
            active_tool_names: vec!["calculate".into()],
            steering_mode: QueueMode::OneAtATime,
            follow_up_mode: QueueMode::OneAtATime,
        })
        .expect("harness"),
    );

    let harness_for_sub = harness.clone();
    let second_model_for_sub = second_model.clone();
    harness
        .subscribe(move |event, _| {
            let harness = harness_for_sub.clone();
            let second_model = second_model_for_sub.clone();
            async move {
                if let AgentHarnessEvent::Agent(elph_agent::AgentEvent::ToolExecutionStart { .. }) = event {
                    harness.set_model(second_model).await.expect("set model");
                    harness
                        .set_thinking_level(AgentThinkingLevel::High)
                        .await
                        .expect("set thinking");
                    harness
                        .set_resources(AgentHarnessResources {
                            skills: vec![Skill {
                                name: "prompt".into(),
                                description: "prompt".into(),
                                content: "second prompt".into(),
                                file_path: "/skills/prompt".into(),
                                disable_model_invocation: false,
                            }],
                            ..Default::default()
                        })
                        .await
                        .expect("set resources");
                    harness
                        .set_tools(
                            vec![calculate_tool(), get_current_time_tool()],
                            Some(vec!["get_current_time".into()]),
                        )
                        .await
                        .expect("set tools");
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let captured = captured.lock().expect("capture lock").clone();
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].model_id, "first");
    assert_eq!(captured[0].system_prompt, "first prompt");
    assert_eq!(captured[0].tools, vec!["calculate".to_string()]);
    assert_eq!(captured[1].model_id, "second");
    assert_eq!(captured[1].system_prompt, "second prompt");
    assert_eq!(captured[1].tools, vec!["get_current_time".to_string()]);
    assert_eq!(harness.get_thinking_level().await, AgentThinkingLevel::High);
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_orders_pending_listener_writes_after_agent_messages() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let harness = Arc::new(make_harness(&faux, models, env, HarnessOptions::default()));
    let wrote_pending = Arc::new(tokio::sync::Mutex::new(false));
    let wrote_pending_clone = wrote_pending.clone();
    let harness_for_sub = harness.clone();
    harness
        .subscribe(move |event, _| {
            let wrote_pending = wrote_pending_clone.clone();
            let harness = harness_for_sub.clone();
            async move {
                if let AgentHarnessEvent::Agent(elph_agent::AgentEvent::MessageEnd { message }) = event
                    && message.role() == "assistant"
                {
                    let mut guard = wrote_pending.lock().await;
                    if !*guard {
                        *guard = true;
                        harness
                            .append_message(create_custom_message(
                                "listener",
                                CustomMessageContent::Text("listener write".into()),
                                true,
                                None,
                                "2026-01-01T00:00:00.000Z",
                            ))
                            .await
                            .expect("append listener message");
                    }
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let roles: Vec<_> = harness
        .session_entries()
        .await
        .into_iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message { message, .. } => Some(message.role().to_string()),
            SessionTreeEntry::CustomMessage { .. } => Some("custom".to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "custom"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_wait_for_idle_waits_for_async_subscribers() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let harness = Arc::new(make_harness(&faux, models, env, HarnessOptions::default()));
    let barrier = Arc::new(tokio::sync::Mutex::new(None::<tokio::sync::oneshot::Receiver<()>>));
    let (barrier_tx, barrier_rx) = tokio::sync::oneshot::channel::<()>();
    *barrier.lock().await = Some(barrier_rx);
    let listener_waiting = Arc::new(AtomicBool::new(false));
    let listener_finished = Arc::new(AtomicBool::new(false));

    let barrier_clone = barrier.clone();
    let listener_waiting_clone = listener_waiting.clone();
    let listener_finished_clone = listener_finished.clone();
    harness
        .subscribe(move |event, _| {
            let barrier = barrier_clone.clone();
            let listener_waiting = listener_waiting_clone.clone();
            let listener_finished = listener_finished_clone.clone();
            async move {
                if matches!(event, AgentHarnessEvent::Agent(elph_agent::AgentEvent::AgentEnd { .. }))
                    && let Some(rx) = barrier.lock().await.take()
                {
                    listener_waiting.store(true, Ordering::SeqCst);
                    let _ = rx.await;
                    listener_finished.store(true, Ordering::SeqCst);
                }
            }
        })
        .await;

    let harness_for_prompt = harness.clone();
    let prompt_task = tokio::spawn(async move { harness_for_prompt.prompt("hello", None).await });

    while harness.phase().await == AgentHarnessPhase::Idle {
        tokio::task::yield_now().await;
    }

    let idle_resolved = Arc::new(AtomicBool::new(false));
    let idle_resolved_for_task = idle_resolved.clone();
    let harness_for_idle = harness.clone();
    let idle_task = tokio::spawn(async move {
        harness_for_idle.wait_for_idle().await.expect("wait for idle");
        idle_resolved_for_task.store(true, Ordering::SeqCst);
    });

    while !listener_waiting.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }
    assert!(!idle_resolved.load(Ordering::SeqCst));
    assert!(!listener_finished.load(Ordering::SeqCst));

    barrier_tx.send(()).expect("release barrier");
    prompt_task.await.expect("prompt task").expect("prompt");
    idle_task.await.expect("idle task");
    assert!(idle_resolved.load(Ordering::SeqCst));
    assert!(listener_finished.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_validates_constructor_tool_names() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();
    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));

    let missing = AgentHarness::new(AgentHarnessOptions {
        env: env.clone(),
        session: session.clone(),
        models: models.clone(),
        tools: vec![calculate_tool()],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model: model.clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec!["missing".into()],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    });
    assert_eq!(
        missing.err().expect("missing tool error").code,
        AgentHarnessErrorCode::InvalidArgument
    );

    let duplicate_tools = AgentHarness::new(AgentHarnessOptions {
        env: env.clone(),
        session: session.clone(),
        models: models.clone(),
        tools: vec![calculate_tool(), calculate_tool()],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model: model.clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec!["calculate".into()],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    });
    assert_eq!(
        duplicate_tools.err().expect("duplicate tools error").code,
        AgentHarnessErrorCode::InvalidArgument
    );

    let duplicate_active = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools: vec![calculate_tool()],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec!["calculate".into(), "calculate".into()],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    });
    assert_eq!(
        duplicate_active.err().expect("duplicate active error").code,
        AgentHarnessErrorCode::InvalidArgument
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_tools_update_events_and_validation() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let mut inspect = calculate_tool();
    inspect.tool.name = "inspect".into();
    let mut search = calculate_tool();
    search.tool.name = "search".into();

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: vec![inspect.clone(), search.clone()],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model: faux.provider.get_models()[0].clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec!["inspect".into()],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    })
    .expect("harness");

    let updates = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let updates_clone = updates.clone();
    harness
        .subscribe(move |event, _| {
            let updates = updates_clone.clone();
            async move {
                if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::ToolsUpdate(update)) = event {
                    updates.lock().await.push((
                        update.tool_names.clone(),
                        update.previous_tool_names.clone(),
                        update.active_tool_names.clone(),
                        update.previous_active_tool_names.clone(),
                        update.source,
                    ));
                }
            }
        })
        .await;

    assert_eq!(
        harness
            .get_active_tools()
            .await
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>(),
        vec!["inspect".to_string()]
    );

    harness
        .set_active_tools(vec!["search".into()])
        .await
        .expect("set active tools");
    harness
        .set_tools(vec![search.clone()], Some(vec!["search".into()]))
        .await
        .expect("set tools");

    let missing = harness.set_active_tools(vec!["missing".into()]).await;
    assert_eq!(
        missing.expect_err("missing active tool error").code,
        AgentHarnessErrorCode::InvalidArgument
    );
    let duplicate_active = harness.set_active_tools(vec!["search".into(), "search".into()]).await;
    assert_eq!(
        duplicate_active.expect_err("duplicate active tool error").code,
        AgentHarnessErrorCode::InvalidArgument
    );
    let duplicate_tools = harness.set_tools(vec![inspect.clone()], None).await;
    assert_eq!(
        duplicate_tools.expect_err("single tool set error").code,
        AgentHarnessErrorCode::InvalidArgument
    );
    let duplicate_names = harness
        .set_tools(vec![inspect.clone(), inspect], Some(vec!["inspect".into()]))
        .await;
    assert_eq!(
        duplicate_names.expect_err("duplicate tool names error").code,
        AgentHarnessErrorCode::InvalidArgument
    );

    let updates = updates.lock().await.clone();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].2, vec!["search".to_string()]);
    assert_eq!(updates[1].0, vec!["search".to_string()]);
    assert_eq!(
        harness
            .get_active_tools()
            .await
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>(),
        vec!["search".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_resources_update_events_clone_resources() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model: faux.provider.get_models()[0].clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
    })
    .expect("harness");

    let updates = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let updates_clone = updates.clone();
    harness
        .subscribe(move |event, _| {
            let updates = updates_clone.clone();
            async move {
                if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::ResourcesUpdate(update)) = event {
                    updates.lock().await.push((
                        update.resources.skills.first().map(|skill| skill.content.clone()),
                        update
                            .previous_resources
                            .skills
                            .first()
                            .map(|skill| skill.content.clone()),
                    ));
                }
            }
        })
        .await;

    let resources = AgentHarnessResources {
        skills: vec![Skill {
            name: "inspect".into(),
            description: "Inspect things".into(),
            content: "Use inspection tools.".into(),
            file_path: "/skills/inspect/SKILL.md".into(),
            disable_model_invocation: false,
        }],
        prompt_templates: vec![elph_agent::PromptTemplate {
            name: "review".into(),
            description: "Review".into(),
            content: "Review $1".into(),
        }],
    };
    harness.set_resources(resources.clone()).await.expect("set resources");
    harness.set_resources(resources).await.expect("set resources again");

    let resolved = harness.get_resources().await;
    assert_eq!(resolved.skills[0].content, "Use inspection tools.");
    assert_eq!(resolved.prompt_templates[0].content, "Review $1");

    let updates = updates.lock().await.clone();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].0.as_deref(), Some("Use inspection tools."));
    assert_eq!(updates[0].1, None);
    assert_eq!(updates[1].0.as_deref(), Some("Use inspection tools."));
    assert_eq!(updates[1].1.as_deref(), Some("Use inspection tools."));
}

fn user_agent_message(text: &str) -> elph_agent::AgentMessage {
    llm_message_to_agent(Message::User {
        content: UserContent::Text(text.into()),
        timestamp: 0,
    })
}

fn assistant_agent_message(text: &str) -> elph_agent::AgentMessage {
    elph_agent::AgentMessage::Llm(Box::new(Message::Assistant(faux_assistant_message(
        vec![faux_text(text)],
        None,
    ))))
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_session_before_compact_overrides_custom_instructions() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let captured_prompt = Arc::new(StdMutex::new(String::new()));
    let captured_prompt_clone = captured_prompt.clone();
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(move |context, _, _, _| {
        let prompt = context
            .messages
            .iter()
            .filter_map(|message| match message {
                Message::User { content, .. } => match content {
                    UserContent::Text(text) => Some(text.clone()),
                    UserContent::Blocks(_) => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        *captured_prompt_clone.lock().expect("capture lock") = prompt;
        faux_assistant_message(vec![faux_text("## Goal\nCompacted")], None)
    }))]);

    let mut session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    session
        .append_message(user_agent_message(&"old ".repeat(200)))
        .await
        .expect("append old user");
    session
        .append_message(assistant_agent_message("old reply"))
        .await
        .expect("append old assistant");
    session
        .append_message(user_agent_message(&"recent ".repeat(200)))
        .await
        .expect("append recent user");
    session
        .append_message(assistant_agent_message("recent reply"))
        .await
        .expect("append recent assistant");

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
        .on_session_before_compact(|event| {
            let custom_instructions = event.custom_instructions.clone();
            async move {
                assert_eq!(custom_instructions.as_deref(), Some("original"));
                Some(elph_agent::SessionBeforeCompactResult {
                    custom_instructions: Some("hook override".into()),
                    ..Default::default()
                })
            }
        })
        .await;

    harness.compact(Some("original")).await.expect("compact");

    let prompt = captured_prompt.lock().expect("capture lock").clone();
    assert!(
        prompt.contains("Additional focus: hook override"),
        "expected hook override in prompt, got: {prompt}"
    );
    assert!(
        !prompt.contains("Additional focus: original"),
        "expected original instructions to be replaced, got: {prompt}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_session_before_tree_runs_during_navigate_tree() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let mut session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let user1 = session
        .append_message(user_agent_message("branch root"))
        .await
        .expect("user1");
    session
        .append_message(assistant_agent_message("first reply"))
        .await
        .expect("assistant1");
    session
        .append_message(user_agent_message("current path"))
        .await
        .expect("user2");
    session
        .append_message(assistant_agent_message("current reply"))
        .await
        .expect("assistant2");

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

    let hook_target = Arc::new(StdMutex::new(None::<String>));
    let hook_target_clone = hook_target.clone();
    let target_id = user1.clone();
    harness
        .on_session_before_tree(move |event| {
            let hook_target = hook_target_clone.clone();
            let hook_target_id = event.preparation.target_id.clone();
            let user_wants_summary = event.preparation.user_wants_summary;
            async move {
                *hook_target.lock().expect("hook target lock") = Some(hook_target_id);
                assert!(user_wants_summary);
                Some(SessionBeforeTreeResult {
                    summary: Some(BranchSummarySummary {
                        summary: "hook-provided summary".into(),
                        details: None,
                    }),
                    ..Default::default()
                })
            }
        })
        .await;

    let result = harness
        .navigate_tree(
            &target_id,
            Some(NavigateTreeOptions {
                summarize: true,
                ..Default::default()
            }),
        )
        .await
        .expect("navigate tree");

    assert_eq!(
        hook_target.lock().expect("hook target lock").as_deref(),
        Some(target_id.as_str())
    );
    assert!(!result.cancelled);
    let summary_entry = result.summary_entry.expect("summary entry");
    match summary_entry {
        SessionTreeEntry::BranchSummary { summary, .. } => {
            assert_eq!(summary, "hook-provided summary");
        }
        other => panic!("expected branch summary entry, got {other:?}"),
    }
}
