//! Extract JSON values from tool-result text blocks.

use serde_json::Value;

use super::fence::is_toon_encoded;

/// Parse JSON from tool output text, tolerating markdown fences and surrounding prose.
pub fn extract_json_value(text: &str) -> Option<Value> {
    if is_toon_encoded(text) {
        return None;
    }

    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }

    if let Some(fenced) = strip_markdown_json_fence(trimmed)
        && let Ok(value) = serde_json::from_str::<Value>(&fenced)
    {
        return Some(value);
    }

    extract_embedded_json_object(trimmed)
}

fn strip_markdown_json_fence(text: &str) -> Option<String> {
    let open = text.find("```")?;
    let after_open = &text[open + 3..];
    let lang_end = after_open.find('\n')?;
    let body_start = open + 3 + lang_end + 1;
    let body_region = &text[body_start..];
    let close = body_region.find("\n```").or_else(|| body_region.find("```"))?;
    Some(body_region[..close].trim().to_string())
}

fn json_start_index(text: &str) -> Option<usize> {
    match (text.find('['), text.find('{')) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn extract_embedded_json_object(text: &str) -> Option<Value> {
    let start = json_start_index(text)?;
    let slice = text[start..].trim();
    if let Ok(value) = serde_json::from_str::<Value>(slice) {
        return Some(value);
    }

    let bytes = slice.as_bytes();
    let open = bytes.first().copied()?;
    let close = match open {
        b'{' => b'}',
        b'[' => b']',
        _ => return None,
    };

    let mut depth = 0u32;
    let mut in_string = false;
    let mut escape = false;

    for (index, byte) in bytes.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if *byte == b'\\' {
                escape = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b if *b == open => depth += 1,
            b if *b == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let candidate = &slice[..=index];
                    if let Ok(value) = serde_json::from_str::<Value>(candidate) {
                        return Some(value);
                    }
                    break;
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_plain_json() {
        let value = extract_json_value(r#"[{"id":1},{"id":2}]"#).expect("json");
        assert_eq!(value, json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn parses_fenced_json() {
        let text = "Result:\n```json\n[{\"id\":1},{\"id\":2}]\n```\n";
        let value = extract_json_value(text).expect("json");
        assert_eq!(value, json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn skips_toon_blocks() {
        let text = "Data\n\n```toon\nitems[1]{id}:\n  1\n```";
        assert!(extract_json_value(text).is_none());
    }

    #[test]
    fn leaves_plain_text_unparsed() {
        assert!(extract_json_value("line one\nline two").is_none());
    }

    #[test]
    fn parses_json_with_trailing_prose() {
        let text = r#"Tool output: [{"id":1,"name":"a"},{"id":2,"name":"b"}] (done)"#;
        let value = extract_json_value(text).expect("json");
        assert_eq!(value, json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]));
    }

    #[test]
    fn prefers_earliest_array_or_object_opener() {
        assert_eq!(json_start_index(r#"{"items":[1]}"#), Some(0));
        assert_eq!(json_start_index(r#"data: [{"id":1}]"#), Some(6));
    }
}
