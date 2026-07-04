#[cfg(unix)]
use super::app::SHOULD_KILL_PARENT;
use super::app::WAS_INTERRUPTED;
use iocraft::prelude::*;

/// First Ctrl+C / SIGINT clears the prompt; second exits (when prompt is empty).
pub fn handle_prompt_interrupt(
    prompt: &mut State<String>,
    should_exit: &mut State<bool>,
    prompt_reset: &mut State<u32>,
) {
    if prompt.read().is_empty() {
        should_exit.set(true);
        {
            use std::sync::atomic::Ordering;
            WAS_INTERRUPTED.store(true, Ordering::Relaxed);
            #[cfg(unix)]
            SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
        }
    } else {
        prompt.set(String::new());
        prompt_reset.set(prompt_reset.get().wrapping_add(1));
    }
}
