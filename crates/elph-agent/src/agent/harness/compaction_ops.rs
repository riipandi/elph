//! Agent harness compaction operations.

use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::AgentHarnessError;
use crate::agent::harness::types::AgentHarnessErrorCode;
use crate::agent::harness::types::AgentHarnessOwnEvent;
use crate::agent::harness::types::AgentHarnessPhase;
use crate::agent::harness::types::CompactResult;
use crate::agent::harness::types::CompactionSettings;
use crate::agent::harness::types::SessionBeforeCompactEvent;
use crate::compaction::{compact_with_timeout, prepare_compaction};
use crate::session::types::{CompactionRetryEvent, HasSessionId, SessionStorage, SessionTreeEntry};

use super::helpers::{compaction_error, module_to_compact_result, session_error};
use super::{AgentHarness, HarnessOpResult};

/// Default retry configuration for compaction operations.
const COMPACTION_MAX_RETRIES: u32 = 3;
const COMPACTION_RETRY_BASE_DELAY_MS: u64 = 1000;

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    /// Compact with retry lifecycle support.
    ///
    /// Retries on transient failures with exponential backoff,
    /// emitting lifecycle events via the hook system.
    async fn compact_with_retry(
        &self,
        preparation: crate::compaction::CompactionPreparation,
        model: &elph_ai::Model,
        effective_custom_instructions: Option<&str>,
        from_hook: Option<CompactResult>,
        signal: Option<CancellationToken>,
    ) -> HarnessOpResult<CompactResult> {
        if let Some(result) = from_hook {
            return Ok(result);
        }

        let thinking = self.shared.thinking_level.lock().await.to_stream_reasoning();
        let timeout_ms = self.shared.stream_options.lock().await.timeout_ms;
        let max_retries = COMPACTION_MAX_RETRIES;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            // Emit retry lifecycle event
            if attempt > 1 {
                self.shared
                    .hooks
                    .emit_subscriber(
                        crate::agent::harness::hooks::AgentHarnessEvent::Own(AgentHarnessOwnEvent::CompactionRetry(
                            CompactionRetryEvent::Attempt { attempt, max_retries },
                        )),
                        None,
                    )
                    .await
                    .ok();
            }

            let module_result = match signal.as_ref() {
                Some(token) if token.is_cancelled() => {
                    Err(AgentHarnessError::new(AgentHarnessErrorCode::Compaction, "compaction aborted"))
                }
                _ => compact_with_timeout(
                    preparation.clone(),
                    &self.shared.models,
                    model,
                    effective_custom_instructions,
                    timeout_ms,
                    signal.clone(),
                    thinking,
                )
                .await
                .map_err(compaction_error),
            };

            match module_result {
                Ok(result) => {
                    if attempt > 1 {
                        self.shared
                            .hooks
                            .emit_subscriber(
                                crate::agent::harness::hooks::AgentHarnessEvent::Own(
                                    AgentHarnessOwnEvent::CompactionRetry(CompactionRetryEvent::Recovered { attempt }),
                                ),
                                None,
                            )
                            .await
                            .ok();
                    }
                    return Ok(module_to_compact_result(result));
                }
                Err(error) => {
                    let error_msg = error.to_string();
                    last_error = Some(error_msg.clone());

                    log::warn!("compaction attempt {attempt}/{max_retries} failed: {error_msg}");
                    if attempt < max_retries {
                        let delay_ms = COMPACTION_RETRY_BASE_DELAY_MS * (1u64 << (attempt - 1));
                        self.shared
                            .hooks
                            .emit_subscriber(
                                crate::agent::harness::hooks::AgentHarnessEvent::Own(
                                    AgentHarnessOwnEvent::CompactionRetry(CompactionRetryEvent::Retry {
                                        attempt,
                                        error: error_msg,
                                        delay_ms,
                                    }),
                                ),
                                None,
                            )
                            .await
                            .ok();
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        // All retries exhausted
        let error_msg = last_error.unwrap_or_else(|| "Unknown compaction error".to_string());
        log::warn!("compaction failed after {max_retries} retries: {error_msg}");
        self.shared
            .hooks
            .emit_subscriber(
                crate::agent::harness::hooks::AgentHarnessEvent::Own(AgentHarnessOwnEvent::CompactionRetry(
                    CompactionRetryEvent::Failed {
                        error: error_msg.clone(),
                    },
                )),
                None,
            )
            .await
            .ok();
        Err(AgentHarnessError::new(
            AgentHarnessErrorCode::Compaction,
            format!("Compaction failed after {max_retries} retries: {error_msg}"),
        ))
    }

    /// Compact session history.
    ///
    /// `model_override`, when set, is used only for summarization LLM calls and does **not**
    /// change the harness session model (footer / subsequent turns).
    pub async fn compact(
        &self,
        custom_instructions: Option<&str>,
        model_override: Option<&elph_ai::Model>,
    ) -> HarnessOpResult<CompactResult> {
        self.compact_with_settings(custom_instructions, model_override, self.shared.compaction_settings)
            .await
    }

    /// Compact session history with per-operation retention settings.
    ///
    /// The override affects the compaction boundary and physical pruning for this operation
    /// only; it does not mutate the harness defaults used by later turns.
    pub async fn compact_with_settings(
        &self,
        custom_instructions: Option<&str>,
        model_override: Option<&elph_ai::Model>,
        settings: CompactionSettings,
    ) -> HarnessOpResult<CompactResult> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "compact() requires idle harness",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Compaction;
        let op_id = self
            .journal_operation_started(crate::session::durability::OperationKind::Compaction)
            .await
            .unwrap_or_else(|_| crate::session::durability::new_id("op"));
        let result = self.compact_inner(custom_instructions, model_override, settings).await;
        let outcome = if result.is_ok() {
            crate::session::durability::OperationOutcome::Completed
        } else {
            crate::session::durability::OperationOutcome::Failed
        };
        let _ = self
            .journal_operation_finished(op_id, outcome, result.as_ref().err().map(|e| e.to_string()))
            .await;
        *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        result
    }

    async fn compact_inner(
        &self,
        custom_instructions: Option<&str>,
        model_override: Option<&elph_ai::Model>,
        settings: CompactionSettings,
    ) -> HarnessOpResult<CompactResult> {
        let model = match model_override {
            Some(m) => m.clone(),
            None => self.shared.model.lock().await.clone(),
        };
        let branch_entries = self
            .shared
            .session
            .lock()
            .await
            .branch(None)
            .await
            .map_err(session_error)?;
        let Some(preparation) = prepare_compaction(&branch_entries, settings).map_err(compaction_error)? else {
            return Ok(CompactResult::empty());
        };

        let hook_result = self
            .shared
            .hooks
            .emit_session_before_compact(&SessionBeforeCompactEvent {
                preparation: preparation.clone(),
                branch_entries: branch_entries.clone(),
                custom_instructions: custom_instructions.map(str::to_string),
                abort_token: CancellationToken::new(),
            })
            .await?;

        if hook_result.as_ref().is_some_and(|r| r.cancel) {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Compaction,
                "Compaction cancelled",
            ));
        }

        let from_hook = hook_result.as_ref().and_then(|r| r.compaction.clone());
        let effective_custom_instructions = hook_result
            .as_ref()
            .and_then(|r| r.custom_instructions.clone())
            .or_else(|| custom_instructions.map(str::to_string));

        // Tie the summarization LLM call to the current run's abort token when a
        // turn is active (auto-compact mid/after turn): Ctrl+C should also stop a
        // hung summarization, not just the shell. When idle (manual /compact) this
        // is `None` and compaction keeps its own lifecycle.
        let signal = if self.phase_async().await == crate::agent::harness::types::AgentHarnessPhase::Compaction {
            self.shared
                .active_run
                .lock()
                .await
                .as_ref()
                .map(|run| run.abort_token.clone())
        } else {
            None
        };

        let compact_result = self
            .compact_with_retry(
                preparation,
                &model,
                effective_custom_instructions.as_deref(),
                from_hook.clone(),
                signal,
            )
            .await?;

        // No-op result (nothing worth compacting) — skip appending a Compaction entry
        // so subsequent /compact calls can still find work to do.
        if compact_result.is_noop() {
            return Ok(compact_result);
        }

        let entry_id = self
            .shared
            .session
            .lock()
            .await
            .append_compaction(
                &compact_result.summary,
                &compact_result.first_kept_entry_id,
                compact_result.tokens_before,
                compact_result.details.clone(),
                Some(from_hook.is_some()),
            )
            .await
            .map_err(session_error)?;

        // Physical prune: drop tree rows no longer on the active post-compaction branch.
        if settings.physical_prune {
            let mut session = self.shared.session.lock().await;
            match session.branch(None).await {
                Ok(branch) => {
                    let keep_ids: Vec<String> = branch.iter().map(|e| e.id().to_string()).collect();
                    match session.storage_mut().physical_prune_except(&keep_ids).await {
                        Ok(n) if n > 0 => {
                            log::info!("compaction physical_prune removed {n} session_entries");
                        }
                        Ok(_) => {}
                        Err(err) => {
                            // Non-fatal: context is already compacted; reclaim can retry later.
                            log::warn!("compaction physical_prune failed: {err}");
                        }
                    }
                }
                Err(err) => log::warn!("compaction physical_prune: branch load failed: {err}"),
            }
        }

        if let Some(entry) = self.shared.session.lock().await.entry(&entry_id).await
            && matches!(entry, SessionTreeEntry::Compaction { .. })
        {
            self.emit_own(AgentHarnessOwnEvent::SessionCompact(
                crate::agent::harness::types::SessionCompactEvent {
                    compaction_entry: entry,
                    from_hook: from_hook.is_some(),
                },
            ))
            .await?;
        }

        Ok(compact_result)
    }
}
