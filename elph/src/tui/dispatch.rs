//! Async turn dispatch for the interactive TUI.

use std::sync::Arc;

use crate::agent::CodingAgentSession;

pub struct TurnDispatcher;

impl TurnDispatcher {
    pub fn spawn_turn(session: Arc<CodingAgentSession>, text: String, steer: bool) {
        tokio::spawn(async move {
            if let Err(err) = session.submit_prompt(text, steer).await {
                log::error!("agent turn failed: {err}");
            }
        });
    }

    pub fn spawn_abort(session: Arc<CodingAgentSession>) {
        tokio::spawn(async move {
            if let Err(err) = session.abort().await {
                log::warn!("abort failed: {err}");
            }
        });
    }
}
