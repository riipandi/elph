use super::Theme;

/// Resolves the active theme from `ELPH_THEME`, terminal `COLORFGBG`, or defaults to dark.
pub fn detect() -> Theme {
    if let Ok(value) = std::env::var("ELPH_THEME") {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => return Theme::light(),
            "dark" => return Theme::dark(),
            _ => {}
        }
    }

    if let Ok(fgbg) = std::env::var("COLORFGBG")
        && let Some(bg) = fgbg.split(';').nth(1).and_then(|part| part.trim().parse::<u8>().ok())
    {
        return if bg >= 8 { Theme::light() } else { Theme::dark() };
    }

    Theme::dark()
}
