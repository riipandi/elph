//! Output sanitization for captured shell streams.

/// Remove control characters and invalid Unicode from shell output.
pub fn sanitize_binary_output(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            if code == 0x09 || code == 0x0a || code == 0x0d {
                return true;
            }
            if code <= 0x1f {
                return false;
            }
            if (0xfff9..=0xfffb).contains(&code) {
                return false;
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_nul_bytes() {
        assert_eq!(sanitize_binary_output("a\u{0}b"), "ab");
    }

    #[test]
    fn keeps_newlines_and_tabs() {
        assert_eq!(sanitize_binary_output("a\tb\nc"), "a\tb\nc");
    }
}
