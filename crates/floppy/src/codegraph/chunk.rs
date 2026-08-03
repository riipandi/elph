//! AST / fallback chunking for source files (storage- and CPU-conscious).

use std::path::Path;

use ast_grep_core::Language;
use ast_grep_core::matcher::KindMatcher;
use ast_grep_core::ops::Any;
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::SupportLang;

use super::types::RawChunk;

/// Hard cap on stored chunk body characters (FTS still useful; avoids giant blobs).
pub const MAX_STORED_CHUNK_CHARS: usize = 6_000;
/// Max chunks kept per file (largest / named defs preferred via AST order + cap).
pub const MAX_CHUNKS_PER_FILE: usize = 48;
/// Skip embedding text beyond this (compact form used for oversized bodies).
pub const MAX_EMBED_CHARS: usize = 1_800;
/// Min non-whitespace chars for a chunk to be worth indexing.
const MIN_CHUNK_CHARS: usize = 12;

/// Map path → language when AST support is available.
pub fn language_for_path(path: &Path) -> Option<SupportLang> {
    SupportLang::from_path(path).filter(is_tier1)
}

fn is_tier1(lang: &SupportLang) -> bool {
    matches!(
        lang,
        SupportLang::Python
            | SupportLang::C
            | SupportLang::Cpp
            | SupportLang::Java
            | SupportLang::CSharp
            | SupportLang::JavaScript
            | SupportLang::TypeScript
            | SupportLang::Tsx
            | SupportLang::Rust
            | SupportLang::Go
            | SupportLang::Elixir
    )
}

pub fn lang_name(lang: SupportLang) -> &'static str {
    match lang {
        SupportLang::Python => "python",
        SupportLang::C => "c",
        SupportLang::Cpp => "cpp",
        SupportLang::Java => "java",
        SupportLang::CSharp => "csharp",
        SupportLang::JavaScript => "javascript",
        SupportLang::TypeScript => "typescript",
        SupportLang::Tsx => "tsx",
        SupportLang::Rust => "rust",
        SupportLang::Go => "go",
        SupportLang::Elixir => "elixir",
        _ => "other",
    }
}

/// Extension-based lang label for fallback / SQL.
pub fn lang_label_for_path(path: &Path) -> &'static str {
    if let Some(lang) = language_for_path(path) {
        return lang_name(lang);
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "sql" => "sql",
        "md" | "markdown" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        other if !other.is_empty() => "text",
        _ => "text",
    }
}

/// Whether this path should use whole-file text fallback when AST is unavailable.
/// Config / data dumps are skipped to keep the index lean.
pub fn allows_text_fallback(path: &Path) -> bool {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "sql" | "md" | "markdown" | "rs" | "py" | "go" | "js" | "jsx" | "ts" | "tsx" | "java" | "c" | "h" | "cc"
        | "cpp" | "hpp" | "cs" | "ex" | "exs" | "kt" | "kts" | "rb" | "php" | "swift" => true,
        // Skip bulk config / lock / data unless tiny (handled by caller size checks)
        "json" | "yaml" | "yml" | "toml" | "lock" | "svg" | "xml" | "html" | "css" | "scss" => false,
        _ => false,
    }
}

fn def_kinds(lang: SupportLang) -> &'static [&'static str] {
    match lang {
        SupportLang::Rust => &[
            "function_item",
            "impl_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "mod_item",
            "type_item",
            "const_item",
            "static_item",
            "macro_definition",
        ],
        SupportLang::Python => &["function_definition", "class_definition", "decorated_definition"],
        SupportLang::Go => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
            "const_declaration",
            "var_declaration",
        ],
        SupportLang::JavaScript | SupportLang::TypeScript | SupportLang::Tsx => &[
            "function_declaration",
            "generator_function_declaration",
            "class_declaration",
            "method_definition",
            "lexical_declaration",
            "export_statement",
        ],
        SupportLang::Java => &[
            "method_declaration",
            "constructor_declaration",
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
        ],
        SupportLang::C | SupportLang::Cpp => &[
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
            "type_definition",
        ],
        SupportLang::CSharp => &[
            "method_declaration",
            "constructor_declaration",
            "class_declaration",
            "struct_declaration",
            "interface_declaration",
            "enum_declaration",
            "record_declaration",
        ],
        SupportLang::Elixir => &["call", "anonymous_function"],
        _ => &[],
    }
}

/// Chunk source text for `rel_path`.
pub fn chunk_source(rel_path: &str, source: &str, max_chunk_lines: u32) -> Vec<RawChunk> {
    let path = Path::new(rel_path);
    if let Some(lang) = language_for_path(path) {
        let chunks = chunk_ast(rel_path, source, lang, max_chunk_lines);
        if !chunks.is_empty() {
            return cap_file_chunks(chunks);
        }
    }
    if !allows_text_fallback(path) {
        return Vec::new();
    }
    cap_file_chunks(chunk_fallback(rel_path, source, max_chunk_lines))
}

/// Compact text used for embeddings (shorter than stored FTS body when large).
pub fn embed_text_for_chunk(chunk: &RawChunk) -> String {
    let header = match &chunk.name {
        Some(n) => format!("{} {} {}\n", chunk.path, chunk.kind, n),
        None => format!("{} {}\n", chunk.path, chunk.kind),
    };
    let body = &chunk.content;
    if header.chars().count() + body.chars().count() <= MAX_EMBED_CHARS {
        return format!("{header}{body}");
    }
    let budget = MAX_EMBED_CHARS.saturating_sub(header.chars().count() + 1);
    let truncated: String = body.chars().take(budget).collect();
    format!("{header}{truncated}")
}

fn cap_file_chunks(mut chunks: Vec<RawChunk>) -> Vec<RawChunk> {
    chunks.retain(|c| c.content.chars().filter(|ch| !ch.is_whitespace()).count() >= MIN_CHUNK_CHARS);
    if chunks.len() > MAX_CHUNKS_PER_FILE {
        // Prefer named defs (symbols) over anonymous file slices.
        chunks.sort_by(|a, b| {
            let an = a.name.is_some() as u8;
            let bn = b.name.is_some() as u8;
            bn.cmp(&an).then_with(|| a.start_line.cmp(&b.start_line))
        });
        chunks.truncate(MAX_CHUNKS_PER_FILE);
        chunks.sort_by_key(|c| c.start_line);
    }
    for c in &mut chunks {
        c.content = truncate_chars(&c.content, MAX_STORED_CHUNK_CHARS);
    }
    chunks
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

fn chunk_ast(rel_path: &str, source: &str, lang: SupportLang, max_chunk_lines: u32) -> Vec<RawChunk> {
    let kinds = def_kinds(lang);
    if kinds.is_empty() {
        return Vec::new();
    }

    let grep = lang.ast_grep(source);
    let root = grep.root();
    let matchers: Vec<KindMatcher> = kinds
        .iter()
        .filter_map(|k| KindMatcher::try_new(k, lang).ok())
        .collect();
    if matchers.is_empty() {
        return Vec::new();
    }
    let any = Any::new(matchers);

    let mut out = Vec::new();
    for m in root.find_all(any) {
        let node = m.get_node();
        let start_line = (node.start_pos().line() as u32) + 1;
        let end_line = (node.end_pos().line() as u32) + 1;
        if end_line < start_line {
            continue;
        }
        // Prefer top-level-ish defs: skip nested functions deeper than ~2 levels by size heuristic
        // (very small methods inside huge impls still kept if named).
        let kind = node.kind().into_owned();
        let name = node.field("name").map(|n| n.text().into_owned()).or_else(|| {
            node.field("declarator")
                .and_then(|d| d.field("declarator"))
                .map(|n| n.text().into_owned())
        });
        let content = node.text().into_owned();
        if content.trim().is_empty() {
            continue;
        }
        // Skip tiny nested lexical decls in JS/TS noise
        if kind == "lexical_declaration" && name.is_none() && content.lines().count() < 3 {
            continue;
        }
        push_split(&mut out, rel_path, &kind, name, start_line, end_line, &content, max_chunk_lines);
        if out.len() >= MAX_CHUNKS_PER_FILE * 2 {
            // Early stop over-collection; cap_file_chunks will trim.
            break;
        }
    }
    out
}

fn chunk_fallback(rel_path: &str, source: &str, max_chunk_lines: u32) -> Vec<RawChunk> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    // Minified / single-line dumps: one short digest only
    if looks_minified(source) {
        let digest: String = source.chars().take(MAX_STORED_CHUNK_CHARS.min(800)).collect();
        return vec![RawChunk {
            path: rel_path.to_string(),
            kind: "minified".into(),
            name: None,
            start_line: 1,
            end_line: lines.len() as u32,
            content: digest,
        }];
    }
    let kind = if rel_path.ends_with(".sql") { "sql" } else { "file" };
    let mut out = Vec::new();
    let max = max_chunk_lines.max(1) as usize;
    let mut start = 0usize;
    while start < lines.len() && out.len() < MAX_CHUNKS_PER_FILE {
        let end = (start + max).min(lines.len());
        let content = lines[start..end].join("\n");
        if !content.trim().is_empty() {
            out.push(RawChunk {
                path: rel_path.to_string(),
                kind: kind.to_string(),
                name: None,
                start_line: (start as u32) + 1,
                end_line: end as u32,
                content,
            });
        }
        start = end;
    }
    out
}

fn looks_minified(source: &str) -> bool {
    let lines = source.lines().count().max(1);
    let avg = source.len() / lines;
    avg > 240 || (lines <= 3 && source.len() > 2_000)
}

// Internal helper with a single call site; each arg is a flat RawChunk field,
// so a parameter struct would add ceremony without clarity. DeepWiki
// (rust-clippy: too_many_arguments) sanctions #[allow] for such helpers.
#[allow(clippy::too_many_arguments)]
fn push_split(
    out: &mut Vec<RawChunk>,
    path: &str,
    kind: &str,
    name: Option<String>,
    start_line: u32,
    end_line: u32,
    content: &str,
    max_chunk_lines: u32,
) {
    let line_count = end_line.saturating_sub(start_line) + 1;
    if line_count <= max_chunk_lines {
        out.push(RawChunk {
            path: path.to_string(),
            kind: kind.to_string(),
            name,
            start_line,
            end_line,
            content: content.to_string(),
        });
        return;
    }

    let lines: Vec<&str> = content.lines().collect();
    let max = max_chunk_lines.max(1) as usize;
    let mut offset = 0usize;
    let mut part = 0u32;
    while offset < lines.len() {
        let end = (offset + max).min(lines.len());
        let slice = lines[offset..end].join("\n");
        let s_line = start_line + offset as u32;
        let e_line = start_line + end as u32 - 1;
        let part_name = name.as_ref().map(|n| {
            if part == 0 {
                n.clone()
            } else {
                format!("{n}#part{part}")
            }
        });
        out.push(RawChunk {
            path: path.to_string(),
            kind: kind.to_string(),
            name: part_name,
            start_line: s_line,
            end_line: e_line,
            content: slice,
        });
        part += 1;
        offset = end;
        if part as usize >= MAX_CHUNKS_PER_FILE {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_rust_function() {
        let src = "fn hello() {\n    println!(\"hi\");\n}\n";
        let chunks = chunk_source("src/main.rs", src, 150);
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .any(|c| c.kind.contains("function") || c.content.contains("fn hello"))
        );
    }

    #[test]
    fn sql_fallback() {
        let src = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);\nSELECT * FROM users;\n";
        let chunks = chunk_source("schema.sql", src, 150);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, "sql");
    }

    #[test]
    fn skips_json_fallback() {
        let src = r#"{"a":1,"b":2}"#;
        let chunks = chunk_source("package.json", src, 150);
        assert!(chunks.is_empty());
    }

    #[test]
    fn embed_text_is_bounded() {
        let long = "x".repeat(10_000);
        let chunk = RawChunk {
            path: "a.rs".into(),
            kind: "function_item".into(),
            name: Some("big".into()),
            start_line: 1,
            end_line: 400,
            content: long,
        };
        let e = embed_text_for_chunk(&chunk);
        assert!(e.chars().count() <= MAX_EMBED_CHARS + 5);
    }
}
