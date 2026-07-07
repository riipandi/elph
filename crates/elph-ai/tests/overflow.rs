mod common;

use common::error_assistant_message;
use elph_ai::utils::overflow::is_context_overflow;

#[test]
fn detects_explicit_ollama_prompt_too_long_errors() {
    let message = error_assistant_message("400 `prompt too long; exceeded max context length by 100918 tokens`");
    assert!(is_context_overflow(&message));
}

#[test]
fn detects_together_context_length_errors() {
    let message = error_assistant_message(
        "400 The input (516368 tokens) is longer than the model's context length (262144 tokens).",
    );
    assert!(is_context_overflow(&message));
}

#[test]
fn detects_openai_compatible_maximum_context_length_errors() {
    let message =
        error_assistant_message("Error: 400 Input length (265330) exceeds model's maximum context length (262144).");
    assert!(is_context_overflow(&message));
}

#[test]
fn does_not_treat_generic_errors_as_overflow() {
    let message = error_assistant_message("500 `model runner crashed unexpectedly`");
    assert!(!is_context_overflow(&message));
}

#[test]
fn detects_context_length_exceeded_code() {
    let message = error_assistant_message("context_length_exceeded: prompt is too long");
    assert!(is_context_overflow(&message));
}

#[test]
fn detects_input_is_too_long_message() {
    let message = error_assistant_message("400 input is too long for the model");
    assert!(is_context_overflow(&message));
}

#[test]
fn requires_error_stop_reason() {
    let mut message = error_assistant_message("context length exceeded");
    message.stop_reason = elph_ai::StopReason::Stop;
    assert!(!is_context_overflow(&message));
}
