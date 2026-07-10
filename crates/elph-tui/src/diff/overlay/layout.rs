use crate::diff::component::LineComponent;

/// Anchor position for overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayAnchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    LeftCenter,
    RightCenter,
}

/// Margin from terminal edges.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayMargin {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

/// Width/height as absolute columns/rows or percent of terminal size.
#[derive(Debug, Clone, Copy)]
pub enum SizeValue {
    Absolute(u16),
    Percent(f32),
}

/// Overlay positioning and sizing options.
#[derive(Debug, Clone, Default)]
pub struct OverlayOptions {
    pub width: Option<SizeValue>,
    pub min_width: Option<u16>,
    pub max_height: Option<SizeValue>,
    pub anchor: OverlayAnchor,
    pub offset_x: i16,
    pub offset_y: i16,
    pub row: Option<SizeValue>,
    pub col: Option<SizeValue>,
    pub margin: Option<OverlayMargin>,
    pub non_capturing: bool,
}

/// Resolved overlay layout for compositing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayLayout {
    pub width: u16,
    pub row: u16,
    pub col: u16,
    pub max_height: Option<u16>,
}

/// Handle to a shown overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayHandle {
    pub(crate) slot: usize,
}

pub(crate) struct OverlayEntry {
    pub component: Box<dyn LineComponent>,
    pub options: OverlayOptions,
    pub pre_focus: Option<usize>,
    pub hidden: bool,
    pub alive: bool,
    pub focus_order: u64,
}

impl OverlayEntry {
    pub fn is_visible(&self, term_width: u16, term_height: u16) -> bool {
        self.alive && !self.hidden && resolve_layout(&self.options, 1, term_width, term_height).is_some()
    }
}

fn parse_size(value: SizeValue, reference: u16) -> u16 {
    match value {
        SizeValue::Absolute(v) => v,
        SizeValue::Percent(p) => {
            let pct = p.clamp(0.0, 100.0);
            ((reference as f32) * pct / 100.0).floor() as u16
        }
    }
}

/// Computes overlay layout for the given terminal size.
pub fn resolve_layout(
    options: &OverlayOptions,
    overlay_height: u16,
    term_width: u16,
    term_height: u16,
) -> Option<OverlayLayout> {
    let margin = options.margin.unwrap_or_default();
    let avail_width = term_width.saturating_sub(margin.left + margin.right).max(1);
    let avail_height = term_height.saturating_sub(margin.top + margin.bottom).max(1);

    let mut width = options
        .width
        .map(|v| parse_size(v, term_width))
        .unwrap_or(avail_width.min(80));
    if let Some(min) = options.min_width {
        width = width.max(min);
    }
    width = width.max(1).min(avail_width);

    let mut max_height = options.max_height.map(|v| parse_size(v, term_height));
    if let Some(h) = max_height {
        max_height = Some(h.max(1).min(avail_height));
    }

    let effective_height = max_height.map(|h| overlay_height.min(h)).unwrap_or(overlay_height);

    let row = if let Some(row_val) = options.row {
        margin.top.saturating_add(parse_size(row_val, avail_height))
    } else {
        resolve_anchor_row(options.anchor, effective_height, avail_height, margin.top)
    };

    let col = if let Some(col_val) = options.col {
        margin.left.saturating_add(parse_size(col_val, avail_width))
    } else {
        resolve_anchor_col(options.anchor, width, avail_width, margin.left)
    };

    let mut row = (row as i32 + options.offset_y as i32).max(margin.top as i32) as u16;
    let mut col = (col as i32 + options.offset_x as i32).max(margin.left as i32) as u16;

    row = row.min(
        term_height
            .saturating_sub(margin.bottom)
            .saturating_sub(effective_height),
    );
    col = col.min(term_width.saturating_sub(margin.right).saturating_sub(width));

    Some(OverlayLayout {
        width,
        row,
        col,
        max_height,
    })
}

fn resolve_anchor_row(anchor: OverlayAnchor, height: u16, avail: u16, margin_top: u16) -> u16 {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::TopCenter | OverlayAnchor::TopRight => margin_top,
        OverlayAnchor::BottomLeft | OverlayAnchor::BottomCenter | OverlayAnchor::BottomRight => {
            margin_top.saturating_add(avail.saturating_sub(height))
        }
        OverlayAnchor::LeftCenter | OverlayAnchor::Center | OverlayAnchor::RightCenter => {
            margin_top.saturating_add(avail.saturating_sub(height) / 2)
        }
    }
}

fn resolve_anchor_col(anchor: OverlayAnchor, width: u16, avail: u16, margin_left: u16) -> u16 {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::LeftCenter | OverlayAnchor::BottomLeft => margin_left,
        OverlayAnchor::TopRight | OverlayAnchor::RightCenter | OverlayAnchor::BottomRight => {
            margin_left.saturating_add(avail.saturating_sub(width))
        }
        OverlayAnchor::TopCenter | OverlayAnchor::Center | OverlayAnchor::BottomCenter => {
            margin_left.saturating_add(avail.saturating_sub(width) / 2)
        }
    }
}
