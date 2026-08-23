//! JSON-RPC wire tests against an in-process ACP agent.

use std::time::Duration;

use agent_client_protocol::ByteStreams;
use elph::platform::acp::{AcpMode, run_agent_on};
use elph::platform::{Paths, Settings};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_initialize_and_rejects_relative_cwd() {
    let (mut reader, mut writer) = spawn_agent(AcpMode::V1).await;
    let init = rpc(
        1,
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": { "name": "elph-test", "version": "0" }
        }),
    );
    write_line(&mut writer, &init).await;
    let response = read_response(&mut reader, 1).await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], 1);
    assert_eq!(response["result"]["agentInfo"]["name"], "elph");
    assert_eq!(response["result"]["agentCapabilities"]["mcpCapabilities"]["http"], true);
    assert_eq!(response["result"]["agentCapabilities"]["loadSession"], true);
    assert!(
        response["result"]["agentCapabilities"]["sessionCapabilities"]["delete"].is_object(),
        "v1 must advertise session/delete: {response}"
    );
    assert!(
        response["result"]["agentCapabilities"]["auth"]["logout"].is_object(),
        "v1 must advertise logout: {response}"
    );
    let methods = response["result"]["authMethods"].as_array().expect("authMethods");
    assert!(
        methods.iter().any(|m| m["id"] == "existing-credentials"),
        "v1 authMethods: {methods:?}"
    );
    assert!(
        methods
            .iter()
            .any(|m| m["type"] == "terminal" && (m["id"] == "elph-provider-connect")),
        "v1 must advertise terminal auth: {methods:?}"
    );

    write_line(
        &mut writer,
        &rpc(2, "session/new", json!({ "cwd": "relative", "mcpServers": [] })),
    )
    .await;
    let unauth = read_response(&mut reader, 2).await;
    assert_eq!(unauth["id"], 2);
    assert!(
        unauth["error"]["code"] == -32000
            || unauth["error"]["data"].as_str().unwrap_or("").contains("absolute")
            || unauth["error"]["message"].as_str().unwrap_or("").contains("absolute"),
        "auth or cwd: {unauth}"
    );

    write_line(&mut writer, &rpc(3, "authenticate", json!({ "methodId": "not-a-method" }))).await;
    let bad = read_json(&mut reader).await;
    assert!(bad.get("error").is_some(), "unknown method: {bad}");

    // Safety: process-local test credential so login can succeed.
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-acp-wire-test") };
    write_line(
        &mut writer,
        &rpc(4, "authenticate", json!({ "methodId": "existing-credentials" })),
    )
    .await;
    let logged_in = read_json(&mut reader).await;
    assert!(logged_in.get("result").is_some(), "authenticate: {logged_in}");

    write_line(
        &mut writer,
        &rpc(5, "session/new", json!({ "cwd": "relative", "mcpServers": [] })),
    )
    .await;
    let failed = read_json(&mut reader).await;
    assert_eq!(failed["id"], 5);
    assert!(failed.get("error").is_some(), "relative cwd must be rejected: {failed}");
    // A bad cwd is a caller mistake, not an agent fault: invalid_params, not internal_error.
    assert_eq!(failed["error"]["code"], -32602, "relative cwd must be invalid_params: {failed}");
    assert!(
        failed["error"]["data"].as_str().unwrap_or("").contains("absolute"),
        "invalid_params must explain the cwd rule: {failed}"
    );

    write_line(&mut writer, &rpc(6, "logout", json!({}))).await;
    let out = read_json(&mut reader).await;
    assert!(out.get("result").is_some(), "logout: {out}");
    write_line(
        &mut writer,
        &rpc(7, "session/new", json!({ "cwd": "relative", "mcpServers": [] })),
    )
    .await;
    let after = read_json(&mut reader).await;
    assert_eq!(after["error"]["code"], -32000, "auth_required after logout: {after}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_initialize_advertises_mcp_and_cancel_is_notification() {
    let (mut reader, mut writer) = spawn_agent(AcpMode::V2).await;
    write_line(
        &mut writer,
        &rpc(
            1,
            "initialize",
            json!({
                "protocolVersion": 2,
                "info": { "name": "elph-test", "version": "0" },
                "capabilities": {}
            }),
        ),
    )
    .await;
    let response = read_response(&mut reader, 1).await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], 2);
    let session = &response["result"]["capabilities"]["session"];
    assert!(session.get("mcp").is_some(), "v2 must advertise session.mcp: {response}");
    assert!(session["mcp"].get("stdio").is_some());
    assert!(session["mcp"].get("http").is_some());
    let methods = response["result"]["authMethods"].as_array().expect("authMethods");
    assert!(
        methods
            .iter()
            .any(|m| m["methodId"] == "existing-credentials" || m["id"] == "existing-credentials"),
        "v2 authMethods: {methods:?}"
    );
    assert!(
        methods.iter().any(|m| m["type"] == "terminal"),
        "v2 must advertise terminal auth: {methods:?}"
    );

    write_line(
        &mut writer,
        &rpc(8, "session/new", json!({ "cwd": "also-relative", "mcpServers": [] })),
    )
    .await;
    let unauth = read_response(&mut reader, 8).await;
    assert!(
        unauth["error"]["code"] == -32000 || unauth["error"]["data"].as_str().unwrap_or("").contains("absolute"),
        "v2 session/new auth or cwd: {unauth}"
    );

    write_line(
        &mut writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": { "sessionId": "missing" }
        })
        .to_string(),
    )
    .await;
    write_line(
        &mut writer,
        &rpc(3, "session/new", json!({ "cwd": "also-relative", "mcpServers": [] })),
    )
    .await;
    let failed = tokio::time::timeout(Duration::from_secs(5), read_json(&mut reader))
        .await
        .expect("session/new response after cancel");
    assert_eq!(failed["id"], 3);
    assert!(failed.get("error").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_session_new_list_close() {
    if !live_agent_enabled() {
        eprintln!("skipping v1_session_new_list_close: set ELPH_ACP_LIVE_TESTS=1 with a real API key to run");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).expect("project dir");
    let cwd = project.canonicalize().unwrap_or(project);
    std::fs::create_dir_all(cwd.join(".elph")).expect(".elph");
    let cfg = tmp.path().join("cfg");
    std::fs::create_dir_all(&cfg).expect("cfg");
    std::fs::write(cfg.join("auth.json"), r#"{"provider":{"openai":{"apiKey":"sk-acp-e2e"}}}"#).expect("auth.json");
    let (mut reader, mut writer) = spawn_agent_on(AcpMode::V1, tmp).await;

    write_line(
        &mut writer,
        &rpc(
            1,
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": { "name": "elph-test", "version": "0" }
            }),
        ),
    )
    .await;
    let init = read_response(&mut reader, 1).await;
    assert_eq!(init["result"]["protocolVersion"], 1);

    write_line(
        &mut writer,
        &rpc(2, "authenticate", json!({ "methodId": "existing-credentials" })),
    )
    .await;
    let login = read_response(&mut reader, 2).await;
    assert!(login.get("result").is_some(), "authenticate: {login}");

    write_line(&mut writer, &rpc(3, "session/new", json!({ "cwd": cwd, "mcpServers": [] }))).await;
    let created = read_response(&mut reader, 3).await;
    assert!(created.get("result").is_some(), "session/new: {created}");
    let sid = created["result"]["sessionId"].as_str().expect("sessionId").to_string();
    assert!(!sid.is_empty());
    let commands = wait_available_commands(&mut reader, Duration::from_secs(15)).await;
    assert!(
        commands.iter().any(|c| c == "help"),
        "v1 must advertise slash commands after session/new: {commands:?}"
    );

    write_line(&mut writer, &rpc(4, "session/list", json!({}))).await;
    let listed = read_response(&mut reader, 4).await;
    assert!(listed.get("result").is_some(), "session/list: {listed}");

    write_line(&mut writer, &rpc(5, "session/close", json!({ "sessionId": sid }))).await;
    let closed = read_response(&mut reader, 5).await;
    assert!(closed.get("result").is_some(), "session/close: {closed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_help_prompt_returns_stop_reason() {
    if !live_agent_enabled() {
        eprintln!("skipping v1_help_prompt_returns_stop_reason: set ELPH_ACP_LIVE_TESTS=1 with a real API key to run");
        return;
    }
    let (cwd, tmp) = project_with_auth();
    let (mut reader, mut writer) = spawn_agent_on(AcpMode::V1, tmp).await;
    login_v1(&mut reader, &mut writer).await;
    let sid = new_session(&mut reader, &mut writer, &cwd, 3).await;

    write_line(
        &mut writer,
        &rpc(
            4,
            "session/prompt",
            json!({
                "sessionId": sid,
                "prompt": [{ "type": "text", "text": "/help" }]
            }),
        ),
    )
    .await;
    let prompt = read_response(&mut reader, 4).await;
    assert!(prompt.get("result").is_some(), "v1 prompt: {prompt}");
    assert_eq!(
        prompt["result"]["stopReason"], "end_turn",
        "v1 holds prompt until stopReason: {prompt}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_help_prompt_ack_then_idle() {
    if !live_agent_enabled() {
        eprintln!("skipping v2_help_prompt_ack_then_idle: set ELPH_ACP_LIVE_TESTS=1 with a real API key to run");
        return;
    }
    let (cwd, tmp) = project_with_auth();
    let (mut reader, mut writer) = spawn_agent_on(AcpMode::V2, tmp).await;
    login_v2(&mut reader, &mut writer).await;
    let sid = new_session(&mut reader, &mut writer, &cwd, 3).await;
    let commands = wait_available_commands(&mut reader, Duration::from_secs(15)).await;
    assert!(
        commands.iter().any(|c| c == "help"),
        "v2 must advertise slash commands after session/new: {commands:?}"
    );
    drain_notifications(&mut reader, Duration::from_millis(200)).await;

    write_line(
        &mut writer,
        &rpc(
            4,
            "session/prompt",
            json!({
                "sessionId": sid,
                "prompt": [{ "type": "text", "text": "/help" }]
            }),
        ),
    )
    .await;

    let mut saw_ack = false;
    let mut saw_user = false;
    let mut saw_running = false;
    let mut saw_agent = false;
    let mut idle_reasons = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        let value = read_json(&mut reader).await;
        if value.get("id") == Some(&json!(4)) {
            assert!(value.get("result").is_some(), "v2 prompt ack: {value}");
            assert!(
                value["result"]
                    .as_object()
                    .is_some_and(|o| o.is_empty() || !o.contains_key("stopReason"))
            );
            saw_ack = true;
            continue;
        }
        if value.get("method") != Some(&json!("session/update")) {
            continue;
        }
        let update = &value["params"]["update"];
        match update["sessionUpdate"].as_str() {
            Some("user_message") => {
                assert!(update.get("messageId").is_some(), "user_message needs messageId: {update}");
                saw_user = true;
            }
            Some("state_update") => match update["state"].as_str() {
                Some("running") => saw_running = true,
                Some("idle") => idle_reasons.push(update["stopReason"].as_str().unwrap_or("").to_string()),
                _ => {}
            },
            Some("agent_message") | Some("agent_message_chunk") => saw_agent = true,
            _ => {}
        }
        if saw_ack && saw_user && saw_running && saw_agent && !idle_reasons.is_empty() {
            break;
        }
    }
    assert!(saw_ack, "v2 must ack session/prompt");
    assert!(saw_user, "v2 must emit user_message");
    assert!(saw_running, "v2 must emit running");
    assert!(saw_agent, "v2 must emit agent text");
    assert_eq!(
        idle_reasons,
        vec!["end_turn".to_string()],
        "exactly one idle end_turn: {idle_reasons:?}"
    );
}

fn project_with_auth() -> (std::path::PathBuf, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).expect("project dir");
    let cwd = project.canonicalize().unwrap_or(project);
    std::fs::create_dir_all(cwd.join(".elph")).expect(".elph");
    let cfg = tmp.path().join("cfg");
    std::fs::create_dir_all(&cfg).expect("cfg");
    std::fs::write(cfg.join("auth.json"), r#"{"provider":{"openai":{"apiKey":"sk-acp-e2e"}}}"#).expect("auth.json");
    (cwd, tmp)
}

/// Skip live-backend ACP integration tests unless explicitly enabled.
///
/// These tests create a real agent session, which performs a provider handshake
/// that blocks on the network without a reachable LLM backend and valid credentials.
/// They therefore hang (and time out the whole suite) outside CI with a real key.
fn live_agent_enabled() -> bool {
    std::env::var("ELPH_ACP_LIVE_TESTS").is_ok()
}

async fn login_v1(reader: &mut BufReader<tokio::io::DuplexStream>, writer: &mut tokio::io::DuplexStream) {
    write_line(
        writer,
        &rpc(
            1,
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": { "name": "elph-test", "version": "0" }
            }),
        ),
    )
    .await;
    let init = read_response(reader, 1).await;
    assert_eq!(init["result"]["protocolVersion"], 1);
    write_line(writer, &rpc(2, "authenticate", json!({ "methodId": "existing-credentials" }))).await;
    let login = read_response(reader, 2).await;
    assert!(login.get("result").is_some(), "authenticate: {login}");
}

async fn login_v2(reader: &mut BufReader<tokio::io::DuplexStream>, writer: &mut tokio::io::DuplexStream) {
    write_line(
        writer,
        &rpc(
            1,
            "initialize",
            json!({
                "protocolVersion": 2,
                "info": { "name": "elph-test", "version": "0" },
                "capabilities": {}
            }),
        ),
    )
    .await;
    let init = read_response(reader, 1).await;
    assert_eq!(init["result"]["protocolVersion"], 2);
    write_line(writer, &rpc(2, "auth/login", json!({ "methodId": "existing-credentials" }))).await;
    let login = read_response(reader, 2).await;
    assert!(login.get("result").is_some(), "auth/login: {login}");
}

async fn new_session(
    reader: &mut BufReader<tokio::io::DuplexStream>,
    writer: &mut tokio::io::DuplexStream,
    cwd: &std::path::Path,
    id: u64,
) -> String {
    write_line(writer, &rpc(id, "session/new", json!({ "cwd": cwd, "mcpServers": [] }))).await;
    let created = read_response(reader, id).await;
    assert!(created.get("result").is_some(), "session/new: {created}");
    created["result"]["sessionId"].as_str().expect("sessionId").to_string()
}

async fn wait_available_commands(reader: &mut BufReader<tokio::io::DuplexStream>, wait: Duration) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let mut line = String::new();
        match tokio::time::timeout(remaining, reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(_)) => {}
            Ok(Err(_)) => break,
        }
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if value.get("method") != Some(&json!("session/update")) {
            continue;
        }
        let update = &value["params"]["update"];
        if update["sessionUpdate"] != "available_commands_update" {
            continue;
        }
        return update["availableCommands"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|c| c["name"].as_str().map(str::to_string))
            .collect();
    }
    Vec::new()
}

async fn drain_notifications(reader: &mut BufReader<tokio::io::DuplexStream>, wait: Duration) {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let mut line = String::new();
        match tokio::time::timeout(remaining, reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(_)) => {}
            Ok(Err(_)) => break,
        }
    }
}

async fn spawn_agent(mode: AcpMode) -> (BufReader<tokio::io::DuplexStream>, tokio::io::DuplexStream) {
    spawn_agent_on(mode, tempfile::tempdir().expect("tempdir")).await
}

async fn spawn_agent_on(
    mode: AcpMode,
    tmp: tempfile::TempDir,
) -> (BufReader<tokio::io::DuplexStream>, tokio::io::DuplexStream) {
    let cfg = tmp.path().join("cfg");
    let data = tmp.path().join("data");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&cfg).expect("cfg");
    std::fs::create_dir_all(&data).expect("data");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::create_dir_all(project.join(".elph")).expect(".elph");
    let paths = Paths::from_dirs(cfg, data, project);
    if let Err(error) = elph::platform::ensure_datastore_blocking(&paths) {
        log::warn!("ACP wire datastore warmup skipped: {error:#}");
    }
    let settings = Settings::defaults();

    let (client_writer, server_reader) = tokio::io::duplex(64 * 1024);
    let (server_writer, client_reader) = tokio::io::duplex(64 * 1024);

    tokio::spawn(async move {
        let _tmp = tmp;
        let _ = run_agent_on(
            paths,
            settings,
            mode,
            ByteStreams::new(server_writer.compat_write(), server_reader.compat()),
        )
        .await;
    });

    (BufReader::new(client_reader), client_writer)
}

async fn read_response(reader: &mut BufReader<tokio::io::DuplexStream>, id: u64) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let mut line = String::new();
        tokio::time::timeout(remaining, reader.read_line(&mut line))
            .await
            .unwrap_or_else(|_| panic!("timeout waiting for ACP response id={id}"))
            .expect("read line");
        let value: Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("invalid json {e}: {line}"));
        if value.get("id") == Some(&json!(id)) {
            return value;
        }
    }
}

fn rpc(id: u64, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

async fn write_line(writer: &mut tokio::io::DuplexStream, line: &str) {
    writer.write_all(line.as_bytes()).await.expect("write");
    writer.write_all(b"\n").await.expect("newline");
    writer.flush().await.expect("flush");
}

async fn read_json(reader: &mut BufReader<tokio::io::DuplexStream>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("timeout waiting for ACP line")
        .expect("read line");
    serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("invalid json {e}: {line}"))
}
