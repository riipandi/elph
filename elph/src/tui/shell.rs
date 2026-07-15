//! Root shell: layout zones, global keyboard handling, and session state.

use chrono::Local;
use iocraft::prelude::*;
use std::time::Duration;

use crate::platform::Paths;
use crate::types::{AgentMode, ThinkingLevel, is_quit_command};

use super::header::Header;
use super::labels::session_label;
use super::prompt_chrome::PromptChrome;
use super::session_prefs::persist_session_prefs;
use super::status_row::StatusRow;
use super::transcript::{TranscriptMessage, TranscriptPanel, TranscriptStyle, seed_transcript_messages};

#[derive(Default, Props)]
pub struct MainShellProps {
    pub resume_id: Option<String>,
    pub initial_agent_mode: AgentMode,
    pub initial_thinking_level: ThinkingLevel,
    pub model_label: String,
    pub project_label: String,
    pub sticky_scroll: bool,
}

#[component]
pub fn MainShell(props: &mut MainShellProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(Local::now);
    let mut should_exit = hooks.use_state(|| false);
    let mut agent_mode = hooks.use_state(|| props.initial_agent_mode);
    let mut thinking_level = hooks.use_state(|| props.initial_thinking_level);
    let mut draft = hooks.use_state(String::new);
    let mut live_draft = hooks.use_ref(String::new);
    let mut messages = hooks.use_state(seed_transcript_messages);
    let mut suppress_enter_newline = hooks.use_ref(|| false);
    let session_label = session_label(props.resume_id.as_deref());
    let paths = hooks.use_state(|| Paths::resolve().expect("resolve elph paths"));

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            time.set(Local::now());
        }
    });

    hooks.use_terminal_events({
        let paths = paths.read().clone();
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }

            match (modifiers, code) {
                (m, KeyCode::Char('a')) if m.contains(KeyModifiers::CONTROL) => {
                    let next = agent_mode.get().next();
                    agent_mode.set(next);
                    persist_session_prefs(&paths, next, thinking_level.get());
                }
                (m, KeyCode::BackTab) if m.contains(KeyModifiers::SHIFT) => {
                    let next = thinking_level.get().next();
                    thinking_level.set(next);
                    persist_session_prefs(&paths, agent_mode.get(), next);
                }
                (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => should_exit.set(true),
                _ => {}
            }
        }
    });

    if should_exit.get() {
        system.exit();
    }

    let time_label = format!("Current Time: {}", time.get().format("%r"));

    element! {
        View(
            width: screen_width,
            height: screen_height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            margin: 0,
            padding: 0,
        ) {
            Header(
                screen_width: screen_width,
                session_label: session_label,
            )
            TranscriptPanel(
                screen_width: screen_width,
                messages: messages.read().clone(),
                sticky_scroll: props.sticky_scroll,
            )
            StatusRow(
                screen_width: screen_width,
                time_label: time_label,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                project_label: props.project_label.clone(),
                model_label: props.model_label.clone(),
                draft: Some(draft),
                live_draft: Some(live_draft),
                suppress_enter_newline: Some(suppress_enter_newline),
                on_submit: move |text: String| {
                    if text.trim().is_empty() {
                        return;
                    }
                    if is_quit_command(&text) {
                        should_exit.set(true);
                        draft.set(String::new());
                        live_draft.set(String::new());
                        suppress_enter_newline.set(true);
                        return;
                    }
                    messages.set({
                        let mut list = messages.read().clone();
                        list.push(TranscriptMessage {
                            content: text,
                            style: TranscriptStyle::User,
                        });
                        list
                    });
                    draft.set(String::new());
                    live_draft.set(String::new());
                    suppress_enter_newline.set(true);
                },
            )
        }
    }
}
