//! Desktop notification dispatcher.
//!
//! Thin wrapper around `notify-rust` gated by [`NotificationSettings`].
//! Errors are non-fatal — logged at `warn` level and swallowed so the
//! TUI never crashes due to a notification failure.
//!
//! # Platform support
//!
//! | Platform | Mechanism                          |
//! |----------|------------------------------------|
//! | macOS    | Notification Center (UNUserNotification) |
//! | Linux    | D-Bus (XDG Desktop Notifications)  |
//! | Windows  | Toast notifications                |
//!
//! On headless / CI environments `notify-rust` will fail silently and
//! the `warn` log entry is the only trace.

use crate::platform::NotificationSettings;

/// Send a desktop notification if the event type is enabled in settings.
pub fn notify(settings: &NotificationSettings, kind: NotifKind) {
    if !settings.enabled {
        return;
    }
    if !kind.is_enabled(settings) {
        return;
    }

    let (summary, body) = kind.message();
    let _ = notify_rust::Notification::new()
        .summary(&summary)
        .body(&body)
        .appname(&settings.app_name)
        .timeout(notify_rust::Timeout::Milliseconds(8000))
        .show()
        .inspect_err(|e| log::warn!("desktop notification failed: {e}"));
}

/// All notification event kinds.
///
/// Each variant knows its own settings gate and message text.
#[derive(Debug, Clone)]
pub enum NotifKind<'a> {
    /// Agent finished responding to a prompt.
    TurnComplete { elapsed_secs: f64 },
    /// Agent requests permission to execute a tool.
    ToolPermission { tool_name: &'a str },
    /// Agent asks the user a question.
    UserQuestion { summary: String },
    /// An error occurred (agent / MCP / bootstrap failure).
    Error { message: &'a str },
    /// The user canceled a running turn.
    TurnCancel { elapsed_secs: f64 },
    /// Bootstrap / startup completed and the agent is ready.
    StartupReady,
}

impl NotifKind<'_> {
    /// Check whether this notification kind is enabled in the given settings.
    pub fn is_enabled(&self, settings: &NotificationSettings) -> bool {
        match self {
            Self::TurnComplete { elapsed_secs } => {
                settings.on_turn_complete && *elapsed_secs >= settings.min_turn_duration_secs
            }
            Self::ToolPermission { .. } => settings.on_tool_permission,
            Self::UserQuestion { .. } => settings.on_user_question,
            Self::Error { .. } => settings.on_error,
            Self::TurnCancel { .. } => settings.on_turn_cancel,
            Self::StartupReady => settings.on_startup_ready,
        }
    }

    /// Human-readable summary and body text for the notification.
    pub fn message(&self) -> (String, String) {
        match self {
            Self::TurnComplete { elapsed_secs } => {
                let dur = format_duration_secs(*elapsed_secs);
                ("Turn complete".into(), format!("Agent finished responding · {dur}"))
            }
            Self::ToolPermission { tool_name } => (
                "Tool permission required".into(),
                format!("Agent wants to execute: {tool_name}"),
            ),
            Self::UserQuestion { summary } => ("Agent has a question".into(), summary.to_string()),
            Self::Error { message } => ("Error".into(), message.to_string()),
            Self::TurnCancel { elapsed_secs } => {
                let dur = format_duration_secs(*elapsed_secs);
                ("Turn canceled".into(), format!("Active turn was canceled · {dur}"))
            }
            Self::StartupReady => ("Elph is ready".into(), "Agent and MCP servers are initialized.".into()),
        }
    }
}

/// Compact duration formatting (e.g. `1m50s`, `12s`, `450ms`).
fn format_duration_secs(secs: f64) -> String {
    let total_ms = (secs * 1000.0) as u64;
    if total_ms < 1000 {
        return format!("{total_ms}ms");
    }
    let secs = total_ms / 1000;
    let mins = secs / 60;
    let secs = secs % 60;
    if mins > 0 {
        format!("{mins}m{secs}s")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notif_turn_complete_respects_min_duration() {
        let mut settings = NotificationSettings::default();
        settings.min_turn_duration_secs = 5.0;

        let fast = NotifKind::TurnComplete { elapsed_secs: 2.0 };
        let slow = NotifKind::TurnComplete { elapsed_secs: 10.0 };

        assert!(!fast.is_enabled(&settings));
        assert!(slow.is_enabled(&settings));
    }

    #[test]
    fn notif_turn_cancel_disabled_by_default() {
        let settings = NotificationSettings::default();
        let cancel = NotifKind::TurnCancel { elapsed_secs: 30.0 };
        assert!(!cancel.is_enabled(&settings));
    }

    #[test]
    fn notif_master_switch_disables_all() {
        let mut settings = NotificationSettings::default();
        settings.enabled = false;
        // Should not reach the notification call, but is_enabled is still true.
        // The gate is checked in `notify()` before `is_enabled()`.
        let complete = NotifKind::TurnComplete { elapsed_secs: 10.0 };
        assert!(complete.is_enabled(&settings)); // individual flag still on
    }

    #[test]
    fn format_duration_secs_variants() {
        assert_eq!(format_duration_secs(0.45), "450ms");
        assert_eq!(format_duration_secs(1.0), "1s");
        assert_eq!(format_duration_secs(110.0), "1m50s");
        assert_eq!(format_duration_secs(3661.0), "61m1s");
    }
}
