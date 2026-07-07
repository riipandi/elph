//! Message conversion tests.

use elph_agent::{
    AgentMessage, CustomAgentMessage, bash_execution_to_text, default_convert_to_llm, llm_message_to_agent,
};
use elph_ai::{Message, UserContent};

fn bash_execution(fields: BashExecutionFields) -> CustomAgentMessage {
    CustomAgentMessage::BashExecution {
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

struct BashExecutionFields {
    command: String,
    output: Option<String>,
    exit_code: Option<i32>,
    cancelled: bool,
    truncated: bool,
    full_output_path: Option<String>,
    timestamp: i64,
    exclude_from_context: bool,
}

impl Default for BashExecutionFields {
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
fn bash_execution_to_text_includes_exit_code() {
    let msg = bash_execution(BashExecutionFields {
        exit_code: Some(1),
        ..Default::default()
    });

    let text = bash_execution_to_text(&msg).expect("bash execution text");
    assert!(text.starts_with("Ran `echo hi`\n"));
    assert!(text.contains("```\nhi\n\n```"));
    assert!(text.ends_with("Command exited with code 1"));
}

#[test]
fn bash_execution_to_text_includes_cancelled_message() {
    let msg = bash_execution(BashExecutionFields {
        cancelled: true,
        exit_code: Some(137),
        ..Default::default()
    });

    let text = bash_execution_to_text(&msg).expect("bash execution text");
    assert!(text.contains("(command cancelled)"));
    assert!(!text.contains("Command exited with code"));
}

#[test]
fn bash_execution_to_text_includes_truncated_message() {
    let msg = bash_execution(BashExecutionFields {
        truncated: true,
        full_output_path: Some("/tmp/bash-output.txt".to_string()),
        ..Default::default()
    });

    let text = bash_execution_to_text(&msg).expect("bash execution text");
    assert!(text.contains("[Output truncated. Full output: /tmp/bash-output.txt]"));
}

#[test]
fn bash_execution_to_text_reports_no_output() {
    let msg = bash_execution(BashExecutionFields {
        output: None,
        ..Default::default()
    });

    let text = bash_execution_to_text(&msg).expect("bash execution text");
    assert!(text.contains("(no output)"));
    assert!(!text.contains("```"));
}

#[test]
fn default_convert_to_llm_excludes_bash_execution_when_exclude_from_context_is_true() {
    let messages = vec![
        llm_message_to_agent(Message::User {
            content: UserContent::Text("visible".into()),
            timestamp: 1,
        }),
        AgentMessage::Custom(bash_execution(BashExecutionFields {
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
fn default_convert_to_llm_formats_bash_execution_with_upstream_text() {
    let messages = vec![AgentMessage::Custom(bash_execution(BashExecutionFields::default()))];

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
