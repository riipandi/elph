//! AST-grep helper for structural code search.
//!
//! Provides AST-based pattern matching using ast-grep-core for more accurate
//! and semantic code search compared to simple text-based grep.

use std::path::Path;

use anyhow::Result;

#[cfg(feature = "tools-grep")]
use ast_grep_core::matcher::Pattern;
#[cfg(feature = "tools-grep")]
use ast_grep_language::SupportLang;

/// Language mapping from file extensions/types to ast-grep language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstGrepLang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Elixir,
}

impl AstGrepLang {
    /// Map file extension to language
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(AstGrepLang::Rust),
            "py" => Some(AstGrepLang::Python),
            "js" | "mjs" | "cjs" => Some(AstGrepLang::JavaScript),
            "ts" | "tsx" => Some(AstGrepLang::TypeScript),
            "go" => Some(AstGrepLang::Go),
            "java" => Some(AstGrepLang::Java),
            "c" | "h" => Some(AstGrepLang::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(AstGrepLang::Cpp),
            "cs" => Some(AstGrepLang::CSharp),
            "ex" | "exs" => Some(AstGrepLang::Elixir),
            _ => None,
        }
    }

    /// Map ripgrep type name to language
    pub fn from_type_name(type_name: &str) -> Option<Self> {
        match type_name.to_lowercase().as_str() {
            "rust" => Some(AstGrepLang::Rust),
            "py" | "python" => Some(AstGrepLang::Python),
            "js" | "javascript" => Some(AstGrepLang::JavaScript),
            "ts" | "typescript" => Some(AstGrepLang::TypeScript),
            "go" | "golang" => Some(AstGrepLang::Go),
            "java" => Some(AstGrepLang::Java),
            "c" => Some(AstGrepLang::C),
            "cpp" | "c++" => Some(AstGrepLang::Cpp),
            "cs" | "csharp" => Some(AstGrepLang::CSharp),
            "ex" | "elixir" => Some(AstGrepLang::Elixir),
            _ => None,
        }
    }

    /// Get the ast-grep SupportLang implementation
    #[cfg(feature = "tools-grep")]
    pub fn to_support_lang(self) -> SupportLang {
        match self {
            AstGrepLang::Rust => SupportLang::Rust,
            AstGrepLang::Python => SupportLang::Python,
            AstGrepLang::JavaScript => SupportLang::JavaScript,
            AstGrepLang::TypeScript => SupportLang::TypeScript,
            AstGrepLang::Go => SupportLang::Go,
            AstGrepLang::Java => SupportLang::Java,
            AstGrepLang::C => SupportLang::C,
            AstGrepLang::Cpp => SupportLang::Cpp,
            AstGrepLang::CSharp => SupportLang::CSharp,
            AstGrepLang::Elixir => SupportLang::Elixir,
        }
    }
}

/// Detect if a pattern is likely an AST pattern (structural code search)
/// vs a simple text pattern.
///
/// AST patterns typically contain:
/// - Metavariables ($VAR, $MATCH, etc.) - most reliable indicator
/// - Code-like structure with metavariables (e.g., 'fn $NAME($ARGS)')
/// - Language-specific syntax with metavariables
///
/// Note: Operators alone (==, &&, ||) are NOT considered AST patterns
/// to avoid false positives for text search.
pub fn is_ast_pattern(pattern: &str) -> bool {
    // Metavariables are the most reliable AST pattern indicator
    let has_metavar = pattern.contains('$')
        && (pattern.contains("$MATCH")
            || pattern.contains("$VAR")
            || pattern.contains("$PATTERN")
            || pattern.contains("$EXPR")
            || (pattern.chars().any(|c| c.is_uppercase()) && pattern.contains('$')));

    // Code structure with metavariables is also strong indicator
    let has_code_structure_with_metavar = has_metavar
        && (pattern.contains('(') && pattern.contains(')')
            || pattern.contains('{') && pattern.contains('}')
            || pattern.contains('=') && !pattern.contains("==") && !pattern.contains("!=")
            || pattern.contains("fn ")
            || pattern.contains("function ")
            || pattern.contains("class ")
            || pattern.contains("def "));

    has_metavar || has_code_structure_with_metavar
}

/// Result of an AST search operation
pub struct AstSearchResult {
    pub matches: Vec<String>,
    pub limit_reached: bool,
}

/// Search a single file using AST pattern matching
#[cfg(feature = "tools-grep")]
fn search_file_ast(file_path: &Path, pattern: &str, lang: SupportLang) -> Result<Vec<String>> {
    const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100MB

    // Check file size first
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    if file_size > MAX_FILE_SIZE {
        log::debug!(
            "Skipping large file in AST search: {} ({} bytes)",
            file_path.display(),
            file_size
        );
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file_path.display(), e))?;

    // Compile pattern - Pattern::new may panic on invalid patterns
    // Use catch_unwind to handle potential panics gracefully
    let sg_pattern = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Pattern::new(pattern, lang)));

    let sg_pattern = match sg_pattern {
        Ok(p) => p,
        Err(_) => {
            log::debug!("Invalid AST pattern caused panic: {}", pattern);
            return Err(anyhow::anyhow!("Invalid AST pattern: syntax error"));
        }
    };

    let parsed = ast_grep_core::AstGrep::new(content, lang);
    let root = parsed.root();

    let matches = root.find_all(sg_pattern);
    let mut results = Vec::new();

    for m in matches {
        let pos = m.start_pos();
        let line = pos.line() + 1; // 1-indexed
        let text = m.text();
        results.push(format!("{}:{}:{}", file_path.display(), line, text.trim()));
    }

    Ok(results)
}

/// Perform AST-based search across multiple files
#[cfg(feature = "tools-grep")]
pub fn search_ast(
    paths: &[String],
    pattern: &str,
    lang_hint: Option<AstGrepLang>,
    limit: usize,
) -> Result<AstSearchResult> {
    let mut all_matches = Vec::new();
    let mut limit_reached = false;

    for path in paths {
        if all_matches.len() >= limit {
            limit_reached = true;
            break;
        }

        let path_obj = Path::new(path);
        if !path_obj.is_file() {
            continue;
        }

        // Detect language from file extension if not provided
        let lang = lang_hint.or_else(|| {
            path_obj
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(AstGrepLang::from_extension)
        });

        let Some(lang) = lang else {
            // Skip files with unsupported extensions
            log::debug!("Skipping file with unsupported extension: {}", path);
            continue;
        };

        let support_lang = lang.to_support_lang();

        match search_file_ast(path_obj, pattern, support_lang) {
            Ok(mut matches) => {
                if all_matches.len() + matches.len() > limit {
                    let remaining = limit.saturating_sub(all_matches.len());
                    matches.truncate(remaining);
                    limit_reached = true;
                }
                all_matches.extend(matches);
            }
            Err(e) => {
                // Log error but continue with other files
                log::debug!("AST search failed for {}: {}", path, e);
            }
        }
    }

    Ok(AstSearchResult {
        matches: all_matches,
        limit_reached,
    })
}

#[cfg(not(feature = "tools-grep"))]
pub fn search_ast(
    _paths: &[String],
    _pattern: &str,
    _lang_hint: Option<AstGrepLang>,
    _limit: usize,
) -> Result<AstSearchResult> {
    Err(anyhow!("AST search requires tools-grep feature"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_mapping() {
        assert_eq!(AstGrepLang::from_extension("rs"), Some(AstGrepLang::Rust));
        assert_eq!(AstGrepLang::from_extension("py"), Some(AstGrepLang::Python));
        assert_eq!(AstGrepLang::from_extension("js"), Some(AstGrepLang::JavaScript));
        assert_eq!(AstGrepLang::from_extension("ts"), Some(AstGrepLang::TypeScript));
        assert_eq!(AstGrepLang::from_extension("go"), Some(AstGrepLang::Go));
        assert_eq!(AstGrepLang::from_extension("unknown"), None);
    }

    #[test]
    fn test_type_name_mapping() {
        assert_eq!(AstGrepLang::from_type_name("rust"), Some(AstGrepLang::Rust));
        assert_eq!(AstGrepLang::from_type_name("python"), Some(AstGrepLang::Python));
        assert_eq!(AstGrepLang::from_type_name("javascript"), Some(AstGrepLang::JavaScript));
        assert_eq!(AstGrepLang::from_type_name("unknown"), None);
    }

    #[test]
    fn test_ast_pattern_detection() {
        // Metavariable patterns - these ARE AST patterns
        assert!(is_ast_pattern("fn $NAME($ARGS)"));
        assert!(is_ast_pattern("let $X = $Y"));
        assert!(is_ast_pattern("$VAR = $EXPR"));
        assert!(is_ast_pattern("$MATCH"));
        assert!(is_ast_pattern("$PATTERN"));

        // Code structure with metavariables - these ARE AST patterns
        assert!(is_ast_pattern("function $NAME()"));
        assert!(is_ast_pattern("class $NAME"));
        assert!(is_ast_pattern("def $NAME()"));

        // Operators WITHOUT metavariables - these are NOT AST patterns (text search)
        assert!(!is_ast_pattern("x == y"));
        assert!(!is_ast_pattern("a && b"));
        assert!(!is_ast_pattern("x != y"));
        assert!(!is_ast_pattern("a || b"));

        // Simple text patterns (not AST)
        assert!(!is_ast_pattern("hello"));
        assert!(!is_ast_pattern("test_function"));
        assert!(!is_ast_pattern("variable_name"));

        // Code structure WITHOUT metavariables - these are NOT AST patterns
        assert!(!is_ast_pattern("function test()"));
        assert!(!is_ast_pattern("class MyClass"));
        assert!(!is_ast_pattern("def example()"));
    }

    #[test]
    fn test_non_ast_patterns() {
        assert!(!is_ast_pattern("simple text"));
        assert!(!is_ast_pattern("function_name"));
        assert!(!is_ast_pattern("123"));
        assert!(!is_ast_pattern("test_var"));
    }

    #[cfg(feature = "tools-grep")]
    #[test]
    fn test_ast_search_rust_file() {
        use std::fs;

        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_ast_search.rs");
        fs::write(&test_file, "fn main() {\n    let x = 1;\n    println!(\"{}\");\n}\n").expect("write");

        let result = search_ast(&[test_file.display().to_string()], "let $X = $Y", Some(AstGrepLang::Rust), 10);

        // Clean up
        let _ = fs::remove_file(&test_file);

        assert!(result.is_ok());
        let search_result = result.unwrap();
        assert!(!search_result.matches.is_empty());
        assert!(search_result.matches[0].contains("let x"));
    }

    #[cfg(feature = "tools-grep")]
    #[test]
    fn test_ast_search_no_matches() {
        use std::fs;

        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_ast_no_match.rs");
        fs::write(&test_file, "fn main() {\n    let x = 1;\n}\n").expect("write");

        let result = search_ast(&[test_file.display().to_string()], "class $NAME", Some(AstGrepLang::Rust), 10);

        // Clean up
        let _ = fs::remove_file(&test_file);

        assert!(result.is_ok());
        let search_result = result.unwrap();
        assert!(search_result.matches.is_empty());
    }
}
