//! Syntax highlighting for diff content using syntect.
//!
//! Each diff line is split into its prefix (`+ ` / `- ` / `  `) and its
//! content; the content is passed through syntect when a language is known,
//! then reassembled as [`MixedTextContent`] segments with appropriate colors.

use std::path::Path;

use iocraft::prelude::*;
use similar::ChangeTag;

use super::render::diff_line_color;
use crate::components::markdown::colors::syntect_to_styled_span;
use crate::components::markdown::syntax::syntax_highlight_raw;
use crate::components::theme::UiTheme;

/// Highlight the content portion of a diff line using syntect.
///
/// Strips the diff prefix (`"+ "`, `"- "`, `"  "` or `"+"`/`"-"`/`" "`),
/// highlights the remaining content, then re-applies the prefix as a
/// tag-colored segment.
pub fn highlight_diff_line(
    raw_line: &str,
    tag: ChangeTag,
    language: Option<&str>,
    theme: UiTheme,
) -> Vec<MixedTextContent> {
    let (prefix, content) = split_diff_prefix(raw_line, tag);
    let prefix_color = diff_line_color(theme, tag);

    let mut parts: Vec<MixedTextContent> = Vec::new();

    // Emit the diff prefix with the tag-appropriate color.
    if !prefix.is_empty() {
        parts.push(MixedTextContent::new(prefix).color(prefix_color));
    }

    // Highlight the content, or fall back to plain text.
    if let Some(lang) = language
        && !lang.is_empty()
        && !content.is_empty()
    {
        let highlighted = syntax_highlight_raw(lang, content);
        if let Some(lines) = highlighted {
            if let Some(regions) = lines.first() {
                for (style, segment) in regions {
                    let span = syntect_to_styled_span(*style, segment.as_str(), theme.text_secondary, theme);
                    let mut mt = MixedTextContent::new(segment.as_str()).color(span.color);
                    if span.weight != Weight::Normal {
                        mt = mt.weight(span.weight);
                    }
                    if span.italic {
                        mt = mt.italic();
                    }
                    parts.push(mt);
                }
            } else {
                parts.push(MixedTextContent::new(content).color(theme.text_secondary));
            }
        } else {
            parts.push(MixedTextContent::new(content).color(theme.text_secondary));
        }
    } else if !content.is_empty() {
        parts.push(MixedTextContent::new(content).color(theme.text_secondary));
    }

    if parts.is_empty() {
        // Preserve blank lines as a visible space.
        parts.push(MixedTextContent::new(" ").color(prefix_color));
    }

    parts
}

/// Split a raw diff line into (prefix, content).
///
/// Handles both the long prefix forms (`"+ "`, `"- "`, `"  "`) used by
/// `diff_line_prefix` and the short forms (`"+"`, `"-"`, `" "`).
fn split_diff_prefix(raw: &str, _tag: ChangeTag) -> (&str, &str) {
    if raw.starts_with("+ ") || raw.starts_with("- ") || raw.starts_with("  ") {
        (&raw[..2], &raw[2..])
    } else if raw.starts_with('+') || raw.starts_with('-') || raw.starts_with(' ') {
        (&raw[..1], &raw[1..])
    } else {
        ("", raw)
    }
}

/// Detect language from a file path for syntax highlighting.
///
/// Maps common extensions to syntect fence tokens.
/// Falls back to `None` (no highlighting) when unknown.
pub fn language_from_file_path(file_path: Option<&str>) -> Option<String> {
    let path = file_path?;
    let ext = Path::new(path).extension()?.to_str()?;
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "zig" => "zig",
        "scala" => "scala",
        "php" => "php",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "pl" => "perl",
        "lua" => "lua",
        "sql" => "sql",
        "r" => "r",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "xml" | "html" | "htm" => "html",
        "css" | "scss" | "sass" | "less" => "css",
        "md" | "markdown" => "markdown",
        "lock" => "toml",
        "dockerfile" | "Dockerfile" => "dockerfile",
        "makefile" | "Makefile" | "mk" => "makefile",
        "cmake" => "cmake",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Language hints from common shebang lines.
pub fn language_from_shebang(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed.starts_with("#!/usr/bin/env ") {
        let rest = trimmed.trim_start_matches("#!/usr/bin/env ").trim();
        return match rest {
            "bash" | "sh" => Some("bash"),
            "python" | "python3" => Some("python"),
            "node" => Some("javascript"),
            "ruby" => Some("ruby"),
            "perl" => Some("perl"),
            "fish" => Some("fish"),
            _ => None,
        };
    }
    if trimmed.starts_with("#!/bin/") {
        let rest = trimmed.trim_start_matches("#!/bin/").trim();
        return match rest {
            "bash" | "sh" => Some("bash"),
            "zsh" => Some("bash"),
            "dash" => Some("bash"),
            _ => None,
        };
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_diff_with_empty_content_produces_part() {
        let theme = UiTheme::default();
        let empty = highlight_diff_line("", ChangeTag::Equal, None, theme);
        assert!(!empty.is_empty());
    }

    #[test]
    fn highlight_diff_with_prefix() {
        let theme = UiTheme::default();
        let parts = highlight_diff_line("- old line", ChangeTag::Delete, None, theme);
        assert!(!parts.is_empty());
    }

    #[test]
    fn language_from_file_path_resolves_common_extensions() {
        assert_eq!(language_from_file_path(Some("main.rs")).as_deref(), Some("rust"));
        assert_eq!(language_from_file_path(Some("script.py")).as_deref(), Some("python"));
        assert_eq!(language_from_file_path(Some("app.js")).as_deref(), Some("javascript"));
        assert_eq!(language_from_file_path(Some("app.tsx")).as_deref(), Some("typescript"));
        assert_eq!(language_from_file_path(Some("style.css")).as_deref(), Some("css"));
        assert_eq!(language_from_file_path(Some("index.html")).as_deref(), Some("html"));
        assert_eq!(language_from_file_path(Some("data.json")).as_deref(), Some("json"));
        assert_eq!(language_from_file_path(Some("config.toml")).as_deref(), Some("toml"));
        assert_eq!(language_from_file_path(Some("unknown.xyz")).as_deref(), None);
        assert_eq!(language_from_file_path(None), None);
    }

    #[test]
    fn language_from_shebang_detection() {
        assert_eq!(language_from_shebang("#!/usr/bin/env bash"), Some("bash"));
        assert_eq!(language_from_shebang("#!/usr/bin/env python3"), Some("python"));
        assert_eq!(language_from_shebang("#!/bin/bash"), Some("bash"));
        assert_eq!(language_from_shebang("#!/bin/sh"), Some("bash"));
        assert_eq!(language_from_shebang("#!/usr/bin/env node"), Some("javascript"));
        assert_eq!(language_from_shebang("/// normal comment"), None);
    }
}
