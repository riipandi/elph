//! Progress indicator widgets for iocraft (KITT scanner + braille spinner).

use std::time::Duration;

use iocraft::prelude::*;

use crate::components::theme::{UiTheme, resolve_ui_theme};
use crate::loader::{KittScanner, KittScannerConfig, LoaderCell, SpinnerLoader};

/// Props for [`KittScannerView`].
#[derive(Clone, Copy, Props)]
pub struct KittScannerViewProps {
    pub width: u16,
    pub accent: Option<Color>,
    pub active: bool,
    pub theme: Option<UiTheme>,
}

impl Default for KittScannerViewProps {
    fn default() -> Self {
        Self {
            width: 8,
            accent: None,
            active: true,
            theme: None,
        }
    }
}

/// Props for [`SpinnerLoaderView`].
#[derive(Clone, Copy, Props)]
pub struct SpinnerLoaderViewProps {
    pub color: Option<Color>,
    pub active: bool,
    pub theme: Option<UiTheme>,
}

impl Default for SpinnerLoaderViewProps {
    fn default() -> Self {
        Self {
            color: None,
            active: true,
            theme: None,
        }
    }
}

/// Renders a KITT-style scanner (`■` head + fading trail, `⬝` inactive dots).
#[component]
pub fn KittScannerView(props: &KittScannerViewProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let accent = props.accent.unwrap_or(theme.warning);
    let mut scanner = hooks.use_ref(|| {
        let config = KittScannerConfig {
            accent,
            width: if props.width > 0 {
                props.width as usize
            } else {
                KittScannerConfig::default().width
            },
            ..Default::default()
        };
        KittScanner::with_config(config)
    });
    let mut frame_tick = hooks.use_state(|| 0u32);

    let mut active = hooks.use_ref(|| props.active);
    active.set(props.active);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(40)).await;
            if !active.get() {
                continue;
            }
            scanner.write().tick();
            frame_tick.set(frame_tick.get().wrapping_add(1));
        }
    });

    if accent != scanner.read().accent() {
        scanner.write().set_accent(accent);
    }

    let _tick = frame_tick.get();
    let render_width = props.width.max(1) as usize;
    let cells: Vec<LoaderCell> = if props.active {
        scanner.read().into_cells(render_width)
    } else {
        idle_cells(render_width, accent)
    };

    let cell_elements: Vec<_> = cells
        .into_iter()
        .map(|cell| {
            element! {
                Text(
                    color: cell.color,
                    wrap: TextWrap::NoWrap,
                    content: cell.ch.to_string(),
                )
            }
        })
        .collect();

    element! {
        View(
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            flex_shrink: 0f32,
        ) {
            #(cell_elements)
        }
    }
}

/// Renders a cycling braille spinner (`⠋⠙⠹…`).
///
/// Frame is **wall-clock based** so laggy event loops skip frames instead of slowing the spin
/// (avoids “about to freeze” UX). Paint is refreshed on a fixed timer only while `active`.
#[component]
pub fn SpinnerLoaderView(props: &SpinnerLoaderViewProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    use crate::loader::SPINNER_FRAME_MS;

    let theme = resolve_ui_theme(&hooks, props.theme);
    let color = props.color.unwrap_or(theme.warning);
    // Paint token only — does not advance animation phase (phase comes from wall clock).
    let paint = hooks.use_state(|| 0u32);
    let mut active_flag = hooks.use_ref(|| false);
    active_flag.set(props.active);

    hooks.use_future({
        let active_flag = active_flag;
        let mut paint = paint;
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(SPINNER_FRAME_MS)).await;
                if active_flag.get() {
                    paint.set(paint.get().wrapping_add(1));
                }
            }
        }
    });

    // Touch paint so re-renders pick a fresh wall-clock glyph.
    let _ = paint.get();
    let glyph = if props.active { SpinnerLoader::glyph_now() } else { " " };

    element! {
        Text(color: color, wrap: TextWrap::NoWrap, content: glyph.to_string())
    }
}

fn idle_cells(width: usize, accent: Color) -> Vec<LoaderCell> {
    let (r, g, b) = rgb_components(accent);
    let faded = Color::Rgb {
        r: (r as f64 * 0.25) as u8,
        g: (g as f64 * 0.25) as u8,
        b: (b as f64 * 0.25) as u8,
    };
    (0..width)
        .map(|_| LoaderCell {
            ch: '⬝', color: faded
        })
        .collect()
}

fn rgb_components(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb { r, g, b } => (r, g, b),
        _ => (128, 128, 128),
    }
}
