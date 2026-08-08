//! Pending session write flushing.

use crate::agent::harness::types::PendingSessionWrite;

use super::super::helpers::session_error;
use super::super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(in crate::agent::harness) async fn flush_pending_session_writes(&self) -> HarnessOpResult<()> {
        loop {
            let front = self.shared.pending_session_writes.lock().await.first().cloned();
            let Some((write_id, write)) = front else { break };
            match write {
                PendingSessionWrite::Message { message } => {
                    let prompt_meta = self.shared.pending_prompt_meta.lock().await.take();
                    if let Some((kind, title)) = prompt_meta {
                        self.shared
                            .session
                            .lock()
                            .await
                            .append_message_with_prompt(message, title, kind)
                            .await
                            .map_err(session_error)?;
                    } else {
                        self.shared
                            .session
                            .lock()
                            .await
                            .append_message(message)
                            .await
                            .map_err(session_error)?;
                    }
                }
                PendingSessionWrite::ModelChange { provider, model_id } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_model_change(&provider, &model_id)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::ThinkingLevelChange { thinking_level } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_thinking_level_change(&thinking_level)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::ActiveToolsChange { active_tool_names } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_active_tools_change(active_tool_names)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Custom { custom_type, data } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_custom_entry(&custom_type, data)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::CustomMessage {
                    custom_type,
                    content,
                    display,
                    details,
                } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_custom_message_entry(&custom_type, content, display, details)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Label { target_id, label } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_label(&target_id, label.as_deref())
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::SessionInfo { name } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_session_name(name.unwrap_or_default())
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Leaf { target_id } => {
                    // The pending-leaf may reference an entry that was pruned or
                    // never written (partial recovery). Skip silently instead of
                    // failing the whole flush — the tree already has a coherent
                    // leaf, and re-pointing at a phantom would brick every branch.
                    if let Some(target) = target_id.as_deref()
                        && self.shared.session.lock().await.entry(target).await.is_none()
                    {
                        // fallthrough: skip
                    } else {
                        self.shared
                            .session
                            .lock()
                            .await
                            .storage_mut()
                            .set_leaf_id(target_id)
                            .await
                            .map_err(session_error)?;
                    }
                }
                PendingSessionWrite::Compaction { .. } | PendingSessionWrite::BranchSummary { .. } => {}
            }
            let _ = self.journal_pending_write_applied(write_id).await;
            self.shared.pending_session_writes.lock().await.remove(0);
        }
        Ok(())
    }
}
