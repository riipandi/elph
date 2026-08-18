//! `/system-prompt` slash command — show the compiled system prompt in a dialog.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::CodingAgentSession;

const COMPILE_TIMEOUT: Duration = Duration::from_millis(800);

pub async fn compiled_system_prompt_message(session: &CodingAgentSession) -> Result<String> {
    session.compiled_system_prompt().await
}

/// Resolve compiled system prompt text for the TUI slash handler (sync).
///
/// Safe to call from the iocraft input path while a turn is streaming:
/// - Prefer the session cache (instant, no locks on the agent run loop).
/// - Otherwise compile on a **detached** thread/runtime with a short timeout
///   (avoids nested `try_block_on` on the TUI runtime, which can panic/deadlock).
pub fn system_prompt_slash_message(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };

    if let Some(cached) = session.cached_system_prompt() {
        return Ok(cached);
    }

    let session = Arc::clone(session);
    match elph_agent::runtime::try_block_on_detached(
        async move { compiled_system_prompt_message(&session).await },
        COMPILE_TIMEOUT,
    ) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(err)) => Err(format!("Failed to compile system prompt: {err}")),
        Err(err) if err.to_string().contains("timed out") => {
            Err("Agent is busy. Wait for the current stream to finish, then run /system-prompt again.".into())
        }
        Err(err) => Err(format!("Failed to compile system prompt: {err}")),
    }
}
