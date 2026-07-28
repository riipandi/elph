//! Agent harness plan mode operations.

use elph_ai::Message;

use crate::agent::harness::hooks::AgentHarnessEvent;
use crate::agent::harness::types::{AgentHarnessError, AgentHarnessErrorCode, AgentHarnessPhase};
use crate::collaboration::{CollaborationMode, PlanConfirmationChoice};
use crate::collaboration::{assistant_message_text, extract_proposed_plan, filter_active_tools, implement_prompt};
use crate::session::id::create_kalid;
use crate::session::tree::BranchSummaryOptions;
use crate::session::types::{HasSessionId, SessionStorage};
use crate::types::{AgentEvent, AgentMessage};

use super::helpers::session_error;
use super::{AgentHarness, HarnessOpResult, PendingPlanConfirmation};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub async fn enter_plan_mode(&self) -> HarnessOpResult<()> {
        self.set_collaboration_mode(CollaborationMode::Plan).await
    }

    pub async fn exit_plan_mode(&self) -> HarnessOpResult<()> {
        self.set_collaboration_mode(CollaborationMode::Default).await
    }

    pub async fn set_collaboration_mode(&self, mode: CollaborationMode) -> HarnessOpResult<()> {
        let previous = *self.shared.collaboration_mode.lock().await;
        if previous == mode {
            return Ok(());
        }

        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_collaboration_mode_change(mode)
                .await
                .map_err(session_error)?;
        }

        *self.shared.collaboration_mode.lock().await = mode;
        let baseline = self.shared.baseline_active_tool_names.lock().await.clone();
        let filtered = filter_active_tools(mode, &baseline, None);
        self.set_active_tools(filtered).await?;
        Ok(())
    }

    pub async fn resolve_plan_confirmation(&self, choice: PlanConfirmationChoice) -> HarnessOpResult<()> {
        let pending = self.shared.pending_plan.lock().await.take();
        let Some(pending) = pending else {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "No plan awaiting confirmation",
            ));
        };

        match choice {
            PlanConfirmationChoice::StayInPlan => {
                // Clear pending so the agent can propose a fresh plan on the next turn.
                *self.shared.pending_plan.lock().await = None;
            }
            PlanConfirmationChoice::Implement => {
                self.set_collaboration_mode(CollaborationMode::Default).await?;
                self.prompt(implement_prompt(&pending.plan_text, pending.plan_file.as_deref()), None)
                    .await?;
            }
            PlanConfirmationChoice::ImplementFresh => {
                self.set_collaboration_mode(CollaborationMode::Default).await?;
                self.fork_fresh_plan_branch(&pending.plan_text).await?;
                self.prompt(implement_prompt(&pending.plan_text, pending.plan_file.as_deref()), None)
                    .await?;
            }
        }
        Ok(())
    }

    /// Store a plan file path on the currently pending plan so `implement_prompt`
    /// can reference it instead of embedding the full plan text.
    pub async fn set_plan_file_path(&self, path: String) -> HarnessOpResult<()> {
        if let Some(pending) = self.shared.pending_plan.lock().await.as_mut() {
            pending.plan_file = Some(path);
        }
        Ok(())
    }

    /// Clear any pending plan confirmation (used when the user chooses "Revise"
    /// so the agent can propose a revised plan).
    pub async fn clear_pending_plan(&self) -> HarnessOpResult<()> {
        *self.shared.pending_plan.lock().await = None;
        Ok(())
    }

    /// Fork a new branch with only a branch-summary entry so the next turn starts without prior messages.
    async fn fork_fresh_plan_branch(&self, plan_text: &str) -> HarnessOpResult<()> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "fork_fresh_plan_branch() requires idle harness",
            ));
        }
        let mut session = self.shared.session.lock().await;
        let current_leaf = session.leaf_id().await.map_err(session_error)?;
        session
            .move_to(
                current_leaf.as_deref(),
                Some(BranchSummaryOptions {
                    summary: format!("Fresh implementation context. Approved plan:\n\n{plan_text}"),
                    details: None,
                    from_hook: Some(false),
                }),
            )
            .await
            .map_err(session_error)?;
        Ok(())
    }

    pub(super) async fn maybe_request_plan_confirmation(&self, message: &AgentMessage) -> HarnessOpResult<()> {
        if *self.shared.collaboration_mode.lock().await != CollaborationMode::Plan {
            return Ok(());
        }
        if self.shared.pending_plan.lock().await.is_some() {
            return Ok(());
        }
        let Some(Message::Assistant(assistant)) = message.as_llm() else {
            return Ok(());
        };
        let text = assistant_message_text(&assistant.content);
        let Some(plan_text) = extract_proposed_plan(&text) else {
            return Ok(());
        };

        let plan_id = create_kalid();
        *self.shared.pending_plan.lock().await = Some(PendingPlanConfirmation {
            plan_id: plan_id.clone(),
            plan_text: plan_text.clone(),
            plan_file: None,
        });

        self.shared
            .hooks
            .emit_subscriber(
                AgentHarnessEvent::Agent(AgentEvent::PlanProposed {
                    plan_id: plan_id.clone(),
                    plan_text: plan_text.clone(),
                }),
                None,
            )
            .await?;
        self.shared
            .hooks
            .emit_subscriber(
                AgentHarnessEvent::Agent(AgentEvent::PlanConfirmationRequired { plan_id, plan_text }),
                None,
            )
            .await?;
        Ok(())
    }
}
