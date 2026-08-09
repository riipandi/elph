//! Shared helpers for `fff-search` backed exploration tools.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use fff_search::file_picker::{FFFMode, FilePicker, FilePickerOptions, FuzzySearchOptions};
use fff_search::grep::{GrepMode, GrepResult, GrepSearchOptions};
use fff_search::types::PaginationArgs;
use fff_search::{AiGrepConfig, FFFQuery, MixedItemRef, MixedSearchConfig, SharedFilePicker, SharedFrecency};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::utils::truncate::GREP_MAX_LINE_LENGTH;
use crate::agent::harness::utils::truncate::truncate_line;

/// Output formatting mode for grep search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepOutputMode {
    /// Standard `file:line:content` format (default).
    Standard,
    /// Only list file paths with matches (like `-l`).
    FilesWithMatches,
    /// Show match count per file (like `-c`).
    Count,
}

/// Options for formatting grep search output.
#[derive(Debug, Clone)]
pub struct GrepOutputOptions {
    /// Which output mode to use.
    pub mode: GrepOutputMode,
    /// Line-length truncation in standard mode.
    pub max_line_length: usize,
    /// Working directory used to render match paths relative to it (token-efficient).
    /// When `None`, absolute paths are used.
    pub cwd: Option<String>,
}

impl Default for GrepOutputOptions {
    fn default() -> Self {
        Self {
            mode: GrepOutputMode::Standard,
            max_line_length: GREP_MAX_LINE_LENGTH,
            cwd: None,
        }
    }
}

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

/// Build search options from user-provided parameters.
#[allow(clippy::too_many_arguments)]
pub fn build_grep_options(
    limit: usize,
    max_matches_per_file: Option<usize>,
    mode: GrepMode,
    ignore_case: bool,
    before_context: usize,
    after_context: usize,
    abort: Arc<AtomicBool>,
) -> GrepSearchOptions {
    GrepSearchOptions {
        page_limit: limit,
        max_matches_per_file: max_matches_per_file.unwrap_or(0),
        mode,
        smart_case: !ignore_case,
        before_context,
        after_context,
        trim_whitespace: false,
        abort_signal: Some(abort),
        ..Default::default()
    }
}

/// Apply word-regexp boundaries to a pattern.
pub fn make_word_regexp(pattern: &str) -> String {
    // Strip any (?i) prefix and re-apply after wrapping
    let (prefix, inner) = if let Some(rest) = pattern.strip_prefix("(?i)") {
        ("(?i)", rest)
    } else {
        ("", pattern)
    };
    format!("{prefix}\\b{}\\b", inner)
}

/// Escape regex special characters for literal matching.
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

/// Format grep search output according to the given options.
pub fn format_grep_output_ex(
    picker: &FilePicker,
    result: &GrepResult<'_>,
    options: &GrepOutputOptions,
) -> (Vec<String>, bool) {
    let base = normalize_path(picker.base_path());
    let mut lines = Vec::with_capacity(result.matches.len());
    let mut lines_truncated = false;

    match options.mode {
        GrepOutputMode::Standard => {
            let mut current_file_index = None;
            for grep_match in &result.matches {
                let file = result.files[grep_match.file_index];
                let relative = file.relative_path(picker);
                let absolute = join_paths(&base, &relative);
                let display = make_display_path(&absolute, &options.cwd);

                // Print file header when switching files
                if current_file_index != Some(grep_match.file_index) {
                    if current_file_index.is_some() && !grep_match.context_before.is_empty() {
                        lines.push(String::new());
                    }
                    current_file_index = Some(grep_match.file_index);
                }

                // Context before — each line gets its correct line number
                let ctx_before_count = grep_match.context_before.len();
                for (ctx_i, ctx_line) in grep_match.context_before.iter().enumerate() {
                    let (rendered, truncated) = truncate_line(ctx_line, options.max_line_length);
                    if truncated {
                        lines_truncated = true;
                    }
                    let ctx_line_num = grep_match.line_number.saturating_sub((ctx_before_count - ctx_i) as u64);
                    lines.push(format!("{display}:{ctx_line_num}:{rendered}"));
                }

                // Match line
                let (rendered, truncated) = truncate_line(&grep_match.line_content, options.max_line_length);
                if truncated {
                    lines_truncated = true;
                }
                lines.push(format!("{}:{}:{}", display, grep_match.line_number, rendered));

                // Context after — each line gets its correct line number
                for (ctx_i, ctx_line) in grep_match.context_after.iter().enumerate() {
                    let (rendered, truncated) = truncate_line(ctx_line, options.max_line_length);
                    if truncated {
                        lines_truncated = true;
                    }
                    let ctx_line_num = grep_match.line_number + 1 + ctx_i as u64;
                    lines.push(format!("{display}:{ctx_line_num}:{rendered}"));
                }

                // Blank line separator between match groups when context is present
                if !grep_match.context_before.is_empty() || !grep_match.context_after.is_empty() {
                    lines.push(String::new());
                }
            }
        }
        GrepOutputMode::FilesWithMatches => {
            let mut seen_files = std::collections::BTreeSet::new();
            for grep_match in &result.matches {
                let file = result.files[grep_match.file_index];
                let relative = file.relative_path(picker);
                let absolute = join_paths(&base, &relative);
                seen_files.insert(make_display_path(&absolute, &options.cwd));
            }
            lines.extend(seen_files);
        }
        GrepOutputMode::Count => {
            let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for grep_match in &result.matches {
                let file = result.files[grep_match.file_index];
                let relative = file.relative_path(picker);
                let absolute = join_paths(&base, &relative);
                *counts.entry(make_display_path(&absolute, &options.cwd)).or_insert(0) += 1;
            }
            let mut total = 0;
            for (path, count) in &counts {
                lines.push(format!("{path}:{count}"));
                total += count;
            }
            if counts.len() > 1 {
                lines.push(format!("total:{total}"));
            }
        }
    }

    (lines, lines_truncated)
}

/// Legacy wrapper that uses standard output mode.
pub fn format_grep_output(picker: &FilePicker, result: &GrepResult<'_>) -> (Vec<String>, bool) {
    format_grep_output_ex(picker, result, &GrepOutputOptions::default())
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

/// Render `absolute` as a path relative to `cwd` when possible, so grep output is
/// token-efficient. The result stays actionable because other tools (read_file,
/// edit_file, …) resolve relative paths against the working directory. When `cwd`
/// is `None` or `absolute` is not under `cwd`, the absolute path is returned unchanged.
fn make_display_path(absolute: &str, cwd: &Option<String>) -> String {
    let Some(cwd) = cwd else {
        return absolute.to_string();
    };
    let norm_abs = absolute.replace('\\', "/");
    let norm_cwd = cwd.replace('\\', "/").trim_end_matches('/').to_string();
    match norm_abs.strip_prefix(&format!("{norm_cwd}/")) {
        Some(rest) => rest.to_string(),
        None => absolute.to_string(),
    }
}

pub fn resolve_search_base(absolute_path: &str, is_file: bool) -> String {
    grep_search_scope(absolute_path, is_file).0
}

pub fn resolve_path_scope(absolute_path: &str, is_file: bool) -> String {
    grep_search_scope(absolute_path, is_file).1
}

/// One fuzzy-search hit for `@` mention completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionSearchHit {
    pub path: String,
    pub is_directory: bool,
}

const MENTION_INDEX_SCAN_TIMEOUT: Duration = Duration::from_secs(30);

/// Workspace file index for `@` mention fuzzy search in the TUI.
///
/// Uses [`SharedFilePicker`] with a background scan and filesystem watcher so
/// the index stays warm across `@` completions without rebuilding per query.
pub struct MentionSearchIndex {
    shared_picker: SharedFilePicker,
    /// Keeps the noop frecency handle alive for background scan/watcher threads.
    _frecency: SharedFrecency,
}

impl MentionSearchIndex {
    pub fn build(base_path: &str) -> Result<Self> {
        let shared_picker = SharedFilePicker::default();
        let shared_frecency = SharedFrecency::noop();

        FilePicker::new_with_shared_state(
            shared_picker.clone(),
            shared_frecency.clone(),
            FilePickerOptions {
                base_path: base_path.to_string(),
                mode: FFFMode::Ai,
                watch: true,
                enable_mmap_cache: false,
                enable_content_indexing: false,
                ..Default::default()
            },
        )
        .map_err(|error| anyhow!("{error}"))?;

        if !shared_picker.wait_for_scan(MENTION_INDEX_SCAN_TIMEOUT) {
            return Err(anyhow!("file index scan timed out after {MENTION_INDEX_SCAN_TIMEOUT:?}"));
        }

        Ok(Self {
            shared_picker,
            _frecency: shared_frecency,
        })
    }

    pub fn fuzzy_search_paths(&self, query: &str, limit: usize, show_hidden: bool) -> Vec<MentionSearchHit> {
        let Ok(guard) = self.shared_picker.read() else {
            return Vec::new();
        };
        let Some(picker) = guard.as_ref() else {
            return Vec::new();
        };
        fuzzy_search_relative_paths(picker, query, limit, show_hidden)
    }
}

/// Ensure a directory path ends with exactly one `/` separator.
pub fn format_directory_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    format!("{trimmed}/")
}

/// Fuzzy-search indexed files and directories; optionally hide dot-prefixed path segments.
pub fn fuzzy_search_relative_paths(
    picker: &FilePicker,
    query: &str,
    limit: usize,
    show_hidden: bool,
) -> Vec<MentionSearchHit> {
    use fff_search::{FuzzySearchOptions, PaginationArgs, QueryParser};

    let trimmed = query.trim();
    let parser = QueryParser::new(MixedSearchConfig);
    let parsed = parser.parse(trimmed);
    let fetch_limit = if show_hidden {
        limit
    } else {
        limit.saturating_mul(4).max(limit)
    };
    let result = picker.fuzzy_search_mixed(
        &parsed,
        None,
        FuzzySearchOptions {
            pagination: PaginationArgs {
                offset: 0,
                limit: fetch_limit,
            },
            ..Default::default()
        },
    );

    let mut hits = Vec::with_capacity(result.items.len().min(fetch_limit));
    hits.extend(
        result
            .items
            .iter()
            .map(|item| mixed_item_relative_path(picker, item))
            .filter(|hit| show_hidden || !path_has_hidden_component(&hit.path)),
    );
    hits.truncate(limit);
    hits
}

fn mixed_item_relative_path(picker: &FilePicker, item: &MixedItemRef<'_>) -> MentionSearchHit {
    match item {
        MixedItemRef::File(file) => MentionSearchHit {
            path: file.relative_path(picker),
            is_directory: false,
        },
        MixedItemRef::Dir(dir) => MentionSearchHit {
            path: format_directory_path(&dir.relative_path(picker)),
            is_directory: true,
        },
    }
}

fn path_has_hidden_component(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment.starts_with('.') && !segment.is_empty())
}

#[cfg(test)]
mod mention_tests {
    use super::*;
    use std::fs;

    #[test]
    fn path_has_hidden_component_detects_dotfiles() {
        assert!(path_has_hidden_component(".env"));
        assert!(path_has_hidden_component("src/.hidden/foo.rs"));
        assert!(!path_has_hidden_component("src/main.rs"));
    }

    #[test]
    fn mention_index_fuzzy_search_finds_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("hello.rs");
        fs::write(&file, "fn main() {}\n").expect("write");
        let index = MentionSearchIndex::build(&dir.path().to_string_lossy()).expect("index");
        let hits = index.fuzzy_search_paths("hello", 10, true);
        assert!(
            hits.iter()
                .any(|hit| hit.path.ends_with("hello.rs") && !hit.is_directory)
        );
    }

    #[test]
    fn mention_index_fuzzy_search_finds_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let subdir = dir.path().join("components");
        fs::create_dir_all(&subdir).expect("mkdir");
        fs::write(subdir.join("button.rs"), "fn main() {}\n").expect("write");
        let index = MentionSearchIndex::build(&dir.path().to_string_lossy()).expect("index");
        let hits = index.fuzzy_search_paths("components", 10, true);
        let dir_hit = hits
            .iter()
            .find(|hit| hit.is_directory)
            .expect("expected directory in hits");
        assert_eq!(dir_hit.path, "components/");
    }

    #[test]
    fn format_directory_path_adds_single_trailing_slash() {
        assert_eq!(format_directory_path("src"), "src/");
        assert_eq!(format_directory_path("src/"), "src/");
        assert_eq!(format_directory_path("src//"), "src/");
    }

    #[test]
    fn make_word_regexp_wraps_boundaries() {
        assert_eq!(make_word_regexp("foo"), "\\bfoo\\b");
        assert_eq!(make_word_regexp(r"(?i)hello"), r"(?i)\bhello\b");
    }

    #[test]
    fn escape_regex_literal_escapes_special_chars() {
        let escaped = escape_regex_literal("fn main()");
        assert_eq!(escaped, r"fn main\(\)");
    }

    #[test]
    fn grep_output_mode_default_is_standard() {
        let opts = GrepOutputOptions::default();
        assert_eq!(opts.mode, GrepOutputMode::Standard);
    }

    #[test]
    fn make_display_path_strips_cwd_prefix() {
        let cwd = Some("/Users/me/project".to_string());
        assert_eq!(make_display_path("/Users/me/project/src/main.rs", &cwd), "src/main.rs");
        assert_eq!(make_display_path("/Users/me/project/src/foo/bar.rs", &cwd), "src/foo/bar.rs");
    }

    #[test]
    fn make_display_path_keeps_absolute_when_outside_cwd() {
        let cwd = Some("/Users/me/project".to_string());
        // Sibling directory whose name shares a prefix must not be stripped.
        assert_eq!(
            make_display_path("/Users/me/project-other/lib.rs", &cwd),
            "/Users/me/project-other/lib.rs"
        );
        // Path entirely outside cwd is unchanged.
        assert_eq!(make_display_path("/etc/hosts", &cwd), "/etc/hosts");
        // No cwd configured -> absolute preserved.
        assert_eq!(
            make_display_path("/Users/me/project/src/main.rs", &None),
            "/Users/me/project/src/main.rs"
        );
    }
}
