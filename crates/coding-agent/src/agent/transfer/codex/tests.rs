//! Unit tests for the Codex session transfer reader.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use super::*;

// ── fixtures ───────────────────────────────────────────────────────────────

const UUID: &str = "019d58bb-1226-79b1-a4f4-2807b93763f5";

fn rollout_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir
        .join("sessions")
        .join("2026")
        .join("07")
        .join("29")
        .join(format!("rollout-2026-07-29T16-06-50-{UUID}.jsonl"))
}

fn meta_record(cwd: &str) -> Value {
    json!({
        "timestamp": "2026-07-29T09:06:50.000Z",
        "type": "session_meta",
        "payload": {
            "id": UUID,
            "cwd": cwd,
            "source": "cli",
            "cli_version": "0.118.0",
            "git": { "branch": "main" }
        }
    })
}

fn user_msg(text: &str) -> Value {
    json!({
        "timestamp": "2026-07-29T09:07:42.547Z",
        "type": "event_msg",
        "payload": { "type": "user_message", "message": text }
    })
}

fn assistant_msg(text: &str) -> Value {
    json!({
        "timestamp": "2026-07-29T09:07:45.000Z",
        "type": "event_msg",
        "payload": { "type": "agent_message", "message": text }
    })
}

fn response_msg(role: &str, text: &str) -> Value {
    json!({
        "timestamp": "2026-07-29T09:07:46.000Z",
        "type": "response_item",
        "payload": { "type": "message", "role": role, "content": [ { "type": "input_text", "text": text } ] }
    })
}

fn response_tool_call(name: &str, args: Value) -> Value {
    json!({
        "timestamp": "2026-07-29T09:07:47.000Z",
        "type": "response_item",
        "payload": { "type": "function_call", "id": "fc_1", "name": name, "arguments": args }
    })
}

fn response_tool_output(output: &str) -> Value {
    json!({
        "timestamp": "2026-07-29T09:07:48.000Z",
        "type": "response_item",
        "payload": { "type": "function_call_output", "id": "fc_1", "output": output }
    })
}

fn write_rollout(config_dir: &Path, _cwd: &str, records: &[Value]) -> std::path::PathBuf {
    let path = rollout_path(config_dir);
    fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    let mut text = String::new();
    for record in records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write rollout");
    path
}

fn read_texts(transfer: &CodexTransfer) -> Vec<String> {
    transfer.turns.iter().map(|turn| turn.text.clone()).collect()
}

// ── discovery ──────────────────────────────────────────────────────────────

#[test]
fn discovers_rollouts_for_cwd_newest_first() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let cwd = "/Users/me/repo".to_string();

    // Newer session (explicit later mtime).
    let uuid2 = "019d58bb-2222-79b1-a4f4-2807b93763f5";
    let p2 = config
        .join("sessions")
        .join("2026")
        .join("07")
        .join("30")
        .join(format!("rollout-2026-07-30T10-00-00-{uuid2}.jsonl"));
    fs::create_dir_all(p2.parent().expect("parent")).expect("dirs");
    fs::write(
        &p2,
        format!(
            "{}\n{}\n",
            json!({
                "type": "session_meta",
                "payload": { "id": uuid2, "cwd": cwd, "source": "cli" }
            }),
            user_msg("fix the build")
        ),
    )
    .expect("write");
    set_mtime(&p2, now_ms() - 60_000);

    // Older session.
    let p1 = rollout_path(&config);
    fs::create_dir_all(p1.parent().expect("parent")).expect("dirs");
    fs::write(&p1, format!("{}\n{}\n", meta_record(&cwd), user_msg("older user text"))).expect("write");
    set_mtime(&p1, now_ms() - 3_600_000);

    let sessions = discover_codex_sessions_with_config(Path::new("/Users/me/repo"), &config);
    assert_eq!(sessions.len(), 2, "both sessions discovered: {sessions:?}");
    assert_eq!(sessions[0].session_id, uuid2, "newest first by mtime");
    assert_eq!(sessions[0].title, "fix the build");
    assert_eq!(sessions[0].source, "cli");
    assert_eq!(sessions[1].title, "older user text");
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn set_mtime(path: &std::path::Path, unix_ms: u64) {
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_millis(unix_ms);
    let file = fs::OpenOptions::new().write(true).open(path).expect("open for mtime");
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .expect("set mtime");
}

#[test]
fn discovery_ignores_wrong_cwd_and_non_cli_sources() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let p = rollout_path(&config);
    fs::create_dir_all(p.parent().expect("parent")).expect("dirs");
    // Wrong cwd (different repo).
    fs::write(&p, format!("{}\n", meta_record("/elsewhere"))).expect("write");
    assert!(discover_codex_sessions_with_config(Path::new("/Users/me/repo"), &config).is_empty());

    // Non-cli source (atlas subagent): excluded from discovery.
    fs::write(
        &p,
        format!(
            "{}\n",
            json!({ "type": "session_meta", "payload": { "id": UUID, "cwd": "/Users/me/repo", "source": "atlas" } })
        ),
    )
    .expect("write");
    assert!(discover_codex_sessions_with_config(Path::new("/Users/me/repo"), &config).is_empty());
}

// ── raw turn normalization ─────────────────────────────────────────────────

#[test]
fn normalizes_user_assistant_tool_turns() {
    let raw = response_tool_call("exec_command", json!({"cmd": "ls"}));
    let turn = raw_turn(&raw).expect("tool call turn");
    assert_eq!(turn.role, "assistant");
    assert!(turn.text.contains("called inert foreign tool: exec_command"));
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "exec_command");
    assert!(turn.tool_calls[0].inert);
}

#[test]
fn tool_output_turns_become_tool_results() {
    let raw = response_tool_output("hello\nworld");
    let turn = raw_turn(&raw).expect("tool output turn");
    assert_eq!(turn.role, "user");
    assert!(turn.text.is_empty());
    assert_eq!(turn.tool_results.len(), 1);
    assert_eq!(turn.tool_results[0].content, "hello\nworld");
    assert!(turn.tool_results[0].inert);
}

#[test]
fn developer_and_control_records_are_skipped() {
    // developer-role messages are not user/assistant.
    assert!(raw_turn(&response_msg("developer", "do not include")).is_none());
    // Reasoning / token_count control items have no text.
    assert!(
        raw_turn(&json!({
            "type": "response_item",
            "payload": { "type": "reasoning", "summary": [{ "type": "summary_text", "text": "think" }] }
        }))
        .is_none()
    );
    // Unknown outer type.
    assert!(
        raw_turn(&json!({ "type": "something_else", "payload": { "type": "user_message", "message": "x" } })).is_none()
    );
}

// ── full read ──────────────────────────────────────────────────────────────

#[test]
fn oversized_line_is_skipped_with_warning() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let p = rollout_path(&config);
    fs::create_dir_all(p.parent().expect("parent")).expect("dirs");
    // One line massively larger than MAX_RECORD_BYTES, then a normal user msg.
    let huge = "x".repeat(MAX_RECORD_BYTES + 100);
    fs::write(
        &p,
        format!(
            "{}\n{} {huge}\n{}",
            meta_record("/repo"),
            json!({ "type": "response_item", "payload": { "type": "reasoning", "summary": [] } }),
            user_msg("hello")
        ),
    )
    .expect("write");
    let h = read_codex_session(&p).expect("read");
    assert!(
        h.warnings.iter().any(|w| w.code == "oversized_records_skipped"),
        "{:?}",
        h.warnings
    );
    // The user message is still recovered.
    assert_eq!(read_texts(&h), vec!["hello"]);
}

#[test]
fn transcript_over_record_cap_is_truncated_with_warning() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let p = rollout_path(&config);
    fs::create_dir_all(p.parent().expect("parent")).expect("dirs");
    let mut content = format!("{}\n", meta_record("/repo"));
    // Push well past MAX_TRANSCRIPT_RECORDS with alternating user/assistant msgs.
    for i in 0..(super::MAX_TRANSCRIPT_RECORDS + 100) {
        let record = if i % 2 == 0 {
            user_msg(&format!("msg {i}"))
        } else {
            assistant_msg(&format!("reply {i}"))
        };
        content.push_str(&record.to_string());
        content.push('\n');
    }
    fs::write(&p, content).expect("write");
    let h = read_codex_session(&p).expect("read");
    assert!(h.warnings.iter().any(|w| w.code == "transcript_truncated"), "{:?}", h.warnings);
    assert!(h.turns.len() < super::MAX_TRANSCRIPT_RECORDS);
}

#[test]
fn oversized_transcript_file_is_rejected_not_slurped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let p = rollout_path(&config);
    fs::create_dir_all(p.parent().expect("parent")).expect("dirs");
    // Create a sparse file larger than MAX_TRANSCRIPT_BYTES via seek.
    let file = fs::File::create(&p).expect("create");
    let limit = super::MAX_TRANSCRIPT_BYTES as u64 + 4096;
    file.set_len(limit).expect("sparse extend");
    drop(file);
    match read_codex_session(&p) {
        Err(TransferError::ReadFailed(msg)) => assert!(msg.contains("too large"), "msg: {msg}"),
        other => panic!("expected too-large rejection, got {other:?}"),
    }
}

#[test]
fn reads_full_rollout_chain_with_tools() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let path = write_rollout(
        &config,
        "/Users/me/repo",
        &[
            meta_record("/Users/me/repo"),
            user_msg("investigate flaky test"),
            response_tool_call("exec_command", json!({"cmd": "cargo test"})),
            response_tool_output("failures: test_a"),
            assistant_msg("Found the flake"),
            response_msg("assistant", "It's a race in the cache"),
        ],
    );

    let h = read_codex_session(&path).expect("read");
    assert_eq!(h.session_id, UUID);
    assert_eq!(h.source, "cli");
    assert_eq!(h.cwd.as_deref(), Some("/Users/me/repo"));
    assert_eq!(h.branch.as_deref(), Some("main"));
    assert_eq!(h.created_at.as_deref(), Some("2026-07-29T09:06:50.000Z"));
    // order: user, tool call (assistant), tool output (user), agent msg, assistant msg
    assert_eq!(h.turns.len(), 5);
    assert_eq!(read_texts(&h)[0], "investigate flaky test");
    assert!(read_texts(&h)[1].starts_with("called inert foreign tool: exec_command"));
    assert_eq!(read_texts(&h)[3], "Found the flake");
    assert_eq!(read_texts(&h)[4], "It's a race in the cache");
    // Last signals.
    assert_eq!(h.last_user_request.as_deref(), Some("investigate flaky test"));
    assert_eq!(h.last_assistant_action.as_deref(), Some("It's a race in the cache"));
    // Title = first user text.
    assert_eq!(h.title.as_deref(), Some("investigate flaky test"));
}

#[test]
fn deduplicates_consecutive_response_item_and_event_msg_duplicates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let path = write_rollout(
        &config,
        "/repo",
        &[
            meta_record("/repo"),
            response_msg("user", "hello"),
            response_msg("user", "hello"), // duplicate re-send
            user_msg("hello"),             // same content via event_msg
        ],
    );
    let h = read_codex_session(&path).expect("read");
    assert_eq!(h.turns.len(), 1);
    assert_eq!(read_texts(&h), vec!["hello"]);
}

#[test]
fn unknown_records_surface_warning() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let path = write_rollout(
        &config,
        "/repo",
        &[
            meta_record("/repo"),
            json!({ "type": "future-record-type", "payload": {} }),
            user_msg("hi"),
        ],
    );
    let h = read_codex_session(&path).expect("read");
    assert!(
        h.warnings.iter().any(|w| w.code == "unknown_records_skipped"),
        "{:?}",
        h.warnings
    );
    assert_eq!(h.turns.len(), 1);
}

// ── resolve ────────────────────────────────────────────────────────────────

#[test]
fn resolve_accepts_latest_uuid_and_title() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let cwd = "/Users/me/repo";
    write_rollout(&config, cwd, &[meta_record(cwd), user_msg("setup dev env")]);

    let latest = resolve_codex_session(Path::new(cwd), Some(&config), None).expect("latest");
    assert_eq!(latest.session_id, UUID);
    let by_uuid = resolve_codex_session(Path::new(cwd), Some(&config), Some(UUID)).expect("uuid");
    assert_eq!(by_uuid.session_id, UUID);
    let by_title = resolve_codex_session(Path::new(cwd), Some(&config), Some("dev env")).expect("title");
    assert_eq!(by_title.session_id, UUID);
}

#[test]
fn resolve_ambiguous_or_missing_fails_cleanly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".codex");
    let cwd = "/Users/me/repo";
    write_rollout(&config, cwd, &[meta_record(cwd), user_msg("fix login bug")]);

    let missing = resolve_codex_session(Path::new(cwd), Some(&config), Some("99999999-9999-9999-9999-999999999999"))
        .expect_err("missing uuid");
    assert!(matches!(missing, TransferError::NoSession(_)));
    let no_match =
        resolve_codex_session(Path::new(cwd), Some(&config), Some("nothing-matches-this")).expect_err("no text match");
    assert!(matches!(no_match, TransferError::NoSession(_)));
}

// ── prompt building ────────────────────────────────────────────────────────

#[test]
fn codex_prompt_bounds_turns_and_includes_safety() {
    let transfer = CodexTransfer {
        tool: "codex".into(),
        source: "cli".into(),
        session_id: UUID.into(),
        path: "/tmp/x.jsonl".into(),
        title: Some("The Task".into()),
        cwd: Some("/repo".into()),
        branch: Some("main".into()),
        created_at: Some("2026-07-29T09:06:50.000Z".into()),
        updated_at: Some("2026-07-29T09:10:00.000Z".into()),
        turns: (0..60)
            .map(|i| TransferTurn {
                role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
                text: format!("message {i}"),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                inert: true,
            })
            .collect(),
        warnings: Vec::new(),
        last_user_request: Some("message 58".into()),
        last_assistant_action: Some("message 59".into()),
    };
    let prompt = build_codex_handoff_prompt(&transfer, 0);
    assert!(prompt.starts_with(CODEX_TRANSFER_PROMPT_PREFIX));
    assert!(prompt.contains("## Safety boundary"));
    assert!(prompt.contains("last 40 of 60 turns"));
    assert!(prompt.contains("message 59"));
    assert!(!prompt.contains("message 0"));
}
