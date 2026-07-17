//! Slash command outcomes for the TUI shell.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{ExtensionRegistry, PromptTemplate, Skill};

use crate::agent::{OverlayCommand, SlashDispatch};
use crate::agent::{
    confetti_mode_from_args, dispatch_slash_command, format_help_message, slash_unimplemented_message,
    system_prompt_slash_message, tools_slash_message,
};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;
use crate::tui::confetti::confetti_mode_from_slash_args;

use super::agent_bridge::SlashDispatcher;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashOutcome {
    Quit,
    Status(String),
    Assistant(String),
    Unimplemented(String),
    SpawnAgentTurn,
    OverlayDeferred(OverlayCommand),
    OpenModelSelector { filter: String },
    OpenSystemPromptDialog { text: String },
    PlayConfetti { mode: crate::tui::confetti::ConfettiMode },
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
        SlashDispatch::Tools { args } => match tools_slash_message(ctx.agent_session.as_deref(), &args) {
            Ok(message) => SlashOutcome::Assistant(message),
            Err(message) => SlashOutcome::Status(message),
        },
        SlashDispatch::SystemPrompt => match system_prompt_slash_message(ctx.agent_session.as_deref()) {
            Ok(text) => SlashOutcome::OpenSystemPromptDialog { text },
            Err(message) => SlashOutcome::Status(message),
        },
        SlashDispatch::Confetti { args } => SlashOutcome::PlayConfetti {
            mode: confetti_mode_from_slash_args(confetti_mode_from_args(&args)),
        },
        SlashDispatch::Unimplemented(command) => SlashOutcome::Unimplemented(slash_unimplemented_message(&command)),
        SlashDispatch::OverlayNeeded(overlay) => match overlay {
            OverlayCommand::Model { filter } => SlashOutcome::OpenModelSelector { filter },
            other => SlashOutcome::OverlayDeferred(other),
        },
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

/// Whether the submitted slash line should appear as a user/meta card in the transcript.
pub fn slash_echoes_prompt_in_transcript(outcome: &SlashOutcome) -> bool {
    matches!(outcome, SlashOutcome::SpawnAgentTurn)
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
    fn bare_model_slash_opens_selector() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/model",
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
            SlashOutcome::OpenModelSelector { filter } if filter.is_empty()
        ));
    }

    #[test]
    fn model_slash_opens_selector() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/model opus",
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
            SlashOutcome::OpenModelSelector { filter } if filter == "opus"
        ));
    }

    #[test]
    fn local_slash_outcomes_skip_prompt_echo() {
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::Assistant(String::new())));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::Status(String::new())));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::OpenModelSelector {
            filter: String::new()
        }));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::OpenSystemPromptDialog {
            text: String::new()
        }));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::PlayConfetti {
            mode: crate::tui::confetti::ConfettiMode::Confetti
        }));
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurn));
    }

    #[test]
    fn tools_json_returns_assistant_markdown_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools json",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(!slash_echoes_prompt_in_transcript(&outcome));
        assert!(matches!(
            outcome,
            SlashOutcome::Assistant(message)
                if message.contains("```json")
                    && message.contains("\"format\": \"json\"")
        ));
    }

    #[test]
    fn tools_unknown_format_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools yaml",
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
            SlashOutcome::Status(message) if message.contains("unknown /tools format")
        ));
    }

    #[test]
    fn tools_returns_assistant_markdown_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(!slash_echoes_prompt_in_transcript(&outcome));
        assert!(matches!(
            outcome,
            SlashOutcome::Assistant(message)
                if message.contains("## Available tools")
                    && message.contains("| Tool | Group | Description |")
                    && message.contains("Agent session unavailable")
        ));
    }

    #[test]
    fn system_prompt_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/system-prompt",
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
            SlashOutcome::Status(message) if message == "Agent session required for this command."
        ));
    }

    #[test]
    fn confetti_opens_rain_overlay() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/confetti",
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
            SlashOutcome::PlayConfetti { mode } if mode == crate::tui::confetti::ConfettiMode::Confetti
        ));
    }

    #[test]
    fn confetti_firework_mode() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/confetti firework",
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
            SlashOutcome::PlayConfetti { mode } if mode == crate::tui::confetti::ConfettiMode::Firework
        ));
    }

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
