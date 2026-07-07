//! End-to-end agent integration tests.

mod common;

use std::sync::Arc;

use common::{faux_stream_fn, new_faux, new_faux_with_options, user_agent_message};
use elph_agent::{
    Agent, AgentEvent, AgentMessage, AgentOptions, AgentThinkingLevel, AgentTool, PartialAgentState, ToolExecutionMode,
    llm_message_to_agent, simple_tool,
};
use elph_agent::{
    AgentHarness, AgentHarnessOptions, AgentHarnessResources, InMemorySessionStorage, LocalExecutionEnv, Session,
    SystemPrompt,
};
use elph_ai::api::faux::{FauxModelDefinition, RegisterFauxProviderOptions};
use elph_ai::{
    AssistantContentBlock, ContentBlock, FauxResponseStep, Message, StopReason, Tool, UserContent, builtin_models,
    faux_assistant_message, faux_text, faux_thinking, faux_tool_call,
};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::Mutex;

fn get_text_content(message: &Message) -> String {
    match message {
        Message::Assistant(assistant) => assistant
            .content
            .iter()
            .filter_map(|block| match block {
                AssistantContentBlock::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Message::ToolResult { content, .. } => content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn calculate_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "calculate".into(),
            description: "Evaluate mathematical expressions".into(),
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
                let result = eval_expression(&expression).map_err(|error| anyhow::anyhow!(error))?;
                Ok(elph_agent::AgentToolResult::text(format!("{expression} = {result}")))
            })
        },
    )
}

fn eval_expression(expression: &str) -> Result<i64, String> {
    let expression = expression.trim();
    for op in ["*", "+", "-"] {
        if let Some((left, right)) = expression.split_once(op) {
            let left: i64 = left
                .trim()
                .parse()
                .map_err(|_| format!("invalid expression: {expression}"))?;
            let right: i64 = right
                .trim()
                .parse()
                .map_err(|_| format!("invalid expression: {expression}"))?;
            return Ok(match op {
                "*" => left * right,
                "+" => left + right,
                "-" => left - right,
                _ => unreachable!(),
            });
        }
    }
    expression
        .parse()
        .map_err(|_| format!("invalid expression: {expression}"))
}

fn make_agent(faux: &elph_ai::FauxProviderHandle, options: AgentTestOptions) -> Agent {
    let model = faux.provider.get_models()[0].clone();
    Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: options.system_prompt,
            model: Some(model),
            thinking_level: options.thinking_level,
            tools: options.tools,
            messages: options.messages,
        }),
        stream_fn: Some(faux_stream_fn(faux)),
        tool_execution: ToolExecutionMode::Parallel,
        ..Default::default()
    })
}

#[derive(Default)]
struct AgentTestOptions {
    system_prompt: Option<String>,
    thinking_level: Option<AgentThinkingLevel>,
    tools: Option<Vec<AgentTool>>,
    messages: Option<Vec<AgentMessage>>,
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_prompt_persists_session_messages() {
    let temp = TempDir::new().expect("temp dir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let faux = new_faux_with_options(Default::default());
    let mut models = builtin_models(None);
    models.set_provider(faux.provider.clone());
    let models = models.into_arc();
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("4")],
        None,
    ))]);

    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are a helpful assistant.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: Default::default(),
        active_tool_names: vec![],
        steering_mode: Default::default(),
        follow_up_mode: Default::default(),
    })
    .expect("harness");

    let response = harness.prompt("What is 2+2?", None).await.expect("prompt");
    assert_eq!(response.role, "assistant");
    harness.wait_for_idle().await.expect("idle");
    assert!(
        response
            .content
            .iter()
            .any(|block| matches!(block, elph_ai::AssistantContentBlock::Text(text) if text.text.contains('4')))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_handles_basic_text_prompt() {
    let (faux, _models) = new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("4")],
        None,
    ))]);

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant. Keep your responses concise.".into()),
            ..Default::default()
        },
    );

    agent
        .prompt_text("What is 2+2? Answer with just the number.", None)
        .await
        .expect("prompt");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].role(), "user");
    assert_eq!(state.messages[1].role(), "assistant");
    let Some(Message::Assistant(assistant)) = state.messages[1].as_llm() else {
        panic!("expected assistant message");
    };
    assert!(get_text_content(&Message::Assistant(assistant.clone())).contains('4'));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_executes_tools_and_tracks_pending_tool_calls() {
    let (faux, _models) = new_faux();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_text("Let me calculate that."),
                faux_tool_call("calculate", json!({ "expression": "123 * 456" }), Some("calc-1".into())),
            ],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("The result is 56088.")], None)),
    ]);

    let pending_events = Arc::new(Mutex::new(Vec::new()));
    let pending_events_capture = pending_events.clone();
    let agent = Arc::new(make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant. Always use the calculator tool for math.".into()),
            tools: Some(vec![calculate_tool()]),
            ..Default::default()
        },
    ));
    let agent_listener = agent.clone();
    let _subscription = agent
        .subscribe(Arc::new(move |event, _| {
            let pending_events = pending_events_capture.clone();
            let agent = agent_listener.clone();
            Box::pin(async move {
                match &event {
                    AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                        let pending = agent.state().await.pending_tool_calls;
                        pending_events.lock().await.push((
                            "tool_execution_start".to_string(),
                            pending.into_iter().collect::<Vec<_>>(),
                            tool_call_id.clone(),
                        ));
                    }
                    AgentEvent::ToolExecutionEnd { .. } => {
                        let pending = agent.state().await.pending_tool_calls;
                        pending_events.lock().await.push((
                            "tool_execution_end".to_string(),
                            pending.into_iter().collect::<Vec<_>>(),
                            String::new(),
                        ));
                    }
                    _ => {}
                }
            })
        }))
        .await;

    agent
        .prompt_text("Calculate 123 * 456 using the calculator tool.", None)
        .await
        .expect("prompt");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert!(state.messages.len() >= 4);
    let tool_result = state
        .messages
        .iter()
        .find(|message| message.role() == "toolResult")
        .expect("tool result");
    let Some(Message::ToolResult { .. }) = tool_result.as_llm() else {
        panic!("expected tool result message");
    };
    assert!(get_text_content(tool_result.as_llm().expect("llm")).contains("123 * 456 = 56088"));

    let last = state.messages.last().expect("final assistant");
    assert_eq!(last.role(), "assistant");
    assert!(get_text_content(last.as_llm().expect("llm")).contains("56088"));
    assert!(state.pending_tool_calls.is_empty());

    let events = pending_events.lock().await.clone();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "tool_execution_start");
    assert_eq!(events[0].2, "calc-1");
    assert_eq!(events[0].1, vec!["calc-1".to_string()]);
    assert_eq!(events[1].0, "tool_execution_end");
    assert!(events[1].1.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_handles_abort_during_streaming() {
    let faux = new_faux_with_options(RegisterFauxProviderOptions {
        tokens_per_second: Some(20.0),
        token_size_min: Some(2),
        token_size_max: Some(2),
        ..Default::default()
    });
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text(
            "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen",
        )],
        None,
    ))]);

    let agent = Arc::new(make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant.".into()),
            ..Default::default()
        },
    ));

    let agent_for_prompt = agent.clone();
    let prompt_task = tokio::spawn(async move {
        let _ = agent_for_prompt.prompt_text("Count slowly from 1 to 20.", None).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    agent.abort().await;
    prompt_task.await.expect("prompt task");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert!(state.messages.len() >= 2);
    let last = state.messages.last().expect("assistant");
    let Some(Message::Assistant(assistant)) = last.as_llm() else {
        panic!("expected assistant message");
    };
    assert_eq!(assistant.stop_reason, StopReason::Aborted);
    assert!(assistant.error_message.is_some());
    assert_eq!(state.error_message, assistant.error_message);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_emits_lifecycle_updates_while_streaming() {
    let faux = new_faux_with_options(RegisterFauxProviderOptions {
        token_size_min: Some(1),
        token_size_max: Some(1),
        ..Default::default()
    });
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("1 2 3 4 5")],
        None,
    ))]);

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant.".into()),
            ..Default::default()
        },
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_capture = events.clone();
    let _subscription = agent
        .subscribe(Arc::new(move |event, _| {
            let events = events_capture.clone();
            Box::pin(async move {
                let name = match &event {
                    AgentEvent::AgentStart => "agent_start",
                    AgentEvent::TurnStart => "turn_start",
                    AgentEvent::MessageStart { .. } => "message_start",
                    AgentEvent::MessageUpdate { .. } => "message_update",
                    AgentEvent::MessageEnd { .. } => "message_end",
                    AgentEvent::TurnEnd { .. } => "turn_end",
                    AgentEvent::AgentEnd { .. } => "agent_end",
                    _ => "other",
                };
                events.lock().await.push(name.to_string());
            })
        }))
        .await;

    agent.prompt_text("Count from 1 to 5.", None).await.expect("prompt");
    agent.wait_for_idle().await;

    let recorded = events.lock().await.clone();
    for expected in [
        "agent_start",
        "turn_start",
        "message_start",
        "message_update",
        "message_end",
        "turn_end",
        "agent_end",
    ] {
        assert!(recorded.iter().any(|event| event == expected), "missing {expected}");
    }
    assert!(
        recorded.iter().position(|e| e == "agent_start").unwrap()
            < recorded.iter().position(|e| e == "message_start").unwrap()
    );
    assert!(
        recorded.iter().position(|e| e == "message_start").unwrap()
            < recorded.iter().position(|e| e == "message_end").unwrap()
    );
    assert!(
        recorded.iter().position(|e| e == "message_end").unwrap()
            < recorded.iter().rposition(|e| e == "agent_end").unwrap()
    );

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert_eq!(state.messages.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_maintains_context_across_multiple_turns() {
    let (faux, _models) = new_faux();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Nice to meet you, Alice.")],
            None,
        )),
        FauxResponseStep::Factory(Arc::new(|context, _, _, _| {
            let has_alice = context.messages.iter().any(|message| match message {
                Message::User { content, .. } => match content {
                    UserContent::Text(text) => text.contains("Alice"),
                    UserContent::Blocks(blocks) => blocks
                        .iter()
                        .any(|block| matches!(block, ContentBlock::Text { text } if text.contains("Alice"))),
                },
                _ => false,
            });
            faux_assistant_message(
                vec![faux_text(if has_alice {
                    "Your name is Alice."
                } else {
                    "I do not know your name."
                })],
                None,
            )
        })),
    ]);

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant.".into()),
            ..Default::default()
        },
    );

    agent.prompt_text("My name is Alice.", None).await.expect("prompt");
    agent.wait_for_idle().await;
    assert_eq!(agent.state().await.messages.len(), 2);

    agent.prompt_text("What is my name?", None).await.expect("prompt");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert_eq!(state.messages.len(), 4);
    let last = state.messages.last().expect("assistant");
    assert!(
        get_text_content(last.as_llm().expect("llm"))
            .to_lowercase()
            .contains("alice")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_preserves_thinking_content_blocks() {
    let faux = new_faux_with_options(RegisterFauxProviderOptions {
        models: Some(vec![FauxModelDefinition {
            id: "faux-reasoning".to_string(),
            name: Some("Faux Reasoning".to_string()),
            reasoning: Some(true),
            input: None,
            context_window: None,
            max_tokens: None,
        }]),
        ..Default::default()
    });
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_thinking("step by step"), faux_text("4")],
        None,
    ))]);

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant.".into()),
            thinking_level: Some(AgentThinkingLevel::Low),
            ..Default::default()
        },
    );

    agent.prompt_text("What is 2+2?", None).await.expect("prompt");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    let Some(Message::Assistant(assistant)) = state.messages[1].as_llm() else {
        panic!("expected assistant message");
    };
    assert_eq!(assistant.content.len(), 2);
    assert!(matches!(
        assistant.content[0],
        AssistantContentBlock::Thinking(ref thinking) if thinking.thinking == "step by step"
    ));
    assert!(matches!(
        assistant.content[1],
        AssistantContentBlock::Text(ref text) if text.text == "4"
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_continue_throws_when_no_messages_in_context() {
    let (faux, _models) = new_faux();
    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("Test".into()),
            ..Default::default()
        },
    );

    let error = agent.continue_run().await.unwrap_err();
    assert!(error.to_string().contains("No messages to continue from"));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_continue_throws_when_last_message_is_assistant() {
    let (faux, _models) = new_faux();
    let model = faux.provider.get_models()[0].clone();
    let mut assistant = faux_assistant_message(vec![faux_text("Hello")], None);
    assistant.api = model.api.clone();
    assistant.provider = model.provider.clone();
    assistant.model = model.id.clone();

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("Test".into()),
            messages: Some(vec![llm_message_to_agent(Message::Assistant(assistant))]),
            ..Default::default()
        },
    );

    let error = agent.continue_run().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Cannot continue from message role: assistant")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_continue_from_user_message_gets_response() {
    let (faux, _models) = new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("HELLO WORLD")],
        None,
    ))]);

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some("You are a helpful assistant. Follow instructions exactly.".into()),
            messages: Some(vec![user_agent_message("Say exactly: HELLO WORLD")]),
            ..Default::default()
        },
    );

    agent.continue_run().await.expect("continue");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].role(), "user");
    assert_eq!(state.messages[1].role(), "assistant");
    assert!(
        get_text_content(state.messages[1].as_llm().expect("llm"))
            .to_uppercase()
            .contains("HELLO WORLD")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_continue_from_tool_result_processes_results() {
    let (faux, _models) = new_faux();
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("The answer is 8.")],
        None,
    ))]);

    let mut assistant = faux_assistant_message(
        vec![
            faux_text("Let me calculate that."),
            faux_tool_call("calculate", json!({ "expression": "5 + 3" }), Some("calc-1".into())),
        ],
        Some(StopReason::ToolUse),
    );
    assistant.api = model.api.clone();
    assistant.provider = model.provider.clone();
    assistant.model = model.id.clone();

    let agent = make_agent(
        &faux,
        AgentTestOptions {
            system_prompt: Some(
                "You are a helpful assistant. After getting a calculation result, state the answer clearly.".into(),
            ),
            tools: Some(vec![calculate_tool()]),
            messages: Some(vec![
                user_agent_message("What is 5 + 3?"),
                llm_message_to_agent(Message::Assistant(assistant)),
                llm_message_to_agent(Message::ToolResult {
                    tool_call_id: "calc-1".into(),
                    tool_name: "calculate".into(),
                    content: vec![ContentBlock::Text {
                        text: "5 + 3 = 8".into(),
                    }],
                    details: None,
                    is_error: false,
                    timestamp: 0,
                }),
            ]),
            ..Default::default()
        },
    );

    agent.continue_run().await.expect("continue");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert!(!state.is_streaming);
    assert!(state.messages.len() >= 4);
    let last = state.messages.last().expect("assistant");
    assert_eq!(last.role(), "assistant");
    assert!(get_text_content(last.as_llm().expect("llm")).contains('8'));
}
