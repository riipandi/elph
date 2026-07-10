//! Tool call preparation and argument validation.

use elph_ai::validation::validate_tool_call;
use elph_ai::{AssistantMessage, ToolCall};
use tokio_util::sync::CancellationToken;

use crate::types::{AgentContext, AgentLoopConfig, AgentToolResult, BeforeToolCallContext};

use super::{Preparation, PreparedToolCall};

pub(super) fn prepare_tool_call_arguments(tool: &crate::types::AgentTool, tool_call: &ToolCall) -> ToolCall {
    if let Some(prepare) = &tool.prepare_arguments {
        let prepared = prepare(tool_call.arguments.clone());
        if prepared == tool_call.arguments {
            return tool_call.clone();
        }
        let mut tc = tool_call.clone();
        tc.arguments = prepared;
        tc
    } else {
        tool_call.clone()
    }
}

pub(super) async fn prepare_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> Preparation {
    let Some(tool) = current_context.tools.iter().find(|t| t.name() == tool_call.name) else {
        return Preparation::Immediate {
            result: AgentToolResult::error(format!("Tool {} not found", tool_call.name)),
            is_error: true,
        };
    };

    let prepared_tool_call = prepare_tool_call_arguments(tool, tool_call);

    let mut validated_args = match validate_tool_call(&tool.tool, &prepared_tool_call) {
        Ok(()) => prepared_tool_call.arguments.clone(),
        Err(msg) => {
            return Preparation::Immediate {
                result: AgentToolResult::error(msg),
                is_error: true,
            };
        }
    };

    if let Some(before) = &config.before_tool_call {
        let before_result = before(
            BeforeToolCallContext {
                assistant_message: assistant_message.clone(),
                tool_call: tool_call.clone(),
                args: validated_args.clone(),
                context: current_context.clone(),
            },
            signal.clone(),
        )
        .await;
        if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
            return Preparation::Immediate {
                result: AgentToolResult::error("Operation aborted"),
                is_error: true,
            };
        }
        if let Some(result) = before_result {
            if result.block {
                return Preparation::Immediate {
                    result: AgentToolResult::error(
                        result
                            .reason
                            .unwrap_or_else(|| "Tool execution was blocked".to_string()),
                    ),
                    is_error: true,
                };
            }
            if let Some(args) = result.args {
                validated_args = args;
            }
        }
    }

    if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Preparation::Immediate {
            result: AgentToolResult::error("Operation aborted"),
            is_error: true,
        };
    }

    Preparation::Prepared(Box::new(PreparedToolCall {
        tool_call: tool_call.clone(),
        tool: tool.clone(),
        args: validated_args,
    }))
}
