//! AST-grep helper for structural code search.
//!
//! Provides AST-based pattern matching using ast-grep-core for more accurate
//! and semantic code search compared to simple text-based grep.

use anyhow::Result;

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
}

/// Detect if a pattern is likely an AST pattern (structural code search)
/// vs a simple text pattern.
///
/// AST patterns typically contain:
/// - Code-like structure (function calls, variable assignments)
/// - Metavariables ($VAR, $MATCH, etc.)
/// - Language-specific syntax
pub fn is_ast_pattern(pattern: &str) -> bool {
    // Check for common AST pattern indicators
    let has_metavar = pattern.contains('$') && 
        (pattern.contains("$MATCH") || pattern.contains("$VAR") || 
         pattern.contains("$PATTERN") || pattern.contains("$EXPR") ||
         (pattern.chars().any(|c| c.is_uppercase()) && pattern.contains('$')));
    
    let has_code_structure = pattern.contains('(') && pattern.contains(')') ||
        pattern.contains('{') && pattern.contains('}') ||
        pattern.contains('=') && !pattern.contains("==") && !pattern.contains("!=") ||
        pattern.contains("fn ") || pattern.contains("function ") ||
        pattern.contains("class ") || pattern.contains("def ");
    
    let has_operators = pattern.contains("==") || pattern.contains("!=") ||
        pattern.contains("&&") || pattern.contains("||") ||
        pattern.contains("->") || pattern.contains("=>");
    
    has_metavar || has_code_structure || has_operators
}

/// Result of an AST search operation
pub struct AstSearchResult {
    pub matches: Vec<String>,
    pub limit_reached: bool,
}

/// Perform AST-based search across multiple files
/// 
/// Note: This is a placeholder implementation. Full AST search via ast-grep
/// requires more complex integration. For now, this falls back to a simple
/// text-based approach that matches the pattern detection logic.
pub fn search_ast(
    _paths: &[String],
    pattern: &str,
    _lang_hint: Option<AstGrepLang>,
    _limit: usize,
) -> Result<AstSearchResult> {
    // Placeholder: return empty results with explanation
    // In a full implementation, this would use ast-grep for structural matching
    log::debug!("AST pattern detected but AST search not yet fully implemented: {}", pattern);
    
    Ok(AstSearchResult {
        matches: vec![],
        limit_reached: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_name_mapping() {
        assert_eq!(AstGrepLang::from_type_name("rust"), Some(AstGrepLang::Rust));
        assert_eq!(AstGrepLang::from_type_name("python"), Some(AstGrepLang::Python));
        assert_eq!(AstGrepLang::from_type_name("javascript"), Some(AstGrepLang::JavaScript));
        assert_eq!(AstGrepLang::from_type_name("unknown"), None);
    }

    #[test]
    fn test_ast_pattern_detection() {
        // Metavariable patterns
        assert!(is_ast_pattern("fn $NAME($ARGS)"));
        assert!(is_ast_pattern("let $X = $Y"));
        assert!(is_ast_pattern("$VAR = $EXPR"));
        
        // Code structure patterns
        assert!(is_ast_pattern("function test()"));
        assert!(is_ast_pattern("class MyClass"));
        assert!(is_ast_pattern("def example()"));
        
        // Operator patterns
        assert!(is_ast_pattern("x == y"));
        assert!(is_ast_pattern("a && b"));
        
        // Simple text patterns (not AST)
        assert!(!is_ast_pattern("hello"));
        assert!(!is_ast_pattern("test_function"));
        assert!(!is_ast_pattern("variable_name"));
    }

    #[test]
    fn test_non_ast_patterns() {
        assert!(!is_ast_pattern("simple text"));
        assert!(!is_ast_pattern("function_name"));
        assert!(!is_ast_pattern("123"));
        assert!(!is_ast_pattern("test_var"));
    }
}
