use elph_ai::api::openai_compat::{detect_compat, get_compat};
use elph_ai::get_builtin_model;

#[test]
fn detect_compat_matches_pi_ai_defaults() {
    let model = get_builtin_model("deepseek", "deepseek-v4-flash").expect("model exists");
    let compat = detect_compat(&model);
    assert_eq!(compat.thinking_format, "deepseek");
    assert!(!compat.supports_store);
    assert!(compat.requires_reasoning_content_on_assistant_messages);

    let openrouter = get_builtin_model("openrouter", "anthropic/claude-3-haiku").expect("model exists");
    let or_compat = get_compat(&openrouter);
    assert_eq!(or_compat.thinking_format, "openrouter");
    assert_eq!(or_compat.cache_control_format.as_deref(), Some("anthropic"));
}
