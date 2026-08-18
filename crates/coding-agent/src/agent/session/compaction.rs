//! Compaction UX: lifecycle notices, auto-compact, model-switch fit, model refs.

use anyhow::Result;
use elph_agent::compaction::CompactionSettings;
use elph_agent::compaction::{estimate_context_tokens, estimate_tokens_with_system_prompt, should_compact};
use elph_agent::harness::CompactResult;
use elph_agent::session::build_session_context;
use elph_ai::Model;
use elph_ai::estimate::count_tokens_text;

use super::super::events::AgentUiEvent;
use super::CodingAgentSession;

/// Build the auto-compaction notice, distinguishing "history alone" from "history + prompt".
pub(crate) fn auto_compact_will_message(
    used: u64,
    window: u64,
    prompt_tokens: u64,
    settings: CompactionSettings,
) -> String {
    let threshold = match settings.threshold_pct {
        Some(pct) => window * (pct as u64) / 100,
        None => window.saturating_sub(settings.reserve_tokens),
    };
    let history_over = used > threshold;
    let pct = used.saturating_mul(100).checked_div(window).unwrap_or(0);
    if !history_over && prompt_tokens > 0 {
        format!(
            "Auto-compaction: context ~{used}/{window} tokens ({pct}%) + prompt ~{prompt_tokens} tokens would exceed threshold — summarizing older history…"
        )
    } else {
        format!(
            "Auto-compaction: context ~{used}/{window} tokens ({pct}%) exceeds threshold — summarizing older history…"
        )
    }
}

/// Why compaction was requested (affects notice copy and pass limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactSource {
    Manual,
    Automatic,
    ModelSwitch,
}

impl CodingAgentSession {
    /// Resolve `models.compactionModel` / title-style refs against the live session model.
    pub fn resolve_settings_model_ref(&self, setting: &str) -> Result<Model> {
        let inherit = self.selection.read().model.clone();
        resolve_settings_model_ref(setting, &inherit)
    }

    pub(crate) fn resolve_compaction_model(&self) -> Model {
        self.resolve_settings_model_ref(&self.compaction_model_ref)
            .unwrap_or_else(|_| self.selection.read().model.clone())
    }

    /// Estimate tokens the LLM would see on the active branch (after compaction transform).
    ///
    /// Mirrors the header's context-usage label (`tui/chrome/stats.rs`): the session-message
    /// estimate plus the compiled system prompt — counted once — so the auto-compaction
    /// decision is made on exactly the number the user sees in the chrome.
    pub async fn estimate_context_usage(&self) -> Result<(u64, u64)> {
        let model = self.harness.get_model().await;
        let window = model.context_window as u64;
        let entries = self
            .harness
            .session_branch_entries()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let context = build_session_context(&entries);
        let estimate = estimate_context_tokens(&context.messages);
        let tokens = estimate_tokens_with_system_prompt(estimate, self.cached_system_prompt().as_deref());
        Ok((tokens, window))
    }

    fn notice(&self, message: impl Into<String>) {
        let _ = self.ui_tx.send(AgentUiEvent::TranscriptNotice(message.into()));
    }

    /// Run harness compact with lifecycle notices. Caller must hold `turn_gate` when required.
    pub(crate) async fn run_compact_with_notices(
        &self,
        source: CompactSource,
        custom_instructions: Option<&str>,
        will_message: Option<String>,
        model_override: Option<&Model>,
    ) -> Result<CompactResult> {
        let had_will = will_message.is_some();
        if let Some(msg) = will_message {
            self.notice(msg);
        }

        // Resolve the summarization model up front so it can be shown while compaction runs.
        let model = model_override
            .cloned()
            .unwrap_or_else(|| self.resolve_compaction_model());
        let model_ref = format!("{}/{}", model.provider.as_str(), model.id);

        let running = match source {
            CompactSource::Manual => format!("Compacting history with {model_ref}"),
            CompactSource::Automatic => format!("Auto-compacting history with {model_ref}"),
            CompactSource::ModelSwitch => {
                format!("Compacting history for the new model’s context limit with {model_ref}")
            }
        };
        self.notice(running.clone());
        // Surface the running label on the status row (busy indicator) so the user sees the
        // agent is actively compacting history — not frozen — while the turn is still busy.
        // The status text must exactly match the sticky notice so the transcript applier
        // collapses the pair into a single card (see `TranscriptEventApplier::push_status`).
        let _ = self.ui_tx.send(AgentUiEvent::Status(running.clone()));

        let before = self.estimate_context_usage().await.ok().map(|(t, _)| t);
        let result = self
            .harness
            .compact(custom_instructions, Some(&model))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if result.is_noop() {
            if matches!(source, CompactSource::Manual) {
                self.notice("History is already up to date.");
            } else if had_will {
                self.notice("Compaction skipped — nothing left to summarize.");
            }
            return Ok(result);
        }

        let after = self.estimate_context_usage().await.ok().map(|(t, _)| t);
        let before_s = before
            .or(Some(result.tokens_before))
            .map(|t| format!("~{t}"))
            .unwrap_or_else(|| "?".into());
        let after_s = after.map(|t| format!("~{t}")).unwrap_or_else(|| "?".into());
        self.notice(format!("Compaction complete: {before_s} → {after_s} tokens using {model_ref}."));
        Ok(result)
    }

    /// Auto-compact when usage exceeds threshold. Returns `true` when a compaction ran.
    ///
    /// Always considered; threshold comes from settings. There is no host kill-switch.
    pub(crate) async fn maybe_auto_compact(&self, prompt_text: Option<&str>) -> bool {
        let settings = self.harness.compaction_settings();
        let Ok((used, window)) = self.estimate_context_usage().await else {
            return false;
        };
        let prompt_tokens = prompt_text.map(count_tokens_text).unwrap_or(0);
        let total = used.saturating_add(prompt_tokens);
        if window == 0 || !should_compact(total, window, settings) {
            return false;
        }
        let will = auto_compact_will_message(used, window, prompt_tokens, settings);
        match self
            .run_compact_with_notices(CompactSource::Automatic, None, Some(will), None)
            .await
        {
            Ok(result) => !result.is_noop(),
            Err(err) => {
                log::warn!("auto-compact failed: {err}");
                self.notice(format!("Compaction failed: {err}"));
                false
            }
        }
    }

    /// Recover from a turn that ended in a provider error: compact when the failure is a
    /// context-limit error (or usage is already over threshold) so a retry has room.
    ///
    /// Returns `true` when a compaction actually ran — the caller may then auto-resume
    /// the interrupted prompt exactly once.
    pub(crate) async fn recover_from_turn_error(&self, error_text: &str) -> bool {
        let settings = self.harness.compaction_settings();
        if !settings.enabled {
            return false;
        }
        let overflow_likely = looks_like_context_overflow(error_text);
        let over_threshold = self
            .estimate_context_usage()
            .await
            .map(|(used, window)| window > 0 && should_compact(used, window, settings))
            .unwrap_or(false);
        if !overflow_likely && !over_threshold {
            return false;
        }
        let will = "Context may exceed the model limit — compacting history before retrying…".to_string();
        match self
            .run_compact_with_notices(CompactSource::Automatic, None, Some(will), None)
            .await
        {
            Ok(result) => !result.is_noop(),
            Err(err) => {
                log::warn!("auto-compact after turn error failed: {err}");
                self.notice(format!("Compaction failed: {err}"));
                false
            }
        }
    }

    /// After compaction in `recover_errored_turn`, verify the retry has room. If the prompt
    /// itself is so large that even an empty history wouldn't fit, retrying would just fail
    /// again — surface a clear message instead.
    pub(crate) async fn retry_fits_after_compaction(&self, prompt_text: &str) -> bool {
        let Ok((used_after, window)) = self.estimate_context_usage().await else {
            return true;
        };
        let prompt_tokens = count_tokens_text(prompt_text);
        window == 0 || used_after.saturating_add(prompt_tokens) <= window
    }

    /// After switching to a smaller context window, compact until history fits (or max 2 passes).
    pub(crate) async fn ensure_context_fits_new_model(&self, old_window: u64, new_window: u64) -> Result<()> {
        if new_window == 0 || new_window >= old_window {
            return Ok(());
        }

        let settings = self.harness.compaction_settings();
        let reserve = settings.reserve_tokens.max(4_096);
        let hard_budget = new_window.saturating_sub(reserve.min(new_window / 4).max(1));

        let Ok((used, _)) = self.estimate_context_usage().await else {
            return Ok(());
        };

        let soft_over = should_compact(used, new_window, settings);
        let hard_over = used > hard_budget;
        if !hard_over && !soft_over {
            return Ok(());
        }

        let old_k = old_window / 1000;
        let new_k = new_window / 1000;
        let will = format!(
            "Model context is smaller ({old_k}k → {new_k}k). Current history ~{used} tokens exceeds the new limit — compacting…"
        );

        for pass in 1..=2u32 {
            let Ok((used_now, _)) = self.estimate_context_usage().await else {
                break;
            };
            // Stop when under hard budget (pass 2+) or under soft threshold when not hard-over.
            if used_now <= hard_budget {
                break;
            }

            let will_msg = if pass == 1 {
                Some(will.clone())
            } else {
                Some(format!(
                    "History still large (~{used_now} tokens) after pass {} — compacting again…",
                    pass - 1
                ))
            };
            match self
                .run_compact_with_notices(CompactSource::ModelSwitch, None, will_msg, None)
                .await
            {
                Ok(r) if r.is_noop() => break,
                Ok(_) => {}
                Err(err) => {
                    self.notice(format!("Compaction failed: {err}"));
                    return Err(err);
                }
            }
        }

        if let Ok((used_final, _)) = self.estimate_context_usage().await
            && used_final > hard_budget
        {
            self.notice(format!(
                "History still exceeds the new context after compaction (~{used_final} tokens, limit ~{new_window}). Use /compact or switch to a larger model."
            ));
        }
        Ok(())
    }
}

/// Substrings (lowercased) that identify provider context-limit errors.
const CONTEXT_OVERFLOW_MARKERS: &[&str] = &[
    "context length",
    "context_length",
    "context window",
    "max context",
    "maximum context",
    "prompt is too long",
    "prompt too long",
    "input is too long",
    "token limit",
    "maximum input token",
    "too many tokens",
    "reduce the length",
    "maximum length",
    "reduce your prompt",
    "prompt length",
    "token count",
];

/// Heuristic: does the provider error text point at a context-window overflow?
pub fn looks_like_context_overflow(error_text: &str) -> bool {
    let lower = error_text.to_ascii_lowercase();
    CONTEXT_OVERFLOW_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Resolve `inherit` / empty / `provider/model_id` against the session model.
pub fn resolve_settings_model_ref(setting: &str, inherit: &Model) -> Result<Model> {
    let trimmed = setting.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        return Ok(inherit.clone());
    }
    let (provider, model_id) = trimmed
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Invalid model ref (expected provider/model_id or inherit): {trimmed}"))?;
    elph_ai::get_builtin_model(provider.trim(), model_id.trim())
        .ok_or_else(|| anyhow::anyhow!("Model not found for settings ref: {trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherit_and_empty_use_session_model() {
        let m = elph_ai::get_builtin_model("openai", "gpt-5.6-luna").expect("model");
        let a = resolve_settings_model_ref("inherit", &m).expect("ok");
        let b = resolve_settings_model_ref("  ", &m).expect("ok");
        assert_eq!(a.id, m.id);
        assert_eq!(b.id, m.id);
    }

    #[test]
    fn context_overflow_markers_match_provider_errors() {
        for text in [
            "400: maximum context length is 200000 tokens",
            "prompt is too long (202001 > 200000)",
            "messages: prompt is too long",
            "This model's maximum context length is 128000 tokens",
            "400: the input is too long",
            "Error: context_length_exceeded",
            "the request exceeds the model's context window",
            "400: maximum input token limit reached",
        ] {
            assert!(looks_like_context_overflow(text), "expected match: {text}");
        }
    }

    #[test]
    fn context_overflow_markers_ignore_unrelated_errors() {
        for text in [
            "401: invalid api key",
            "429: rate limited — too many requests",
            "upstream connection error",
            "500: internal server error",
            "Request aborted",
        ] {
            assert!(!looks_like_context_overflow(text), "expected no match: {text}");
        }
    }

    #[test]
    fn context_overflow_markers_match_additional_provider_errors() {
        for text in [
            "400: too many tokens in prompt",
            "reduce the length of your prompt",
            "maximum length exceeded",
            "prompt length is over the limit",
            "token count exceeds maximum",
        ] {
            assert!(looks_like_context_overflow(text), "expected match: {text}");
        }
    }

    #[test]
    fn auto_compact_will_message_history_only() {
        let settings = CompactionSettings {
            enabled: true,
            reserve_tokens: 16_384,
            threshold_pct: Some(80),
            keep_recent_tokens: 20_000,
            physical_prune: true,
        };
        // History over threshold → normal message.
        let msg = auto_compact_will_message(180_000, 200_000, 0, settings);
        assert!(msg.contains("180000/200000 tokens (90%) exceeds threshold"), "{msg}");
        assert!(!msg.contains("+ prompt"), "{msg}");
    }

    #[test]
    fn auto_compact_will_message_prompt_pushes_over_threshold() {
        let settings = CompactionSettings {
            enabled: true,
            reserve_tokens: 16_384,
            threshold_pct: Some(80),
            keep_recent_tokens: 20_000,
            physical_prune: true,
        };
        // History under threshold, but history + prompt over → mention prompt.
        let msg = auto_compact_will_message(150_000, 200_000, 20_000, settings);
        assert!(msg.contains("150000/200000 tokens (75%)"), "{msg}");
        assert!(msg.contains("+ prompt ~20000 tokens would exceed threshold"), "{msg}");
    }
}
