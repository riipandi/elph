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

/// Static glyph when animation is off (or for non-running states).
pub fn process_status_glyph(status: ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Queued => "○",
        ProcessStatus::Running => "◌",
        ProcessStatus::Done => "●",
        ProcessStatus::Failed => "✕",
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
#[component]
pub fn ProcessStatusIndicator(props: &ProcessStatusIndicatorProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let color = props.color.unwrap_or(match props.status {
        ProcessStatus::Queued => theme.text_muted,
        ProcessStatus::Running => theme.warning,
        ProcessStatus::Done => theme.success,
        ProcessStatus::Failed => theme.error,
    });

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
    pub label: String,
    /// Elapsed seconds shown dimmed after the label when set (e.g. `· 1.2s`).
    pub duration_secs: Option<f64>,
    pub queued_color: Option<Color>,
    pub running_color: Option<Color>,
    pub done_color: Option<Color>,
    pub failed_color: Option<Color>,
    pub duration_color: Option<Color>,
    pub theme: Option<UiTheme>,
    /// When true, running rows use bold label text (default: true).
    pub emphasize_running: bool,
    /// When false, running rows use the static `◌` glyph instead of a braille spinner.
    pub animate_running: bool,
}

impl Default for ProcessStatusRowProps {
    fn default() -> Self {
        Self {
            status: ProcessStatus::Queued,
            label: String::new(),
            queued_color: None,
            running_color: None,
            done_color: None,
            failed_color: None,
            theme: None,
            duration_secs: None,
            duration_color: None,
            emphasize_running: true,
            animate_running: true,
        }
    }
}

fn format_row_duration_secs(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 60.0 {
        let rounded_tenth = (secs * 10.0).round() / 10.0;
        let whole = rounded_tenth.floor();
        if (rounded_tenth - whole).abs() < 0.05 {
            return format!(" · {whole}s");
        }
        return format!(" · {rounded_tenth:.1}s");
    }
    let total = secs.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    let body = if hours > 0 {
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
    };
    format!(" · {body}")
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
    let running = props.status == ProcessStatus::Running;
    let weight = if running && props.emphasize_running {
        Weight::Bold
    } else {
        Weight::Normal
    };
    let duration_suffix = props.duration_secs.map(format_row_duration_secs);

    element! {
        View(flex_direction: FlexDirection::Row, gap: theme.gap_md, align_items: AlignItems::Center) {
            ProcessStatusIndicator(
                status: props.status,
                color: Some(color),
                theme: Some(theme),
                animate_running: props.animate_running,
            )
            Text(content: props.label.clone(), color: color, weight: weight, wrap: TextWrap::NoWrap)
            #(duration_suffix.map(|suffix| element! {
                Text(content: suffix, color: duration_color, wrap: TextWrap::NoWrap)
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
    fn glyphs_match_lifecycle() {
        assert_eq!(process_status_glyph(ProcessStatus::Queued), "○");
        assert_eq!(process_status_glyph(ProcessStatus::Running), "◌");
        assert_eq!(process_status_glyph(ProcessStatus::Done), "●");
        assert_eq!(process_status_glyph(ProcessStatus::Failed), "✕");
    }

    #[test]
    fn dialog_progress_maps_to_process_status() {
        assert_eq!(ProcessStatus::from(DialogTodoProgress::Running), ProcessStatus::Running);
        assert_eq!(ProcessStatus::from(DialogTodoProgress::Queued), ProcessStatus::Queued);
    }
}
