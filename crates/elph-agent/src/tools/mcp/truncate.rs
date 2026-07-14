//! Truncate oversized MCP tool / resource results before they enter the agent context.

use elph_ai::TextContent;

use crate::types::ToolResultContent;

/// Default max characters kept per text block in an MCP tool result.
pub const DEFAULT_MAX_TOOL_RESULT_CHARS: usize = 32_768;

/// Max characters kept for structured_content JSON in `details` (smaller; body is primary).
pub const DEFAULT_MAX_STRUCTURED_DETAIL_CHARS: usize = 4_096;

/// Truncate a string at a char boundary, preferring a newline near the cut.
pub fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !input.is_empty());
    }
    let total = input.chars().count();
    if total <= max_chars {
        return (input.to_string(), false);
    }
    let mut cut: String = input.chars().take(max_chars).collect();
    if let Some(idx) = cut.rfind('\n')
        && idx > max_chars / 2
    {
        cut.truncate(idx);
    }
    let omitted = total.saturating_sub(cut.chars().count());
    cut.push_str(&format!("\n\n… [truncated {omitted} characters of MCP tool output]"));
    (cut, true)
}

/// Apply truncation to all text blocks in tool result content.
pub fn truncate_tool_content(content: &mut [ToolResultContent], max_chars: usize) -> bool {
    let mut any = false;
    for block in content.iter_mut() {
        if let ToolResultContent::Text(t) = block {
            let (next, truncated) = truncate_chars(&t.text, max_chars);
            if truncated {
                *t = TextContent::new(next);
                any = true;
            }
        }
    }
    any
}

/// Truncate a JSON value's string form for inclusion in details.
pub fn truncate_json_value(value: &serde_json::Value, max_chars: usize) -> serde_json::Value {
    let s = value.to_string();
    let (cut, truncated) = truncate_chars(&s, max_chars);
    if truncated {
        serde_json::json!({ "_truncated": true, "preview": cut })
    } else {
        value.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_when_short() {
        let (s, t) = truncate_chars("hello", 100);
        assert_eq!(s, "hello");
        assert!(!t);
    }

    #[test]
    fn truncates_long() {
        let long = "a".repeat(100);
        let (s, t) = truncate_chars(&long, 20);
        assert!(t);
        assert!(s.contains("truncated"));
        assert!(s.chars().count() < 100);
    }

    #[test]
    fn prefers_newline() {
        let input = format!("{}\nsecond line here", "x".repeat(30));
        let (s, t) = truncate_chars(&input, 35);
        assert!(t);
        assert!(!s.contains("second line"));
    }

    #[test]
    fn truncate_json_value_short() {
        let v = serde_json::json!("short");
        let result = truncate_json_value(&v, 100);
        assert_eq!(result, v);
    }

    #[test]
    fn truncate_json_value_long() {
        let v = serde_json::json!("x".repeat(200));
        let result = truncate_json_value(&v, 20);
        assert!(result.is_object());
        assert_eq!(result["_truncated"], serde_json::json!(true));
    }
}
