//! Build scroll-view bubbles from transcript messages.

use iocraft::prelude::*;

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::kinds::{
    chat_response_card, error_card, meta_card, skill_prompt_card, status_line_card, suppressed_sticky_user_prompt_card,
    thinking_card, thinking_response_pair_card, tool_call_card, user_prompt_card,
};
use super::toggle_ctx::CollapsibleToggleCtx;

pub fn build_transcript_bubbles(
    screen_width: u16,
    messages: &[TranscriptMessage],
    suppress_sticky_source: Option<usize>,
    toggle: Option<CollapsibleToggleCtx>,
) -> Vec<AnyElement<'static>> {
    let mut bubbles = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        if let Some(next) = messages.get(index + 1)
            && message.style.forms_flush_pair_with(next.style)
        {
            // Pair ends on the assistant (or thinking) sibling — gap after pair uses that row's state.
            let pair_last = next;
            let margin_bottom = pair_last.transcript_margin_bottom(messages.get(index + 2));
            bubbles.push(thinking_response_pair_card(
                screen_width,
                message,
                next,
                index,
                margin_bottom,
                toggle,
            ));
            index += 2;
            continue;
        }
        let margin_bottom = message.transcript_margin_bottom(messages.get(index + 1));
        bubbles.push(transcript_message_bubble(
            screen_width,
            message,
            index,
            margin_bottom,
            suppress_sticky_source == Some(index),
            toggle,
        ));
        index += 1;
    }
    bubbles
}

pub fn transcript_message_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    message_index: usize,
    margin_bottom: u16,
    suppress_sticky_source: bool,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    match message.style {
        TranscriptStyle::User if suppress_sticky_source => {
            suppressed_sticky_user_prompt_card(screen_width, message, margin_bottom)
        }
        TranscriptStyle::User => user_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::SkillPrompt => skill_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::Thinking => thinking_card(screen_width, message, margin_bottom, message_index, toggle),
        TranscriptStyle::Assistant => chat_response_card(screen_width, message, margin_bottom, message_index, toggle),
        TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed => {
            tool_call_card(screen_width, message, margin_bottom, message_index, toggle)
        }
        TranscriptStyle::Error => error_card(screen_width, message, margin_bottom),
        TranscriptStyle::Meta => meta_card(screen_width, message, margin_bottom),
        TranscriptStyle::StatusRunning | TranscriptStyle::StatusSuccess | TranscriptStyle::StatusFailed => {
            status_line_card(screen_width, message, margin_bottom)
        }
    }
}
