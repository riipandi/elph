#[cfg(unix)]
use super::app::SHOULD_KILL_PARENT;
use super::app::WAS_INTERRUPTED;
use slt::TextareaState;
use std::sync::atomic::{AtomicU64, Ordering};

/// Suppresses the duplicate SIGINT delivery that follows a clear-from-non-empty prompt.
static LAST_INTERRUPT_CLEAR_MS: AtomicU64 = AtomicU64::new(0);

const INTERRUPT_COALESCE_MS: u64 = 250;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// First Ctrl+C / SIGINT clears the prompt; second exits (when prompt is empty).
pub fn handle_prompt_interrupt(prompt: &mut TextareaState) -> bool {
    if !prompt.value().is_empty() {
        prompt.set_value("");
        LAST_INTERRUPT_CLEAR_MS.store(now_ms(), Ordering::Relaxed);
        return false;
    }

    let cleared_at = LAST_INTERRUPT_CLEAR_MS.load(Ordering::Relaxed);
    if cleared_at != 0 && now_ms().saturating_sub(cleared_at) < INTERRUPT_COALESCE_MS {
        return false;
    }

    WAS_INTERRUPTED.store(true, Ordering::Relaxed);
    #[cfg(unix)]
    SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
    true
}

#[cfg(test)]
fn interrupt_coalesce_should_suppress_exit(cleared_at_ms: u64, now_ms: u64) -> bool {
    cleared_at_ms != 0 && now_ms.saturating_sub(cleared_at_ms) < INTERRUPT_COALESCE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_suppresses_exit_immediately_after_clear() {
        assert!(interrupt_coalesce_should_suppress_exit(1_000, 1_100));
        assert!(!interrupt_coalesce_should_suppress_exit(1_000, 1_300));
    }
}
