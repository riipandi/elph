//! Live chrome stats (cost, tokens, git) for header and footer.

use std::path::Path;
use std::sync::Arc;

use elph_agent::SessionTreeEntry;
use elph_agent::{build_session_context, estimate_context_tokens};
use elph_ai::get_builtin_model;

use crate::agent::CodingAgentSession;
use crate::platform::exit_message::aggregate_usage_from_entries;

use crate::tui::labels::{GitFooterInfo, header_stats_label, model_footer_label};

/// Snapshot of usage and model metadata shown in header/footer chrome.
#[derive(Debug, Clone, PartialEq)]
pub struct ChromeStats {
    pub cost_usd: f64,
    pub tokens_used: u64,
    pub context_pct: f64,
    pub context_limit: u64,
    pub model_label: String,
    pub supports_images: bool,
    pub turn_count: u32,
}

impl Default for ChromeStats {
    fn default() -> Self {
        Self {
            cost_usd: 0.0,
            tokens_used: 0,
            context_pct: 0.0,
            context_limit: 200_000,
            model_label: String::new(),
            supports_images: false,
            turn_count: 0,
        }
    }
}

/// Count user-initiated turns on the active session branch (one per user message).
pub fn count_user_turns(path_entries: &[SessionTreeEntry]) -> u32 {
    path_entries
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                SessionTreeEntry::Message { message, .. } if message.role() == "user"
            )
        })
        .count() as u32
}

pub fn read_git_branch(project_dir: &Path) -> Option<String> {
    elph_core::utils::git::read_branch(project_dir)
}

/// Branch name and changed-file count for the footer, when `project_dir` is a git work tree.
pub fn read_git_footer_info(project_dir: &Path) -> Option<GitFooterInfo> {
    if !elph_core::utils::git::is_worktree(project_dir) {
        return None;
    }
    let branch = read_git_branch(project_dir).unwrap_or_else(|| "HEAD".to_string());
    let stats = elph_core::utils::git::read_worktree_stats(project_dir).unwrap_or_default();
    Some(GitFooterInfo {
        branch,
        files_added: stats.files_added,
        lines_added: stats.lines_added,
        files_deleted: stats.files_deleted,
        lines_deleted: stats.lines_deleted,
    })
}

/// Immediate footer/header model metadata from the live session selection (no branch I/O).
pub fn chrome_stats_from_session(session: &CodingAgentSession, fallback_context_limit: u64) -> ChromeStats {
    let context_limit = session.context_window().max(1) as u64;
    ChromeStats {
        context_limit: if context_limit > 0 {
            context_limit
        } else {
            fallback_context_limit
        },
        model_label: model_footer_label(Some(session.model_provider()), Some(session.model_id())),
        supports_images: session.supports_image_input(),
        ..ChromeStats::default()
    }
}

pub async fn refresh_chrome_stats(
    session: Arc<CodingAgentSession>,
    fallback_context_limit: u64,
    fallback_model_label: &str,
    fallback_supports_images: bool,
) -> ChromeStats {
    let live_provider = session.model_provider();
    let live_model_id = session.model_id();

    let entries = match session.branch_entries().await {
        Ok(entries) => entries,
        Err(err) => {
            log::debug!("chrome stats: branch entries unavailable: {err}");
            return ChromeStats {
                context_limit: session.context_window() as u64,
                model_label: model_footer_label(Some(live_provider), Some(live_model_id)),
                supports_images: session.supports_image_input(),
                ..ChromeStats::default()
            };
        }
    };

    let (_totals, cost_usd) = aggregate_usage_from_entries(&entries);
    let context = build_session_context(&entries);
    let estimate = estimate_context_tokens(&context.messages);

    let (context_limit, model_label, supports_images) = resolve_model_chrome(
        &context,
        live_provider,
        live_model_id,
        fallback_context_limit,
        fallback_model_label,
        fallback_supports_images,
    );

    let tokens_used = estimate.tokens;
    let context_pct = if context_limit > 0 {
        (tokens_used as f64 / context_limit as f64) * 100.0
    } else {
        0.0
    };
    let turn_count = count_user_turns(&entries);

    ChromeStats {
        cost_usd,
        tokens_used,
        context_pct,
        context_limit,
        model_label,
        supports_images,
        turn_count,
    }
}

fn effective_model_ids<'a>(
    context_model: Option<&'a elph_agent::SessionModelRef>,
    live_provider: &'a str,
    live_model_id: &'a str,
) -> (&'a str, &'a str) {
    if let Some(model_ref) = context_model {
        (model_ref.provider.as_str(), model_ref.model_id.as_str())
    } else {
        (live_provider, live_model_id)
    }
}

fn resolve_model_chrome(
    context: &elph_agent::SessionContext,
    live_provider: &str,
    live_model_id: &str,
    fallback_context_limit: u64,
    _fallback_model_label: &str,
    fallback_supports_images: bool,
) -> (u64, String, bool) {
    let (provider, model_id) = effective_model_ids(context.model.as_ref(), live_provider, live_model_id);

    let model_label = model_footer_label(Some(provider), Some(model_id));
    let Some(model) = get_builtin_model(provider, model_id) else {
        return (fallback_context_limit, model_label, fallback_supports_images);
    };

    let context_limit = model.context_window as u64;
    let supports_images = model.input.iter().any(|cap| cap == "image");
    (context_limit, model_label, supports_images)
}

pub fn header_stats_from_chrome(stats: &ChromeStats, footer_token_display: &str) -> String {
    header_stats_label(
        stats.cost_usd,
        stats.tokens_used,
        stats.context_pct,
        stats.context_limit,
        footer_token_display,
    )
}

#[cfg(test)]
mod tests {
    use elph_agent::llm_message_to_agent;
    use elph_ai::Message;

    use super::*;

    fn user_entry(id: &str, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message {
            id: id.to_string(),
            parent_id: None,
            timestamp: "2026-01-01T00:00:00.000Z".to_string(),
            message: llm_message_to_agent(Message::User {
                content: elph_ai::UserContent::Text(text.into()),
                timestamp: 0,
            }),
        }
    }

    fn assistant_entry(id: &str, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message {
            id: id.to_string(),
            parent_id: None,
            timestamp: "2026-01-01T00:00:00.000Z".to_string(),
            message: llm_message_to_agent(Message::Assistant(elph_ai::faux_assistant_message(
                vec![elph_ai::faux_text(text)],
                None,
            ))),
        }
    }

    #[test]
    fn count_user_turns_counts_only_user_messages() {
        let entries = vec![
            user_entry("u1", "hello"),
            assistant_entry("a1", "hi"),
            user_entry("u2", "again"),
        ];
        assert_eq!(count_user_turns(&entries), 2);
    }

    #[test]
    fn effective_model_ids_prefers_context_then_live_selection() {
        use elph_agent::SessionModelRef;

        let context_model = SessionModelRef {
            provider: "anthropic".to_string(),
            model_id: "claude-sonnet-4".to_string(),
        };
        assert_eq!(
            effective_model_ids(Some(&context_model), "opencode", "big-pickle"),
            ("anthropic", "claude-sonnet-4")
        );
        assert_eq!(effective_model_ids(None, "opencode", "big-pickle"), ("opencode", "big-pickle"));
    }

    #[test]
    fn header_stats_from_chrome_formats_defaults() {
        let stats = ChromeStats {
            cost_usd: 0.12,
            tokens_used: 131_000,
            context_pct: 48.2,
            context_limit: 272_000,
            ..ChromeStats::default()
        };
        let label = header_stats_from_chrome(&stats, "both");
        assert!(label.contains("$0.12"));
        assert!(label.contains("131K"));
        assert!(label.contains("48.2%"));
        assert!(label.contains("272K"));
    }
}
