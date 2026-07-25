//! Short-lived notices pinned above the status row (not in the scrollable transcript).
//!
//! Timed banners expire on their own wall-clock deadline — independent of agent busy/stream
//! state — so a notice can disappear while a turn is still running.

use std::time::{Duration, Instant};

use iocraft::prelude::Color;

use crate::tui::activity::format_quit_while_busy_transcript;
use crate::tui::labels::{agent_mode_busy_notice, agent_mode_change_notice};
use crate::tui::theme::{EPHEMERAL_NOTICE_FG, QUIT_BUSY_NOTICE_FG, TOOL_FAILED_FG};
use crate::types::AgentMode;

use super::types::QUIT_BUSY_NOTICE_KEY;

/// Stable key for agent-mode change banners.
pub const AGENT_MODE_NOTICE_KEY: &str = "transient:agent_mode";

/// Stable key when mode toggle is blocked because a turn is busy.
pub const AGENT_MODE_BUSY_NOTICE_KEY: &str = "transient:agent_mode_busy";

/// Stable key for theme mode change banners (Ctrl+Shift+T).
pub const THEME_MODE_NOTICE_KEY: &str = "transient:theme_mode";

/// Stable key after Ctrl+Y copies the prompt draft.
pub const PROMPT_COPY_NOTICE_KEY: &str = "transient:prompt_copy";

/// Stable key for text-select mode (Ctrl+S) mouse-capture notices.
pub const SELECT_MODE_NOTICE_KEY: &str = "transient:select_mode";

/// How long an agent-mode (or blocked-toggle) banner stays visible.
pub const AGENT_MODE_NOTICE_TTL: Duration = Duration::from_secs(3);

/// How long an API/provider error banner stays visible above the status row.
pub const API_ERROR_NOTICE_TTL: Duration = Duration::from_secs(10);

/// Stable key for API / provider error toasts.
pub const API_ERROR_NOTICE_KEY: &str = "transient:api_error";

/// Banner for HTTP/provider failures (401, 409, rate limit, …).
pub fn api_error_banner(text: impl Into<String>) -> EphemeralBanner {
    EphemeralBanner {
        key: API_ERROR_NOTICE_KEY,
        text: text.into(),
        kind: EphemeralBannerKind::Error,
        expires_at: Some(Instant::now() + API_ERROR_NOTICE_TTL),
    }
}

/// Visual weight for a pinned ephemeral banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EphemeralBannerKind {
    /// Soft amber — mode changes and similar info toasts.
    Notice,
    /// Warm orange — quit-while-busy confirmation.
    Warning,
    /// Error red — API / provider failures (401, 409, …).
    Error,
}

/// Fixed banner shown above the status row until expiry or explicit clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralBanner {
    pub key: &'static str,
    pub text: String,
    pub kind: EphemeralBannerKind,
    /// When set, auto-clear after this instant. `None` stays until replaced/cleared.
    pub expires_at: Option<Instant>,
}

impl EphemeralBanner {
    pub fn color(&self) -> Color {
        match self.kind {
            EphemeralBannerKind::Notice => EPHEMERAL_NOTICE_FG,
            EphemeralBannerKind::Warning => QUIT_BUSY_NOTICE_FG,
            EphemeralBannerKind::Error => TOOL_FAILED_FG,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|until| Instant::now() >= until)
    }

    pub fn is_key(&self, key: &str) -> bool {
        self.key == key
    }

    /// Remaining TTL for async expiry; `None` if sticky or already expired.
    pub fn remaining_ttl(&self) -> Option<Duration> {
        let until = self.expires_at?;
        let now = Instant::now();
        if until <= now {
            None
        } else {
            Some(until.saturating_duration_since(now))
        }
    }
}

/// Banner for Shift+Tab agent-mode changes (auto-expires).
pub fn agent_mode_banner(mode: AgentMode) -> EphemeralBanner {
    EphemeralBanner {
        key: AGENT_MODE_NOTICE_KEY,
        text: agent_mode_change_notice(mode),
        kind: EphemeralBannerKind::Notice,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner when the user tries to change mode during a busy turn (auto-expires).
pub fn agent_mode_busy_banner() -> EphemeralBanner {
    EphemeralBanner {
        key: AGENT_MODE_BUSY_NOTICE_KEY,
        text: agent_mode_busy_notice(),
        kind: EphemeralBannerKind::Notice,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner after Ctrl+Shift+T rolls Auto → Light → Dark.
pub fn theme_mode_banner(label: &str) -> EphemeralBanner {
    EphemeralBanner {
        key: THEME_MODE_NOTICE_KEY,
        text: format!("Theme: {label}"),
        kind: EphemeralBannerKind::Notice,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner after Ctrl+Y copies the full prompt draft to the system clipboard.
pub fn prompt_copy_banner(char_count: usize) -> EphemeralBanner {
    let text = if char_count == 0 {
        "Nothing to copy · prompt is empty".to_string()
    } else {
        format!("Copied full prompt ({char_count} chars) · Ctrl+Y")
    };
    EphemeralBanner {
        key: PROMPT_COPY_NOTICE_KEY,
        text,
        kind: EphemeralBannerKind::Notice,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner when clipboard write fails.
pub fn prompt_copy_failed_banner() -> EphemeralBanner {
    EphemeralBanner {
        key: PROMPT_COPY_NOTICE_KEY,
        text: "Could not copy prompt · check clipboard access".to_string(),
        kind: EphemeralBannerKind::Error,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner when mouse capture is turned off (native drag-select enabled).
///
/// The footer right cluster shows a sticky `sel |` badge while select mode is on;
/// this toast is transitional only.
pub fn select_mode_on_banner() -> EphemeralBanner {
    EphemeralBanner {
        key: SELECT_MODE_NOTICE_KEY,
        // Warning weight: mode disables wheel/click until the user toggles back.
        text: "Text select on · drag to select · Ctrl+S to turn off".to_string(),
        kind: EphemeralBannerKind::Warning,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Banner when mouse capture is restored (wheel/click handling on).
pub fn select_mode_off_banner() -> EphemeralBanner {
    EphemeralBanner {
        key: SELECT_MODE_NOTICE_KEY,
        text: "Text select off · scroll and click restored · Ctrl+S to enable".to_string(),
        kind: EphemeralBannerKind::Notice,
        expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
    }
}

/// Sticky quit-while-busy confirmation (cleared on y/n / Esc).
pub fn quit_busy_banner() -> EphemeralBanner {
    EphemeralBanner {
        key: QUIT_BUSY_NOTICE_KEY,
        text: format_quit_while_busy_transcript(),
        kind: EphemeralBannerKind::Warning,
        expires_at: None,
    }
}

/// Clear a banner when it matches `key` (or clear any expired banner).
pub fn clear_ephemeral_banner(banner: &mut Option<EphemeralBanner>, key: Option<&str>) -> bool {
    let should_clear = match (banner.as_ref(), key) {
        (Some(b), Some(k)) => b.is_key(k),
        (Some(b), None) => b.is_expired(),
        (None, _) => false,
    };
    if should_clear {
        *banner = None;
        true
    } else {
        false
    }
}

/// Drop expired banners; returns true when state changed.
pub fn expire_ephemeral_banner(banner: &mut Option<EphemeralBanner>) -> bool {
    clear_ephemeral_banner(banner, None)
}

/// Generation counter for async TTL clears — ignore stale clear tasks after replace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EphemeralBannerGeneration(pub u64);

impl EphemeralBannerGeneration {
    pub fn bump(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Publish a banner (timed or sticky). Returns generation id and optional async TTL.
///
/// Always bumps generation so a prior async clear cannot wipe a newer banner.
pub fn publish_ephemeral_banner(
    slot: &mut Option<EphemeralBanner>,
    generation: &mut EphemeralBannerGeneration,
    banner: EphemeralBanner,
) -> (u64, Option<Duration>) {
    let ttl = banner.remaining_ttl();
    let id = generation.bump();
    *slot = Some(banner);
    (id, ttl)
}

/// Clear only if the generation still matches (stale async tasks no-op).
pub fn clear_ephemeral_banner_if_generation(
    slot: &mut Option<EphemeralBanner>,
    generation: &EphemeralBannerGeneration,
    expected: u64,
) -> bool {
    if generation.get() != expected {
        return false;
    }
    if slot.is_some() {
        *slot = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_mode_banner_replaces_text_and_expires() {
        let first = agent_mode_banner(AgentMode::Plan);
        assert_eq!(first.text, "Agent mode: Plan.");
        assert_eq!(first.kind, EphemeralBannerKind::Notice);
        assert!(first.expires_at.is_some());
        assert!(!first.is_expired());

        let second = agent_mode_banner(AgentMode::Ask);
        assert_eq!(second.text, "Agent mode: Ask.");
        assert_eq!(second.key, AGENT_MODE_NOTICE_KEY);
    }

    #[test]
    fn agent_mode_busy_banner_is_timed_notice() {
        let banner = agent_mode_busy_banner();
        assert_eq!(banner.key, AGENT_MODE_BUSY_NOTICE_KEY);
        assert!(banner.text.contains("busy"));
        assert!(banner.remaining_ttl().is_some());
    }

    #[test]
    fn quit_busy_banner_is_sticky_warning() {
        let banner = quit_busy_banner();
        assert_eq!(banner.key, QUIT_BUSY_NOTICE_KEY);
        assert_eq!(banner.kind, EphemeralBannerKind::Warning);
        assert!(banner.expires_at.is_none());
        assert!(!banner.is_expired());
        assert_eq!(banner.color(), QUIT_BUSY_NOTICE_FG);
    }

    #[test]
    fn expire_and_clear_banner() {
        let mut slot = Some(EphemeralBanner {
            key: AGENT_MODE_NOTICE_KEY,
            text: "gone".into(),
            kind: EphemeralBannerKind::Notice,
            expires_at: Some(Instant::now() - Duration::from_millis(1)),
        });
        assert!(expire_ephemeral_banner(&mut slot));
        assert!(slot.is_none());

        slot = Some(quit_busy_banner());
        assert!(!expire_ephemeral_banner(&mut slot));
        assert!(clear_ephemeral_banner(&mut slot, Some(QUIT_BUSY_NOTICE_KEY)));
        assert!(slot.is_none());
    }

    #[test]
    fn generation_guards_async_clear() {
        let mut slot = None;
        let mut banner_gen = EphemeralBannerGeneration::default();
        let (g1, ttl) = publish_ephemeral_banner(&mut slot, &mut banner_gen, agent_mode_busy_banner());
        assert!(ttl.is_some());
        assert!(slot.is_some());

        let (g2, _) = publish_ephemeral_banner(&mut slot, &mut banner_gen, agent_mode_banner(AgentMode::Plan));
        assert_ne!(g1, g2);
        // Stale clear for g1 must not drop the newer banner.
        assert!(!clear_ephemeral_banner_if_generation(&mut slot, &banner_gen, g1));
        assert!(slot.is_some());
        assert!(clear_ephemeral_banner_if_generation(&mut slot, &banner_gen, g2));
        assert!(slot.is_none());
    }

    #[test]
    fn prompt_copy_banner_empty_and_counted() {
        let empty = prompt_copy_banner(0);
        assert_eq!(empty.key, PROMPT_COPY_NOTICE_KEY);
        assert!(empty.text.to_ascii_lowercase().contains("empty"));
        assert_eq!(empty.kind, EphemeralBannerKind::Notice);

        let ok = prompt_copy_banner(42);
        assert!(ok.text.contains("42"));
        assert!(ok.text.contains("Ctrl+Y"));
        assert!(ok.expires_at.is_some());

        let fail = prompt_copy_failed_banner();
        assert_eq!(fail.kind, EphemeralBannerKind::Error);
        assert!(fail.text.to_ascii_lowercase().contains("could not copy"));
    }

    #[test]
    fn select_mode_banners_use_text_not_color_alone() {
        let on = select_mode_on_banner();
        assert_eq!(on.key, SELECT_MODE_NOTICE_KEY);
        assert_eq!(on.kind, EphemeralBannerKind::Warning);
        assert!(on.text.to_ascii_lowercase().contains("text select on"));
        assert!(on.text.contains("Ctrl+S"));
        assert!(!on.text.contains("Esc"));

        let off = select_mode_off_banner();
        assert_eq!(off.key, SELECT_MODE_NOTICE_KEY);
        assert_eq!(off.kind, EphemeralBannerKind::Notice);
        assert!(off.text.to_ascii_lowercase().contains("text select off"));
        assert!(off.text.contains("Ctrl+S"));
    }
}
