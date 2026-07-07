use elph_ai::utils::sanitize_unicode::{sanitize_surrogates, sanitize_utf16_code_units};

#[test]
fn removes_unpaired_high_surrogate() {
    let units: Vec<u16> = "before"
        .encode_utf16()
        .chain(std::iter::once(0xD83D))
        .chain("after".encode_utf16())
        .collect();
    let sanitized = sanitize_utf16_code_units(&units);
    assert_eq!(String::from_utf16(&sanitized).unwrap(), "beforeafter");
}

#[test]
fn preserves_valid_emoji() {
    let text = "hello 🙈 world";
    assert_eq!(sanitize_surrogates(text), text);
}

#[test]
fn preserves_ascii_and_bmp_text() {
    let text = "Mario Zechner wann? Wo? Bin grad äußersr eventuninformiert";
    assert_eq!(sanitize_surrogates(text), text);
}
