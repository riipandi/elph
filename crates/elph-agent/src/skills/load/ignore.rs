//! Ignore-pattern matching for skill discovery.

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::env::{LocalExecutionEnv, join_env_path, relative_env_path};
use crate::harness::types::{FileErrorCode, FileKind, FileSystem, Result};

const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

use super::types::{SkillDiagnostic, SkillDiagnosticCode};

fn diagnostic(code: SkillDiagnosticCode, message: impl Into<String>, path: impl Into<String>) -> SkillDiagnostic {
    SkillDiagnostic {
        code,
        message: message.into(),
        path: path.into(),
    }
}

pub(super) struct IgnoreMatcher {
    root: String,
    patterns: Vec<String>,
    matcher: Option<Gitignore>,
}

impl IgnoreMatcher {
    pub(super) fn new(root: impl Into<String>) -> Self {
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

    pub(super) fn ignores(&mut self, path: &str, is_dir: bool) -> bool {
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

pub(super) async fn add_ignore_rules(
    env: &LocalExecutionEnv,
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
                diagnostics.push(diagnostic(SkillDiagnosticCode::FileInfoFailed, error.message, ignore_path));
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
