//! `/session` slash command — format current session metadata for display.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Timelike, Utc};
use elph_agent::{
    AgentMessage, FileSystem, SessionTreeEntry, Skill, build_session_context, estimate_context_tokens,
    estimate_tokens_with_system_prompt,
};
use elph_ai::utils::estimate::count_tokens_text;

use super::CodingAgentSession;
use crate::tui::chrome::count_user_turns;

const SESSION_INFO_TIMEOUT: Duration = Duration::from_millis(1_200);

/// Human-readable API backend label for chrome / session info (e.g. `openai-responses` → `Responses`).
pub fn format_api_backend(api: &str) -> String {
    let trimmed = api.trim();
    if trimmed.is_empty() {
        return "Unknown".to_string();
    }
    // Prefer the last kebab segment when present (`openai-responses` → `responses`).
    let segment = trimmed.rsplit('-').next().unwrap_or(trimmed);
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) => {
            let mut out = first.to_uppercase().collect::<String>();
            out.push_str(&chars.as_str().to_lowercase());
            out
        }
        None => "Unknown".to_string(),
    }
}

/// Format an ISO / RFC3339 session timestamp for display (local wall clock).
///
/// Examples: `2026-07-27 15:42`, `2026-07-27 15:42:03` (seconds kept when non-zero).
pub fn format_session_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "—".to_string();
    }
    let Some(utc) = parse_session_timestamp(trimmed) else {
        return trimmed.to_string();
    };
    let local = utc.with_timezone(&Local);
    if local.timestamp_subsec_millis() == 0 && local.second() == 0 {
        local.format("%Y-%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

fn parse_session_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ")
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
}

/// Compact context line: `7.0K / 200K tokens (4%)`.
pub fn format_context_usage_line(tokens_used: u64, context_limit: u64, context_pct: u64) -> String {
    format!(
        "{} / {} tokens ({}%)",
        fmt_token_one_dec(tokens_used),
        fmt_token_one_dec(context_limit),
        context_pct
    )
}

/// Format token count with one decimal: `0.0K`, `7.0K`, `12.3K`, `1.5M`.
fn fmt_token_one_dec(n: u64) -> String {
    const MILLION: u64 = 1_000_000;
    const THOUSAND: u64 = 1_000;
    if n >= MILLION {
        let whole = n / MILLION;
        let tenths = (n % MILLION) / 100_000;
        format!("{whole}.{tenths}M")
    } else if n >= THOUSAND {
        let whole = n / THOUSAND;
        let frac = (n % THOUSAND) / 100;
        format!("{whole}.{frac}K")
    } else {
        format!("{n}")
    }
}

/// Format session cost as `$0.1234` (4 decimal places).
fn format_session_cost(total_cost: f64) -> String {
    if total_cost < 0.0001 {
        "$0.0000".to_string()
    } else {
        format!("${:.4}", total_cost)
    }
}

/// Token breakdown by category.
struct ContextBreakdown {
    system: u64,
    user_messages: u64,
    assistant: u64,
    tool_results: u64,
    files: u64,
}

/// Categorize context tokens from messages.
///
/// Counts actual tokens from each message type. The system prompt is counted from the
/// cached system prompt text. Total is scaled to match `total_tokens` (provider usage
/// or heuristic estimate) so percentages sum to 100%.
fn categorize_context_tokens(
    messages: &[AgentMessage],
    system_prompt: Option<&str>,
    total_tokens: u64,
) -> ContextBreakdown {
    // Count system prompt from actual text.
    let system_raw = system_prompt.map(count_tokens_text).unwrap_or(0);

    let mut user_messages: u64 = 0;
    let mut assistant: u64 = 0;
    let mut tool_results: u64 = 0;
    let mut files: u64 = 0;

    for msg in messages {
        match msg {
            AgentMessage::Llm(llm) => match llm.as_ref() {
                elph_ai::Message::User { content, .. } => match content {
                    elph_ai::UserContent::Text(text) => user_messages += count_tokens_text(text),
                    elph_ai::UserContent::Blocks(blocks) => {
                        for block in blocks {
                            match block {
                                elph_ai::ContentBlock::Text { text } => user_messages += count_tokens_text(text),
                                elph_ai::ContentBlock::Image { .. } => files += 1200,
                            }
                        }
                    }
                },
                elph_ai::Message::Assistant(assistant_msg) => {
                    for block in &assistant_msg.content {
                        match block {
                            elph_ai::AssistantContentBlock::Text(text) => assistant += count_tokens_text(&text.text),
                            elph_ai::AssistantContentBlock::Thinking(t) => assistant += count_tokens_text(&t.thinking),
                            elph_ai::AssistantContentBlock::ToolCall(tc) => {
                                assistant += count_tokens_text(&tc.name);
                                assistant += count_tokens_text(&tc.id);
                                assistant +=
                                    count_tokens_text(&serde_json::to_string(&tc.arguments).unwrap_or_default());
                            }
                        }
                    }
                }
                elph_ai::Message::ToolResult { content, .. } => {
                    for block in content {
                        match block {
                            elph_ai::ContentBlock::Text { text } => tool_results += count_tokens_text(text),
                            elph_ai::ContentBlock::Image { .. } => tool_results += 1200,
                        }
                    }
                }
            },
            AgentMessage::Custom(custom) => match custom {
                elph_agent::CustomAgentMessage::ShellExecExecution { command, output, .. } => {
                    tool_results += count_tokens_text(command);
                    if let Some(out) = output.as_ref() {
                        tool_results += count_tokens_text(out);
                    }
                }
                elph_agent::CustomAgentMessage::BranchSummary { summary, .. }
                | elph_agent::CustomAgentMessage::CompactionSummary { summary, .. } => {
                    assistant += count_tokens_text(summary);
                }
                elph_agent::CustomAgentMessage::Custom { content, .. } => {
                    tool_results += content
                        .as_str()
                        .map(count_tokens_text)
                        .unwrap_or_else(|| count_tokens_text(&serde_json::to_string(content).unwrap_or_default()));
                }
            },
        }
    }

    let counted = system_raw + user_messages + assistant + tool_results + files;

    if counted > 0 && counted != total_tokens {
        let scale = total_tokens as f64 / counted as f64;
        ContextBreakdown {
            system: (system_raw as f64 * scale).round() as u64,
            user_messages: (user_messages as f64 * scale).round() as u64,
            assistant: (assistant as f64 * scale).round() as u64,
            tool_results: (tool_results as f64 * scale).round() as u64,
            files: (files as f64 * scale).round() as u64,
        }
    } else {
        ContextBreakdown {
            system: system_raw,
            user_messages,
            assistant,
            tool_results,
            files,
        }
    }
}

/// Render breakdown with section grouping: System, then User Context.
fn format_context_breakdown(total_tokens: u64, context_limit: u64, breakdown: &ContextBreakdown) -> String {
    if total_tokens == 0 {
        return String::new();
    }

    let limit = context_limit.max(1);
    fn pct_of(part: u64, total: u64) -> String {
        let p = (part as f64 / total as f64) * 100.0;
        format!("{:.2}%", p)
    }
    fn fmt_count(n: u64) -> String {
        if n == 0 {
            "0.0K".to_string()
        } else {
            fmt_token_one_dec(n)
        }
    }

    let rows: Vec<(&str, &str, u64)> = vec![
        // (section, label, value)
        // Always show all categories (including zero) for detailed view.
        ("System", "System Prompt", breakdown.system),
        ("User Context", "User Messages", breakdown.user_messages),
        ("User Context", "Assistant Replies", breakdown.assistant),
        ("User Context", "Tool Results", breakdown.tool_results),
        ("User Context", "Files", breakdown.files),
    ];

    if rows.is_empty() {
        return String::new();
    }

    let label_w = rows.iter().map(|(_, l, _)| l.len()).max().unwrap_or(14);
    let mut lines: Vec<String> = Vec::new();

    for (_, label, val) in &rows {
        lines.push(format!("  {:label_w$}  {:>6}  {}", label, fmt_count(*val), pct_of(*val, limit),));
    }

    lines.join("\n")
}

/// Sum session cost from all assistant messages in session entries.
fn calculate_session_cost(entries: &[SessionTreeEntry]) -> f64 {
    entries
        .iter()
        .filter_map(|entry| {
            let SessionTreeEntry::Message { message, .. } = entry else {
                return None;
            };
            let AgentMessage::Llm(llm) = message else { return None };
            let elph_ai::Message::Assistant(assistant) = llm.as_ref() else {
                return None;
            };
            if matches!(assistant.stop_reason, elph_ai::StopReason::Aborted | elph_ai::StopReason::Error) {
                return None;
            }
            let cost = assistant.usage.cost.total;
            if cost > 0.0 { Some(cost) } else { None }
        })
        .sum()
}

/// Build the multi-line session info body shown by `/session`.
pub async fn format_session_info(session: &CodingAgentSession, skills: Option<&[Skill]>) -> String {
    let title = session
        .harness()
        .session_name()
        .await
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "(untitled)".to_string());
    let session_id = session.session_id().to_string();
    let cwd = session.harness().env().cwd().to_string();
    let provider = session.model_provider();
    let model_id = session.model_id();
    let api_backend = format_api_backend(&session.model_api());
    let title_model = session.title_model();
    let last_activity = {
        let meta = session.harness().session_metadata().await;
        format_session_timestamp(&meta.updated_at)
    };

    let (turn_count, tokens_used, context_limit, context_pct, session_cost, breakdown) =
        match session.branch_entries().await {
            Ok(entries) => {
                let turn_count = count_user_turns(&entries);
                let context = build_session_context(&entries);
                let estimate = estimate_context_tokens(&context.messages);
                let limit = session.context_window().max(1) as u64;
                let used = estimate_tokens_with_system_prompt(estimate, session.cached_system_prompt().as_deref());
                let pct = if limit > 0 {
                    ((used as f64 / limit as f64) * 100.0).round() as u64
                } else {
                    0
                };
                let cost = calculate_session_cost(&entries);
                let breakdown =
                    categorize_context_tokens(&context.messages, session.cached_system_prompt().as_deref(), used);
                (turn_count, used, limit, pct, cost, breakdown)
            }
            Err(_) => {
                let limit = session.context_window().max(1) as u64;
                let breakdown = ContextBreakdown {
                    system: 0,
                    user_messages: 0,
                    assistant: 0,
                    tool_results: 0,
                    files: 0,
                };
                (0, 0, limit, 0, 0.0, breakdown)
            }
        };
    let context_line = format_context_usage_line(tokens_used, context_limit, context_pct);
    let cost_line = format_session_cost(session_cost);
    let breakdown_lines = format_context_breakdown(tokens_used, context_limit, &breakdown);
    let mcp_lines = format_mcp_info(session);
    let tools_line = format_tools_info(session).await;
    let skills_line = format_skills_info(skills);

    // Assemble sections with blank-line separators.
    let mut parts: Vec<String> = Vec::new();

    // ── Session ──
    parts.push(format!("Title: {title}"));
    parts.push(format!("Session ID: {session_id}"));
    parts.push(format!("Working directory: {cwd}"));

    // ── Model ──
    parts.push(String::new());
    parts.push(format!("Model: {provider}/{model_id}"));
    parts.push(format!("Title model: {title_model}"));
    parts.push(format!("API Backend: {api_backend}"));

    // ── Activity ──
    parts.push(String::new());
    parts.push(format!("Last activity: {last_activity}"));
    parts.push(format!("Turn: {turn_count}"));

    // ── MCP ──
    if !mcp_lines.is_empty() {
        parts.push(String::new());
        parts.push(mcp_lines);
    }

    // ── Usage ──
    parts.push(String::new());
    parts.push(format!("Context Window: {context_line}"));
    if !breakdown_lines.is_empty() {
        parts.push(breakdown_lines);
    }
    parts.push(format!("Session Cost: {cost_line}"));

    // ── Tools ──
    parts.push(String::new());
    parts.push(format!("Tools: {}", tools_line.trim_end()));
    if !skills_line.is_empty() {
        parts.push(skills_line.trim_end().to_string());
    }

    parts.join("\n")
}

/// MCP servers as a multi-line list (one per row).
fn format_mcp_info(session: &CodingAgentSession) -> String {
    match session.mcp_registry() {
        Some(registry) => {
            let report = registry.load_report();
            if report.servers.is_empty() {
                return "MCP: \u{2014}".to_string();
            }
            let mut lines = vec!["MCP:".to_string()];
            // Find longest server name for alignment.
            let max_name = report.servers.iter().map(|s| s.name.len()).max().unwrap_or(0);
            for s in &report.servers {
                let detail = if s.ok {
                    format!("  {:max_name$}  \u{2713}  {} tools", s.name, s.tool_count)
                } else {
                    format!("  {:max_name$}  \u{2717}  ({})", s.name, s.message)
                };
                lines.push(detail);
            }
            lines.join("\n")
        }
        None => "MCP: \u{2014}".to_string(),
    }
}

/// Active tools line: count and a compact grouping hint.
async fn format_tools_info(session: &CodingAgentSession) -> String {
    let tools = session.harness().get_active_tools().await;
    if tools.is_empty() {
        return "\u{2014}".to_string();
    }
    let total = tools.len();

    // Separate MCP tools (`mcp_{server}__{tool}`) from built-in tools.
    let mcp_count = tools.iter().filter(|t| t.name().starts_with("mcp_")).count();
    let builtin_count = total.saturating_sub(mcp_count);

    let detail = if mcp_count > 0 && builtin_count > 0 {
        format!("{total} tools ({builtin_count} built-in, {mcp_count} MCP)")
    } else if mcp_count > 0 {
        format!("{total} tools (all MCP)")
    } else {
        format!("{total} tools")
    };
    detail.to_string()
}

/// Loaded skills summary line (count and names when few enough).
fn format_skills_info(skills: Option<&[Skill]>) -> String {
    let Some(skills) = skills else {
        return String::new();
    };
    if skills.is_empty() {
        return "Skills: \u{2014}".to_string();
    }
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    if names.len() <= 4 {
        format!("Skills: {}", names.join(", "))
    } else {
        format!("Skills: {} skills", names.len())
    }
}

/// Sync entry point for the TUI slash handler.
pub fn session_info_slash_message(
    session: Option<&Arc<CodingAgentSession>>,
    skills: Option<&[Skill]>,
) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };
    let session = Arc::clone(session);
    let skills: Option<Vec<Skill>> = skills.map(|s| s.to_vec());
    match elph_agent::try_block_on_detached(
        async move { format_session_info(&session, skills.as_deref()).await },
        SESSION_INFO_TIMEOUT,
    ) {
        Ok(text) => Ok(text),
        Err(err) if err.to_string().contains("timed out") => {
            Err("Agent is busy. Wait for the current stream to finish, then run /session again.".into())
        }
        Err(err) => Err(format!("Failed to load session info: {err}")),
    }
}

/// Resolve current session title for `/rename` prefill (empty when untitled).
pub fn session_title_for_rename(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for this command.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::try_block_on_detached(
        async move { session.harness().session_name().await.unwrap_or_default() },
        Duration::from_millis(400),
    ) {
        Ok(name) => Ok(name),
        Err(err) if err.to_string().contains("timed out") => Ok(String::new()),
        Err(err) => Err(format!("Failed to load session title: {err}")),
    }
}

/// Persist a new session title (sync wrapper for the rename dialog submit path).
pub fn rename_session_title(session: &Arc<CodingAgentSession>, title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Session title cannot be empty.".into());
    }
    let session = Arc::clone(session);
    let title = title.to_string();
    match elph_agent::try_block_on_detached(
        async move {
            session
                .harness()
                .set_session_name(title)
                .await
                .map_err(|e| e.to_string())
        },
        Duration::from_millis(800),
    ) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(format!("Failed to rename session: {err}")),
        Err(err) => Err(format!("Failed to rename session: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_api_backend_last_segment() {
        assert_eq!(format_api_backend("openai-responses"), "Responses");
        assert_eq!(format_api_backend("openai-completions"), "Completions");
        assert_eq!(format_api_backend("anthropic-messages"), "Messages");
        assert_eq!(format_api_backend("Responses"), "Responses");
        assert_eq!(format_api_backend(""), "Unknown");
    }

    #[test]
    fn session_info_layout_keys() {
        let sample = "Title: Fix login\n\
Session ID: abc\n\
Working directory: /tmp\n\
\n\
Model: openai/gpt-4o\n\
Title model: inherit\n\
API Backend: Responses\n\
\n\
Last activity: 2026-07-27 12:00\n\
Turn: 15\n\
\n\
MCP:\n\
  server-1   ✓  3 tools\n\
  server-2   ✗  (connection refused)\n\
\n\
Context Window: 75.0K / 500.0K tokens (15%)\n\
  System Prompt        75.0K  15.00%\n\
  User Messages        12.0K  2.40%\n\
  Assistant Replies     3.0K  0.60%\n\
  Tool Results          0.0K  0.00%\n\
  Files                 0.0K  0.00%\n\
Session Cost: $0.0000\n\
\n\
Tools: 5 tools\n\
Skills: 3 skills";
        for key in [
            "Title:",
            "Session ID:",
            "Working directory:",
            "Model:",
            "Title model:",
            "API Backend:",
            "Last activity:",
            "MCP:",
            "Session Cost:",
            "Context Window:",
            "System Prompt",
            "User Messages",
            "Assistant Replies",
            "Tools:",
            "Skills:",
            "Turn:",
        ] {
            assert!(sample.contains(key), "missing {key}");
        }
    }

    #[test]
    fn context_usage_uses_compact_counts() {
        assert_eq!(format_context_usage_line(75_377, 500_000, 15), "75.3K / 500.0K tokens (15%)");
        assert_eq!(format_context_usage_line(1_500_000, 2_000_000, 75), "1.5M / 2.0M tokens (75%)");
        assert_eq!(format_context_usage_line(42, 999, 4), "42 / 999 tokens (4%)");
    }

    #[test]
    fn session_timestamp_formats_rfc3339() {
        let formatted = format_session_timestamp("2026-07-27T12:00:00.000Z");
        // Local offset may shift the hour; date prefix should remain readable.
        assert!(
            formatted.starts_with("2026-07-2"),
            "unexpected formatted timestamp: {formatted}"
        );
        assert!(!formatted.contains('T'));
        assert!(!formatted.ends_with('Z'));
        assert_eq!(format_session_timestamp(""), "—");
        assert_eq!(format_session_timestamp("not-a-date"), "not-a-date");
    }
}
