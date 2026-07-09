//! Root iocraft application for the Owly interactive shell.

use std::sync::Arc;

use std::time::Duration;

use elph_agent::try_block_on;
use elph_tui::{
    AgentMode, DEFAULT_TRANSCRIPT_CAP, PromptInput, Theme, ToolExecutionState, disable_keyboard_enhancement,
    enable_keyboard_enhancement, is_force_quit_key, is_interrupt_key, is_quit_command, is_theme_toggle_key,
    push_capped,
};
use iocraft::prelude::*;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::env;
use crate::onboarding::{self, SetupCredentials};

use super::activity::ActivityBar;
use super::banner::{OwlyBanner, directory_display};
use super::chat_stream::OwlyChatStream;
use super::chrome::{H_INSET, SECTION_PAD};
use super::entries::OwlyEntry;
use super::launch::LaunchState;
use super::setup::SetupWizard;
use super::transcript::{TranscriptApplier, append_shell_lines, command_label_for_input, lines_to_entries};

struct SubmitInbox {
    rx: mpsc::UnboundedReceiver<String>,
    initial: Option<String>,
}

struct LaunchBootstrap {
    app_context: super::context::AppContext,
    startup_lines: Vec<String>,
    submit_tx: mpsc::UnboundedSender<String>,
    inbox: Arc<Mutex<SubmitInbox>>,
}

fn apply_ui_events(
    ui_rx: &mut mpsc::UnboundedReceiver<crate::ui_events::AgentUiEvent>,
    entries: &mut State<Vec<OwlyEntry>>,
    live_tools: &mut State<Vec<ToolExecutionState>>,
    show_thinking: bool,
    first: crate::ui_events::AgentUiEvent,
) {
    let mut entries_guard = entries.write();
    let mut live_tools_guard = live_tools.write();
    let mut applier = TranscriptApplier::new(&mut entries_guard, &mut live_tools_guard, show_thinking);
    applier.apply(first);
    while let Ok(event) = ui_rx.try_recv() {
        applier.apply(event);
    }
}

fn drain_ui_events(
    ui_rx: &mut mpsc::UnboundedReceiver<crate::ui_events::AgentUiEvent>,
    entries: &mut State<Vec<OwlyEntry>>,
    live_tools: &mut State<Vec<ToolExecutionState>>,
    show_thinking: bool,
) {
    let mut entries_guard = entries.write();
    let mut live_tools_guard = live_tools.write();
    let mut applier = TranscriptApplier::new(&mut entries_guard, &mut live_tools_guard, show_thinking);
    while let Ok(event) = ui_rx.try_recv() {
        applier.apply(event);
    }
}

struct KeyboardEnhancementGuard;

impl Drop for KeyboardEnhancementGuard {
    fn drop(&mut self) {
        if let Err(err) = disable_keyboard_enhancement() {
            tracing::warn!(error = %err, "failed to restore keyboard enhancements");
        }
    }
}

#[component]
pub fn OwlyRoot(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _keyboard_guard = KeyboardEnhancementGuard;

    let mut launch = hooks.use_ref(LaunchState::take);
    let pending_setup = launch.read().pending_setup;
    let default_provider = launch.read().provider.clone();
    let default_model = launch.read().model.clone();
    let setup_context = launch.read().app_context.clone();
    let mut setup_complete = hooks.use_state(|| !pending_setup);
    let mut provider = hooks.use_state(|| default_provider.clone());
    let mut model = hooks.use_state(|| default_model.clone());
    let mut setup_error = hooks.use_state(|| None::<String>);
    let mut theme = hooks.use_state(Theme::detect);

    let bootstrap = hooks.use_ref(|| {
        let mut state = launch.write();
        LaunchBootstrap {
            app_context: state.app_context.clone(),
            startup_lines: state.startup_lines.clone(),
            submit_tx: state.submit_tx.clone(),
            inbox: Arc::new(Mutex::new(SubmitInbox {
                rx: state.submit_rx.take().expect("submit receiver"),
                initial: state.initial.take(),
            })),
        }
    });

    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut prompt = hooks.use_state(String::new);
    let mut prompt_reset = hooks.use_state(|| 0u32);
    let startup_entries = lines_to_entries(&bootstrap.read().startup_lines);
    let mut entries = hooks.use_state(move || startup_entries);
    let mut live_tools = hooks.use_state(Vec::<ToolExecutionState>::new);
    let mut mode = hooks.use_state(|| AgentMode::Ask);
    let mut should_exit = hooks.use_state(|| false);
    let mut running = hooks.use_state(|| false);
    let mut active_command = hooks.use_state(|| None::<String>);

    let app_context = bootstrap.read().app_context.clone();
    let show_thinking = app_context.verbose();
    let directory = directory_display(app_context.cwd());
    let submit_tx = Arc::new(bootstrap.read().submit_tx.clone());

    hooks.use_effect(
        || {
            if let Err(err) = enable_keyboard_enhancement() {
                tracing::warn!(error = %err, "keyboard enhancement unavailable");
            }
        },
        (),
    );

    hooks.use_future({
        let inbox = bootstrap.read().inbox.clone();
        let app_context = bootstrap.read().app_context.clone();
        async move {
            loop {
                while !setup_complete.get() {
                    sleep(Duration::from_millis(25)).await;
                }

                let input = {
                    let mut guard = inbox.lock().await;
                    if let Some(text) = guard.initial.take() {
                        text
                    } else {
                        match guard.rx.recv().await {
                            Some(text) => text,
                            None => break,
                        }
                    }
                };

                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    push_capped(&mut entries.write(), OwlyEntry::user(trimmed), DEFAULT_TRANSCRIPT_CAP);
                }

                active_command.set(command_label_for_input(trimmed).map(|label| label.to_string()));
                live_tools.write().clear();
                running.set(true);
                let (ui_tx, mut ui_rx) = mpsc::unbounded_channel();
                let mut dispatch = Box::pin(app_context.dispatch(input, Some(ui_tx)));

                let turn_result = loop {
                    tokio::select! {
                        event = ui_rx.recv() => {
                            let Some(event) = event else {
                                continue;
                            };
                            apply_ui_events(
                                &mut ui_rx,
                                &mut entries,
                                &mut live_tools,
                                show_thinking,
                                event,
                            );
                        }
                        result = &mut dispatch => {
                            drain_ui_events(&mut ui_rx, &mut entries, &mut live_tools, show_thinking);
                            break result;
                        }
                    }
                };
                running.set(false);
                active_command.set(None);
                live_tools.write().clear();

                match turn_result {
                    Ok(result) => {
                        append_shell_lines(&mut entries.write(), &result.lines);
                        if result.should_exit {
                            should_exit.set(true);
                            break;
                        }
                    }
                    Err(err) => {
                        push_capped(
                            &mut entries.write(),
                            OwlyEntry::assistant(format!("Error: {err:#}")),
                            DEFAULT_TRANSCRIPT_CAP,
                        );
                    }
                }
            }
        }
    });

    hooks.use_terminal_events(move |event| {
        if !setup_complete.get() {
            return;
        }

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
            prompt.set(String::new());
            prompt_reset.set(prompt_reset.get().wrapping_add(1));
            return;
        }

        if is_force_quit_key(code, modifiers) {
            should_exit.set(true);
            return;
        }

        if is_theme_toggle_key(code, modifiers) {
            theme.set(theme.get().toggle());
        }
    });

    if should_exit.get() {
        system.exit();
    }

    let palette = theme.get();
    let busy = running.get();
    let active_command_label = active_command.read().clone();
    let provider_label = provider.read().clone();
    let model_label = model.read().clone();

    let show_setup = !setup_complete.get();

    element! {
        View(
            width,
            height,
            background_color: palette.view_background(),
            flex_direction: FlexDirection::Column,
        ) {
            #(if show_setup {
                element! {
                    SetupWizard(
                        default_provider: default_provider,
                        default_model: default_model,
                        theme: palette,
                        setup_error: Some(setup_error),
                        on_complete: move |credentials: SetupCredentials| {
                            setup_error.set(None);
                            let apply_context = setup_context.clone();
                            let persist_context = setup_context.clone();

                            let config = match try_block_on(async move {
                                let snapshot = apply_context.config_snapshot().await;
                                onboarding::apply_setup(credentials, &snapshot)
                            }) {
                                Ok(Ok(config)) => config,
                                Ok(Err(err)) => {
                                    setup_error.set(Some(format!("{err:#}")));
                                    return;
                                }
                                Err(_) => {
                                    setup_error.set(Some("Failed to apply setup.".to_string()));
                                    return;
                                }
                            };

                            if let Err(err) = env::setup_environment(&config) {
                                setup_error.set(Some(format!("{err:#}")));
                                return;
                            }

                            if try_block_on(persist_context.replace_config(config.clone())).is_err() {
                                setup_error.set(Some("Failed to update session configuration.".to_string()));
                                return;
                            }

                            launch.write().provider = config.provider.clone();
                            launch.write().model = config.model_id.clone();
                            provider.set(config.provider);
                            model.set(config.model_id);
                            setup_complete.set(true);
                        },
                    )
                }.into_any()
            } else {
                element! {
                    View(
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        width: 100pct,
                        height: 100pct,
                    ) {
                        OwlyBanner(
                            provider: provider_label.clone(),
                            model: model_label.clone(),
                            directory: directory.clone(),
                            version: env!("CARGO_PKG_VERSION").to_string(),
                            theme: palette,
                        )
                        View(
                            flex_grow: 1.0,
                            flex_shrink: 1.0,
                            min_height: 0,
                            height: 100pct,
                            width: 100pct,
                            overflow: Overflow::Hidden,
                            padding_left: H_INSET,
                            padding_right: H_INSET,
                            padding_top: SECTION_PAD,
                            padding_bottom: 0,
                        ) {
                            OwlyChatStream(
                                entries_state: Some(entries),
                                scroll_enabled: true,
                                theme: palette,
                                show_thinking: show_thinking,
                            )
                        }
                        #(if busy {
                            Some(element! {
                                ActivityBar(
                                    command: active_command_label,
                                    live_tools: Some(live_tools),
                                    theme: palette,
                                )
                            }.into_any())
                        } else {
                            None
                        })
                        View(
                            flex_shrink: 0.0,
                            width: 100pct,
                            padding_left: H_INSET,
                            padding_right: H_INSET,
                            padding_top: SECTION_PAD,
                            padding_bottom: 0,
                        ) {
                            PromptInput(
                                value: Some(prompt),
                                reset_nonce: Some(prompt_reset),
                                model_name: model_label,
                                mode: mode.get(),
                                theme: palette,
                                has_focus: true,
                                on_submit: {
                                    let submit_tx = submit_tx.clone();
                                    move |text: String| {
                                        if is_quit_command(&text) {
                                            let _ = submit_tx.send("/exit".to_string());
                                            return;
                                        }
                                        let trimmed = text.trim();
                                        if trimmed.is_empty() {
                                            return;
                                        }
                                        let _ = submit_tx.send(text);
                                    }
                                },
                                on_mode_change: move |next| mode.set(next),
                            )
                        }
                    }
                }.into_any()
            })
        }
    }
}
