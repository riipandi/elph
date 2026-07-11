use std::collections::HashMap;

use elph_ai::api::http_proxy::{
    UNSUPPORTED_PROXY_PROTOCOL_MESSAGE, resolve_http_proxy_url_for_target, websocket_proxy_lookup_url,
};
use elph_ai::types::ProviderEnv;

const TARGET_URL: &str = "https://bedrock-runtime.us-east-1.amazonaws.com";

fn env_with(vars: &[(&str, &str)]) -> ProviderEnv {
    vars.iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>()
}

#[test]
fn maps_websocket_urls_for_proxy_lookup() {
    assert_eq!(
        websocket_proxy_lookup_url("wss://chatgpt.com/backend-api/codex/responses"),
        "https://chatgpt.com/backend-api/codex/responses"
    );
    assert_eq!(websocket_proxy_lookup_url("ws://localhost:9001/ws"), "http://localhost:9001/ws");
}

#[test]
fn resolves_proxy_for_websocket_target_urls() {
    let env = env_with(&[("HTTPS_PROXY", "http://proxy.example:8080")]);
    let ws_target = websocket_proxy_lookup_url("wss://chatgpt.com/backend-api/codex/responses");
    let proxy = resolve_http_proxy_url_for_target(&ws_target, Some(&env))
        .expect("resolve")
        .expect("proxy");
    assert_eq!(proxy.as_str(), "http://proxy.example:8080/");
}

#[test]
fn respects_no_proxy_exclusions() {
    let env = env_with(&[
        ("HTTPS_PROXY", "http://proxy.example:8080"),
        ("NO_PROXY", "bedrock-runtime.us-east-1.amazonaws.com"),
    ]);

    let proxy = resolve_http_proxy_url_for_target(TARGET_URL, Some(&env)).expect("resolve");
    assert!(proxy.is_none());
}

#[test]
fn resolves_https_proxy_url() {
    let env = env_with(&[("HTTPS_PROXY", "http://proxy.example:8080")]);

    let proxy = resolve_http_proxy_url_for_target(TARGET_URL, Some(&env))
        .expect("resolve")
        .expect("proxy");
    assert_eq!(proxy.as_str(), "http://proxy.example:8080/");
}

#[test]
fn resolves_all_proxy_url() {
    let env = env_with(&[("ALL_PROXY", "http://all-proxy.example:3128")]);

    let proxy = resolve_http_proxy_url_for_target(TARGET_URL, Some(&env))
        .expect("resolve")
        .expect("proxy");
    assert_eq!(proxy.as_str(), "http://all-proxy.example:3128/");
}

#[test]
fn rejects_unsupported_proxy_protocols() {
    let env = env_with(&[("HTTPS_PROXY", "socks5://proxy.example:1080")]);

    let error = resolve_http_proxy_url_for_target(TARGET_URL, Some(&env)).expect_err("reject socks");
    assert!(error.to_string().contains(UNSUPPORTED_PROXY_PROTOCOL_MESSAGE));
}
