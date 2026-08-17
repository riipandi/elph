//! JSON-RPC wire tests against an in-process ACP agent.

use std::time::Duration;

use agent_client_protocol::ByteStreams;
use elph::platform::acp::{AcpMode, run_agent_on};
use elph::platform::{Paths, Settings};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[tokio::test]
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
    let response = read_json(&mut reader).await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], 1);
    assert_eq!(response["result"]["agentInfo"]["name"], "elph");
    assert_eq!(response["result"]["agentCapabilities"]["mcpCapabilities"]["http"], true);
    assert_eq!(response["result"]["agentCapabilities"]["loadSession"], true);

    write_line(
        &mut writer,
        &rpc(2, "session/new", json!({ "cwd": "relative", "mcpServers": [] })),
    )
    .await;
    let failed = read_json(&mut reader).await;
    assert_eq!(failed["id"], 2);
    assert!(failed.get("error").is_some(), "relative cwd must be rejected: {failed}");
}

#[tokio::test]
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
    let response = read_json(&mut reader).await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], 2);
    let session = &response["result"]["capabilities"]["session"];
    assert!(session.get("mcp").is_some(), "v2 must advertise session.mcp: {response}");
    assert!(session["mcp"].get("stdio").is_some());
    assert!(session["mcp"].get("http").is_some());

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

async fn spawn_agent(mode: AcpMode) -> (BufReader<tokio::io::DuplexStream>, tokio::io::DuplexStream) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = Paths::from_dirs(tmp.path().join("cfg"), tmp.path().join("data"), tmp.path().join("project"));
    let settings = Settings::defaults();

    let (client_writer, server_reader) = tokio::io::duplex(64 * 1024);
    let (server_writer, client_reader) = tokio::io::duplex(64 * 1024);

    tokio::spawn(async move {
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
