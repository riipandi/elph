//! Prompt template argument parsing and substitution.

use crate::harness::types::PromptTemplate;

/// Parse an argument string using simple shell-style single and double quotes.
pub fn parse_command_args(args_string: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in args_string.chars() {
        if let Some(quote) = in_quote {
            if ch == quote {
                in_quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Substitute prompt template placeholders with command arguments.
pub fn substitute_args(content: &str, args: &[String]) -> String {
    let mut result = content.to_string();

    let positional = regex_replace_numbered(&result, args);
    result = positional;

    let slice = regex_replace_slice(&result, args);
    result = slice;

    let all_args = args.join(" ");
    result = result.replace("$ARGUMENTS", &all_args);
    result.replace("$@", &all_args)
}

fn regex_replace_numbered(content: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut digits = String::new();
            while let Some(&next) = chars.peek()
                && next.is_ascii_digit()
            {
                digits.push(chars.next().expect("peeked digit"));
            }
            if !digits.is_empty() {
                let index: usize = digits.parse().unwrap_or(0);
                let value = args.get(index.saturating_sub(1)).cloned().unwrap_or_default();
                result.push_str(&value);
                continue;
            }
        }
        result.push(ch);
    }
    result
}

fn regex_replace_slice(content: &str, args: &[String]) -> String {
    let mut result = String::new();
    let bytes = content.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$'
            && index + 3 < bytes.len()
            && bytes[index + 1] == b'{'
            && bytes[index + 2] == b'@'
            && bytes[index + 3] == b':'
        {
            let start = index + 4;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start {
                let mut slice_end = end;
                if slice_end < bytes.len() && bytes[slice_end] == b':' {
                    slice_end += 1;
                    while slice_end < bytes.len() && bytes[slice_end].is_ascii_digit() {
                        slice_end += 1;
                    }
                }
                if slice_end < bytes.len() && bytes[slice_end] == b'}' {
                    let start_num: usize = std::str::from_utf8(&bytes[start..end])
                        .unwrap_or("1")
                        .parse()
                        .unwrap_or(1);
                    let mut start_index = start_num.saturating_sub(1);
                    if start_index >= args.len() {
                        start_index = 0;
                    }
                    let replacement = if end + 1 < slice_end {
                        let length: usize = std::str::from_utf8(&bytes[end + 1..slice_end])
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0);
                        args.iter()
                            .skip(start_index)
                            .take(length)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        args.iter().skip(start_index).cloned().collect::<Vec<_>>().join(" ")
                    };
                    result.push_str(&replacement);
                    index = slice_end + 1;
                    continue;
                }
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

/// Format a prompt template invocation with positional arguments.
pub fn format_prompt_template_invocation(template: &PromptTemplate, args: &[String]) -> String {
    substitute_args(&template.content, args)
}
