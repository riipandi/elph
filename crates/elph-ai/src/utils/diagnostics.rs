//! Assistant message diagnostics for provider/runtime failures and recoveries.

use serde_json::Value;

use crate::types::{AssistantMessage, AssistantMessageDiagnostic};

pub fn create_assistant_message_diagnostic(
    kind: impl Into<String>,
    message: impl Into<String>,
    details: Option<Value>,
) -> AssistantMessageDiagnostic {
    AssistantMessageDiagnostic {
        kind: kind.into(),
        message: message.into(),
        details,
    }
}

pub fn append_assistant_message_diagnostic(message: &mut AssistantMessage, diagnostic: AssistantMessageDiagnostic) {
    message.diagnostics.get_or_insert_with(Vec::new).push(diagnostic);
}
