//! Message conversion tests.

use elph_agent::AgentMessage;
use elph_agent::CustomAgentMessage;
use elph_agent::llm_message_to_agent;
use elph_agent::messages::{default_convert_to_llm, shell_exec_execution_to_text};
use elph_ai::{Message, UserContent};

fn shell_exec_execution(fields: ShellExecExecutionFields) -> CustomAgentMessage {
    CustomAgentMessage::ShellExecExecution {
        command: fields.command,
        output: fields.output,
        exit_code: fields.exit_code,
        cancelled: fields.cancelled,
        truncated: fields.truncated,
        full_output_path: fields.full_output_path,
        timestamp: fields.timestamp,
        exclude_from_context: fields.exclude_from_context,
    }
}

struct ShellExecExecutionFields {
    command: String,
    output: Option<String>,
    exit_code: Option<i32>,
    cancelled: bool,
    truncated: bool,
    full_output_path: Option<String>,
    timestamp: i64,
    exclude_from_context: bool,
}

impl Default for ShellExecExecutionFields {
    fn default() -> Self {
        Self {
            command: "echo hi".to_string(),
            output: Some("hi\n".to_string()),
            exit_code: None,
            cancelled: false,
            truncated: false,
            full_output_path: None,
            timestamp: 42,
            exclude_from_context: false,
        }
    }
}

#[test]
fn shell_exec_execution_to_text_includes_exit_code() {
    let msg = shell_exec_execution(ShellExecExecutionFields {
        exit_code: Some(1),
        ..Default::default()
    });

    let text = shell_exec_execution_to_text(&msg).expect("shell_exec execution text");
    assert!(text.starts_with("Ran `echo hi`\n"));
    assert!(text.contains("```\nhi\n\n```"));
    assert!(text.ends_with("Command exited with code 1"));
}

#[test]
fn shell_exec_execution_to_text_includes_cancelled_message() {
    let msg = shell_exec_execution(ShellExecExecutionFields {
        cancelled: true,
        exit_code: Some(137),
        ..Default::default()
    });

    let text = shell_exec_execution_to_text(&msg).expect("shell_exec execution text");
    assert!(text.contains("(command cancelled)"));
    assert!(!text.contains("Command exited with code"));
}

#[test]
fn shell_exec_execution_to_text_includes_truncated_message() {
    let msg = shell_exec_execution(ShellExecExecutionFields {
        truncated: true,
        full_output_path: Some("/tmp/shell-exec-output.txt".to_string()),
        ..Default::default()
    });

    let text = shell_exec_execution_to_text(&msg).expect("shell_exec execution text");
    assert!(text.contains("[Output truncated. Full output: /tmp/shell-exec-output.txt]"));
}

#[test]
fn shell_exec_execution_to_text_reports_no_output() {
    let msg = shell_exec_execution(ShellExecExecutionFields {
        output: None,
        ..Default::default()
    });

    let text = shell_exec_execution_to_text(&msg).expect("shell_exec execution text");
    assert!(text.contains("(no output)"));
    assert!(!text.contains("```"));
}

#[test]
fn default_convert_to_llm_excludes_shell_exec_execution_when_exclude_from_context_is_true() {
    let messages = vec![
        llm_message_to_agent(Message::User {
            content: UserContent::Text("visible".into()),
            timestamp: 1,
        }),
        AgentMessage::Custom(shell_exec_execution(ShellExecExecutionFields {
            exclude_from_context: true,
            ..Default::default()
        })),
    ];

    let converted = default_convert_to_llm(messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role(), "user");
    match &converted[0] {
        Message::User {
            content: UserContent::Text(text),
            ..
        } => assert_eq!(text, "visible"),
        other => panic!("unexpected message: {other:?}"),
    }
}

#[test]
fn default_convert_to_llm_formats_shell_exec_execution_with_upstream_text() {
    let messages = vec![AgentMessage::Custom(shell_exec_execution(
        ShellExecExecutionFields::default(),
    ))];

    let converted = default_convert_to_llm(messages);
    assert_eq!(converted.len(), 1);
    match &converted[0] {
        Message::User {
            content: UserContent::Text(text),
            timestamp,
        } => {
            assert_eq!(*timestamp, 42);
            assert_eq!(text, "Ran `echo hi`\n```\nhi\n\n```");
        }
        other => panic!("unexpected message: {other:?}"),
    }
}
