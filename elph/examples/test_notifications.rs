//! Example program to test Elph's notification system.
//!
//! This example demonstrates how to use the notification queue system
//! and tests various notification types with rate limiting using OSC escape sequences.
//!
//! Run with:
//! ```sh
//! cargo run -p elph --example test_notifications
//! ```
//!
//! The notifications will be sent via terminal escape sequences (OSC 99, OSC 9, OSC 777)
//! which are handled by your terminal emulator. Supported terminals include:
//! - Kitty (OSC 99)
//! - iTerm2 (OSC 9)
//! - WezTerm (OSC 777)
//! - Ghostty (OSC 777)
//! - Windows Terminal (OSC 9)
//! - VTE-based terminals like GNOME Terminal (OSC 777)

use std::time::Duration;
use tokio::time::sleep;

/// Mock notification settings for testing.
#[derive(Debug, Clone)]
struct MockNotificationSettings {
    enabled: bool,
    on_turn_complete: bool,
    on_tool_permission: bool,
    on_user_question: bool,
    on_error: bool,
    on_turn_cancel: bool,
    on_startup_ready: bool,
    min_turn_duration_secs: f64,
    #[allow(dead_code)]
    app_name: String,
}

impl Default for MockNotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            on_turn_complete: true,
            on_tool_permission: true,
            on_user_question: true,
            on_error: true,
            on_turn_cancel: false,
            on_startup_ready: true,
            min_turn_duration_secs: 5.0,
            app_name: "Elph Example".to_string(),
        }
    }
}

/// Mock notification kind for testing.
#[derive(Debug, Clone)]
enum MockNotifKind<'a> {
    TurnComplete { elapsed_secs: f64 },
    ToolPermission { tool_name: &'a str },
    UserQuestion { summary: String },
    Error { message: &'a str },
    TurnCancel { elapsed_secs: f64 },
    StartupReady,
}

impl MockNotifKind<'_> {
    /// Check whether this notification kind is enabled in the given settings.
    fn is_enabled(&self, settings: &MockNotificationSettings) -> bool {
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
    fn message(&self) -> (String, String) {
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

/// Mock notification queue for testing.
#[derive(Clone)]
struct MockNotifierQueue {
    last_sent: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, std::time::Instant>>>,
    rate_limit: Duration,
}

impl MockNotifierQueue {
    fn new() -> Self {
        Self {
            last_sent: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            rate_limit: Duration::from_secs(1),
        }
    }

    #[allow(dead_code)]
    fn with_rate_limit(rate_limit: Duration) -> Self {
        Self {
            last_sent: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            rate_limit,
        }
    }

    async fn should_send(&self, kind_key: &str) -> bool {
        let mut last_sent = self.last_sent.write().await;
        let now = std::time::Instant::now();

        if let Some(&last) = last_sent.get(kind_key)
            && now.duration_since(last) < self.rate_limit
        {
            return false; // Rate limited
        }

        last_sent.insert(kind_key.to_string(), now);
        true
    }
}

impl Default for MockNotifierQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a mock desktop notification using OSC escape sequences.
async fn send_mock_notification(settings: &MockNotificationSettings, kind: MockNotifKind<'_>) {
    if !settings.enabled {
        println!("❌ Notifications disabled in settings");
        return;
    }
    if !kind.is_enabled(settings) {
        println!("❌ This notification type is disabled in settings");
        return;
    }

    let (summary, body) = kind.message();
    let kind_key = kind.key();

    println!("📬 Sending notification: {}", kind_key);
    println!("   Summary: {}", summary);
    println!("   Body: {}", body);

    // Send OSC escape sequence notification
    send_osc_notification(&summary, &body);
}

/// Send notification using OSC escape sequences.
fn send_osc_notification(summary: &str, body: &str) {
    // OSC 99 (Kitty) - most feature-rich, supports title and body
    let osc_99 = format!(
        "\x1b]99;title={};body={}\x1b\\",
        escape_osc_string(summary),
        escape_osc_string(body)
    );
    print!("{}", osc_99);

    // OSC 9 (iTerm2, Windows Terminal) - simpler format
    let osc_9 = format!("\x1b]9;{}\x1b\\", escape_osc_string(&format!("{}: {}", summary, body)));
    print!("{}", osc_9);

    // OSC 777 (WezTerm, Ghostty, VTE) - notify protocol
    let osc_777 = format!(
        "\x1b]777;notify;{};{}\x1b\\",
        escape_osc_string(summary),
        escape_osc_string(body)
    );
    print!("{}", osc_777);

    // Flush to ensure the escape sequences are sent immediately
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Escape special characters for OSC sequences.
fn escape_osc_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[tokio::main]
async fn main() {
    println!("🔔 Elph Notification System Test");
    println!("===================================\n");

    let settings = MockNotificationSettings::default();
    let queue = MockNotifierQueue::new();

    // Test 1: Basic notification
    println!("Test 1: Basic turn complete notification");
    let kind = MockNotifKind::TurnComplete { elapsed_secs: 10.0 };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    sleep(Duration::from_millis(500)).await;
    println!();

    // Test 2: Rate limiting (should be blocked)
    println!("Test 2: Rate limiting (immediate duplicate should be blocked)");
    let kind = MockNotifKind::TurnComplete { elapsed_secs: 10.0 };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    } else {
        println!("✅ Notification rate-limited (as expected)");
    }
    println!();

    // Test 3: Different notification type (should not be blocked)
    println!("Test 3: Different notification type (should not be rate-limited)");
    let kind = MockNotifKind::ToolPermission { tool_name: "read_file" };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    sleep(Duration::from_millis(500)).await;
    println!();

    // Test 4: User question notification
    println!("Test 4: User question notification");
    let kind = MockNotifKind::UserQuestion {
        summary: "Do you want to proceed with the refactor?".to_string(),
    };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    sleep(Duration::from_millis(500)).await;
    println!();

    // Test 5: Error notification
    println!("Test 5: Error notification");
    let kind = MockNotifKind::Error {
        message: "Failed to connect to database",
    };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    sleep(Duration::from_millis(500)).await;
    println!();

    // Test 6: Startup ready notification
    println!("Test 6: Startup ready notification");
    let kind = MockNotifKind::StartupReady;
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    sleep(Duration::from_millis(500)).await;
    println!();

    // Test 7: Rate limit expiration (should work again after delay)
    println!("Test 7: Rate limit expiration (waiting 1.5 seconds...)");
    sleep(Duration::from_millis(1500)).await;
    let kind = MockNotifKind::TurnComplete { elapsed_secs: 10.0 };
    if queue.should_send(kind.key()).await {
        send_mock_notification(&settings, kind).await;
    }
    println!();

    // Test 8: Fast turn (below minimum duration)
    println!("Test 8: Fast turn (below minimum duration threshold)");
    let kind = MockNotifKind::TurnComplete { elapsed_secs: 2.0 };
    if kind.is_enabled(&settings) {
        send_mock_notification(&settings, kind).await;
    } else {
        println!("✅ Notification blocked (turn too fast, below threshold)");
    }
    println!();

    // Test 9: Disabled notification type
    println!("Test 9: Disabled notification type (turn cancel)");
    let settings_cancel_disabled = MockNotificationSettings {
        on_turn_cancel: false,
        ..Default::default()
    };
    let kind = MockNotifKind::TurnCancel { elapsed_secs: 30.0 };
    if kind.is_enabled(&settings_cancel_disabled) {
        send_mock_notification(&settings_cancel_disabled, kind).await;
    } else {
        println!("✅ Notification blocked (turn cancel disabled in settings)");
    }
    println!();

    // Test 10: Master switch disabled
    println!("Test 10: Master switch disabled");
    let settings_disabled = MockNotificationSettings {
        enabled: false,
        ..Default::default()
    };
    let kind = MockNotifKind::TurnComplete { elapsed_secs: 10.0 };
    if kind.is_enabled(&settings_disabled) {
        send_mock_notification(&settings_disabled, kind).await;
    } else {
        println!("✅ Notification blocked (master switch disabled)");
    }
    println!();

    println!("===================================");
    println!("✅ All notification tests completed!");
    println!();
    println!("You should have received 6 terminal notifications via OSC escape sequences:");
    println!("  1. Turn complete (10s)");
    println!("  2. Tool permission request");
    println!("  3. User question");
    println!("  4. Error message");
    println!("  5. Startup ready");
    println!("  6. Turn complete (after rate limit expiration)");
    println!();
    println!("Note: Terminal notifications depend on your terminal emulator support.");
    println!("Supported terminals: Kitty, iTerm2, WezTerm, Ghostty, Windows Terminal, VTE-based terminals");
}
