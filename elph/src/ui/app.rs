use crate::runtime::exit_message::ExitSnapshot;
use crate::runtime::{SHOULD_KILL_PARENT, WAS_INTERRUPTED, exit_message, handle_prompt_interrupt};
use elph_tui::{
    AgentMode, ChatStream, PromptInput, enable_keyboard_enhancement, is_interrupt_key, is_quit_command, sigint_channel,
};
use iocraft::prelude::*;
use signal_hook::consts::SIGINT;

#[component]
pub fn App(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut prompt = hooks.use_state(String::new);
    let mut prompt_reset = hooks.use_state(|| 0u32);
    let mut messages = hooks.use_state(Vec::<String>::new);
    let mut mode = hooks.use_state(AgentMode::default);
    let mut should_exit = hooks.use_state(|| false);
    let session_id = hooks.use_state(exit_message::new_session_id);

    hooks.use_effect(
        || {
            let _ = enable_keyboard_enhancement();
        },
        (),
    );

    hooks.use_future(async move {
        let mut sigint = sigint_channel();
        while let Some(signal) = sigint.recv().await {
            if signal != SIGINT {
                continue;
            }
            handle_prompt_interrupt(&mut prompt, &mut should_exit, &mut prompt_reset);
        }
    });

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent {
            code, kind, modifiers, ..
        }) = event
        else {
            return;
        };

        if kind == KeyEventKind::Release {
            return;
        }

        if is_interrupt_key(code, modifiers) {
            handle_prompt_interrupt(&mut prompt, &mut should_exit, &mut prompt_reset);
            return;
        }

        if code == KeyCode::Char('q') && modifiers.contains(KeyModifiers::CONTROL) {
            should_exit.set(true);
            {
                use std::sync::atomic::Ordering;
                WAS_INTERRUPTED.store(true, Ordering::Relaxed);
                #[cfg(unix)]
                SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
            }
            return;
        }

        if code == KeyCode::Tab
            && !modifiers.contains(KeyModifiers::SHIFT)
            && !modifiers.contains(KeyModifiers::CONTROL)
        {
            mode.set(mode.get().next());
        }
    });

    if should_exit.get() {
        exit_message::record(ExitSnapshot {
            session_id: session_id.read().clone(),
            has_history: !messages.read().is_empty(),
        });
        system.exit();
    }

    element! {
        View(
            width,
            height,
            background_color: Color::Reset,
            flex_direction: FlexDirection::Column,
        ) {
            View(
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: 0,
                height: 100pct,
                width: 100pct,
                overflow: Overflow::Hidden,
                padding_left: 1,
                padding_right: 1,
                padding_top: 1,
            ) {
                ChatStream(messages: messages.read().clone())
            }
            View(
                flex_shrink: 0.0,
                width: 100pct,
                padding_left: 1,
                padding_right: 1,
                padding_bottom: 1,
            ) {
                PromptInput(
                    value: Some(prompt),
                    reset_nonce: Some(prompt_reset),
                    model_name: "Claude Fable 5".to_string(),
                    mode: mode.get(),
                    has_focus: true,
                    on_submit: move |text: String| {
                        if is_quit_command(&text) {
                            prompt.set(String::new());
                            should_exit.set(true);
                            return;
                        }
                        let mut next = messages.read().clone();
                        next.push(text);
                        messages.set(next);
                    },
                    on_mode_change: |_| {},
                )
            }
        }
    }
}
