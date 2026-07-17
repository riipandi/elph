//! Animated status glyphs for in-flight tasks and tool cards.

use crate::types::DialogTodoProgress;
use iocraft::prelude::*;

use super::progress_indicator::{KittScannerView, SpinnerLoaderView};
use super::theme::{UiTheme, resolve_ui_theme};

/// Lifecycle state for a process row or tool card header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessStatus {
    #[default]
    Queued,
    Running,
    Done,
    Failed,
}

impl From<DialogTodoProgress> for ProcessStatus {
    fn from(state: DialogTodoProgress) -> Self {
        match state {
            DialogTodoProgress::Queued => Self::Queued,
            DialogTodoProgress::Running => Self::Running,
            DialogTodoProgress::Done => Self::Done,
            DialogTodoProgress::Failed => Self::Failed,
        }
    }
}

// ── Process / status indicator glyphs (Unicode, typically one terminal cell) ─
//
// Keep this set geometric + paired so transcript, todo, and dialog rows match.
// Running live UI prefers braille spinner frames (`⠋⠙…`); static uses RUNNING.

/// Queued / pending — U+25CB WHITE CIRCLE.
pub const GLYPH_QUEUED: &str = "\u{25CB}"; // ○
/// Running (static) — U+25CC DOTTED CIRCLE (live rows use braille spinner).
pub const GLYPH_RUNNING: &str = "\u{25CC}"; // ◌
/// Success / done — U+2713 CHECK MARK.
pub const GLYPH_DONE: &str = "\u{2713}"; // ✓
/// Failed / error — U+2717 BALLOT X (pairs with check mark).
pub const GLYPH_FAILED: &str = "\u{2717}"; // ✗
/// Meta separator between label and detail/duration — U+00B7 MIDDLE DOT.
pub const GLYPH_META_SEP: &str = "\u{00B7}"; // ·
/// Horizontal ellipsis for truncation / path collapse — U+2026.
pub const GLYPH_ELLIPSIS: &str = "\u{2026}"; // …
/// Rightwards arrow (copy/move, flow) — U+2192.
pub const GLYPH_ARROW_RIGHT: &str = "\u{2192}"; // →

/// Static glyph when animation is off (or for non-running states).
///
/// Shapes encode lifecycle without relying on color alone (a11y):
/// - `○` queued / pending
/// - `◌` running (static fallback; live UI prefers the braille spinner)
/// - `✓` success / done
/// - `✗` failed / error
pub fn process_status_glyph(status: ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Queued => GLYPH_QUEUED,
        ProcessStatus::Running => GLYPH_RUNNING,
        ProcessStatus::Done => GLYPH_DONE,
        ProcessStatus::Failed => GLYPH_FAILED,
    }
}

/// Short plain-language status word for linear readers / screen linearization.
pub fn process_status_word(status: ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Queued => "queued",
        ProcessStatus::Running => "running",
        ProcessStatus::Done => "done",
        ProcessStatus::Failed => "failed",
    }
}

/// Resolve the foreground color for a status row.
pub fn process_status_color(status: ProcessStatus, queued: Color, running: Color, done: Color, failed: Color) -> Color {
    match status {
        ProcessStatus::Queued => queued,
        ProcessStatus::Running => running,
        ProcessStatus::Done => done,
        ProcessStatus::Failed => failed,
    }
}

fn resolve_row_color(
    status: ProcessStatus,
    theme: UiTheme,
    queued: Option<Color>,
    running: Option<Color>,
    done: Option<Color>,
    failed: Option<Color>,
) -> Color {
    process_status_color(
        status,
        queued.unwrap_or(theme.text_muted),
        running.unwrap_or(theme.warning),
        done.unwrap_or(theme.success),
        failed.unwrap_or(theme.error),
    )
}

/// Props for [`ProcessStatusIndicator`] — glyph or animated spinner only.
#[derive(Clone, Copy, Props)]
pub struct ProcessStatusIndicatorProps {
    pub status: ProcessStatus,
    pub color: Option<Color>,
    pub theme: Option<UiTheme>,
    /// When false, running rows use the static `◌` glyph instead of a braille spinner.
    pub animate_running: bool,
}

impl Default for ProcessStatusIndicatorProps {
    fn default() -> Self {
        Self {
            status: ProcessStatus::Queued,
            color: None,
            theme: None,
            animate_running: true,
        }
    }
}

/// Single-character (or braille spinner) status marker.
///
/// Running + `animate_running` uses a braille spinner; terminal-only readers still get a
/// distinct static glyph (`◌` / `✓` / `✗`) when animation is off.
#[component]
pub fn ProcessStatusIndicator(props: &ProcessStatusIndicatorProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let color = props.color.unwrap_or(match props.status {
        ProcessStatus::Queued => theme.text_muted,
        ProcessStatus::Running => theme.warning,
        ProcessStatus::Done => theme.success,
        ProcessStatus::Failed => theme.error,
    });
    let weight = match props.status {
        ProcessStatus::Running | ProcessStatus::Failed => Weight::Bold,
        ProcessStatus::Done | ProcessStatus::Queued => Weight::Normal,
    };

    let indicator: AnyElement<'static> = if props.status == ProcessStatus::Running && props.animate_running {
        element! {
            SpinnerLoaderView(color: Some(color), active: true, theme: Some(theme))
        }
        .into()
    } else {
        element! {
            Text(
                content: process_status_glyph(props.status).to_string(),
                color: color,
                weight: weight,
                wrap: TextWrap::NoWrap,
            )
        }
        .into()
    };

    element! {
        View(flex_shrink: 0f32) {
            #(indicator)
        }
    }
}

/// Props for [`ProcessStatusRow`] — indicator + label, with running emphasis.
#[derive(Clone, Props)]
pub struct ProcessStatusRowProps {
    pub status: ProcessStatus,
    /// Primary task title only (may be bold when finished).
    pub label: String,
    /// Optional secondary text (params / action / phase) — always normal weight.
    pub detail: String,
    /// Elapsed seconds shown dimmed after the label when set (e.g. `· 1.2s`).
    pub duration_secs: Option<f64>,
    pub queued_color: Option<Color>,
    pub running_color: Option<Color>,
    pub done_color: Option<Color>,
    pub failed_color: Option<Color>,
    pub duration_color: Option<Color>,
    pub detail_color: Option<Color>,
    /// When set, task title uses this ink instead of the status/row color (glyph keeps status hue).
    pub label_color: Option<Color>,
    pub theme: Option<UiTheme>,
    /// When true, running rows use bold **task** label text (default: false).
    pub emphasize_running: bool,
    /// When true, done/failed rows use bold **task** label only (default: true).
    pub emphasize_finished: bool,
    /// When false, running rows use the static `◌` glyph instead of a braille spinner.
    pub animate_running: bool,
}

impl Default for ProcessStatusRowProps {
    fn default() -> Self {
        Self {
            status: ProcessStatus::Queued,
            label: String::new(),
            detail: String::new(),
            queued_color: None,
            running_color: None,
            done_color: None,
            failed_color: None,
            theme: None,
            duration_secs: None,
            duration_color: None,
            detail_color: None,
            label_color: None,
            emphasize_running: false,
            emphasize_finished: true,
            animate_running: true,
        }
    }
}

/// Auto-scaled duration suffix (` · 45ms` / ` · 1.2s` / ` · 1m30s` / ` · 1h2m`).
fn format_row_duration_secs(secs: f64) -> String {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    let body = if secs < 1.0 {
        format!("{}ms", (secs * 1000.0).round() as u64)
    } else if secs < 10.0 {
        let rounded_tenth = (secs * 10.0).round() / 10.0;
        let whole = rounded_tenth.floor();
        if (rounded_tenth - whole).abs() < 0.05 {
            format!("{}s", whole as u64)
        } else {
            format!("{rounded_tenth:.1}s")
        }
    } else if secs < 60.0 {
        format!("{}s", secs.round() as u64)
    } else {
        let total = secs.round() as u64;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;
        if hours > 0 {
            if seconds > 0 {
                format!("{hours}h{minutes}m{seconds}s")
            } else if minutes > 0 {
                format!("{hours}h{minutes}m")
            } else {
                format!("{hours}h")
            }
        } else if seconds > 0 {
            format!("{minutes}m{seconds}s")
        } else {
            format!("{minutes}m")
        }
    };
    format!(" {GLYPH_META_SEP} {body}")
}

/// One status line: animated marker + label.
#[component]
pub fn ProcessStatusRow(props: &ProcessStatusRowProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let color = resolve_row_color(
        props.status,
        theme,
        props.queued_color,
        props.running_color,
        props.done_color,
        props.failed_color,
    );
    let duration_color = props.duration_color.unwrap_or(theme.text_muted);
    let detail_color = props.detail_color.unwrap_or(theme.text_muted);
    let task_color = props.label_color.unwrap_or(color);
    let running = props.status == ProcessStatus::Running;
    let finished = matches!(props.status, ProcessStatus::Done | ProcessStatus::Failed);
    // Bold only the task title — never params, timestamps, or other detail.
    let task_weight = if (running && props.emphasize_running) || (finished && props.emphasize_finished) {
        Weight::Bold
    } else {
        Weight::Normal
    };
    let duration_suffix = props.duration_secs.map(format_row_duration_secs);
    let detail = props.detail.trim().to_string();
    let has_detail = !detail.is_empty();

    // Single-cell gap between glyph and label (tight scan line). Use gap_md (1), not larger.
    // Detail/duration sit next to the task with the same gap so the row stays compact.
    element! {
        View(flex_direction: FlexDirection::Row, gap: theme.gap_md, align_items: AlignItems::Center) {
            ProcessStatusIndicator(
                status: props.status,
                color: Some(color),
                theme: Some(theme),
                animate_running: props.animate_running,
            )
            Text(
                content: props.label.clone(),
                color: task_color,
                weight: task_weight,
                wrap: TextWrap::NoWrap,
            )
            #(has_detail.then(|| element! {
                Text(
                    content: detail,
                    color: detail_color,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                )
            }))
            #(duration_suffix.map(|suffix| element! {
                Text(
                    content: suffix,
                    color: duration_color,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                )
            }))
        }
    }
}

/// Props for [`ProcessActivityTrail`] — KITT scanner shown while a card is running.
#[derive(Clone, Copy, Props)]
pub struct ProcessActivityTrailProps {
    pub width: u16,
    pub active: bool,
    pub accent: Option<Color>,
    pub theme: Option<UiTheme>,
}

impl Default for ProcessActivityTrailProps {
    fn default() -> Self {
        Self {
            width: 16,
            active: false,
            accent: None,
            theme: None,
        }
    }
}

/// Short scanner trail for long-running cards without output yet.
#[component]
pub fn ProcessActivityTrail(props: &ProcessActivityTrailProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    if !props.active || props.width == 0 {
        return element!(View);
    }
    element! {
        View(padding_top: theme.gap_sm) {
            KittScannerView(
                width: props.width.max(8),
                accent: props.accent,
                active: true,
                theme: Some(theme),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_match_lifecycle_unicode_set() {
        assert_eq!(process_status_glyph(ProcessStatus::Queued), GLYPH_QUEUED);
        assert_eq!(process_status_glyph(ProcessStatus::Running), GLYPH_RUNNING);
        assert_eq!(process_status_glyph(ProcessStatus::Done), GLYPH_DONE);
        assert_eq!(process_status_glyph(ProcessStatus::Failed), GLYPH_FAILED);
        // Explicit code points for a11y / font audit.
        assert_eq!(GLYPH_QUEUED, "\u{25CB}");
        assert_eq!(GLYPH_RUNNING, "\u{25CC}");
        assert_eq!(GLYPH_DONE, "\u{2713}");
        assert_eq!(GLYPH_FAILED, "\u{2717}");
        assert_eq!(GLYPH_META_SEP, "\u{00B7}");
        assert_eq!(GLYPH_ELLIPSIS, "\u{2026}");
        assert_eq!(GLYPH_ARROW_RIGHT, "\u{2192}");
    }

    #[test]
    fn status_words_are_readable_without_color() {
        assert_eq!(process_status_word(ProcessStatus::Running), "running");
        assert_eq!(process_status_word(ProcessStatus::Done), "done");
        assert_eq!(process_status_word(ProcessStatus::Failed), "failed");
    }

    #[test]
    fn dialog_progress_maps_to_process_status() {
        assert_eq!(ProcessStatus::from(DialogTodoProgress::Running), ProcessStatus::Running);
        assert_eq!(ProcessStatus::from(DialogTodoProgress::Queued), ProcessStatus::Queued);
    }

    #[test]
    fn row_duration_auto_scales_ms_and_seconds() {
        assert_eq!(format_row_duration_secs(0.045), " · 45ms");
        assert_eq!(format_row_duration_secs(1.2), " · 1.2s");
        assert_eq!(format_row_duration_secs(12.0), " · 12s");
        assert_eq!(format_row_duration_secs(90.0), " · 1m30s");
        assert!(format_row_duration_secs(1.0).contains(GLYPH_META_SEP));
    }
}
