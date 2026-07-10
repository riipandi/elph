//! Agent harness tree navigation.

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::compaction::{GenerateBranchSummaryOptions, collect_entries_for_branch_summary, generate_branch_summary};
use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOwnEvent, AgentHarnessPhase, NavigateTreeResult,
    SessionBeforeTreeEvent,
};
use crate::session::tree::BranchSummaryOptions;
use crate::session::types::{HasSessionId, SessionStorage};

use super::helpers::{NavigateTreeOptions, branch_summary_error, editor_state_for_target, session_error};
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub async fn navigate_tree(
        &self,
        target_id: &str,
        options: Option<NavigateTreeOptions>,
    ) -> HarnessOpResult<NavigateTreeResult> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "navigate_tree() requires idle harness",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::BranchSummary;
        let result = self.navigate_tree_inner(target_id, options).await;
        *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        result
    }

    async fn navigate_tree_inner(
        &self,
        target_id: &str,
        options: Option<NavigateTreeOptions>,
    ) -> HarnessOpResult<NavigateTreeResult> {
        let options = options.unwrap_or_default();
        let old_leaf_id = self
            .shared
            .session
            .lock()
            .await
            .leaf_id()
            .await
            .map_err(session_error)?;
        if old_leaf_id.as_deref() == Some(target_id) {
            return Ok(NavigateTreeResult {
                cancelled: false,
                editor_text: None,
                summary_entry: None,
            });
        }

        let target_entry = self.shared.session.lock().await.entry(target_id).await.ok_or_else(|| {
            AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidArgument,
                format!("Entry {target_id} not found"),
            )
        })?;

        let collected = {
            let session = self.shared.session.lock().await;
            collect_entries_for_branch_summary(&session, old_leaf_id.as_deref(), target_id)
                .await
                .map_err(session_error)?
        };

        let preparation = crate::harness::types::TreePreparation {
            target_id: target_id.to_string(),
            old_leaf_id: old_leaf_id.clone(),
            common_ancestor_id: collected.common_ancestor_id.clone(),
            entries_to_summarize: collected.entries.clone(),
            user_wants_summary: options.summarize,
            custom_instructions: options.custom_instructions.clone(),
            replace_instructions: options.replace_instructions,
            label: options.label.clone(),
        };

        let hook_result = self
            .shared
            .hooks
            .emit_session_before_tree(&SessionBeforeTreeEvent {
                preparation: preparation.clone(),
                abort_token: CancellationToken::new(),
            })
            .await?;

        if hook_result.as_ref().is_some_and(|r| r.cancel) {
            return Ok(NavigateTreeResult {
                cancelled: true,
                editor_text: None,
                summary_entry: None,
            });
        }

        let mut summary_text = hook_result
            .as_ref()
            .and_then(|r| r.summary.as_ref())
            .map(|s| s.summary.clone());
        let mut summary_details = hook_result
            .as_ref()
            .and_then(|r| r.summary.as_ref())
            .and_then(|s| s.details.clone());

        if summary_text.is_none() && options.summarize && !collected.entries.is_empty() {
            let model = self.shared.model.lock().await.clone();
            let branch_summary = generate_branch_summary(
                &collected.entries,
                &self.shared.models,
                &model,
                GenerateBranchSummaryOptions {
                    custom_instructions: hook_result
                        .as_ref()
                        .and_then(|r| r.custom_instructions.clone())
                        .or(options.custom_instructions.clone()),
                    replace_instructions: hook_result
                        .as_ref()
                        .map(|r| r.replace_instructions)
                        .unwrap_or(options.replace_instructions),
                    ..Default::default()
                },
            )
            .await
            .map_err(branch_summary_error)?;

            summary_text = Some(branch_summary.summary);
            summary_details = Some(json!({
                "readFiles": branch_summary.read_files,
                "modifiedFiles": branch_summary.modified_files,
            }));
        }

        let (new_leaf_id, editor_text) = editor_state_for_target(&target_entry);

        let summary_id = self
            .shared
            .session
            .lock()
            .await
            .move_to(
                new_leaf_id.as_deref(),
                summary_text.as_ref().map(|summary| BranchSummaryOptions {
                    summary: summary.clone(),
                    details: summary_details.clone(),
                    from_hook: Some(hook_result.as_ref().and_then(|r| r.summary.as_ref()).is_some()),
                }),
            )
            .await
            .map_err(session_error)?;

        let summary_entry = if let Some(summary_id) = summary_id {
            self.shared.session.lock().await.entry(&summary_id).await
        } else {
            None
        };

        let new_leaf = self
            .shared
            .session
            .lock()
            .await
            .leaf_id()
            .await
            .map_err(session_error)?;
        self.emit_own(AgentHarnessOwnEvent::SessionTree(
            crate::harness::types::SessionTreeEvent {
                new_leaf_id: new_leaf,
                old_leaf_id,
                summary_entry: summary_entry.clone(),
                from_hook: Some(hook_result.as_ref().and_then(|r| r.summary.as_ref()).is_some()),
            },
        ))
        .await?;

        Ok(NavigateTreeResult {
            cancelled: false,
            editor_text,
            summary_entry,
        })
    }
}
