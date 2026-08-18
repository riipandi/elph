use elph_agent::DEFAULT_SYSTEM_PROMPT;
use elph_agent::resolve_system_prompt_text;

#[test]
fn default_prompt_is_generic_assistant() {
    assert!(DEFAULT_SYSTEM_PROMPT.contains("efficient"));
}

#[test]
fn resolve_empty_uses_default() {
    assert_eq!(resolve_system_prompt_text(None), DEFAULT_SYSTEM_PROMPT);
    assert_eq!(resolve_system_prompt_text(Some("   ")), DEFAULT_SYSTEM_PROMPT);
    assert_eq!(resolve_system_prompt_text(Some("custom")), "custom");
}
