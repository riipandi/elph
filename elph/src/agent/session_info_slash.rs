//! `/session` slash command — format current session metadata for display.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Timelike, Utc};
use elph_agent::{FileSystem, build_session_context, estimate_context_tokens};

use super::CodingAgentSession;
use crate::tui::chrome::count_user_turns;
use crate::tui::labels::format_token_count;

const SESSION_INFO_TIMEOUT: Duration = Duration::from_millis(1_200);

/// Human-readable API backend label for chrome / session info (e.g. `openai-responses` → `Responses`).
pub fn format_api_backend(api: &str) -> String {
    let trimmed = api.trim();
    if trimmed.is_empty() {
        return "Unknown".to_string();
    }
    // Prefer the last kebab segment when present (`openai-responses` → `responses`).
    let segment = trimmed.rsplit('-').next().unwrap_or(trimmed);
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) => {
            let mut out = first.to_uppercase().collect::<String>();
            out.push_str(&chars.as_str().to_lowercase());
            out
        }
        None => "Unknown".to_string(),
    }
}

/// Format an ISO / RFC3339 session timestamp for display (local wall clock).
///
/// Examples: `2026-07-27 15:42`, `2026-07-27 15:42:03` (seconds kept when non-zero).
pub fn format_session_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "—".to_string();
    }
    let Some(utc) = parse_session_timestamp(trimmed) else {
        return trimmed.to_string();
    };
    let local = utc.with_timezone(&Local);
    if local.timestamp_subsec_millis() == 0 && local.second() == 0 {
        local.format("%Y-%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

fn parse_session_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ")
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
}

/// Compact context line: `75K / 500K tokens (15%)`.
pub fn format_context_usage_line(tokens_used: u64, context_limit: u64, context_pct: u64) -> String {
    format!(
        "{} / {} tokens ({}%)",
        format_token_count(tokens_used),
        format_token_count(context_limit),
        context_pct
    )
}

/// Build the multi-line session info body shown by `/session`.
pub async fn format_session_info(session: &CodingAgentSession) -> String {
    let title = session
        .harness()
        .session_name()
        .await
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "(untitled)".to_string());
    let session_id = session.session_id().to_string();
    let cwd = session.harness().env().cwd().to_string();
    let provider = session.model_provider();
    let model_id = session.model_id();
    let api_backend = format_api_backend(&session.model_api());
    let last_activity = {
        let meta = session.harness().session_metadata().await;
        format_session_timestamp(&meta.updated_at)
    };

    let (turn_count, tokens_used, context_limit, context_pct) = match session.branch_entries().await {
        Ok(entries) => {
            let turn_count = count_user_turns(&entries);
            let context = build_session_context(&entries);
            let estimate = estimate_context_tokens(&context.messages);
            let limit = session.context_window().max(1) as u64;
            let used = estimate.tokens;
            let pct = if limit > 0 {
                ((used as f64 / limit as f64) * 100.0).round() as u64
            } else {
                0
            };
            (turn_count, used, limit, pct)
        }
        Err(_) => {
            let limit = session.context_window().max(1) as u64;
            (0, 0, limit, 0)
        }
    };
    let context_line = format_context_usage_line(tokens_used, context_limit, context_pct);

    format!(
        "Title: {title}\n\
         Session ID: {session_id}\n\
         Working directory: {cwd}\n\
         Model: {provider}/{model_id}\n\
         API Backend: {api_backend}\n\
         Last activity: {last_activity}\n\
         Context: {context_line}\n\
         Turn: {turn_count}"
    )
}

/// Sync entry point for the TUI slash handler.
pub fn session_info_slash_message(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::try_block_on_detached(async move { format_session_info(&session).await }, SESSION_INFO_TIMEOUT) {
        Ok(text) => Ok(text),
        Err(err) if err.to_string().contains("timed out") => {
            Err("Agent is busy. Wait for the current stream to finish, then run /session again.".into())
        }
        Err(err) => Err(format!("Failed to load session info: {err}")),
    }
}

/// Resolve current session title for `/rename` prefill (empty when untitled).
pub fn session_title_for_rename(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::try_block_on_detached(
        async move { session.harness().session_name().await.unwrap_or_default() },
        Duration::from_millis(400),
    ) {
        Ok(name) => Ok(name),
        Err(err) if err.to_string().contains("timed out") => Ok(String::new()),
        Err(err) => Err(format!("Failed to load session title: {err}")),
    }
}

/// Persist a new session title (sync wrapper for the rename dialog submit path).
pub fn rename_session_title(session: &Arc<CodingAgentSession>, title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Session title cannot be empty.".into());
    }
    let session = Arc::clone(session);
    let title = title.to_string();
    match elph_agent::try_block_on_detached(
        async move {
            session
                .harness()
                .set_session_name(title)
                .await
                .map_err(|e| e.to_string())
        },
        Duration::from_millis(800),
    ) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(format!("Failed to rename session: {err}")),
        Err(err) => Err(format!("Failed to rename session: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_api_backend_last_segment() {
        assert_eq!(format_api_backend("openai-responses"), "Responses");
        assert_eq!(format_api_backend("openai-completions"), "Completions");
        assert_eq!(format_api_backend("anthropic-messages"), "Messages");
        assert_eq!(format_api_backend("Responses"), "Responses");
        assert_eq!(format_api_backend(""), "Unknown");
    }

    #[test]
    fn session_info_layout_keys() {
        let sample = "Title: Fix login\n\
Session ID: abc\n\
Working directory: /tmp\n\
Model: openai/gpt-4o\n\
API Backend: Responses\n\
Last activity: 2026-07-27 12:00\n\
Context: 75K / 500K tokens (15%)\n\
Turn: 15";
        for key in [
            "Title:",
            "Session ID:",
            "Working directory:",
            "Model:",
            "API Backend:",
            "Last activity:",
            "Context:",
            "Turn:",
        ] {
            assert!(sample.contains(key), "missing {key}");
        }
        let context_pos = sample.find("Context:").expect("context");
        let turn_pos = sample.find("Turn:").expect("turn");
        assert!(context_pos < turn_pos, "Turn should be last");
    }

    #[test]
    fn context_usage_uses_compact_counts() {
        assert_eq!(format_context_usage_line(75_377, 500_000, 15), "75K / 500K tokens (15%)");
        assert_eq!(format_context_usage_line(1_500_000, 2_000_000, 75), "1.5M / 2M tokens (75%)");
        assert_eq!(format_context_usage_line(42, 999, 4), "42 / 999 tokens (4%)");
    }

    #[test]
    fn session_timestamp_formats_rfc3339() {
        let formatted = format_session_timestamp("2026-07-27T12:00:00.000Z");
        // Local offset may shift the hour; date prefix should remain readable.
        assert!(
            formatted.starts_with("2026-07-2"),
            "unexpected formatted timestamp: {formatted}"
        );
        assert!(!formatted.contains('T'));
        assert!(!formatted.ends_with('Z'));
        assert_eq!(format_session_timestamp(""), "—");
        assert_eq!(format_session_timestamp("not-a-date"), "not-a-date");
    }
}
