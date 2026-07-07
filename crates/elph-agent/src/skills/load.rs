//! Skill discovery — elph-agent module.

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Deserialize;

use crate::env::{basename_env_path, dirname_env_path, join_env_path, relative_env_path};
use crate::harness::types::{ExecutionEnv, FileErrorCode, FileInfo, FileKind, Result, Skill, err, ok};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
    InvalidMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    pub code: SkillDiagnosticCode,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSkillsResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkill<TSkill, TSource> {
    pub skill: TSkill,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkillDiagnostic<TSource> {
    pub code: SkillDiagnosticCode,
    pub message: String,
    pub path: String,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSourcedSkillsResult<TSkill, TSource> {
    pub skills: Vec<SourcedSkill<TSkill, TSource>>,
    pub diagnostics: Vec<SourcedSkillDiagnostic<TSource>>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "disable-model-invocation")]
    disable_model_invocation: Option<bool>,
}

struct IgnoreMatcher {
    root: String,
    patterns: Vec<String>,
    matcher: Option<Gitignore>,
}

impl IgnoreMatcher {
    fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            patterns: Vec::new(),
            matcher: None,
        }
    }

    fn add(&mut self, patterns: Vec<String>) {
        if patterns.is_empty() {
            return;
        }
        self.patterns.extend(patterns);
        self.matcher = None;
    }

    fn ignores(&mut self, path: &str, is_dir: bool) -> bool {
        if self.matcher.is_none() {
            let mut builder = GitignoreBuilder::new(&self.root);
            for pattern in &self.patterns {
                let _ = builder.add_line(None, pattern);
            }
            self.matcher = builder.build().ok();
        }
        self.matcher
            .as_ref()
            .map(|matcher| matches!(matcher.matched(path, is_dir), Match::Ignore(_)))
            .unwrap_or(false)
    }
}

fn diagnostic(code: SkillDiagnosticCode, message: impl Into<String>, path: impl Into<String>) -> SkillDiagnostic {
    SkillDiagnostic {
        code,
        message: message.into(),
        path: path.into(),
    }
}

/// Load skills from one or more directories.
pub async fn load_skills(env: &dyn ExecutionEnv, dirs: &[&str]) -> LoadSkillsResult {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    for dir in dirs {
        let root_info_result = env.file_info(dir, None).await;
        let root_info = match root_info_result {
            Result::Ok(info) => info,
            Result::Err(error) => {
                if error.code != FileErrorCode::NotFound {
                    diagnostics.push(diagnostic(
                        SkillDiagnosticCode::FileInfoFailed,
                        error.message,
                        dir.to_string(),
                    ));
                }
                continue;
            }
        };

        if resolve_kind(env, &root_info, &mut diagnostics).await != Some(FileKind::Directory) {
            continue;
        }

        let result = load_skills_from_dir_internal(
            env,
            &root_info.path,
            true,
            &mut IgnoreMatcher::new(&root_info.path),
            &root_info.path,
        )
        .await;
        skills.extend(result.skills);
        diagnostics.extend(result.diagnostics);
    }

    LoadSkillsResult { skills, diagnostics }
}

/// Load skills from source-tagged directories.
pub async fn load_sourced_skills<TSource>(
    env: &dyn ExecutionEnv,
    inputs: &[(String, TSource)],
) -> LoadSourcedSkillsResult<Skill, TSource>
where
    TSource: Clone,
{
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    for (path, source) in inputs {
        let result = load_skills(env, &[path.as_str()]).await;
        for skill in result.skills {
            skills.push(SourcedSkill {
                skill,
                source: source.clone(),
            });
        }
        for item in result.diagnostics {
            diagnostics.push(SourcedSkillDiagnostic {
                code: item.code,
                message: item.message,
                path: item.path,
                source: source.clone(),
            });
        }
    }

    LoadSourcedSkillsResult { skills, diagnostics }
}

async fn load_skills_from_dir_internal(
    env: &dyn ExecutionEnv,
    dir: &str,
    include_root_files: bool,
    ignore_matcher: &mut IgnoreMatcher,
    root_dir: &str,
) -> LoadSkillsResult {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    let dir_info_result = env.file_info(dir, None).await;
    let dir_info = match dir_info_result {
        Result::Ok(info) => info,
        Result::Err(error) => {
            if error.code != FileErrorCode::NotFound {
                diagnostics.push(diagnostic(SkillDiagnosticCode::FileInfoFailed, error.message, dir));
            }
            return LoadSkillsResult { skills, diagnostics };
        }
    };

    if resolve_kind(env, &dir_info, &mut diagnostics).await != Some(FileKind::Directory) {
        return LoadSkillsResult { skills, diagnostics };
    }

    add_ignore_rules(env, ignore_matcher, dir, root_dir, &mut diagnostics).await;

    let entries_result = env.list_dir(dir, None).await;
    let entries = match entries_result {
        Result::Ok(entries) => entries,
        Result::Err(error) => {
            diagnostics.push(diagnostic(SkillDiagnosticCode::ListFailed, error.message, dir));
            return LoadSkillsResult { skills, diagnostics };
        }
    };

    for entry in &entries {
        if entry.name != "SKILL.md" {
            continue;
        }
        let kind = resolve_kind(env, entry, &mut diagnostics).await;
        if kind != Some(FileKind::File) {
            continue;
        }
        let rel_path = relative_env_path(root_dir, &entry.path);
        if ignore_matcher.ignores(&rel_path, false) {
            continue;
        }
        let result = load_skill_from_file(env, &entry.path).await;
        if let Some(skill) = result.skill {
            skills.push(skill);
        }
        diagnostics.extend(result.diagnostics);
        return LoadSkillsResult { skills, diagnostics };
    }

    let mut sorted_entries = entries;
    sorted_entries.sort_by(|left, right| left.name.cmp(&right.name));

    for entry in sorted_entries {
        if entry.name.starts_with('.') || entry.name == "node_modules" {
            continue;
        }
        let kind = resolve_kind(env, &entry, &mut diagnostics).await;
        let Some(kind) = kind else {
            continue;
        };

        let rel_path = relative_env_path(root_dir, &entry.path);
        let ignore_path = if kind == FileKind::Directory {
            format!("{rel_path}/")
        } else {
            rel_path.clone()
        };
        if ignore_matcher.ignores(&ignore_path, kind == FileKind::Directory) {
            continue;
        }

        if kind == FileKind::Directory {
            let result = Box::pin(load_skills_from_dir_internal(
                env,
                &entry.path,
                false,
                ignore_matcher,
                root_dir,
            ))
            .await;
            skills.extend(result.skills);
            diagnostics.extend(result.diagnostics);
            continue;
        }

        if kind != FileKind::File || !include_root_files || !entry.name.ends_with(".md") {
            continue;
        }
        let result = load_skill_from_file(env, &entry.path).await;
        if let Some(skill) = result.skill {
            skills.push(skill);
        }
        diagnostics.extend(result.diagnostics);
    }

    LoadSkillsResult { skills, diagnostics }
}

async fn add_ignore_rules(
    env: &dyn ExecutionEnv,
    ignore_matcher: &mut IgnoreMatcher,
    dir: &str,
    root_dir: &str,
    diagnostics: &mut Vec<SkillDiagnostic>,
) {
    let relative_dir = relative_env_path(root_dir, dir);
    let prefix = if relative_dir.is_empty() {
        String::new()
    } else {
        format!("{relative_dir}/")
    };

    for filename in IGNORE_FILE_NAMES {
        let ignore_path = join_env_path(dir, filename);
        let info = env.file_info(&ignore_path, None).await;
        let Result::Ok(info) = info else {
            if let Result::Err(error) = info
                && error.code != FileErrorCode::NotFound
            {
                diagnostics.push(diagnostic(
                    SkillDiagnosticCode::FileInfoFailed,
                    error.message,
                    ignore_path,
                ));
            }
            continue;
        };
        if info.kind != FileKind::File {
            continue;
        }
        let content = env.read_text_file(&ignore_path, None).await;
        let Result::Ok(content) = content else {
            if let Result::Err(error) = content {
                diagnostics.push(diagnostic(SkillDiagnosticCode::ReadFailed, error.message, ignore_path));
            }
            continue;
        };
        let patterns = content
            .lines()
            .filter_map(|line| prefix_ignore_pattern(line, &prefix))
            .collect::<Vec<_>>();
        ignore_matcher.add(patterns);
    }
}

fn prefix_ignore_pattern(line: &str, prefix: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('#') && !trimmed.starts_with("\\#") {
        return None;
    }

    let mut pattern = line.to_string();
    let mut negated = false;
    if pattern.starts_with('!') {
        negated = true;
        pattern = pattern[1..].to_string();
    } else if let Some(rest) = pattern.strip_prefix("\\!") {
        pattern = rest.to_string();
    }
    if let Some(rest) = pattern.strip_prefix('/') {
        pattern = rest.to_string();
    }
    let prefixed = if prefix.is_empty() {
        pattern
    } else {
        format!("{prefix}{pattern}")
    };
    Some(if negated { format!("!{prefixed}") } else { prefixed })
}

struct ParsedSkillFile {
    skill: Option<Skill>,
    diagnostics: Vec<SkillDiagnostic>,
}

async fn load_skill_from_file(env: &dyn ExecutionEnv, file_path: &str) -> ParsedSkillFile {
    let mut diagnostics = Vec::new();
    let raw_content = env.read_text_file(file_path, None).await;
    let Result::Ok(raw_content) = raw_content else {
        if let Result::Err(error) = raw_content {
            diagnostics.push(diagnostic(SkillDiagnosticCode::ReadFailed, error.message, file_path));
        }
        return ParsedSkillFile {
            skill: None,
            diagnostics,
        };
    };

    let parsed = parse_frontmatter::<SkillFrontmatter>(&raw_content);
    let parsed = match parsed {
        Result::Ok(value) => value,
        Result::Err(error) => {
            diagnostics.push(diagnostic(SkillDiagnosticCode::ParseFailed, error, file_path));
            return ParsedSkillFile {
                skill: None,
                diagnostics,
            };
        }
    };

    let skill_dir = dirname_env_path(file_path);
    let parent_dir_name = basename_env_path(&skill_dir);
    let description = parsed.frontmatter.description.as_deref();

    for error in validate_description(description) {
        diagnostics.push(diagnostic(SkillDiagnosticCode::InvalidMetadata, error, file_path));
    }

    let frontmatter_name = parsed.frontmatter.name.as_deref();
    let name = frontmatter_name.unwrap_or(&parent_dir_name).to_string();
    for error in validate_name(&name, &parent_dir_name) {
        diagnostics.push(diagnostic(SkillDiagnosticCode::InvalidMetadata, error, file_path));
    }

    if description.is_none_or(|value| value.trim().is_empty()) {
        return ParsedSkillFile {
            skill: None,
            diagnostics,
        };
    }

    ParsedSkillFile {
        skill: Some(Skill {
            name,
            description: description.unwrap().to_string(),
            content: parsed.body,
            file_path: file_path.to_string(),
            disable_model_invocation: parsed.frontmatter.disable_model_invocation == Some(true),
        }),
        diagnostics,
    }
}

fn validate_name(name: &str, parent_dir_name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(format!(
            "name \"{name}\" does not match parent directory \"{parent_dir_name}\""
        ));
    }
    if name.len() > MAX_NAME_LENGTH {
        errors.push(format!("name exceeds {MAX_NAME_LENGTH} characters ({})", name.len()));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)".to_string());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".to_string());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".to_string());
    }
    errors
}

fn validate_description(description: Option<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    match description {
        None => errors.push("description is required".to_string()),
        Some(value) if value.trim().is_empty() => errors.push("description is required".to_string()),
        Some(value) if value.len() > MAX_DESCRIPTION_LENGTH => errors.push(format!(
            "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({})",
            value.len()
        )),
        _ => {}
    }
    errors
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
        Err(error) => return err(to_error(error)),
    };
    ok(ParsedFrontmatter { frontmatter, body })
}

fn to_error(error: serde_yaml::Error) -> String {
    error.to_string()
}

async fn resolve_kind(
    env: &dyn ExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<SkillDiagnostic>,
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
                SkillDiagnosticCode::FileInfoFailed,
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
                SkillDiagnosticCode::FileInfoFailed,
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
