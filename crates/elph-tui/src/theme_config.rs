//! Configurable dark / light / auto theme resolution.
//!
//! Settings-shaped payload (host maps this from `settings.ui`):
//!
//! ```json
//! {
//!   "mode": "auto",
//!   "dark": { "textPrimary": "#d4d5d9", "accent": "rgb(102, 153, 255)" },
//!   "light": { "textPrimary": "#1a1b1e", "codeBlockBg": "#e8eaed" }
//! }
//! ```
//!
//! Color strings accept hex (`#RGB` / `#RRGGBB`), `rgb()` / `rgba()`, `r,g,b`, or
//! `{ "r", "g", "b" }` when loaded via JSON value helpers.

use serde::{Deserialize, Serialize};

use crate::color::{parse_color, parse_color_value};
use crate::components::theme::UiTheme;
use iocraft::prelude::Color;

/// Preferred appearance: fixed dark/light, or follow the terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemeMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// Display label for status notices (`Auto` / `Light` / `Dark`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Roll `Auto` → `Light` → `Dark` → `Auto` (Ctrl+Shift+T).
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Auto,
        }
    }
}

/// Resolved terminal appearance (after auto-detect when needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeAppearance {
    Dark,
    Light,
}

/// Partial token overrides for one appearance (dark or light).
///
/// All fields are optional strings (or JSON color forms when using
/// [`ThemeTokenOverrides::from_json_map`]). Unset tokens keep the base palette.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeTokenOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_primary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_muted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_soft: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_focus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_subtle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_border_dimmed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_block_bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog_selection_bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ThemeTokenOverrides {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Apply string overrides onto a base palette. Invalid colors are skipped.
    pub fn apply_to(&self, mut base: UiTheme) -> UiTheme {
        apply_opt(&mut base.text_primary, self.text_primary.as_deref());
        apply_opt(&mut base.text_secondary, self.text_secondary.as_deref());
        apply_opt(&mut base.text_muted, self.text_muted.as_deref());
        apply_opt(&mut base.text_hint, self.text_hint.as_deref());
        apply_opt(&mut base.accent, self.accent.as_deref());
        apply_opt(&mut base.accent_soft, self.accent_soft.as_deref());
        apply_opt(&mut base.border, self.border.as_deref());
        apply_opt(&mut base.border_focus, self.border_focus.as_deref());
        apply_opt(&mut base.border_subtle, self.border_subtle.as_deref());
        apply_opt(&mut base.shell_border, self.shell_border.as_deref());
        apply_opt(&mut base.shell_border_dimmed, self.shell_border_dimmed.as_deref());
        apply_opt(&mut base.surface, self.surface.as_deref());
        apply_opt(&mut base.code_block_bg, self.code_block_bg.as_deref());
        apply_opt(&mut base.selection_bg, self.selection_bg.as_deref());
        apply_opt(&mut base.dialog_selection_bg, self.dialog_selection_bg.as_deref());
        apply_opt(&mut base.success, self.success.as_deref());
        apply_opt(&mut base.warning, self.warning.as_deref());
        apply_opt(&mut base.error, self.error.as_deref());
        base
    }

    /// Merge JSON object keys (camelCase or snake_case) into overrides.
    /// Values may be strings, `[r,g,b]`, or `{r,g,b}` objects.
    pub fn merge_json_map(&mut self, map: &serde_json::Map<String, serde_json::Value>) {
        for (key, value) in map {
            let color = match value {
                serde_json::Value::String(s) => parse_color(s),
                other => parse_color_value(other),
            };
            let Some(color) = color else {
                continue;
            };
            let hex = color_to_hex_string(color);
            match normalize_token_key(key).as_str() {
                "textprimary" => self.text_primary = Some(hex),
                "textsecondary" => self.text_secondary = Some(hex),
                "textmuted" => self.text_muted = Some(hex),
                "texthint" => self.text_hint = Some(hex),
                "accent" => self.accent = Some(hex),
                "accentsoft" => self.accent_soft = Some(hex),
                "border" => self.border = Some(hex),
                "borderfocus" => self.border_focus = Some(hex),
                "bordersubtle" => self.border_subtle = Some(hex),
                "shellborder" => self.shell_border = Some(hex),
                "shellborderdimmed" => self.shell_border_dimmed = Some(hex),
                "surface" => self.surface = Some(hex),
                "codeblockbg" => self.code_block_bg = Some(hex),
                "selectionbg" => self.selection_bg = Some(hex),
                "dialogselectionbg" => self.dialog_selection_bg = Some(hex),
                "success" => self.success = Some(hex),
                "warning" => self.warning = Some(hex),
                "error" => self.error = Some(hex),
                _ => {}
            }
        }
    }
}

/// Dark + light override maps as stored in settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePalettes {
    #[serde(default)]
    pub dark: ThemeTokenOverrides,
    #[serde(default)]
    pub light: ThemeTokenOverrides,
}

/// Full theme config: mode + per-appearance token maps.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    #[serde(default)]
    pub mode: ThemeMode,
    #[serde(default)]
    pub dark: ThemeTokenOverrides,
    #[serde(default)]
    pub light: ThemeTokenOverrides,
}

impl ThemeConfig {
    pub fn from_mode_and_palettes(mode: ThemeMode, palettes: ThemePalettes) -> Self {
        Self {
            mode,
            dark: palettes.dark,
            light: palettes.light,
        }
    }

    /// Resolve the effective [`UiTheme`] (base palette + overrides).
    pub fn resolve(&self) -> UiTheme {
        let appearance = match self.mode {
            ThemeMode::Dark => ThemeAppearance::Dark,
            ThemeMode::Light => ThemeAppearance::Light,
            ThemeMode::Auto => detect_terminal_appearance(),
        };
        let base = match appearance {
            ThemeAppearance::Dark => UiTheme::dark(),
            ThemeAppearance::Light => UiTheme::light(),
        };
        let overrides = match appearance {
            ThemeAppearance::Dark => &self.dark,
            ThemeAppearance::Light => &self.light,
        };
        overrides.apply_to(base)
    }
}

/// Detect dark vs light terminal background via `COLORFGBG` (fg;bg).
///
/// Standard form is `foreground;background` (optional extra fields). A **high**
/// background index (7–15, classically white / bright) means a light terminal.
/// Missing, empty, or unparseable env → **dark** (safe for Ghostty/dark defaults;
/// avoids light-on-dark “blank” UI).
pub fn detect_terminal_appearance() -> ThemeAppearance {
    let Ok(raw) = std::env::var("COLORFGBG") else {
        return ThemeAppearance::Dark;
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return ThemeAppearance::Dark;
    }
    // Formats: "15;0", "0;15", "7;0", sometimes "fg;bg;…"
    let parts: Vec<&str> = raw.split([';', ':']).map(str::trim).filter(|s| !s.is_empty()).collect();
    // Prefer the second field as background (classic COLORFGBG).
    let bg = parts.get(1).and_then(|s| s.parse::<u8>().ok());
    match bg {
        // 7 = white, 8–15 = bright palette (often light backgrounds in practice).
        Some(bg) if bg >= 7 => ThemeAppearance::Light,
        _ => ThemeAppearance::Dark,
    }
}

// ── Process-wide active theme (so `UiTheme::default()` follows config) ─────

use std::sync::RwLock;

static ACTIVE_UI_THEME: RwLock<Option<UiTheme>> = RwLock::new(None);

/// Install the resolved theme for this process (`UiTheme::default()` will use it).
pub fn set_active_ui_theme(theme: UiTheme) {
    if let Ok(mut guard) = ACTIVE_UI_THEME.write() {
        *guard = Some(theme);
    }
}

/// Clear the process theme (tests / shutdown).
pub fn clear_active_ui_theme() {
    if let Ok(mut guard) = ACTIVE_UI_THEME.write() {
        *guard = None;
    }
}

/// Active theme if [`set_active_ui_theme`] / [`install_theme_config`] has run.
pub fn try_active_ui_theme() -> Option<UiTheme> {
    ACTIVE_UI_THEME.read().ok().and_then(|g| *g)
}

/// Active theme, or [`UiTheme::dark`] when unset.
pub fn active_ui_theme() -> UiTheme {
    try_active_ui_theme().unwrap_or_else(UiTheme::dark)
}

/// Resolve config, install as process theme, and return it.
pub fn install_theme_config(config: &ThemeConfig) -> UiTheme {
    let theme = config.resolve();
    set_active_ui_theme(theme);
    theme
}

fn apply_opt(slot: &mut Color, raw: Option<&str>) {
    if let Some(s) = raw
        && let Some(c) = parse_color(s)
    {
        *slot = c;
    }
}

fn normalize_token_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn color_to_hex_string(color: Color) -> String {
    match color {
        Color::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => "reset".into(),
        Color::White => "white".into(),
        Color::Black => "black".into(),
        Color::Red => "red".into(),
        Color::Green => "green".into(),
        Color::Blue => "blue".into(),
        Color::Yellow => "yellow".into(),
        Color::Cyan => "cyan".into(),
        Color::Magenta => "magenta".into(),
        Color::Grey => "grey".into(),
        Color::DarkGrey => "darkgrey".into(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::rgb;

    #[test]
    fn mode_parse() {
        assert_eq!(ThemeMode::parse("auto"), ThemeMode::Auto);
        assert_eq!(ThemeMode::parse("DARK"), ThemeMode::Dark);
        assert_eq!(ThemeMode::parse("light"), ThemeMode::Light);
    }

    #[test]
    fn mode_rolls_auto_light_dark() {
        assert_eq!(ThemeMode::Auto.next(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.next(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.next(), ThemeMode::Auto);
    }

    #[test]
    fn overrides_apply_hex_and_rgb() {
        let o = ThemeTokenOverrides {
            accent: Some("#ff0000".into()),
            success: Some("rgb(0, 255, 0)".into()),
            ..Default::default()
        };
        let theme = o.apply_to(UiTheme::dark());
        assert_eq!(theme.accent, rgb(255, 0, 0));
        assert_eq!(theme.success, rgb(0, 255, 0));
        // untouched
        assert_eq!(theme.error, UiTheme::dark().error);
    }

    #[test]
    fn resolve_dark_mode_uses_dark_overrides() {
        let cfg = ThemeConfig {
            mode: ThemeMode::Dark,
            dark: ThemeTokenOverrides {
                text_primary: Some("#aabbcc".into()),
                ..Default::default()
            },
            light: ThemeTokenOverrides {
                text_primary: Some("#112233".into()),
                ..Default::default()
            },
        };
        assert_eq!(cfg.resolve().text_primary, rgb(0xaa, 0xbb, 0xcc));
    }

    #[test]
    fn resolve_light_mode_uses_light_base() {
        let cfg = ThemeConfig {
            mode: ThemeMode::Light,
            ..Default::default()
        };
        let theme = cfg.resolve();
        // Light text is dark charcoal, not Ghostty dark foreground.
        assert_eq!(theme.text_primary, UiTheme::light().text_primary);
        assert_ne!(theme.text_primary, UiTheme::dark().text_primary);
    }

    #[test]
    fn merge_json_map_accepts_object_colors() {
        let mut o = ThemeTokenOverrides::default();
        let map = serde_json::json!({
            "accent": { "r": 1, "g": 2, "b": 3 },
            "borderFocus": [10, 20, 30]
        })
        .as_object()
        .cloned()
        .unwrap();
        o.merge_json_map(&map);
        let theme = o.apply_to(UiTheme::dark());
        assert_eq!(theme.accent, rgb(1, 2, 3));
        assert_eq!(theme.border_focus, rgb(10, 20, 30));
    }
}
