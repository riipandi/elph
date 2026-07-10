//! Prompt template file parsing and frontmatter extraction.

use serde::Deserialize;

use crate::env::{LocalExecutionEnv, basename_env_path};
use crate::harness::types::{FileSystem, PromptTemplate, Result, err, ok};

use super::{PromptTemplateDiagnostic, PromptTemplateDiagnosticCode};

#[derive(Debug, Default, Deserialize)]
pub(super) struct PromptTemplateFrontmatter {
    description: Option<String>,
    #[serde(rename = "argument-hint")]
    _argument_hint: Option<String>,
}

pub(super) struct ParsedTemplateFile {
    pub prompt_template: Option<PromptTemplate>,
    pub diagnostics: Vec<PromptTemplateDiagnostic>,
}

struct ParsedFrontmatter<T> {
    frontmatter: T,
    body: String,
}

pub(super) fn diagnostic(
    code: PromptTemplateDiagnosticCode,
    message: impl Into<String>,
    path: impl Into<String>,
) -> PromptTemplateDiagnostic {
    PromptTemplateDiagnostic {
        code,
        message: message.into(),
        path: path.into(),
    }
}

pub(super) async fn load_template_from_file(env: &LocalExecutionEnv, file_path: &str) -> ParsedTemplateFile {
    let mut diagnostics = Vec::new();
    let raw_content = env.read_text_file(file_path, None).await;
    let Result::Ok(raw_content) = raw_content else {
        if let Result::Err(error) = raw_content {
            diagnostics.push(diagnostic(
                PromptTemplateDiagnosticCode::ReadFailed,
                error.message,
                file_path,
            ));
        }
        return ParsedTemplateFile {
            prompt_template: None,
            diagnostics,
        };
    };

    let parsed = parse_frontmatter::<PromptTemplateFrontmatter>(&raw_content);
    let parsed = match parsed {
        Result::Ok(value) => value,
        Result::Err(error) => {
            diagnostics.push(diagnostic(PromptTemplateDiagnosticCode::ParseFailed, error, file_path));
            return ParsedTemplateFile {
                prompt_template: None,
                diagnostics,
            };
        }
    };

    let first_line = parsed
        .body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let mut description = parsed.frontmatter.description.unwrap_or_default();
    if description.is_empty() && !first_line.is_empty() {
        if first_line.chars().count() > 60 {
            let truncated: String = first_line.chars().take(60).collect();
            description = format!("{truncated}...");
        } else {
            description = first_line.to_string();
        }
    }

    ParsedTemplateFile {
        prompt_template: Some(PromptTemplate {
            name: basename_env_path(file_path)
                .trim_end_matches(".md")
                .trim_end_matches(".MD")
                .to_string(),
            description,
            content: parsed.body,
        }),
        diagnostics,
    }
}

fn parse_frontmatter<T: for<'de> Deserialize<'de> + Default>(content: &str) -> Result<ParsedFrontmatter<T>, String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return ok(ParsedFrontmatter {
            frontmatter: T::default(),
            body: normalized,
        });
    }
    let Some(end_index) = normalized[3..].find("\n---").map(|index| index + 3) else {
        return ok(ParsedFrontmatter {
            frontmatter: T::default(),
            body: normalized,
        });
    };
    let yaml_string = &normalized[4..end_index];
    let body = normalized[end_index + 4..].trim().to_string();
    let frontmatter: T = match serde_yaml::from_str(yaml_string) {
        Ok(value) => value,
        Err(error) => return err(error.to_string()),
    };
    ok(ParsedFrontmatter { frontmatter, body })
}
