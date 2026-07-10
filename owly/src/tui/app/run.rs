use std::sync::{Arc, Mutex};

use elph_tui::{
    ActivityState, DEFAULT_TRANSCRIPT_CAP, default_run_config, disable_keyboard_enhancement,
    enable_keyboard_enhancement, push_capped,
};

use super::OwlyApp;
use super::events::AppMessage;
use super::render::render_owly_app;
use crate::tui::entries::OwlyEntry;
use crate::tui::launch::LaunchState;

pub async fn run_shell(mut launch: LaunchState) -> anyhow::Result<()> {
    let _ = enable_keyboard_enhancement();
    struct KeyboardGuard;
    impl Drop for KeyboardGuard {
        fn drop(&mut self) {
            let _ = disable_keyboard_enhancement();
        }
    }
    let _guard = KeyboardGuard;

    let initial = launch.initial.take();
    let mut submit_rx = launch.submit_rx.take().expect("submit receiver");
    let app = Arc::new(Mutex::new(OwlyApp::from_launch(launch)));
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<AppMessage>();

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

            let trimmed = input.trim();
            if !trimmed.is_empty()
                && let Ok(mut guard) = app_dispatch.lock()
            {
                guard.turn = guard.turn.saturating_add(1);
                guard.activity = ActivityState::working();
                push_capped(&mut guard.entries, OwlyEntry::user(trimmed), DEFAULT_TRANSCRIPT_CAP);
                guard.chat.pin_to_tail();
                guard.live_tools.clear();
                guard.running = true;
            }

            let context = app_dispatch.lock().expect("owly app lock").context.clone();
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

    let app_ui = Arc::clone(&app);
    tokio::task::spawn_blocking(move || {
        let config = default_run_config();
        slt::run_with(config, move |ui: &mut slt::Context| {
            let mut guard = app_ui.lock().expect("owly app lock");
            while let Ok(message) = msg_rx.try_recv() {
                guard.handle_message(message);
            }
            if guard.should_exit {
                ui.quit();
            }
            render_owly_app(ui, &mut guard);
        })
    })
    .await??;

    // UI exit (e.g. Ctrl+Q) leaves the dispatcher blocked on `submit_rx`; abort so shutdown completes.
    dispatcher.abort();
    let _ = dispatcher.await;
    Ok(())
}
