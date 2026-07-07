use elph_ai::api::openai_prompt_cache::{OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH, clamp_openai_prompt_cache_key};

#[test]
fn clamps_prompt_cache_key_to_sixty_four_characters() {
    let long = "k".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 10);
    let clamped = clamp_openai_prompt_cache_key(Some(&long)).expect("key");
    assert_eq!(clamped.chars().count(), OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH);
}

#[test]
fn returns_none_for_missing_prompt_cache_key() {
    assert!(clamp_openai_prompt_cache_key(None).is_none());
}
