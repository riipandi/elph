use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct PromptTranscriptProps {
    /// Submitted prompt messages, oldest first.
    pub messages: Vec<String>,
}

#[component]
pub fn PromptTranscript(props: &PromptTranscriptProps) -> impl Into<AnyElement<'static>> {
    element! {
        Text(content: format_transcript(&props.messages))
    }
}

fn format_transcript(messages: &[String]) -> String {
    messages
        .iter()
        .filter_map(|message| {
            let block = format_message(message);
            if block.is_empty() { None } else { Some(block) }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_message(message: &str) -> String {
    let trimmed = message.trim_end();
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };

    let mut out = format!("> {first}");
    for line in lines {
        out.push('\n');
        out.push_str("  ");
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_messages_with_single_line_gap() {
        let text = format_transcript(&["hello".into(), "world".into()]);
        assert_eq!(text, "> hello\n> world");
    }

    #[test]
    fn trims_trailing_newlines_from_messages() {
        let text = format_transcript(&["hello\n\n".into(), "world".into()]);
        assert_eq!(text, "> hello\n> world");
    }

    #[test]
    fn formats_multiline_message_with_indent() {
        let text = format_transcript(&["line one\nline two".into()]);
        assert_eq!(text, "> line one\n  line two");
    }
}
