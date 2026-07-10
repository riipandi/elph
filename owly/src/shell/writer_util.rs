use crate::cli::{print_command_header, print_completion};

use super::output::ShellWriter;

pub(super) fn write_command_header(writer: &mut ShellWriter<'_>, command: &str, provider: &str, model: &str) {
    if writer.has_live_ui() {
        writer.command_start(command, provider, model);
    } else if writer.is_transcript() {
        writer.blank();
        writer.line(format!(">_ Owly {command}"));
        writer.line(format!("provider: {provider}"));
        writer.line(format!("model: {model}"));
        writer.blank();
    } else {
        print_command_header(command, provider, model);
    }
}

pub(super) fn write_completion(writer: &mut ShellWriter<'_>, message: &str) {
    if writer.has_live_ui() {
        writer.command_complete(message, true);
    } else if writer.is_transcript() {
        writer.blank();
        writer.line(format!("✓ {message}"));
        writer.blank();
    } else {
        print_completion(message);
    }
}
