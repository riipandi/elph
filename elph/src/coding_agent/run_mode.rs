//! Non-interactive `elph run` execution.

use anyhow::Result;
use std::io::Write;
use std::path::Path;

use super::runtime::{CreateSessionOptions, create_coding_session};
use crate::runtime::{Paths, Settings};

pub struct RunModeOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub prompt: &'a str,
    pub model: Option<&'a str>,
    pub resume_id: Option<&'a str>,
    pub brave: bool,
}

pub async fn run_non_interactive(options: RunModeOptions<'_>) -> Result<()> {
    let mut settings = options.settings.clone();
    if options.brave {
        settings.session.agent_mode = "brave".into();
    }

    let session = create_coding_session(CreateSessionOptions {
        paths: options.paths,
        settings: &settings,
        cwd: options.cwd,
        resume_id: options.resume_id,
        provider_override: None,
        model_override: options.model,
    })
    .await?;

    session.submit_prompt(options.prompt.to_string(), false).await?;

    let entries = session.harness().session_entries().await;
    for entry in entries {
        if let elph_agent::SessionTreeEntry::Message { message, .. } = entry
            && message.role() == "assistant"
            && let Some(elph_ai::Message::Assistant(assistant)) = message.as_llm()
        {
            for block in &assistant.content {
                if let elph_ai::AssistantContentBlock::Text(text) = block {
                    print!("{}", text.text);
                    std::io::stdout().flush().ok();
                }
            }
            println!();
            break;
        }
    }

    Ok(())
}
