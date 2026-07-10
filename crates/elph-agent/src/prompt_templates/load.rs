//! Prompt template loading from filesystem paths.

use crate::env::LocalExecutionEnv;
use crate::harness::types::{FileErrorCode, FileInfo, FileKind, FileSystem, Result};

use super::parse::{diagnostic, load_template_from_file};
use super::{
    LoadPromptTemplatesResult, LoadSourcedPromptTemplatesResult, PromptTemplateDiagnostic,
    PromptTemplateDiagnosticCode, SourcedPromptTemplate, SourcedPromptTemplateDiagnostic,
};

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
) -> LoadSourcedPromptTemplatesResult<crate::harness::types::PromptTemplate, TSource>
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
