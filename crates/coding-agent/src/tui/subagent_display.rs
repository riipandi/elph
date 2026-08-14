//! Accessible subagent labels for activity footer and transcript status rows.

use crate::agent::SubagentUiPhase;
use crate::tui::tool_params::tool_display_verb;

/// Stable transcript key for upserting one status row per subagent.
pub fn subagent_status_key(agent_id: &str) -> String {
    format!("subagent:{agent_id}")
}

/// Short, scannable name: prefer task_name, else path tail, else truncated id.
pub fn subagent_short_name(task_name: &str, agent_path: &str, agent_id: &str) -> String {
    let task = task_name.trim();
    if !task.is_empty() && !task.eq_ignore_ascii_case("default") && !task.eq_ignore_ascii_case("agent") {
        return truncate_chars(task, 24);
    }
    if let Some(tail) = agent_path
        .rsplit('/')
        .find(|part| !part.is_empty() && *part != "main" && *part != "root")
    {
        return truncate_chars(tail, 24);
    }
    if let Some(tail) = agent_path.rsplit('/').find(|part| !part.is_empty()) {
        return truncate_chars(tail, 24);
    }
    truncate_chars(agent_id, 16)
}

/// Nesting depth from `agent_path` (`main` = 0, `main/a` = 1, …), capped for indent.
pub fn subagent_depth(agent_path: &str) -> u32 {
    let segments = agent_path
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
        .saturating_sub(1);
    segments.min(3) as u32
}

/// Human action text from a tool name or free-form message.
pub fn subagent_action_label(message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        return String::new();
    }
    if let Some(tool) = message.strip_prefix("tool:") {
        return tool_display_verb(tool.trim());
    }
    if message.contains('_') && !message.contains(' ') {
        return tool_display_verb(message);
    }
    truncate_chars(message, 40)
}

/// Task title label in the new tree format. Example: `worker-1 [claude-sonnet-4]  Analyze`
///
/// Nesting is applied as whole-row padding ([`subagent_status_indent`]), not leading spaces
/// in the label — spaces in the label push the text far from the status glyph.
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_subagent_task_label(task_name: &str, agent_path: &str, agent_id: &str, model: &str) -> String {
    let name = subagent_short_name(task_name, agent_path, agent_id);
    let model_tag = if model.is_empty() {
        String::new()
    } else {
        format!(" [{}]", model)
    };
    let task = task_name.trim();
    if task.is_empty() || task == name || task.eq_ignore_ascii_case("default") || task.eq_ignore_ascii_case("agent") {
        format!("{name}{model_tag}")
    } else {
        format!("{name}{model_tag}  {task}")
    }
}

/// Extra left pad (cells) for nested subagent status rows so the glyph indents with the label.
pub fn subagent_status_indent(agent_path: &str) -> u16 {
    // Children of `main` (depth 1) stay flush with other process rows; only deeper nests indent.
    // Two cells per extra level so nested agents remain scannable without padding every row.
    subagent_depth(agent_path).saturating_sub(1).saturating_mul(2) as u16
}

/// Secondary detail (never bold): action + phase word. Example: `Read · running`
///
/// Note: In the tree format, detail is now merged into the label content. This function
/// is kept for legacy status rows (non-tree startup/mcp status lines).
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_subagent_status_detail(action: &str, phase: SubagentUiPhase) -> String {
    let phase_word = match phase {
        SubagentUiPhase::Pending => "starting",
        _ => phase.as_word(),
    };
    let action = subagent_action_label(action);
    if action.is_empty() {
        phase_word.to_string()
    } else {
        format!("{action} · {phase_word}")
    }
}

/// Full single-line label for layout/tests (task + detail).
///
/// Example: `worker-1 [claude-sonnet-4]  worker-1 · Read · running`
#[cfg(test)]
#[allow(dead_code)]
pub fn format_subagent_status_label(
    task_name: &str,
    agent_path: &str,
    agent_id: &str,
    action: &str,
    phase: SubagentUiPhase,
) -> String {
    let task = format_subagent_task_label(task_name, agent_path, agent_id, "");
    let detail = format_subagent_status_detail(action, phase);
    if detail.is_empty() {
        task
    } else {
        format!("{task} · {detail}")
    }
}

/// Busy footer label (no indent; keep short).
pub fn format_subagent_activity_label(task_name: &str, agent_path: &str, agent_id: &str, action: &str) -> String {
    let name = subagent_short_name(task_name, agent_path, agent_id);
    let action = subagent_action_label(action);
    if action.is_empty() {
        format!("Subagent {name}")
    } else {
        format!("Subagent {name} · {action}")
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1).max(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_prefers_task_then_path_tail() {
        assert_eq!(
            subagent_short_name("summarize-docs", "main/worker-1", "agent_long_id"),
            "summarize-docs"
        );
        assert_eq!(subagent_short_name("", "main/worker-1", "agent_long_id"), "worker-1");
        assert_eq!(subagent_short_name("default", "main/nested/leaf", "agent_long_id"), "leaf");
    }

    #[test]
    fn depth_from_path_segments() {
        assert_eq!(subagent_depth("main"), 0);
        assert_eq!(subagent_depth("main/worker"), 1);
        assert_eq!(subagent_depth("main/a/b/c"), 3);
    }

    #[test]
    fn tree_task_label_includes_model() {
        // Name from task_name (deduplicated when task_name == short_name).
        assert_eq!(
            format_subagent_task_label("worker-1", "main/worker-1", "id", "claude-sonnet-4"),
            "worker-1 [claude-sonnet-4]"
        );
        // Without model.
        assert_eq!(format_subagent_task_label("worker-1", "main/worker-1", "id", ""), "worker-1");
        // Sentinels fall through to path tail.
        assert_eq!(
            format_subagent_task_label("default", "main/w-1", "agent_01", "claude-sonnet-4"),
            "w-1 [claude-sonnet-4]"
        );
        // Empty falls through to path tail.
        assert_eq!(
            format_subagent_task_label("", "main/leaf-agent", "agent_02", "gpt-4o"),
            "leaf-agent [gpt-4o]"
        );
        // Name from agent_id when no task_name or path tail.
        assert_eq!(format_subagent_task_label("", "", "agent-xyz", "sonnet"), "agent-xyz [sonnet]");
    }

    #[test]
    fn tree_task_label_omits_redundant_task() {
        // When task_name == short_name, only name+model shown (no duplicate).
        let task = format_subagent_task_label("worker-1", "main/worker-1", "id", "claude-sonnet-4");
        assert_eq!(task, "worker-1 [claude-sonnet-4]");
    }

    #[test]
    fn status_label_detail_still_works_legacy() {
        let detail = format_subagent_status_detail("tool: read_file", SubagentUiPhase::Running);
        assert_eq!(subagent_status_indent("main/worker-1"), 0);
        assert_eq!(detail, "Read · running");
    }

    #[test]
    fn nested_path_indents_row_not_label() {
        let task = format_subagent_task_label("", "main/a/b", "id", "claude-sonnet-4");
        assert!(!task.starts_with(' '), "{task:?}");
        // depth 2 → one nesting step past main children → 2 cells.
        assert_eq!(subagent_status_indent("main/a/b"), 2);
        assert_eq!(subagent_status_indent("main"), 0);
        assert_eq!(subagent_status_indent("main/leaf"), 0);
        let detail = format_subagent_status_detail("", SubagentUiPhase::Idle);
        assert_eq!(detail, "idle");
    }

    #[test]
    fn activity_label_is_compact() {
        assert_eq!(
            format_subagent_activity_label("worker-1", "main/worker-1", "id", "tool: shell_exec"),
            "Subagent worker-1 · Shell"
        );
    }
}
