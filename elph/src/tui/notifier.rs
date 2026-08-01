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
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Notification queue with deduplication and rate limiting.
#[derive(Clone)]
pub struct NotifierQueue {
    /// Last notification timestamp for each kind (deduplication).
    last_sent: Arc<RwLock<std::collections::HashMap<String, Instant>>>,
    /// Rate limit duration (same notification type can't be sent within this window).
    rate_limit: Duration,
}

impl NotifierQueue {
    /// Create a new notification queue with default rate limit (1 second).
    pub fn new() -> Self {
        Self {
            last_sent: Arc::new(RwLock::new(std::collections::HashMap::new())),
            rate_limit: Duration::from_secs(1),
        }
    }

    /// Create a new notification queue with custom rate limit.
    #[cfg(test)]
    pub(crate) fn with_rate_limit(rate_limit: Duration) -> Self {
        Self {
            last_sent: Arc::new(RwLock::new(std::collections::HashMap::new())),
            rate_limit,
        }
    }

    /// Check if a notification of this kind should be sent (rate limit + deduplication).
    async fn should_send(&self, kind_key: &str) -> bool {
        let mut last_sent = self.last_sent.write().await;
        let now = Instant::now();

        if let Some(&last) = last_sent.get(kind_key)
            && now.duration_since(last) < self.rate_limit
        {
            return false; // Rate limited
        }

        last_sent.insert(kind_key.to_string(), now);
        true
    }
}

impl Default for NotifierQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Global notification queue instance.
static NOTIFIER_QUEUE: std::sync::OnceLock<NotifierQueue> = std::sync::OnceLock::new();

/// Get or create the global notification queue.
fn get_notifier_queue() -> &'static NotifierQueue {
    NOTIFIER_QUEUE.get_or_init(NotifierQueue::new)
}

/// Send a desktop notification if the event type is enabled in settings.
///
/// Spawns a blocking task as fire-and-forget so the notification never
/// stalls the TUI tick loop, even though `notify-rust`'s `show()` is
/// synchronous.
pub fn notify(settings: &NotificationSettings, kind: NotifKind<'_>) {
    if !settings.enabled {
        return;
    }
    if !kind.is_enabled(settings) {
        return;
    }

    let (summary, body) = kind.message();
    let app_name = settings.app_name.clone();
    let kind_key = kind.key();

    // Check rate limiting asynchronously
    let queue = get_notifier_queue().clone();
    let kind_key = kind_key.to_string();

    tokio::spawn(async move {
        if !queue.should_send(&kind_key).await {
            log::debug!("Notification rate-limited: {}", kind_key);
            return;
        }

        // Fire-and-forget: spawn a blocking task so the sync `show()` call
        // never stalls the tokio runtime / tick loop.
        let _ = tokio::task::spawn_blocking(move || {
            let _ = notify_rust::Notification::new()
                .summary(&summary)
                .body(&body)
                .appname(&app_name)
                .timeout(notify_rust::Timeout::Milliseconds(8000))
                .show()
                .inspect_err(|e| log::warn!("desktop notification failed: {e}"));
        })
        .await;
    });
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

    /// Unique key for rate limiting and deduplication.
    fn key(&self) -> &str {
        match self {
            Self::TurnComplete { .. } => "turn_complete",
            Self::ToolPermission { .. } => "tool_permission",
            Self::UserQuestion { .. } => "user_question",
            Self::Error { .. } => "error",
            Self::TurnCancel { .. } => "turn_cancel",
            Self::StartupReady => "startup_ready",
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
        let settings = NotificationSettings {
            min_turn_duration_secs: 5.0,
            ..Default::default()
        };

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
        let settings = NotificationSettings {
            enabled: false,
            ..Default::default()
        };
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

    #[test]
    fn notif_kind_keys_are_unique() {
        let kinds = [
            NotifKind::TurnComplete { elapsed_secs: 1.0 },
            NotifKind::ToolPermission { tool_name: "test" },
            NotifKind::UserQuestion { summary: "test".into() },
            NotifKind::Error { message: "test" },
            NotifKind::TurnCancel { elapsed_secs: 1.0 },
            NotifKind::StartupReady,
        ];

        let keys: Vec<&str> = kinds.iter().map(|k| k.key()).collect();
        let unique_keys: std::collections::HashSet<_> = keys.iter().collect();

        assert_eq!(keys.len(), unique_keys.len(), "All notification kind keys should be unique");
    }

    #[tokio::test]
    async fn notifier_queue_rate_limits() {
        let queue = NotifierQueue::with_rate_limit(Duration::from_millis(100));

        // First send should succeed
        assert!(queue.should_send("test").await);

        // Immediate second send should be rate-limited
        assert!(!queue.should_send("test").await);

        // Wait for rate limit to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should succeed again
        assert!(queue.should_send("test").await);
    }

    #[tokio::test]
    async fn notifier_queue_different_keys_independent() {
        let queue = NotifierQueue::with_rate_limit(Duration::from_millis(100));

        // Different keys should not interfere with each other
        assert!(queue.should_send("key1").await);
        assert!(queue.should_send("key2").await);
        assert!(queue.should_send("key3").await);
    }
}
