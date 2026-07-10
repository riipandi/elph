use crate::session::SessionRecovery;
use crate::startup::InitialRun;

/// Startup hints shown in the TUI transcript before the first prompt.
pub fn startup_transcript_lines(
    restored_count: usize,
    recovery: &SessionRecovery,
    db_path: &std::path::Path,
) -> Vec<String> {
    let mut lines = Vec::new();
    if restored_count > 0 {
        lines.push(format!(
            "restored {restored_count} message(s) from {}",
            db_path.display()
        ));
    }
    if recovery.draft_restored {
        lines.push("recovered partial assistant response from interrupted turn".to_string());
    }
    if let Some(interrupt) = &recovery.pending_interrupt {
        lines.push(format!(
            "agent was waiting for your input ({interrupt}) when the last session ended"
        ));
    }
    if restored_count > 0 || recovery.draft_restored || recovery.pending_interrupt.is_some() {
        lines.push(String::new());
    }
    lines.push("Type /help for commands or /exit to quit.".to_string());
    lines.push(String::new());
    lines
}

/// Map an initial CLI command to the first prompt submission.
pub fn initial_input(initial: InitialRun) -> String {
    match initial {
        InitialRun::Init => "/init".to_string(),
        InitialRun::Update => "/update".to_string(),
        InitialRun::Chat { message } => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_input_maps_flags() {
        assert_eq!(initial_input(InitialRun::Init), "/init");
        assert_eq!(initial_input(InitialRun::Update), "/update");
        assert_eq!(
            initial_input(InitialRun::Chat {
                message: "hello".into()
            }),
            "hello"
        );
    }
}
