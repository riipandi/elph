//! Slash command outcomes for the TUI shell.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{ExtensionRegistry, PromptTemplate};

use crate::agent::{
    OverlayCommand, SlashDispatch, dispatch_slash_command, format_help_message, slash_unimplemented_message,
};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;

use super::agent_bridge::SlashDispatcher;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashOutcome {
    Quit,
    Status(String),
    Unimplemented(String),
    SpawnAgentTurn,
    OverlayDeferred(OverlayCommand),
}

pub struct SlashContext<'a> {
    pub input: &'a str,
    pub extensions: Option<&'a ExtensionRegistry>,
    pub prompt_templates: Option<&'a [PromptTemplate]>,
    pub agent_session: Option<Arc<crate::agent::CodingAgentSession>>,
    pub extension_host: Option<&'a ExtensionHost>,
    pub paths: Option<&'a Paths>,
    pub cwd: Option<&'a Path>,
}

pub fn handle_slash_submit(ctx: SlashContext<'_>) -> SlashOutcome {
    let Some(dispatch) = dispatch_slash_command(ctx.input, ctx.extensions, ctx.prompt_templates) else {
        return SlashOutcome::SpawnAgentTurn;
    };

    match dispatch {
        SlashDispatch::Quit => SlashOutcome::Quit,
        SlashDispatch::Help => SlashOutcome::Status(format_help_message(ctx.extensions, ctx.prompt_templates)),
        SlashDispatch::Unimplemented(command) => SlashOutcome::Unimplemented(slash_unimplemented_message(&command)),
        SlashDispatch::OverlayNeeded(overlay) => SlashOutcome::OverlayDeferred(overlay),
        SlashDispatch::Compact
        | SlashDispatch::Goal { .. }
        | SlashDispatch::Reload
        | SlashDispatch::Extension { .. }
        | SlashDispatch::PromptTemplate { .. } => {
            if let Some(session) = ctx.agent_session.clone() {
                let paths = ctx.paths.cloned();
                let cwd = ctx.cwd.map(|path| path.to_path_buf());
                let extension_host = ctx.extension_host.cloned();
                SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
                SlashOutcome::SpawnAgentTurn
            } else {
                SlashOutcome::Status("Agent session required for this command.".into())
            }
        }
    }
}

pub fn overlay_deferred_message(overlay: &OverlayCommand) -> String {
    match overlay {
        OverlayCommand::Model { .. } => "/model overlay not yet implemented".into(),
        OverlayCommand::Tree => "/tree overlay not yet implemented".into(),
        OverlayCommand::Resume => "/resume overlay not yet implemented".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_returns_status_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/help",
            extensions: None,
            prompt_templates: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(outcome, SlashOutcome::Status(message) if message.contains("Slash commands:")));
    }

    #[test]
    fn unknown_slash_is_unimplemented() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/not-a-real-command",
            extensions: None,
            prompt_templates: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Unimplemented(message) if message == "not-a-real-command not yet implemented"
        ));
    }
}
