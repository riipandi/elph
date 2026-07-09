//! AgentHarness stream configuration tests.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use elph_agent::harness::types::AgentHarnessStreamOptionsPatch;
use elph_agent::runtime::try_block_on;
use elph_agent::session::types::HasSessionId;
use elph_agent::{
    AgentHarness, AgentHarnessEvent, AgentHarnessOptions, AgentHarnessOwnEvent, AgentHarnessResources,
    AgentHarnessStreamOptions, AgentThinkingLevel, InMemorySessionStorage, LocalExecutionEnv, Session, SystemPrompt,
    simple_tool,
};
use elph_ai::{FauxResponseStep, StopReason, Tool, faux_assistant_message, faux_text, faux_tool_call};
use serde_json::json;
use tempfile::TempDir;

fn test_env() -> (TempDir, Arc<LocalExecutionEnv>) {
    let temp = TempDir::new().expect("temp dir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    (temp, env)
}

fn calculate_tool() -> elph_agent::AgentTool {
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

#[tokio::test(flavor = "multi_thread")]
async fn harness_snapshots_stream_options_before_provider_request() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let captured = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        move |_context, options, _, _| {
            *captured_clone.lock() = options.cloned();
            faux_assistant_message(vec![faux_text("ok")], None)
        },
    ))]);

    let session = Session::new(InMemorySessionStorage::new(None).expect("session storage"));
    let session_id = session.metadata().await.session_id().to_string();

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: AgentHarnessStreamOptions {
            timeout_ms: Some(1000),
            max_retries: Some(2),
            max_retry_delay_ms: Some(3000),
            headers: Some(HashMap::from([("x-base".into(), "base".into())])),
            metadata: Some(json!({ "base": true })),
            ..Default::default()
        },
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    harness.prompt("hello", None).await.expect("prompt");

    let options = captured.lock().clone().expect("stream options");
    assert_eq!(options.timeout_ms, Some(1000));
    assert_eq!(options.max_retries, Some(2));
    assert_eq!(options.max_retry_delay_ms, Some(3000));
    assert_eq!(options.session_id.as_deref(), Some(session_id.as_str()));
    assert_eq!(
        options
            .headers
            .as_ref()
            .and_then(|h| h.get("x-base"))
            .and_then(|v| v.as_deref()),
        Some("base")
    );
    assert_eq!(
        options.metadata.as_ref().and_then(|m| m.get("base")),
        Some(&json!(true))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_save_point_refreshes_stream_options_without_mutating_active_request() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    faux.set_responses(vec![
        FauxResponseStep::Factory({
            let captured_clone = captured_clone.clone();
            Arc::new(move |_context, options, _, _| {
                captured_clone.lock().push(options.and_then(|o| o.timeout_ms));
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
            let captured_clone = captured_clone.clone();
            Arc::new(move |_context, options, _, _| {
                captured_clone.lock().push(options.and_then(|o| o.timeout_ms));
                faux_assistant_message(vec![faux_text("done")], None)
            })
        }),
    ]);

    let harness = Arc::new(
        AgentHarness::new(AgentHarnessOptions {
            env,
            session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
            models,
            tools: vec![calculate_tool()],
            resources: AgentHarnessResources::default(),
            system_prompt: SystemPrompt::Static("You are helpful.".into()),
            stream_options: AgentHarnessStreamOptions {
                timeout_ms: Some(1000),
                headers: Some(HashMap::from([("turn".into(), "first".into())])),
                ..Default::default()
            },
            model,
            thinking_level: AgentThinkingLevel::Off,
            active_tool_names: vec!["calculate".into()],
            steering_mode: elph_agent::QueueMode::OneAtATime,
            follow_up_mode: elph_agent::QueueMode::OneAtATime,
        })
        .expect("harness"),
    );

    let harness_for_sub = harness.clone();
    harness
        .subscribe(move |event, _| {
            let harness = harness_for_sub.clone();
            async move {
                if let AgentHarnessEvent::Agent(elph_agent::AgentEvent::ToolExecutionStart { .. }) = event {
                    harness
                        .set_stream_options(AgentHarnessStreamOptions {
                            timeout_ms: Some(2000),
                            headers: Some(HashMap::from([("turn".into(), "second".into())])),
                            ..Default::default()
                        })
                        .await;
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let timeouts = captured.lock().clone();
    assert_eq!(timeouts, vec![Some(1000), Some(2000)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_chains_provider_request_patches_with_deletion() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let captured = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        move |_context, options, _, _| {
            *captured_clone.lock() = options.cloned();
            faux_assistant_message(vec![faux_text("ok")], None)
        },
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: AgentHarnessStreamOptions {
            timeout_ms: Some(1000),
            max_retries: Some(2),
            headers: Some(HashMap::from([
                ("keep".into(), "base".into()),
                ("remove".into(), "base".into()),
            ])),
            metadata: Some(json!({ "keep": "base", "remove": "base" })),
            ..Default::default()
        },
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    harness
        .on_before_provider_request(|event| {
            let keep = event
                .stream_options
                .headers
                .as_ref()
                .and_then(|h| h.get("keep"))
                .cloned();
            async move {
                assert_eq!(keep.as_deref(), Some("base"));
                Some(elph_agent::BeforeProviderRequestResult {
                    stream_options: Some(AgentHarnessStreamOptionsPatch {
                        headers: Some(Some(HashMap::from([
                            ("first".into(), Some("1".into())),
                            ("remove".into(), None),
                        ]))),
                        metadata: Some(Some(HashMap::from([
                            ("first".into(), Some(json!(1))),
                            ("remove".into(), None),
                        ]))),
                        ..Default::default()
                    }),
                })
            }
        })
        .await;

    harness
        .on_before_provider_request(|event| {
            let first_header = event
                .stream_options
                .headers
                .as_ref()
                .and_then(|h| h.get("first"))
                .cloned();
            let first_metadata = event
                .stream_options
                .metadata
                .as_ref()
                .and_then(|m| m.get("first"))
                .cloned();
            async move {
                assert_eq!(first_header.as_deref(), Some("1"));
                assert_eq!(first_metadata, Some(json!(1)));
                Some(elph_agent::BeforeProviderRequestResult {
                    stream_options: Some(AgentHarnessStreamOptionsPatch {
                        timeout_ms: Some(None),
                        headers: Some(Some(HashMap::from([("second".into(), Some("2".into()))]))),
                        metadata: Some(None),
                        ..Default::default()
                    }),
                })
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let options = captured.lock().clone().expect("stream options");
    assert_eq!(options.timeout_ms, None);
    assert_eq!(options.max_retries, Some(2));
    let headers = options.headers.as_ref().expect("headers");
    assert_eq!(headers.get("keep").and_then(|value| value.as_deref()), Some("base"));
    assert_eq!(headers.get("first").and_then(|value| value.as_deref()), Some("1"));
    assert_eq!(headers.get("second").and_then(|value| value.as_deref()), Some("2"));
    assert!(options.metadata.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_chains_provider_payload_hooks() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let seen_payloads = Arc::new(Mutex::new(Vec::new()));
    let final_payload = Arc::new(Mutex::new(None));
    let seen_payloads_clone = seen_payloads.clone();
    let final_payload_clone = final_payload.clone();

    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        move |_context, options, _, model| {
            let payload = if let Some(on_payload) = options.and_then(|o| o.on_payload.clone()) {
                try_block_on(on_payload(json!({ "steps": ["provider"] }), model.clone()))
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| json!({ "steps": ["provider"] }))
            } else {
                json!({ "steps": ["provider"] })
            };
            *final_payload_clone.lock() = Some(payload);
            faux_assistant_message(vec![faux_text("ok")], None)
        },
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    let seen_payloads_first = seen_payloads_clone.clone();
    harness
        .on_before_provider_payload(move |event| {
            let payload = event.payload.clone();
            let seen_payloads = seen_payloads_first.clone();
            async move {
                seen_payloads.lock().push(payload);
                Some(elph_agent::BeforeProviderPayloadResult {
                    payload: json!({ "steps": ["provider", "first"] }),
                })
            }
        })
        .await;

    let seen_payloads_second = seen_payloads_clone.clone();
    harness
        .on_before_provider_payload(move |event| {
            let payload = event.payload.clone();
            let seen_payloads = seen_payloads_second.clone();
            async move {
                seen_payloads.lock().push(payload);
                Some(elph_agent::BeforeProviderPayloadResult {
                    payload: json!({ "steps": ["provider", "first", "second"] }),
                })
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    assert_eq!(
        seen_payloads.lock().clone(),
        vec![
            json!({ "steps": ["provider"] }),
            json!({ "steps": ["provider", "first"] })
        ]
    );
    assert_eq!(
        final_payload.lock().clone(),
        Some(json!({ "steps": ["provider", "first", "second"] }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_on_chains_provider_payload_hooks() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let seen_payloads = Arc::new(Mutex::new(Vec::new()));
    let final_payload = Arc::new(Mutex::new(None));
    let seen_payloads_clone = seen_payloads.clone();
    let final_payload_clone = final_payload.clone();

    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        move |_context, options, _, model| {
            let payload = if let Some(on_payload) = options.and_then(|o| o.on_payload.clone()) {
                try_block_on(on_payload(json!({ "steps": ["provider"] }), model.clone()))
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| json!({ "steps": ["provider"] }))
            } else {
                json!({ "steps": ["provider"] })
            };
            *final_payload_clone.lock() = Some(payload);
            faux_assistant_message(vec![faux_text("ok")], None)
        },
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    let seen_payloads_first = seen_payloads_clone.clone();
    harness
        .on("before_provider_payload", move |event| {
            let seen_payloads = seen_payloads_first.clone();
            async move {
                let elph_agent::AgentHarnessOwnEvent::BeforeProviderPayload(event) = event else {
                    return None;
                };
                seen_payloads.lock().push(event.payload.clone());
                Some(elph_agent::HarnessHookResult::BeforeProviderPayload(
                    elph_agent::BeforeProviderPayloadResult {
                        payload: json!({ "steps": ["provider", "first"] }),
                    },
                ))
            }
        })
        .await
        .expect("first hook");

    let seen_payloads_second = seen_payloads_clone.clone();
    harness
        .on("before_provider_payload", move |event| {
            let seen_payloads = seen_payloads_second.clone();
            async move {
                let elph_agent::AgentHarnessOwnEvent::BeforeProviderPayload(event) = event else {
                    return None;
                };
                seen_payloads.lock().push(event.payload.clone());
                Some(elph_agent::HarnessHookResult::BeforeProviderPayload(
                    elph_agent::BeforeProviderPayloadResult {
                        payload: json!({ "steps": ["provider", "first", "second"] }),
                    },
                ))
            }
        })
        .await
        .expect("second hook");

    harness.prompt("hello", None).await.expect("prompt");

    assert_eq!(
        seen_payloads.lock().clone(),
        vec![
            json!({ "steps": ["provider"] }),
            json!({ "steps": ["provider", "first"] })
        ]
    );
    assert_eq!(
        final_payload.lock().clone(),
        Some(json!({ "steps": ["provider", "first", "second"] }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_on_rejects_unknown_hook_type() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    let result = harness.on("not_a_real_hook", |_| async { None }).await;

    assert!(result.is_err());
    assert!(
        result
            .expect_err("unknown hook")
            .to_string()
            .contains("Unknown harness hook type")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_after_provider_response_captures_status_and_headers() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    let captured = Arc::new(Mutex::new(None::<(u16, HashMap<String, String>)>));
    let captured_clone = captured.clone();
    harness
        .on_after_provider_response(move |event| {
            let captured = captured_clone.clone();
            let status = event.status;
            let headers = event.headers.clone();
            async move {
                *captured.lock() = Some((status, headers));
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let (status, headers) = captured.lock().clone().expect("response metadata");
    assert_eq!(status, 200);
    assert_eq!(headers.get("x-faux-provider").map(String::as_str), Some("ok"));
    assert_eq!(
        headers.get("content-type").map(String::as_str),
        Some("text/event-stream")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_subscribe_receives_after_provider_response_own_event() {
    let (_temp, env) = test_env();
    let (faux, models) = common::new_faux();
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session: Session::new(InMemorySessionStorage::new(None).expect("session storage")),
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are helpful.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: elph_agent::QueueMode::OneAtATime,
        follow_up_mode: elph_agent::QueueMode::OneAtATime,
    })
    .expect("harness");

    let responses = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let responses_clone = responses.clone();
    harness
        .subscribe(move |event, _| {
            let responses = responses_clone.clone();
            async move {
                if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::AfterProviderResponse(response)) = event {
                    responses.lock().await.push((response.status, response.headers.clone()));
                }
            }
        })
        .await;

    harness.prompt("hello", None).await.expect("prompt");

    let responses = responses.lock().await.clone();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].0, 200);
    assert_eq!(responses[0].1.get("x-faux-provider").map(String::as_str), Some("ok"));
}
