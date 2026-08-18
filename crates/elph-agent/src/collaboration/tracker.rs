//! Slim plan-mode lifecycle next to [`CollaborationMode`].
//!
//! `CollaborationMode` still owns tool filtering and the session-tree event.
//! This tracker only records *when* that flip should happen: user toggle arms
//! [`PlanModeState::Pending`]; the first prompt (or a committed CLI/ACP enter)
//! moves to [`PlanModeState::Active`].

/// Plan-mode lifecycle for one harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanModeState {
    /// Not planning. Tools follow `CollaborationMode::Default`.
    #[default]
    Inactive,
    /// User toggled Plan; the model has not been told yet.
    Pending,
    /// Plan mode is in effect (`CollaborationMode::Plan`).
    Active,
}

/// Process-local plan lifecycle. Not persisted — resume seeds from `CollaborationMode`.
#[derive(Debug, Clone, Default)]
pub struct PlanModeTracker {
    state: PlanModeState,
    was_previously_active: bool,
    /// Set when the most recent activation was a same-session reentry.
    activated_as_reentry: bool,
}

impl PlanModeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed after session restore. `Plan` → Active (reentry-capable); else Inactive.
    pub fn from_collaboration_mode(mode: crate::collaboration::CollaborationMode) -> Self {
        match mode {
            crate::collaboration::CollaborationMode::Plan => Self {
                state: PlanModeState::Active,
                was_previously_active: true,
                activated_as_reentry: false,
            },
            crate::collaboration::CollaborationMode::Default => Self::default(),
        }
    }

    pub fn state(&self) -> PlanModeState {
        self.state
    }

    pub fn is_active(&self) -> bool {
        self.state == PlanModeState::Active
    }

    pub fn is_pending(&self) -> bool {
        self.state == PlanModeState::Pending
    }

    /// True while Pending after a previous Active spell in this process.
    pub fn is_reentry(&self) -> bool {
        self.was_previously_active && self.state == PlanModeState::Pending
    }

    pub fn activated_as_reentry(&self) -> bool {
        self.activated_as_reentry && self.state == PlanModeState::Active
    }

    /// `Inactive → Pending`. No-op (and false) from Pending or Active.
    pub fn enter_pending(&mut self) -> bool {
        if self.state != PlanModeState::Inactive {
            return false;
        }
        self.state = PlanModeState::Pending;
        true
    }

    /// `Pending → Active`. Returns whether the state changed.
    pub fn activate(&mut self) -> bool {
        if self.state != PlanModeState::Pending {
            return false;
        }
        self.activated_as_reentry = self.was_previously_active;
        self.state = PlanModeState::Active;
        self.was_previously_active = true;
        true
    }

    /// Jump to Active (CLI / ACP / `--mode=plan`). Returns whether this activation is a reentry.
    pub fn force_active(&mut self) -> bool {
        let reentry = self.was_previously_active && self.state != PlanModeState::Active;
        if self.state == PlanModeState::Active {
            return false;
        }
        self.activated_as_reentry = reentry;
        self.state = PlanModeState::Active;
        self.was_previously_active = true;
        reentry
    }

    /// Any state → Inactive. Keeps `was_previously_active`.
    pub fn deactivate(&mut self) {
        self.state = PlanModeState::Inactive;
        self.activated_as_reentry = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collaboration::CollaborationMode;

    #[test]
    fn user_toggle_lifecycle() {
        let mut t = PlanModeTracker::new();
        assert_eq!(t.state(), PlanModeState::Inactive);
        assert!(t.enter_pending());
        assert_eq!(t.state(), PlanModeState::Pending);
        assert!(!t.is_reentry());
        assert!(t.activate());
        assert_eq!(t.state(), PlanModeState::Active);
        t.deactivate();
        assert_eq!(t.state(), PlanModeState::Inactive);
    }

    #[test]
    fn pending_cancel_is_clean() {
        let mut t = PlanModeTracker::new();
        t.enter_pending();
        t.deactivate();
        assert_eq!(t.state(), PlanModeState::Inactive);
        t.enter_pending();
        assert!(!t.is_reentry());
    }

    #[test]
    fn reentry_after_deactivate() {
        let mut t = PlanModeTracker::new();
        t.enter_pending();
        t.activate();
        t.deactivate();
        t.enter_pending();
        assert!(t.is_reentry());
        assert!(t.activate());
        assert!(!t.is_reentry());
    }

    #[test]
    fn double_enter_pending_is_noop() {
        let mut t = PlanModeTracker::new();
        assert!(t.enter_pending());
        assert!(!t.enter_pending());
        assert_eq!(t.state(), PlanModeState::Pending);
    }

    #[test]
    fn activate_requires_pending() {
        let mut t = PlanModeTracker::new();
        assert!(!t.activate());
        t.force_active();
        assert!(!t.activate());
    }

    #[test]
    fn force_active_skips_pending() {
        let mut t = PlanModeTracker::new();
        assert!(!t.force_active());
        assert_eq!(t.state(), PlanModeState::Active);
        assert!(!t.force_active());
    }

    #[test]
    fn force_active_reports_reentry() {
        let mut t = PlanModeTracker::new();
        t.force_active();
        t.deactivate();
        assert!(t.force_active());
    }

    #[test]
    fn restore_from_collaboration_mode() {
        let active = PlanModeTracker::from_collaboration_mode(CollaborationMode::Plan);
        assert!(active.is_active());
        assert!(!active.is_reentry());
        let idle = PlanModeTracker::from_collaboration_mode(CollaborationMode::Default);
        assert_eq!(idle.state(), PlanModeState::Inactive);
    }
}
