//! Named prompt template invocation — elph-agent module.

use serde::Deserialize;

use crate::env::LocalExecutionEnv;
use crate::env::basename_env_path;
use crate::harness::types::{FileErrorCode, FileInfo, FileKind, FileSystem, PromptTemplate, Result, err, ok};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplateDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplateDiagnostic {
    pub code: PromptTemplateDiagnosticCode,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPromptTemplatesResult {
    pub prompt_templates: Vec<PromptTemplate>,
    pub diagnostics: Vec<PromptTemplateDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplate<TPromptTemplate, TSource> {
    pub prompt_template: TPromptTemplate,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplateDiagnostic<TSource> {
    pub code: PromptTemplateDiagnosticCode,
    pub message: String,
    pub path: String,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSourcedPromptTemplatesResult<TPromptTemplate, TSource> {
    pub prompt_templates: Vec<SourcedPromptTemplate<TPromptTemplate, TSource>>,
    pub diagnostics: Vec<SourcedPromptTemplateDiagnostic<TSource>>,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTemplateFrontmatter {
    description: Option<String>,
    #[serde(rename = "argument-hint")]
    _argument_hint: Option<String>,
}

fn diagnostic(
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

/// Load prompt templates from one or more paths.
pub async fn load_prompt_templates(env: &LocalExecutionEnv, paths: &[&str]) -> LoadPromptTemplatesResult {
    let mut prompt_templates = Vec::new();
    let mut diagnostics = Vec::new();

    for path in paths {
        let info_result = env.file_info(path, None).await;
        let info = match info_result {
            Result::Ok(info) => info,
            Result::Err(error) => {
                if error.code != FileErrorCode::NotFound {
                    diagnostics.push(diagnostic(
                        PromptTemplateDiagnosticCode::FileInfoFailed,
                        error.message,
                        path.to_string(),
                    ));
                }
                continue;
            }
        };

        let kind = resolve_kind(env, &info, &mut diagnostics).await;
        if kind == Some(FileKind::Directory) {
            let result = load_templates_from_dir(env, &info.path).await;
            prompt_templates.extend(result.prompt_templates);
            diagnostics.extend(result.diagnostics);
        } else if kind == Some(FileKind::File) && info.name.ends_with(".md") {
            let result = load_template_from_file(env, &info.path).await;
            if let Some(template) = result.prompt_template {
                prompt_templates.push(template);
            }
            diagnostics.extend(result.diagnostics);
        }
    }

    LoadPromptTemplatesResult {
        prompt_templates,
        diagnostics,
    }
}

/// Load prompt templates from source-tagged paths.
pub async fn load_sourced_prompt_templates<TSource>(
    env: &LocalExecutionEnv,
    inputs: &[(String, TSource)],
) -> LoadSourcedPromptTemplatesResult<PromptTemplate, TSource>
where
    TSource: Clone,
{
    let mut prompt_templates = Vec::new();
    let mut diagnostics = Vec::new();

    for (path, source) in inputs {
        let result = load_prompt_templates(env, &[path.as_str()]).await;
        for prompt_template in result.prompt_templates {
            prompt_templates.push(SourcedPromptTemplate {
                prompt_template,
                source: source.clone(),
            });
        }
        for item in result.diagnostics {
            diagnostics.push(SourcedPromptTemplateDiagnostic {
                code: item.code,
                message: item.message,
                path: item.path,
                source: source.clone(),
            });
        }
    }

    LoadSourcedPromptTemplatesResult {
        prompt_templates,
        diagnostics,
    }
}

async fn load_templates_from_dir(env: &LocalExecutionEnv, dir: &str) -> LoadPromptTemplatesResult {
    let mut prompt_templates = Vec::new();
    let mut diagnostics = Vec::new();

    let entries_result = env.list_dir(dir, None).await;
    let entries = match entries_result {
        Result::Ok(entries) => entries,
        Result::Err(error) => {
            diagnostics.push(diagnostic(PromptTemplateDiagnosticCode::ListFailed, error.message, dir));
            return LoadPromptTemplatesResult {
                prompt_templates,
                diagnostics,
            };
        }
    };

    let mut sorted_entries = entries;
    sorted_entries.sort_by(|left, right| left.name.cmp(&right.name));

    for entry in sorted_entries {
        let kind = resolve_kind(env, &entry, &mut diagnostics).await;
        if kind != Some(FileKind::File) || !entry.name.ends_with(".md") {
            continue;
        }
        let result = load_template_from_file(env, &entry.path).await;
        if let Some(template) = result.prompt_template {
            prompt_templates.push(template);
        }
        diagnostics.extend(result.diagnostics);
    }

    LoadPromptTemplatesResult {
        prompt_templates,
        diagnostics,
    }
}

struct ParsedTemplateFile {
    prompt_template: Option<PromptTemplate>,
    diagnostics: Vec<PromptTemplateDiagnostic>,
}

async fn load_template_from_file(env: &LocalExecutionEnv, file_path: &str) -> ParsedTemplateFile {
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

struct ParsedFrontmatter<T> {
    frontmatter: T,
    body: String,
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

async fn resolve_kind(
    env: &LocalExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<PromptTemplateDiagnostic>,
) -> Option<FileKind> {
    if matches!(info.kind, FileKind::File | FileKind::Directory) {
        return Some(info.kind);
    }
    let canonical_path = env.canonical_path(&info.path, None).await;
    let Result::Ok(canonical_path) = canonical_path else {
        if let Result::Err(error) = canonical_path
            && error.code != FileErrorCode::NotFound
        {
            diagnostics.push(diagnostic(
                PromptTemplateDiagnosticCode::FileInfoFailed,
                error.message,
                &info.path,
            ));
        }
        return None;
    };
    let target = env.file_info(&canonical_path, None).await;
    let Result::Ok(target) = target else {
        if let Result::Err(error) = target
            && error.code != FileErrorCode::NotFound
        {
            diagnostics.push(diagnostic(
                PromptTemplateDiagnosticCode::FileInfoFailed,
                error.message,
                &info.path,
            ));
        }
        return None;
    };
    match target.kind {
        FileKind::File | FileKind::Directory => Some(target.kind),
        FileKind::Symlink => None,
    }
}

/// Parse an argument string using simple shell-style single and double quotes.
pub fn parse_command_args(args_string: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in args_string.chars() {
        if let Some(quote) = in_quote {
            if ch == quote {
                in_quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Substitute prompt template placeholders with command arguments.
pub fn substitute_args(content: &str, args: &[String]) -> String {
    let mut result = content.to_string();

    let positional = regex_replace_numbered(&result, args);
    result = positional;

    let slice = regex_replace_slice(&result, args);
    result = slice;

    let all_args = args.join(" ");
    result = result.replace("$ARGUMENTS", &all_args);
    result.replace("$@", &all_args)
}

fn regex_replace_numbered(content: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut digits = String::new();
            while let Some(&next) = chars.peek()
                && next.is_ascii_digit()
            {
                digits.push(chars.next().expect("peeked digit"));
            }
            if !digits.is_empty() {
                let index: usize = digits.parse().unwrap_or(0);
                let value = args.get(index.saturating_sub(1)).cloned().unwrap_or_default();
                result.push_str(&value);
                continue;
            }
        }
        result.push(ch);
    }
    result
}

fn regex_replace_slice(content: &str, args: &[String]) -> String {
    let mut result = String::new();
    let bytes = content.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$'
            && index + 3 < bytes.len()
            && bytes[index + 1] == b'{'
            && bytes[index + 2] == b'@'
            && bytes[index + 3] == b':'
        {
            let start = index + 4;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start {
                let mut slice_end = end;
                if slice_end < bytes.len() && bytes[slice_end] == b':' {
                    slice_end += 1;
                    while slice_end < bytes.len() && bytes[slice_end].is_ascii_digit() {
                        slice_end += 1;
                    }
                }
                if slice_end < bytes.len() && bytes[slice_end] == b'}' {
                    let start_num: usize = std::str::from_utf8(&bytes[start..end])
                        .unwrap_or("1")
                        .parse()
                        .unwrap_or(1);
                    let mut start_index = start_num.saturating_sub(1);
                    if start_index >= args.len() {
                        start_index = 0;
                    }
                    let replacement = if end + 1 < slice_end {
                        let length: usize = std::str::from_utf8(&bytes[end + 1..slice_end])
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0);
                        args.iter()
                            .skip(start_index)
                            .take(length)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        args.iter().skip(start_index).cloned().collect::<Vec<_>>().join(" ")
                    };
                    result.push_str(&replacement);
                    index = slice_end + 1;
                    continue;
                }
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

/// Format a prompt template invocation with positional arguments.
pub fn format_prompt_template_invocation(template: &PromptTemplate, args: &[String]) -> String {
    substitute_args(&template.content, args)
}
