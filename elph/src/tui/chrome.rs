//! Live chrome stats (cost, tokens, git) for header and footer.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{build_session_context, estimate_context_tokens};
use elph_ai::get_builtin_model;

use crate::agent::CodingAgentSession;
use crate::platform::exit_message::aggregate_usage_from_entries;

use super::labels::{header_stats_label, model_footer_label};

/// Snapshot of usage and model metadata shown in header/footer chrome.
#[derive(Debug, Clone, PartialEq)]
pub struct ChromeStats {
    pub cost_usd: f64,
    pub tokens_used: u64,
    pub context_pct: f64,
    pub context_limit: u64,
    pub model_label: String,
    pub supports_images: bool,
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
        }
    }
}

pub fn read_git_branch(project_dir: &Path) -> Option<String> {
    elph_core::utils::git::read_branch(project_dir)
}

pub async fn refresh_chrome_stats(
    session: Arc<CodingAgentSession>,
    fallback_context_limit: u64,
    fallback_model_label: &str,
    fallback_supports_images: bool,
) -> ChromeStats {
    let entries = match session.branch_entries().await {
        Ok(entries) => entries,
        Err(err) => {
            log::debug!("chrome stats: branch entries unavailable: {err}");
            return ChromeStats {
                context_limit: fallback_context_limit,
                model_label: fallback_model_label.to_string(),
                supports_images: fallback_supports_images,
                ..ChromeStats::default()
            };
        }
    };

    let (_totals, cost_usd) = aggregate_usage_from_entries(&entries);
    let context = build_session_context(&entries);
    let estimate = estimate_context_tokens(&context.messages);

    let (context_limit, model_label, supports_images) =
        resolve_model_chrome(&context, fallback_context_limit, fallback_model_label, fallback_supports_images);

    let tokens_used = estimate.tokens;
    let context_pct = if context_limit > 0 {
        (tokens_used as f64 / context_limit as f64) * 100.0
    } else {
        0.0
    };

    ChromeStats {
        cost_usd,
        tokens_used,
        context_pct,
        context_limit,
        model_label,
        supports_images,
    }
}

fn resolve_model_chrome(
    context: &elph_agent::SessionContext,
    fallback_context_limit: u64,
    fallback_model_label: &str,
    fallback_supports_images: bool,
) -> (u64, String, bool) {
    let Some(model_ref) = context.model.as_ref() else {
        return (
            fallback_context_limit,
            fallback_model_label.to_string(),
            fallback_supports_images,
        );
    };

    let model_label = model_footer_label(Some(&model_ref.provider), Some(&model_ref.model_id));
    let Some(model) = get_builtin_model(&model_ref.provider, &model_ref.model_id) else {
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
    use super::*;

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
        assert!(label.contains("131k"));
        assert!(label.contains("48.2%"));
        assert!(label.contains("272k"));
    }
}
