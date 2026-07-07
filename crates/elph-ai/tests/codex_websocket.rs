use elph_ai::{
    close_codex_websocket_sessions, get_codex_websocket_debug_stats, get_codex_websocket_input_delta,
    reset_codex_websocket_debug_stats,
};
use serde_json::json;

#[test]
fn cached_input_delta_extracts_only_new_messages() {
    let continuation_body = json!({
        "model": "gpt-5",
        "store": false,
        "input": [{ "role": "user", "content": "hello" }]
    });
    let body = json!({
        "model": "gpt-5",
        "store": false,
        "input": [
            { "role": "user", "content": "hello" },
            { "role": "assistant", "content": "hi" },
            { "role": "user", "content": "world" }
        ]
    });
    let delta = get_codex_websocket_input_delta(
        &body,
        &continuation_body,
        &json!([{ "role": "assistant", "content": "hi" }]),
    )
    .expect("delta");
    assert_eq!(delta, json!([{ "role": "user", "content": "world" }]));
}

#[test]
fn debug_stats_reset_clears_session_state() {
    close_codex_websocket_sessions(Some("session-test"));
    reset_codex_websocket_debug_stats(Some("session-test"));
    assert!(get_codex_websocket_debug_stats("session-test").is_none());
}
