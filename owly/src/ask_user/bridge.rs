//! Routes ask_* prompts to the terminal via dialoguer (blocking thread pool).

use anyhow::{Context, Result};

/// Kind of interactive prompt requested by an ask_* tool.
#[derive(Debug, Clone)]
pub enum AskUserKind {
    Text { default: Option<String> },
    Select { options: Vec<String>, default_index: usize },
    Confirm { default: bool },
}

#[derive(Clone, Default)]
pub struct AskUserBridge;

impl AskUserBridge {
    pub fn new() -> Self {
        Self
    }

    pub async fn prompt_text(&self, tool_call_id: &str, question: &str, default: Option<&str>) -> Result<String> {
        let kind = AskUserKind::Text {
            default: default.map(str::to_string),
        };
        self.prompt(tool_call_id, "ask_text", question, kind).await
    }

    pub async fn prompt_select(
        &self,
        tool_call_id: &str,
        question: &str,
        options: &[String],
        default_index: usize,
    ) -> Result<String> {
        let kind = AskUserKind::Select {
            options: options.to_vec(),
            default_index,
        };
        self.prompt(tool_call_id, "ask_select", question, kind).await
    }

    pub async fn prompt_confirm(&self, tool_call_id: &str, question: &str, default: bool) -> Result<String> {
        let kind = AskUserKind::Confirm { default };
        self.prompt(tool_call_id, "ask_confirm", question, kind).await
    }

    async fn prompt(&self, _tool_call_id: &str, tool_name: &str, question: &str, kind: AskUserKind) -> Result<String> {
        let question = question.to_string();
        let tool_name = tool_name.to_string();
        let tool_label = tool_name.clone();
        tokio::task::spawn_blocking(move || run_dialoguer(&tool_name, &question, kind))
            .await
            .context(format!("{tool_label} interrupted"))?
            .with_context(|| format!("{tool_label} failed"))
    }
}

fn run_dialoguer(tool_name: &str, question: &str, kind: AskUserKind) -> Result<String> {
    match kind {
        AskUserKind::Text { default } => {
            use dialoguer::Input;
            let mut input = Input::<String>::new().with_prompt(question).allow_empty(true);
            if let Some(ref d) = default {
                input = input.default(d.clone());
            }
            let text = input.interact_text()?;
            Ok(resolve_text_answer(text, default.as_deref()))
        }
        AskUserKind::Select { options, default_index } => {
            use dialoguer::Select;
            let index = Select::new()
                .with_prompt(question)
                .items(&options)
                .default(default_index)
                .interact()?;
            options
                .get(index)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{tool_name}: selection out of range"))
        }
        AskUserKind::Confirm { default } => {
            use dialoguer::Confirm;
            let accepted = Confirm::new().with_prompt(question).default(default).interact()?;
            Ok(if accepted { "yes" } else { "no" }.to_string())
        }
    }
}

fn resolve_text_answer(mut text: String, default: Option<&str>) -> String {
    if text.trim().is_empty()
        && let Some(d) = default
        && !d.is_empty()
    {
        text = d.to_string();
    }
    text
}
