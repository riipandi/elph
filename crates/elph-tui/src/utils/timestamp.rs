use chrono::{DateTime, Local};

/// Compact local timestamp for transcript messages (see `docs/tui.md`).
pub fn format_message_timestamp(time: DateTime<Local>) -> String {
    let now = Local::now();
    if time.date_naive() == now.date_naive() {
        time.format("%H:%M:%S").to_string()
    } else {
        time.format("%b %-d %H:%M:%S").to_string()
    }
}

/// Current local timestamp string.
pub fn now_timestamp() -> String {
    format_message_timestamp(Local::now())
}
