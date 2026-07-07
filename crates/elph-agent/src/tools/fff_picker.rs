//! Shared helpers for `fff-search` backed exploration tools.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use fff_search::file_picker::{FFFMode, FilePicker, FilePickerOptions, FuzzySearchOptions};
use fff_search::grep::{GrepMode, GrepResult, GrepSearchOptions};
use fff_search::types::PaginationArgs;
use fff_search::{AiGrepConfig, FFFQuery};
use tokio_util::sync::CancellationToken;

use crate::harness::utils::truncate::{GREP_MAX_LINE_LENGTH, truncate_line};

pub fn build_picker(base_path: &str) -> Result<FilePicker> {
    let mut picker = FilePicker::new(FilePickerOptions {
        base_path: base_path.to_string(),
        mode: FFFMode::Ai,
        watch: false,
        enable_mmap_cache: false,
        enable_content_indexing: false,
        ..Default::default()
    })
    .map_err(|error| anyhow!("{error}"))?;
    picker.collect_files().map_err(|error| anyhow!("{error}"))?;
    Ok(picker)
}

pub fn grep_search_scope(absolute_path: &str, is_file: bool) -> (String, String) {
    let path = Path::new(absolute_path);
    if is_file {
        let base_path = normalize_path(path.parent().unwrap_or(Path::new(".")));
        let relative = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        (base_path, relative)
    } else {
        (normalize_path(path), String::new())
    }
}

pub fn build_grep_query(pattern: &str, path_scope: &str) -> String {
    if path_scope.is_empty() {
        pattern.to_string()
    } else {
        format!("{path_scope} {pattern}")
    }
}

pub fn parse_grep_query(query: &str) -> FFFQuery<'_> {
    FFFQuery::parse(query, AiGrepConfig)
}

pub fn build_grep_mode(pattern: &str, literal: bool, ignore_case: bool) -> (String, GrepMode) {
    if literal {
        if ignore_case {
            (format!("(?i){}", escape_regex_literal(pattern)), GrepMode::Regex)
        } else {
            (pattern.to_string(), GrepMode::PlainText)
        }
    } else if ignore_case && !pattern.starts_with("(?i)") && !pattern.starts_with("(?-i)") {
        (format!("(?i){pattern}"), GrepMode::Regex)
    } else {
        (pattern.to_string(), GrepMode::Regex)
    }
}

pub fn build_grep_options(
    limit: usize,
    mode: GrepMode,
    ignore_case: bool,
    abort: Arc<AtomicBool>,
) -> GrepSearchOptions {
    GrepSearchOptions {
        page_limit: limit,
        mode,
        smart_case: !ignore_case,
        trim_whitespace: false,
        abort_signal: Some(abort),
        ..Default::default()
    }
}

pub fn build_find_glob_pattern(pattern: &str) -> String {
    if pattern.contains('/') {
        pattern.to_string()
    } else {
        format!("**/{pattern}")
    }
}

pub fn build_find_options(limit: usize) -> FuzzySearchOptions<'static> {
    FuzzySearchOptions {
        pagination: PaginationArgs { offset: 0, limit },
        ..Default::default()
    }
}

pub fn format_grep_output(picker: &FilePicker, result: &GrepResult<'_>) -> (Vec<String>, bool) {
    let base = normalize_path(picker.base_path());
    let mut lines = Vec::with_capacity(result.matches.len());
    let mut lines_truncated = false;

    for grep_match in &result.matches {
        let file = result.files[grep_match.file_index];
        let relative = file.relative_path(picker);
        let absolute = join_paths(&base, &relative);
        let (rendered, truncated) = truncate_line(&grep_match.line_content, GREP_MAX_LINE_LENGTH);
        if truncated {
            lines_truncated = true;
        }
        lines.push(format!("{}:{}:{}", absolute, grep_match.line_number, rendered));
    }

    (lines, lines_truncated)
}

pub fn run_with_abort_signal<T>(
    signal: Option<&CancellationToken>,
    work: impl FnOnce(Arc<AtomicBool>) -> Result<T>,
) -> Result<T> {
    if signal.is_some_and(|token| token.is_cancelled()) {
        return Err(anyhow!("Operation aborted"));
    }

    let abort = Arc::new(AtomicBool::new(false));
    if let Some(token) = signal.cloned() {
        let abort_flag = abort.clone();
        thread::scope(|scope| {
            scope.spawn(move || {
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(10));
                }
                abort_flag.store(true, Ordering::Relaxed);
            });
            work(abort)
        })
    } else {
        work(abort)
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn join_paths(base: &str, relative: &str) -> String {
    if relative.is_empty() {
        return base.to_string();
    }
    if base.ends_with('/') {
        format!("{base}{relative}")
    } else {
        format!("{base}/{relative}")
    }
}

fn escape_regex_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '#' | '&' | '~' | '-'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub fn resolve_search_base(absolute_path: &str, is_file: bool) -> String {
    grep_search_scope(absolute_path, is_file).0
}

pub fn resolve_path_scope(absolute_path: &str, is_file: bool) -> String {
    grep_search_scope(absolute_path, is_file).1
}
