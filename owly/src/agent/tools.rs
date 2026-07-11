use elph_agent::{AgentToolResult, ToolResultContent};

pub(super) fn summarize_tool_args(tool_name: &str, args: &serde_json::Value) -> String {
    if crate::ask_user::ASK_TOOL_NAMES.contains(&tool_name) {
        crate::ask_user::format_args_summary(tool_name, args)
    } else {
        args.to_string()
    }
}

pub(super) fn summarize_tool_result(result: &AgentToolResult) -> String {
    const MAX: usize = 4_096;
    let mut out = String::new();
    for block in &result.content {
        match block {
            ToolResultContent::Text(text) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text.text);
            }
            ToolResultContent::Image(_) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("[image output]");
            }
        }
        if out.len() >= MAX {
            out.truncate(MAX);
            out.push_str("...");
            return out;
        }
    }
    if out.is_empty() {
        let details = serde_json::to_string(&result.details).unwrap_or_default();
        if !details.is_empty() && details != "{}" && details != "null" {
            if details.len() > MAX {
                let mut truncated = details;
                truncated.truncate(MAX);
                truncated.push_str("...");
                return truncated;
            }
            return details;
        }
    }
    out
}
