use super::output::ShellWriter;

pub(super) fn slash_message<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .strip_prefix(prefix)
        .or_else(|| input.strip_prefix(&prefix.to_ascii_uppercase()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(super) fn write_help(writer: &mut ShellWriter<'_>) {
    writer.blank();
    writer.line("Commands:");
    writer.line("  /init [message]    Initialize documentation");
    writer.line("  /update [message]  Update existing documentation");
    writer.line("  /history [n]       List recent checkpoints (default 10)");
    writer.line("  /restore <#|id>    Rewind session to a checkpoint");
    writer.line("  /clear             Start a fresh checkpoint thread");
    writer.line("  /help              Show this help");
    writer.line("  /exit              Quit");
    writer.blank();
    writer.line("Any other input is sent to the agent as a chat follow-up.");
    writer.blank();
}

pub(super) fn history_limit(input: &str) -> usize {
    input
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10)
        .clamp(1, 50)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_limit_parses_and_clamps() {
        assert_eq!(history_limit("/history"), 10);
        assert_eq!(history_limit("/history 25"), 25);
        assert_eq!(history_limit("/history 999"), 50);
        assert_eq!(history_limit("/history 0"), 1);
    }
}
