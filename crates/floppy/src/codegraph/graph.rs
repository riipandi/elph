//! Shallow import-edge heuristics (not full call-graph).

use regex::Regex;
use std::sync::OnceLock;

use super::types::RawChunk;

/// File-level node id.
pub fn file_node_id(path: &str) -> String {
    format!("file:{path}")
}

/// Symbol/chunk node id.
pub fn symbol_node_id(path: &str, name: &str, start_line: u32) -> String {
    format!("{path}::{name}@{start_line}")
}

/// Extract import-like targets from file content (shallow heuristics).
pub fn extract_import_targets(path: &str, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "rs" => {
            for cap in rust_use().captures_iter(source) {
                if let Some(m) = cap.get(1) {
                    out.push(m.as_str().replace("::", "/"));
                }
            }
        }
        "py" | "pyi" => {
            for cap in python_import().captures_iter(source) {
                if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                    out.push(m.as_str().replace('.', "/"));
                }
            }
        }
        "go" => {
            for cap in go_import().captures_iter(source) {
                if let Some(m) = cap.get(1) {
                    out.push(m.as_str().to_string());
                }
            }
        }
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => {
            for cap in js_import().captures_iter(source) {
                if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                    out.push(m.as_str().to_string());
                }
            }
        }
        "java" => {
            for cap in java_import().captures_iter(source) {
                if let Some(m) = cap.get(1) {
                    out.push(m.as_str().replace('.', "/"));
                }
            }
        }
        "ex" | "exs" => {
            for cap in elixir_import().captures_iter(source) {
                if let Some(m) = cap.get(1) {
                    out.push(m.as_str().replace('.', "/"));
                }
            }
        }
        _ => {}
    }

    out.sort();
    out.dedup();
    out
}

/// Build node rows for chunks of one file.
pub fn nodes_for_chunks(chunks: &[RawChunk]) -> Vec<(String, String, Option<String>, String, u32, u32)> {
    // (id, path, name, kind, start, end)
    let mut nodes = Vec::new();
    if let Some(first) = chunks.first() {
        nodes.push((
            file_node_id(&first.path),
            first.path.clone(),
            None,
            "file".into(),
            1,
            chunks.iter().map(|c| c.end_line).max().unwrap_or(1),
        ));
    }
    for c in chunks {
        if let Some(ref name) = c.name {
            nodes.push((
                symbol_node_id(&c.path, name, c.start_line),
                c.path.clone(),
                Some(name.clone()),
                c.kind.clone(),
                c.start_line,
                c.end_line,
            ));
        }
    }
    nodes
}

fn rust_use() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^\s*use\s+([a-zA-Z0-9_:]+)").expect("regex"))
}

fn python_import() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^\s*(?:from\s+([a-zA-Z0-9_.]+)\s+import|import\s+([a-zA-Z0-9_.]+))").expect("regex")
    })
}

fn go_import() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)^\s*import\s+(?:"([^"]+)"|`([^`]+)`)"#).expect("regex"))
}

fn js_import() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:from\s+['"]([^'"]+)['"]|require\(\s*['"]([^'"]+)['"]\s*\))"#).expect("regex")
    })
}

fn java_import() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^\s*import\s+(?:static\s+)?([a-zA-Z0-9_.]+)").expect("regex"))
}

fn elixir_import() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^\s*(?:alias|import|require|use)\s+([A-Za-z0-9_.]+)").expect("regex"))
}
