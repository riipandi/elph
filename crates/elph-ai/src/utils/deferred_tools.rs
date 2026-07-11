//! Cache-friendly dynamic tool loading helpers.
//!
//! Split `Context.tools` into prefix (immediate) tools and tools introduced later via
//! `Message::ToolResult.added_tool_names`.

use std::collections::{HashMap, HashSet};

use crate::types::{Context, Message, Tool};

type ToolNameNormalizer = fn(&str) -> String;

fn identity_tool_name(name: &str) -> String {
    name.to_string()
}

/// Split current tools into prefix and transcript-loaded definitions.
pub fn split_deferred_tools(
    context: &Context,
    enabled: bool,
    normalize_name: Option<ToolNameNormalizer>,
) -> (Vec<Tool>, HashMap<String, Tool>) {
    let normalize = normalize_name.unwrap_or(identity_tool_name);

    let mut unique_tools: HashMap<String, Tool> = HashMap::new();
    if let Some(tools) = &context.tools {
        for tool in tools {
            unique_tools.insert(normalize(&tool.name), tool.clone());
        }
    }

    if !enabled {
        return (unique_tools.into_values().collect(), HashMap::new());
    }

    let mut deferred_names: HashSet<String> = HashSet::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for message in &context.messages {
        match message {
            Message::Assistant(assistant) => {
                for block in &assistant.content {
                    if let crate::types::AssistantContentBlock::ToolCall(tc) = block {
                        used_names.insert(normalize(&tc.name));
                    }
                }
            }
            Message::ToolResult {
                added_tool_names: Some(names),
                ..
            } => {
                for name in names {
                    let normalized = normalize(name);
                    if !used_names.contains(&normalized) {
                        deferred_names.insert(normalized);
                    }
                }
            }
            _ => {}
        }
    }

    let mut immediate = Vec::new();
    let mut deferred = HashMap::new();
    for (name, tool) in unique_tools {
        if deferred_names.contains(&name) {
            deferred.insert(name, tool);
        } else {
            immediate.push(tool);
        }
    }
    (immediate, deferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantMessage, ContentBlock, Context, Message, Model, Tool, ToolCall};
    use serde_json::json;

    fn tool(name: &str) -> Tool {
        Tool {
            name: name.into(),
            description: name.into(),
            parameters: json!({"type": "object", "properties": {}}),
        }
    }

    #[test]
    fn split_defers_tools_introduced_by_added_tool_names() {
        let mut assistant = AssistantMessage::empty(&Model {
            id: "m".into(),
            name: "m".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: crate::types::ModelCost::default(),
            context_window: 200_000,
            max_tokens: 4096,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        });
        assistant.content = vec![crate::types::AssistantContentBlock::ToolCall(ToolCall::new(
            "1",
            "bash",
            json!({}),
        ))];

        let context = Context {
            system_prompt: None,
            messages: vec![
                Message::Assistant(assistant),
                Message::ToolResult {
                    tool_call_id: "1".into(),
                    tool_name: "bash".into(),
                    content: vec![ContentBlock::Text { text: "ok".into() }],
                    details: None,
                    added_tool_names: Some(vec!["web_search".into()]),
                    is_error: false,
                    timestamp: 1,
                },
            ],
            tools: Some(vec![tool("bash"), tool("web_search")]),
        };

        let (immediate, deferred) = split_deferred_tools(&context, true, None);
        assert_eq!(immediate.len(), 1);
        assert_eq!(immediate[0].name, "bash");
        assert!(deferred.contains_key("web_search"));
    }
}
