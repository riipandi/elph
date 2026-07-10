//! Agent harness compaction operations.

use tokio_util::sync::CancellationToken;

use crate::compaction::{DEFAULT_COMPACTION_SETTINGS, compact, prepare_compaction};
use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOwnEvent, AgentHarnessPhase, CompactResult,
    SessionBeforeCompactEvent,
};
use crate::session::types::{HasSessionId, SessionStorage, SessionTreeEntry};

use super::helpers::{compaction_error, module_to_compact_result, session_error};
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub async fn compact(&self, custom_instructions: Option<&str>) -> HarnessOpResult<CompactResult> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "compact() requires idle harness",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Compaction;
        let result = self.compact_inner(custom_instructions).await;
        *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        result
    }

    async fn compact_inner(&self, custom_instructions: Option<&str>) -> HarnessOpResult<CompactResult> {
        let model = self.shared.model.lock().await.clone();
        let branch_entries = self
            .shared
            .session
            .lock()
            .await
            .branch(None)
            .await
            .map_err(session_error)?;
        let preparation = prepare_compaction(&branch_entries, DEFAULT_COMPACTION_SETTINGS)
            .map_err(compaction_error)?
            .ok_or_else(|| AgentHarnessError::new(AgentHarnessErrorCode::Compaction, "Nothing to compact"))?;

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
        let compact_result = if let Some(result) = from_hook.clone() {
            result
        } else {
            let thinking = self.shared.thinking_level.lock().await.to_stream_reasoning();
            let module_result = compact(
                preparation,
                &self.shared.models,
                &model,
                effective_custom_instructions.as_deref(),
                None,
                thinking,
            )
            .await
            .map_err(compaction_error)?;
            module_to_compact_result(module_result)
        };

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

        if let Some(entry) = self.shared.session.lock().await.entry(&entry_id).await
            && matches!(entry, SessionTreeEntry::Compaction { .. })
        {
            self.emit_own(AgentHarnessOwnEvent::SessionCompact(
                crate::harness::types::SessionCompactEvent {
                    compaction_entry: entry,
                    from_hook: from_hook.is_some(),
                },
            ))
            .await?;
        }

        Ok(compact_result)
    }
}
