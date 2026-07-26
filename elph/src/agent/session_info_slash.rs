//! `/session` slash command — format current session metadata for display.

use std::sync::Arc;
use std::time::Duration;

use elph_agent::{build_session_context, estimate_context_tokens};

use super::CodingAgentSession;
use crate::tui::chrome::count_user_turns;

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
        if meta.updated_at.trim().is_empty() {
            "—".to_string()
        } else {
            meta.updated_at
        }
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

    format!(
        "Title: {title}\n\
         Session ID: {session_id}\n\
         Working directory: {cwd}\n\
         Model: {provider}/{model_id}\n\
         API Backend: {api_backend}\n\
         Last activity: {last_activity}\n\
         Turn: {turn_count}\n\
         Context: {tokens_used} / {context_limit} tokens ({context_pct}%)"
    )
}

/// Sync entry point for the TUI slash handler.
pub fn session_info_slash_message(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::try_block_on_detached(async move { format_session_info(&session).await }, SESSION_INFO_TIMEOUT)
    {
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
        async move {
            session
                .harness()
                .session_name()
                .await
                .unwrap_or_default()
        },
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
Last activity: 2026-07-27T12:00:00.000Z\n\
Turn: 15\n\
Context: 75377 / 500000 tokens (15%)";
        for key in [
            "Title:",
            "Session ID:",
            "Working directory:",
            "Model:",
            "API Backend:",
            "Last activity:",
            "Turn:",
            "Context:",
        ] {
            assert!(sample.contains(key), "missing {key}");
        }
    }
}
