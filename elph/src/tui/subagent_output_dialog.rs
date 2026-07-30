//! Scrollable dialog that shows live-updating subagent output.
//!
//! Displays a real-time streaming text body for a given subagent, with a
//! spinner indicator while the subagent is still running, auto-scroll so new
//! content is always visible, and a slim header with `[esc]` close button.
//!
//! The shell maintains a registry of `(Arc<RwLock<String>>, Arc<AtomicBool>)`
//! per agent_id and passes the shared arcs into the pending struct so the
//! dialog renders live content without re-opening.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use elph_tui::components::progress_indicator::SpinnerLoaderView;
use elph_tui::components::theme::UiTheme;
use elph_tui::components::{DialogChrome, DialogHeader, DialogShellOverlay};
use elph_tui::components::{dialog_body_min_height, dialog_max_content_height};
use iocraft::prelude::*;

/// Default dialog width as a percentage of the terminal width (`75%`).
pub const DEFAULT_SUBAGENT_OUTPUT_WIDTH_PCT: u8 = 75;
/// Clamp range for width percent (inclusive).
pub const MIN_SUBAGENT_OUTPUT_WIDTH_PCT: u8 = 20;
pub const MAX_SUBAGENT_OUTPUT_WIDTH_PCT: u8 = 100;
/// Floor width on wide terminals so the dialog stays readable.
const MIN_DIALOG_WIDTH: u16 = 40;
const SCREEN_WIDTH_MARGIN: u16 = 2;
const SCREEN_HEIGHT_MARGIN: u16 = 4;
const DEFAULT_SCROLL_STEP: u16 = 3;

/// A pending subagent output dialog request.
///
/// The shell stores this in a `Ref<Option<PendingSubagentOutputDialog>>` and
/// examines it each render to decide whether to show the overlay.
///
/// Unlike `PendingScrollTextDialog` (which holds a static text snapshot), this
/// struct holds `Arc<RwLock<String>>` and `Arc<AtomicBool>` that are shared
/// with the shell's subagent output registry. New `SubagentOutput` events
/// update the buffers in place, so the dialog renders live content.
pub struct PendingSubagentOutputDialog {
    /// Unique subagent identifier.
    pub agent_id: String,
    /// Dialog title (typically the subagent task name).
    pub title: String,
    /// Live-updating text content (shared `Arc<RwLock<String>>` from the output
    /// registry). The shell writes new content into this from the event loop.
    pub text: Arc<RwLock<String>>,
    /// Whether the subagent is still running (shared `Arc<AtomicBool>`).
    pub is_running: Arc<AtomicBool>,
    /// Bumped after content updates so the overlay re-renders.
    #[allow(dead_code)]
    pub scroll_tick: u32,
    /// Outer width as % of terminal width (default [`DEFAULT_SUBAGENT_OUTPUT_WIDTH_PCT`]).
    pub width_pct: u8,
}

impl PendingSubagentOutputDialog {
    /// Open a subagent output dialog with default width.
    pub fn open(
        agent_id: impl Into<String>,
        title: impl Into<String>,
        text: Arc<RwLock<String>>,
        is_running: Arc<AtomicBool>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            title: title.into(),
            text,
            is_running,
            scroll_tick: 0,
            width_pct: DEFAULT_SUBAGENT_OUTPUT_WIDTH_PCT,
        }
    }

    /// Open a subagent output dialog with a custom width percent.
    #[allow(dead_code)]
    pub fn open_with_width(
        agent_id: impl Into<String>,
        title: impl Into<String>,
        text: Arc<RwLock<String>>,
        is_running: Arc<AtomicBool>,
        width_pct: u8,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            title: title.into(),
            text,
            is_running,
            scroll_tick: 0,
            width_pct: clamp_subagent_output_width_pct(width_pct),
        }
    }
}

/// Clamp a width percent into the supported range.
pub fn clamp_subagent_output_width_pct(width_pct: u8) -> u8 {
    width_pct.clamp(MIN_SUBAGENT_OUTPUT_WIDTH_PCT, MAX_SUBAGENT_OUTPUT_WIDTH_PCT)
}

/// Outer dialog width from terminal size and a width percent.
pub fn subagent_output_dialog_width(screen_width: u16, width_pct: u8) -> u16 {
    let pct = clamp_subagent_output_width_pct(width_pct) as u32;
    let usable = screen_width.saturating_sub(SCREEN_WIDTH_MARGIN).max(1) as u32;
    let mut width = (usable * pct / 100).max(1) as u16;
    if usable as u16 >= MIN_DIALOG_WIDTH {
        width = width.max(MIN_DIALOG_WIDTH).min(usable as u16);
    } else {
        width = width.min(usable as u16);
    }
    width
}

/// Slim-header chrome and body viewport height for the subagent output dialog.
pub fn subagent_output_dialog_chrome(screen_width: u16, screen_height: u16, width_pct: u8) -> (DialogChrome, u16) {
    let outer = subagent_output_dialog_width(screen_width, width_pct);
    let chrome = DialogChrome {
        width: outer,
        slim_header: true,
        padding_horizontal: 1,
        ..DialogChrome::default()
    };
    let max_body = dialog_max_content_height(screen_height, &chrome, SCREEN_HEIGHT_MARGIN);
    let body_height = dialog_body_min_height(max_body);
    (
        DialogChrome {
            min_content_height: body_height,
            ..chrome
        },
        body_height,
    )
}

/// Props for [`SubagentOutputDialogOverlay`].
#[derive(Props)]
pub struct SubagentOutputDialogOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    /// Unique subagent identifier (used for focus tracking).
    pub agent_id: String,
    /// Dialog title shown in the slim header.
    pub title: String,
    /// Live-updating text content from an `Arc<RwLock<String>>`. The dialog reads
    /// the current value via `read_text()` on every render.
    ///
    /// The shell owns this `Arc` (created in the output registry) and writes new
    /// content into it from the agent event loop.
    pub text: Arc<RwLock<String>>,
    /// Whether the subagent is still running. When `true`, a spinner
    /// indicator is shown at the bottom of the body.
    pub is_running: Arc<AtomicBool>,
    pub body_height: u16,
    pub chrome: DialogChrome,
    pub scroll_handle: Option<Ref<ScrollViewHandle>>,
    /// Bumped by the shell after content update (kept for re-render sync).
    pub scroll_tick: u32,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
    /// Click on header `[esc]` (keyboard Esc is still handled by the shell).
    pub on_esc: HandlerMut<'static, ()>,
}

impl Default for SubagentOutputDialogOverlayProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            agent_id: String::new(),
            title: String::new(),
            text: Arc::new(RwLock::new(String::new())),
            is_running: Arc::new(AtomicBool::new(true)),
            body_height: 12,
            chrome: DialogChrome::default(),
            scroll_handle: None,
            scroll_tick: 0,
            has_focus: false,
            theme: None,
            on_esc: HandlerMut::default(),
        }
    }
}

/// Read the current text content from an `Arc<RwLock<String>>`.
///
/// Returns an empty string when the lock is poisoned.
pub fn read_text(text: &Arc<RwLock<String>>) -> String {
    text.read().map(|s| s.clone()).unwrap_or_default()
}

/// Check whether the subagent is still running.
pub fn is_running(running: &Arc<AtomicBool>) -> bool {
    running.load(Ordering::Relaxed)
}

/// Estimate whether the scrollbar is needed for the subagent output body.
pub fn subagent_output_needs_scrollbar(text: &str, body_width: u16, body_height: u16) -> bool {
    let wrap_w = body_width.saturating_sub(1).max(1) as usize;
    let mut lines = 0u16;
    for line in text.split('\n') {
        let n = line.chars().count();
        lines = lines.saturating_add(if n == 0 { 1 } else { n.div_ceil(wrap_w) as u16 });
    }
    lines = lines.max(1);
    lines > body_height && body_height > 0
}

/// Centered slim-header dialog with a scrollable, live-updating text body.
///
/// The body re-reads `props.text` (an `Arc<RwLock<String>>`) on every render so
/// new content from the subagent appears in real-time. Auto-scroll is enabled
/// so the viewport follows the latest output. A spinner indicator is rendered
/// at the bottom of the body when the subagent is still running.
#[component]
pub fn SubagentOutputDialogOverlay(
    props: &mut SubagentOutputDialogOverlayProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    // Touch scroll_tick so the shell can force re-render after content updates.
    let _ = props.scroll_tick;
    let _hooks = hooks;
    let theme = props.theme.unwrap_or_default();
    let body_width = props.chrome.inner_body_width().max(1);
    let header = DialogHeader::title(props.title.clone());

    // Read the live text content and running state from the shared buffers.
    let content = read_text(&props.text);
    let running = is_running(&props.is_running);
    let needs_scrollbar = subagent_output_needs_scrollbar(&content, body_width, props.body_height);
    let on_esc = props.on_esc.take();

    // Cohesive dialog palette: accent thumb, muted track.
    let thumb = theme.warning;
    let track = theme.text_muted;

    // Build the body element: a ScrollView with auto-scroll enabled.
    let body = element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            flex_shrink: 0f32,
        ) {
            Text(
                content: content,
                color: theme.text_primary,
                wrap: TextWrap::Wrap,
            )
            #(running.then(|| {
                let spinner_color = theme.warning;
                element! {
                    View(
                        flex_direction: FlexDirection::Row,
                        flex_shrink: 0f32,
                        margin_top: 1,
                    ) {
                        SpinnerLoaderView(color: Some(spinner_color), active: true, theme: Some(theme))
                        Text(
                            content: " Running\u{2026}",
                            color: theme.text_muted,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                }
            }))
        }
    };

    // Shell owns keyboard navigation (↑/↓/PgUp/PgDn) via the shared handle.
    // Mouse wheel is active while the dialog has focus.
    let keyboard_scroll = false;
    let mouse_scroll = props.has_focus;

    element! {
        DialogShellOverlay(
            screen_width: props.screen_width,
            screen_height: props.screen_height,
            chrome: props.chrome.clone(),
            header: header,
            theme: Some(theme),
            on_esc: on_esc,
            on_copy: Option::<HandlerMut<'static, ()>>::None,
        ) {
            View(
                width: body_width,
                height: props.body_height,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                View(width: 100pct, height: 100pct, overflow: Overflow::Hidden) {
                    ScrollView(
                        handle: props.scroll_handle,
                        auto_scroll: true,
                        keyboard_scroll: Some(keyboard_scroll),
                        mouse_scroll: Some(mouse_scroll),
                        scroll_step: Some(DEFAULT_SCROLL_STEP),
                        scrollbar: Some(needs_scrollbar),
                        scrollbar_thumb_color: Some(thumb),
                        scrollbar_track_color: Some(track),
                    ) {
                        #(body)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_width_uses_percent_of_terminal() {
        assert_eq!(subagent_output_dialog_width(100, 75), 73);
        assert_eq!(subagent_output_dialog_width(100, 100), 98);
        assert_eq!(subagent_output_dialog_width(100, 50), 49);
        assert_eq!(subagent_output_dialog_width(30, 75), 21);
    }

    #[test]
    fn chrome_uses_slim_header_and_tall_body() {
        let (chrome, body_height) = subagent_output_dialog_chrome(100, 40, 75);
        assert!(chrome.slim_header);
        assert_eq!(chrome.width, 73);
        assert!(body_height >= 16);
    }

    #[test]
    fn needs_scrollbar_when_content_overflows() {
        assert!(!subagent_output_needs_scrollbar("short", 40, 12));
        let long = "line\n".repeat(40);
        assert!(subagent_output_needs_scrollbar(&long, 40, 12));
    }

    #[test]
    fn pending_stores_agent_id_title_and_arcs() {
        let text = Arc::new(RwLock::new("output text".to_string()));
        let running = Arc::new(AtomicBool::new(true));
        let pending = PendingSubagentOutputDialog::open("agent_01", "Worker", text, running);
        assert_eq!(pending.agent_id, "agent_01");
        assert_eq!(pending.title, "Worker");
        assert_eq!(*pending.text.read().unwrap(), "output text");
        assert!(pending.is_running.load(Ordering::Relaxed));
        assert_eq!(pending.width_pct, DEFAULT_SUBAGENT_OUTPUT_WIDTH_PCT);
    }

    #[test]
    fn pending_with_custom_width() {
        let text = Arc::new(RwLock::new("body".to_string()));
        let running = Arc::new(AtomicBool::new(true));
        let pending = PendingSubagentOutputDialog::open_with_width("a1", "Test", text, running, 90);
        assert_eq!(pending.width_pct, 90);
    }

    #[test]
    fn width_pct_clamped_to_range() {
        assert_eq!(clamp_subagent_output_width_pct(10), MIN_SUBAGENT_OUTPUT_WIDTH_PCT);
        assert_eq!(clamp_subagent_output_width_pct(110), MAX_SUBAGENT_OUTPUT_WIDTH_PCT);
        assert_eq!(clamp_subagent_output_width_pct(75), 75);
    }

    #[test]
    fn read_text_returns_current_value() {
        let text = Arc::new(RwLock::new("hello world".to_string()));
        assert_eq!(read_text(&text), "hello world");
        *text.write().unwrap() = "updated".to_string();
        assert_eq!(read_text(&text), "updated");
    }

    #[test]
    fn is_running_returns_bool() {
        let running = Arc::new(AtomicBool::new(true));
        assert!(is_running(&running));
        running.store(false, Ordering::Relaxed);
        assert!(!is_running(&running));
    }
}
