//! Build scroll-view bubbles from transcript messages.

use iocraft::prelude::*;

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::kinds::{
    chat_response_card, error_card, meta_card, skill_prompt_card, suppressed_sticky_user_prompt_card, thinking_card,
    thinking_response_pair_card, tool_call_card, user_prompt_card,
};

pub fn build_transcript_bubbles(
    screen_width: u16,
    messages: &[TranscriptMessage],
    suppress_sticky_source: Option<usize>,
) -> Vec<AnyElement<'static>> {
    let mut bubbles = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let next_style = messages.get(index + 1).map(|m| m.style);
        if let Some(next) = messages.get(index + 1)
            && message.style.forms_flush_pair_with(next.style)
        {
            let after_pair = messages.get(index + 2).map(|m| m.style);
            let margin_bottom = TranscriptStyle::Assistant.entry_gap_after(after_pair);
            bubbles.push(thinking_response_pair_card(screen_width, message, next, margin_bottom));
            index += 2;
            continue;
        }
        let margin_bottom = message.style.entry_gap_after(next_style);
        bubbles.push(transcript_message_bubble(
            screen_width,
            message,
            margin_bottom,
            suppress_sticky_source == Some(index),
        ));
        index += 1;
    }
    bubbles
}

pub fn transcript_message_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    suppress_sticky_source: bool,
) -> AnyElement<'static> {
    match message.style {
        TranscriptStyle::User if suppress_sticky_source => {
            suppressed_sticky_user_prompt_card(screen_width, message, margin_bottom)
        }
        TranscriptStyle::User => user_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::SkillPrompt => skill_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::Thinking => thinking_card(screen_width, message, margin_bottom),
        TranscriptStyle::Assistant => chat_response_card(screen_width, message, margin_bottom),
        TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed => {
            tool_call_card(screen_width, message, margin_bottom)
        }
        TranscriptStyle::Error => error_card(screen_width, message, margin_bottom),
        TranscriptStyle::Meta => meta_card(screen_width, message, margin_bottom),
    }
}
