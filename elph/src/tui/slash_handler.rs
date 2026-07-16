//! Slash command outcomes for the TUI shell.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{ExtensionRegistry, PromptTemplate, Skill};

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
    pub skills: Option<&'a [Skill]>,
    pub agent_session: Option<Arc<crate::agent::CodingAgentSession>>,
    pub extension_host: Option<&'a ExtensionHost>,
    pub paths: Option<&'a Paths>,
    pub cwd: Option<&'a Path>,
}

pub fn handle_slash_submit(ctx: SlashContext<'_>) -> SlashOutcome {
    let Some(dispatch) = dispatch_slash_command(ctx.input, ctx.extensions, ctx.prompt_templates, ctx.skills) else {
        return SlashOutcome::SpawnAgentTurn;
    };

    match dispatch {
        SlashDispatch::Quit => SlashOutcome::Quit,
        SlashDispatch::Help => {
            SlashOutcome::Status(format_help_message(ctx.extensions, ctx.prompt_templates, ctx.skills))
        }
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
        SlashDispatch::Skill { ref name, ref args } => {
            if let Some(skills) = ctx.skills
                && let Some(skill) = skills.iter().find(|skill| skill.name == *name)
                && let Some(notice) = elph_agent::skill_args_validation_notice(skill, args)
            {
                return SlashOutcome::Status(notice);
            }
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
            skills: None,
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
            skills: None,
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

    #[test]
    fn skill_slash_without_session_returns_status() {
        let skill = elph_agent::Skill {
            name: "debug".into(),
            description: "Debug".into(),
            content: "Steps".into(),
            file_path: "/tmp/debug/SKILL.md".into(),
            ..Default::default()
        };
        let outcome = handle_slash_submit(SlashContext {
            input: "/skill:debug src/main.rs",
            extensions: None,
            prompt_templates: None,
            skills: Some(&[skill]),
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message) if message == "Agent session required for this command."
        ));
    }

    #[test]
    fn skill_slash_missing_required_args_returns_notice() {
        let skill = elph_agent::Skill {
            name: "code-review".into(),
            description: "Review".into(),
            content: "Review".into(),
            file_path: "/tmp/code-review/SKILL.md".into(),
            argument_hint: Some("<file-path>".into()),
            ..Default::default()
        };
        let outcome = handle_slash_submit(SlashContext {
            input: "/skill:code-review",
            extensions: None,
            prompt_templates: None,
            skills: Some(&[skill]),
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message)
                if message.contains("requires arguments")
                    && message.contains("code-review")
                    && message.contains("<file-path>")
        ));
    }
}
