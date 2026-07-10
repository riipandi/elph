//! Pending session write flushing.

use crate::harness::types::PendingSessionWrite;

use super::super::helpers::session_error;
use super::super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(super) async fn flush_pending_session_writes(&self) -> HarnessOpResult<()> {
        loop {
            let write = self.shared.pending_session_writes.lock().await.first().cloned();
            let Some(write) = write else { break };
            match write {
                PendingSessionWrite::Message { message } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_message(message)
                        .await
                        .map_err(session_error)?;
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
                    self.shared
                        .session
                        .lock()
                        .await
                        .storage_mut()
                        .set_leaf_id(target_id)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Compaction { .. } | PendingSessionWrite::BranchSummary { .. } => {}
            }
            self.shared.pending_session_writes.lock().await.remove(0);
        }
        Ok(())
    }
}
