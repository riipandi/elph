use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::OwlyApp;
use super::events::AppMessage;
use crate::tui::entries::OwlyEntry;
use crate::tui::launch::LaunchState;
use crate::tui::setup_tuie::SetupTuie;
use crate::tui::shell_host::OwlyShellHost;
use crate::tui::slash::input_shows_activity;
use elph_tui::{AgentShell, configure_runtime, push_capped, start_shell};

pub async fn run_shell(mut launch: LaunchState) -> anyhow::Result<()> {
    let initial = launch.initial.take();
    let mut submit_rx = launch.submit_rx.take().expect("submit receiver");
    let app = Arc::new(Mutex::new(OwlyApp::from_launch(launch)));
    let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel::<AppMessage>();

    let app_dispatch = Arc::clone(&app);
    let dispatcher = tokio::spawn(async move {
        let mut pending_initial = initial;

        loop {
            let input = if let Some(text) = pending_initial.take() {
                text
            } else {
                match submit_rx.recv().await {
                    Some(text) => text,
                    None => break,
                }
            };

            let context = {
                let mut guard = app_dispatch.lock().expect("owly app lock");
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    guard.turn = guard.turn.saturating_add(1);
                    if input_shows_activity(trimmed) {
                        guard.activity = elph_tui::ActivityState::working();
                    } else {
                        guard.activity.clear();
                    }
                    push_capped(&mut guard.entries, OwlyEntry::user(trimmed), elph_tui::DEFAULT_TRANSCRIPT_CAP);
                    guard.chat.pin_to_tail();
                    guard.live_tools.clear();
                    guard.running = true;
                }
                guard.context.clone()
            };
            let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut dispatch = Box::pin(context.dispatch(input, Some(event_tx)));

            let turn_result = loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        let Some(event) = event else { continue };
                        let _ = msg_tx.send(AppMessage::UiEvent(event));
                    }
                    result = &mut dispatch => break result,
                }
            };

            match turn_result {
                Ok(result) => {
                    let _ = msg_tx.send(AppMessage::DispatchDone {
                        lines: result.lines,
                        should_exit: result.should_exit,
                    });
                    if result.should_exit {
                        break;
                    }
                }
                Err(err) => {
                    let _ = msg_tx.send(AppMessage::DispatchError(format!("{err:#}")));
                }
            }
        }
    });

    let (needs_setup, theme) = {
        let guard = app.lock().expect("owly app lock");
        (!guard.setup_complete, guard.theme)
    };

    tokio::task::spawn_blocking(move || {
        configure_runtime();

        if needs_setup {
            let setup = SetupTuie::new(app.clone(), theme);
            start_shell(setup)?;
        }

        let host: Rc<RefCell<dyn elph_tui::ShellHost>> = Rc::new(RefCell::new(OwlyShellHost::new(app, msg_rx)));
        let root = AgentShell::new(host);
        start_shell(root)
    })
    .await??;

    dispatcher.abort();
    let _ = dispatcher.await;
    Ok(())
}
