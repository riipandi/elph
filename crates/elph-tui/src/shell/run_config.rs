use std::time::Duration;

use slt::RunConfig;

fn base_run_config() -> RunConfig {
    RunConfig::default()
        .mouse(true)
        .tick_rate(Duration::from_millis(50))
        .kitty_keyboard(true)
        .handle_ctrl_c(false)
}

/// Default runtime settings for Elph fullscreen agent shells.
///
/// Enables mouse scrolling, leaves Ctrl+C to app handlers, and uses a moderate
/// tick rate so activity spinners animate smoothly without busy-polling.
pub fn default_run_config() -> RunConfig {
    base_run_config()
}

/// Runtime settings for inline/static shells (no alternate screen).
///
/// Same interaction defaults as [`default_run_config`]; pair with
/// [`slt::run_static_with`] or [`slt::run_inline_with`].
pub fn inline_static_run_config() -> RunConfig {
    base_run_config()
}

/// Reserved inline rows for Owly's static-output shell (setup wizard + slash palette + prompt).
pub const OWLY_INLINE_HEIGHT: u32 = 16;

/// Spinner preset for the activity line (`⡿ Label · N.Ns`).
pub fn default_activity_spinner() -> slt::widgets::SpinnerState {
    slt::widgets::SpinnerState::moon()
}
