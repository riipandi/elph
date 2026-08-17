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

    write_line(&mut writer, &rpc(4, "session/list", json!({}))).await;
    let listed = read_response(&mut reader, 4).await;
    assert!(listed.get("result").is_some(), "session/list: {listed}");

    write_line(&mut writer, &rpc(5, "session/close", json!({ "sessionId": sid }))).await;
    let closed = read_response(&mut reader, 5).await;
    assert!(closed.get("result").is_some(), "session/close: {closed}");
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
