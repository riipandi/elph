//! Terminal graphics protocol detection and encoding.

/// Supported inline image protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    ITerm2,
    Unsupported,
}

/// Detects the best available inline image protocol for the current terminal.
pub fn detect_image_protocol() -> ImageProtocol {
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return ImageProtocol::Kitty;
    }
    if std::env::var("TERM")
        .ok()
        .is_some_and(|t| t.contains("kitty") || t.contains("xterm-kitty"))
    {
        return ImageProtocol::Kitty;
    }
    if std::env::var("TERM_PROGRAM")
        .ok()
        .is_some_and(|t| t.eq_ignore_ascii_case("iTerm.app"))
    {
        return ImageProtocol::ITerm2;
    }
    if std::env::var("TERM")
        .ok()
        .is_some_and(|t| t.contains("ghostty") || t.contains("wezterm"))
    {
        return ImageProtocol::Kitty;
    }
    ImageProtocol::Unsupported
}

/// Encodes raw image bytes for inline terminal display.
pub fn encode_inline_image(data: &[u8], mime: &str, width_cells: u16, height_cells: u16) -> Option<String> {
    use base64::Engine;
    let protocol = detect_image_protocol();
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    match protocol {
        ImageProtocol::Kitty => Some(format!(
            "\x1b_Ga=T,f=100,s={},v={},c={},q=2;{}\x1b\\",
            width_cells, height_cells, mime, encoded
        )),
        ImageProtocol::ITerm2 => Some(format!(
            "\x1b]1337;File=inline=1;width={}px;height={}px:{}\x07",
            width_cells * 8,
            height_cells * 16,
            encoded
        )),
        ImageProtocol::Unsupported => None,
    }
}

/// Reads PNG width/height from the IHDR chunk.
pub fn png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(data[20..24].try_into().ok()?);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_png_dimensions() {
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&(13u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&(10u32).to_be_bytes());
        png.extend_from_slice(&(20u32).to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((10, 20)));
    }
}
