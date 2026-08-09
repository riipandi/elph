//! Unit tests for the Claude session handover reader.

use std::fs;
use std::path::Path;

use serde_json::json;

use super::*;

// ── helpers ────────────────────────────────────────────────────────────────

/// Write `lines` (serde values) as JSONL into `dir/<uuid>.jsonl` under a test
/// Claude config dir, touching the file mtime to `mtime_unix_ms`.
fn write_session(config_dir: &Path, cwd: &Path, uuid: &str, lines: &[serde_json::Value], mtime_unix_ms: u64) {
    let project_dir = config_dir.join("projects").join(slugify(cwd));
    fs::create_dir_all(&project_dir).expect("create project dir");
    let path = project_dir.join(format!("{uuid}.jsonl"));
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write transcript");
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_millis(mtime_unix_ms);
    if let Ok(file) = fs::File::open(&path) {
        let _ = file.set_times(std::fs::FileTimes::new().set_modified(modified));
    }
}

fn user_record(uuid: &str, parent: Option<&str>, text: &str, timestamp: &str) -> serde_json::Value {
    user_record_with_cwd(uuid, parent, text, timestamp, "/repo", Some("main"))
}

fn user_record_with_cwd(
    uuid: &str,
    parent: Option<&str>,
    text: &str,
    timestamp: &str,
    cwd: &str,
    branch: Option<&str>,
) -> serde_json::Value {
    let mut record = json!({
        "type": "user",
        "uuid": uuid,
        "timestamp": timestamp,
        "cwd": cwd,
        "message": { "role": "user", "content": [ { "type": "text", "text": text } ] },
        "sessionId": "session-uuid",
        "version": "2.1.0",
    });
    if let Some(branch) = branch {
        record["gitBranch"] = json!(branch);
    }
    if let Some(parent) = parent {
        record["parentUuid"] = json!(parent);
    }
    record
}

fn assistant_text_record(uuid: &str, parent: Option<&str>, text: &str, timestamp: &str) -> serde_json::Value {
    let mut record = json!({
        "type": "assistant",
        "uuid": uuid,
        "timestamp": timestamp,
        "message": { "role": "assistant", "content": [ { "type": "text", "text": text } ] },
    });
    if let Some(parent) = parent {
        record["parentUuid"] = json!(parent);
    }
    record
}

fn assistant_tool_call_record(
    uuid: &str,
    parent: Option<&str>,
    tool_name: &str,
    input: serde_json::Value,
    timestamp: &str,
) -> serde_json::Value {
    let mut record = json!({
        "type": "assistant",
        "uuid": uuid,
        "timestamp": timestamp,
        "message": {
            "role": "assistant",
            "content": [ { "type": "tool_use", "id": format!("tool_{}", tool_name), "name": tool_name, "input": input } ]
        },
    });
    if let Some(parent) = parent {
        record["parentUuid"] = json!(parent);
    }
    record
}

fn tool_result_record(
    uuid: &str,
    parent: &str,
    tool_use_id: &str,
    content: &str,
    timestamp: &str,
    is_error: bool,
) -> serde_json::Value {
    json!({
        "type": "user",
        "uuid": uuid,
        "parentUuid": parent,
        "timestamp": timestamp,
        "message": {
            "role": "user",
            "content": [ { "type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error } ]
        },
    })
}

fn meta_record(uuid: &str, parent: Option<&str>, text: &str, timestamp: &str) -> serde_json::Value {
    let mut record = json!({
        "type": "user",
        "uuid": uuid,
        "timestamp": timestamp,
        "isMeta": true,
        "message": { "role": "user", "content": [ { "type": "text", "text": text } ] },
    });
    if let Some(parent) = parent {
        record["parentUuid"] = json!(parent);
    }
    record
}

// ── slugify / cwd ──────────────────────────────────────────────────────────

#[test]
fn slugify_matches_reference() {
    assert_eq!(slugify(Path::new("/Users/ariss/dev/my-repo")), "-Users-ariss-dev-my-repo");
    assert_eq!(slugify(Path::new("a/b c.d")), "a-b-c-d");
}

#[test]
fn cwd_within_handles_subdirs() {
    assert!(cwd_is_within("/repo", "/repo"));
    assert!(cwd_is_within("/repo/src", "/repo"));
    assert!(!cwd_is_within("/repo2", "/repo"));
    assert!(!cwd_is_within("/repo-other", "/repo"));
    assert!(!cwd_is_within("/other", "/repo"));
}

// ── discovery ──────────────────────────────────────────────────────────────

#[test]
fn discover_lists_only_matching_cwds_newest_first() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");

    write_session(
        &config,
        cwd,
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        &[user_record("u1", None, "first", "2026-07-01T00:00:00.000Z")],
        1000,
    );
    write_session(
        &config,
        cwd,
        "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
        &[user_record("u2", None, "second", "2026-07-02T00:00:00.000Z")],
        2000,
    );
    // Another repo entirely (descendant-slug trick: `-Users-...-other`).
    // Note: the cwd's *content* field is authoritative — a transcript stored
    // under `/repo-other` with content-cwd `/repo-other` must be excluded from
    // a `/repo` discovery.
    let other_cwd = Path::new("/repo-other");
    write_session(
        &config,
        other_cwd,
        "cccccccc-cccc-cccc-cccc-cccccccccccc",
        &[user_record_with_cwd(
            "u3",
            None,
            "other",
            "2026-07-03T00:00:00.000Z",
            "/repo-other",
            Some("main"),
        )],
        3000,
    );

    let sessions = discover_claude_sessions_with_config(cwd, &config);
    assert_eq!(sessions.len(), 2, "other repo must be excluded: {sessions:?}");
    assert_eq!(sessions[0].session_id, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "newest first");
    assert_eq!(sessions[1].session_id, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    assert_eq!(sessions[0].updated_at_ms, 2000);
    // Title from user text (no custom-title record).
    assert_eq!(sessions[0].title, "second");
    assert_eq!(sessions[0].branch.as_deref(), Some("main"));
}

#[test]
fn discover_includes_subdirectory_sessions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    // Session launched from an ancestor ($HOME) but cd'd into /repo/src.
    let home = Path::new("/Users/ariss");
    let cwd = Path::new("/Users/ariss/Developer/repo/src");
    write_session(
        &config,
        home,
        "dddddddd-dddd-dddd-dddd-dddddddddddd",
        &[json!({
            "type": "user",
            "uuid": "u-disc",
            "timestamp": "2026-07-04T00:00:00.000Z",
            "cwd": "/Users/ariss/Developer/repo/src",
            "message": { "role": "user", "content": [{ "type": "text", "text": "from ancestor" }] }
        })],
        4000,
    );
    let sessions = discover_claude_sessions_with_config(cwd, &config);
    assert!(
        sessions
            .iter()
            .any(|s| s.session_id == "dddddddd-dddd-dddd-dddd-dddddddddddd"),
        "ancestor-dir session with matching content cwd must be listed: {sessions:?}"
    );
}

#[test]
fn discover_ignores_non_uuid_and_empty_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let project_dir = config.join("projects").join(slugify(cwd));
    fs::create_dir_all(&project_dir).expect("create dir");
    fs::write(project_dir.join("not-a-uuid.jsonl"), "{}").expect("write");
    fs::write(project_dir.join("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee.jsonl"), "").expect("write empty");
    let sessions = discover_claude_sessions_with_config(cwd, &config);
    assert!(sessions.is_empty());
}

// ── title resolution ───────────────────────────────────────────────────────

#[test]
fn title_prefers_custom_then_ai_then_prompt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let mut records = vec![user_record("u1", None, "help me", "2026-07-01T00:00:00.000Z")];
    // A first custom title, then a later AI title, then a later custom title.
    records
        .push(json!({ "type": "custom-title", "customTitle": "old custom", "timestamp": "2026-07-01T00:00:01.000Z" }));
    records.push(json!({ "type": "ai-title", "aiTitle": "ai title", "timestamp": "2026-07-01T00:00:02.000Z" }));
    records.push(
        json!({ "type": "custom-title", "customTitle": "final custom", "timestamp": "2026-07-01T00:00:03.000Z" }),
    );
    records
        .push(json!({ "type": "last-prompt", "lastPrompt": "last prompt", "timestamp": "2026-07-01T00:00:04.000Z" }));
    write_session(&config, cwd, "ffffffff-ffff-ffff-ffff-ffffffffffff", &records, 5000);

    let sessions = discover_claude_sessions_with_config(cwd, &config);
    assert_eq!(sessions[0].title, "final custom");

    let handover = read_claude_session(&sessions[0].path).expect("read");
    // custom-title > ai-title > last-prompt > summary (reference priorities).
    assert_eq!(handover.title.as_deref(), Some("final custom"));

    // Discovery (light read) also resolves the same priority chain.
    assert_eq!(discover_claude_sessions_with_config(cwd, &config)[0].title, "final custom");
}

#[test]
fn title_falls_back_to_last_user_text() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    write_session(
        &config,
        cwd,
        "11111111-1111-1111-1111-111111111111",
        &[
            user_record("u1", None, "first prompt", "2026-07-01T00:00:00.000Z"),
            assistant_text_record("a1", Some("u1"), "ok", "2026-07-01T00:00:01.000Z"),
            user_record("u2", Some("a1"), "second prompt", "2026-07-01T00:00:02.000Z"),
        ],
        5000,
    );
    let handover = read_claude_session(
        &config
            .join("projects")
            .join(slugify(cwd))
            .join("11111111-1111-1111-1111-111111111111.jsonl"),
    )
    .expect("read");
    assert_eq!(handover.title.as_deref(), Some("second prompt"));
    assert_eq!(handover.turns.len(), 3);
    assert_eq!(handover.last_user_request.as_deref(), Some("second prompt"));
}

// ── full read: chain / tools / meta ────────────────────────────────────────

#[test]
fn reads_leaf_chain_with_tool_calls_and_results() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("22222222-2222-2222-2222-222222222222.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    let records = vec![
        user_record("u1", None, "chat about bug", "2026-07-01T00:00:00.000Z"),
        assistant_tool_call_record("a1", Some("u1"), "grep", json!({"pattern": "bug"}), "2026-07-01T00:00:01.000Z"),
        tool_result_record("t1", "a1", "tool_grep", "no matches", "2026-07-01T00:00:02.000Z", false),
        assistant_text_record("a2", Some("t1"), "let me search again", "2026-07-01T00:00:03.000Z"),
        user_record("u2", Some("a2"), "yes do it", "2026-07-01T00:00:04.000Z"),
    ];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    assert_eq!(handover.turns.len(), 5);
    assert_eq!(handover.turns[0].role, "user");
    assert_eq!(handover.turns[0].text, "chat about bug");
    // Tool call turn.
    assert_eq!(handover.turns[1].role, "assistant");
    assert_eq!(handover.turns[1].tool_calls.len(), 1);
    assert_eq!(handover.turns[1].tool_calls[0].name, "grep");
    assert!(handover.turns[1].tool_calls[0].inert);
    // Tool result turn.
    assert_eq!(handover.turns[2].role, "user");
    assert_eq!(handover.turns[2].tool_results.len(), 1);
    assert_eq!(handover.turns[2].tool_results[0].tool_use_id.as_deref(), Some("tool_grep"));
    assert_eq!(handover.turns[2].tool_results[0].content, "no matches");
    // Last signals.
    assert_eq!(handover.last_user_request.as_deref(), Some("yes do it"));
    assert_eq!(handover.last_assistant_action.as_deref(), Some("let me search again"));
    assert_eq!(handover.cwd.as_deref(), Some("/repo"));
    assert_eq!(handover.branch.as_deref(), Some("main"));
    assert_eq!(handover.created_at.as_deref(), Some("2026-07-01T00:00:00.000Z"));
    assert_eq!(handover.updated_at.as_deref(), Some("2026-07-01T00:00:04.000Z"));
}

#[test]
fn meta_and_sidechain_records_are_skipped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("33333333-3333-3333-3333-333333333333.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    let records = vec![
        meta_record("m1", None, "SessionStart hook", "2026-07-01T00:00:00.000Z"),
        user_record("u1", Some("m1"), "real prompt", "2026-07-01T00:00:01.000Z"),
        json!({
            "type": "assistant",
            "uuid": "side1",
            "parentUuid": "u1",
            "isSidechain": true,
            "timestamp": "2026-07-01T00:00:02.000Z",
            "message": { "role": "assistant", "content": [ { "type": "text", "text": "sidechain junk" } ] }
        }),
        assistant_text_record("a1", Some("u1"), "main reply", "2026-07-01T00:00:03.000Z"),
    ];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    assert_eq!(handover.turns.len(), 2, "meta + sidechain must be skipped: {handover:?}");
    assert_eq!(handover.turns[0].text, "real prompt");
    assert_eq!(handover.turns[1].text, "main reply");
}

#[test]
fn content_replacement_stubs_are_marked_unavailable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("44444444-4444-4444-4444-444444444444.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    let records = vec![
        json!({
            "type": "content-replacement",
            "replacements": [ { "toolUseId": "tool_big" } ],
            "timestamp": "2026-07-01T00:00:00.000Z"
        }),
        assistant_tool_call_record("a1", None, "write_file", json!({"path": "big.out"}), "2026-07-01T00:00:01.000Z"),
        tool_result_record("t1", "a1", "tool_big", "huge output", "2026-07-01T00:00:02.000Z", false),
    ];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    assert_eq!(handover.turns.len(), 2, "content-replacement record is non-conversational");
    let result = &handover.turns[1].tool_results[0];
    assert!(result.unavailable);
    assert_eq!(result.content, "[output summarized/stored elsewhere]");
}

#[test]
fn generated_meta_and_thinking_blocks_are_dropped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("55555555-5555-5555-5555-555555555555.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    let records = vec![json!({
        "type": "assistant",
        "uuid": "a1",
        "parentUuid": null,
        "timestamp": "2026-07-01T00:00:00.000Z",
        "message": {
            "role": "assistant",
            "content": [
                { "type": "thinking", "thinking": "hidden reasoning" },
                { "type": "text", "text": "visible reply" },
                { "type": "text", "text": "[Request interrupted by user]" },
                { "type": "text", "text": "<command-name>/help</command-name>" },
            ]
        }
    })];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    assert_eq!(handover.turns.len(), 1);
    assert_eq!(handover.turns[0].text, "visible reply");
}

#[test]
fn unknown_record_types_surface_warning() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("66666666-6666-6666-6666-666666666666.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    let records = vec![
        json!({ "type": "brand-new-type", "uuid": "x1", "timestamp": "2026-07-01T00:00:00.000Z" }),
        user_record("u1", None, "hi", "2026-07-01T00:00:01.000Z"),
    ];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    assert!(
        handover.warnings.iter().any(|w| w.code == "unknown_records_skipped"),
        "warnings: {:?}",
        handover.warnings
    );
    assert_eq!(handover.turns.len(), 1);
}

// ── compaction boundaries / preserved segments ─────────────────────────────

#[test]
fn pre_compact_history_before_unpreserved_boundary_is_dropped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    let path = config
        .join("projects")
        .join(slugify(cwd))
        .join("77777777-7777-7777-7777-777777777777.jsonl");
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    // Old conversation, compacted away (boundary without a preserved segment).
    let records = vec![
        user_record("old1", None, "pre-compact prompt", "2026-07-01T00:00:00.000Z"),
        assistant_text_record("old2", Some("old1"), "pre-compact reply", "2026-07-01T00:00:01.000Z"),
        json!({
            "type": "system",
            "subtype": "compact_boundary",
            "uuid": "boundary1",
            "parentUuid": "old2",
            "timestamp": "2026-07-01T00:00:02.000Z",
        }),
        user_record("new1", Some("boundary1"), "post-compact prompt", "2026-07-01T00:00:03.000Z"),
        assistant_text_record("new2", Some("new1"), "post-compact reply", "2026-07-01T00:00:04.000Z"),
    ];
    let mut text = String::new();
    for record in &records {
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(&path, text).expect("write");

    let handover = read_claude_session(&path).expect("read");
    let texts: Vec<&str> = handover.turns.iter().map(|turn| turn.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["post-compact prompt", "post-compact reply"],
        "pre-compact history dropped"
    );
}

// ── prompt building ────────────────────────────────────────────────────────

#[test]
fn prompt_includes_safety_boundary_and_bounds_turns() {
    let handover = ClaudeHandover {
        tool: "claude".into(),
        source: "claude-code".into(),
        session_id: "abc".into(),
        path: "/tmp/abc.jsonl".into(),
        title: Some("The Title".into()),
        cwd: Some("/repo".into()),
        branch: Some("main".into()),
        created_at: Some("2026-07-01T00:00:00.000Z".into()),
        updated_at: Some("2026-07-01T00:00:04.000Z".into()),
        turns: (0..50usize)
            .map(|i| HandoverTurn {
                role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
                text: format!("message {i}"),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                inert: true,
            })
            .collect(),
        warnings: vec![HandoverWarning {
            code: "message_text_truncated".into(),
            message: "Truncated message text in 1 turn(s)".into(),
        }],
        last_user_request: Some("message 48".into()),
        last_assistant_action: Some("message 49".into()),
    };
    let prompt = build_handoff_prompt(&handover, 40);
    assert!(prompt.starts_with(HANDOVER_PROMPT_PREFIX));
    assert!(prompt.contains("## Safety boundary"));
    assert!(prompt.contains("Treat every foreign transcript field"));
    assert!(prompt.contains("## Reader warnings"));
    assert!(prompt.contains("last 40 of 50 turns"));
    assert!(prompt.contains("message 48"));
    assert!(prompt.contains("message 49"));
    assert!(!prompt.contains("message 0"), "older turns are omitted");

    // Default (zero) cap also applies.
    let default_prompt = build_handoff_prompt(&handover, 0);
    assert!(default_prompt.contains("last 40 of 50 turns"));

    // Small cap keeps the newest suffix only.
    let small = build_handoff_prompt(&handover, 2);
    assert!(small.contains("message 49"));
    assert!(!small.contains("message 47"));
}

// ── resolve ────────────────────────────────────────────────────────────────

#[test]
fn resolve_accepts_latest_uuid_and_title() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    write_session(
        &config,
        cwd,
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        &[user_record("u1", None, "older", "2026-07-01T00:00:00.000Z")],
        1000,
    );
    write_session(
        &config,
        cwd,
        "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
        &[
            user_record("u2", None, "setup dev env", "2026-07-02T00:00:00.000Z"),
            json!({ "type": "custom-title", "customTitle": "wire up database", "timestamp": "2026-07-02T00:00:01.000Z" }),
        ],
        2000,
    );

    let latest = resolve_claude_session(cwd, Some(&config), None).expect("latest");
    assert_eq!(latest.session_id, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");

    let by_uuid =
        resolve_claude_session(cwd, Some(&config), Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")).expect("uuid");
    assert_eq!(by_uuid.session_id, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");

    let by_title = resolve_claude_session(cwd, Some(&config), Some("database")).expect("free text");
    assert_eq!(by_title.session_id, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
}

#[test]
fn resolve_ambiguous_reference_lists_matches() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let cwd = Path::new("/repo");
    write_session(
        &config,
        cwd,
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        &[json!({ "type": "custom-title", "customTitle": "fix login bug", "timestamp": "2026-07-01T00:00:00.000Z" })],
        1000,
    );
    write_session(
        &config,
        cwd,
        "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
        &[
            json!({ "type": "custom-title", "customTitle": "another login fix", "timestamp": "2026-07-02T00:00:00.000Z" }),
        ],
        2000,
    );
    let err = resolve_claude_session(cwd, Some(&config), Some("login")).expect_err("ambiguous");
    match err {
        HandoverError::Ambiguous { matches, .. } => assert_eq!(matches.len(), 2),
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn resolve_unknown_id_fails_cleanly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join(".claude");
    let err = resolve_claude_session(Path::new("/repo"), Some(&config), Some("99999999-9999-9999-9999-999999999999"))
        .expect_err("missing");
    assert!(matches!(err, HandoverError::NoSession(_)));
}
