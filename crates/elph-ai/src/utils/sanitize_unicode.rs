/// Remove unpaired UTF-16 surrogates (invalid in UTF-8), matching pi-ai behavior.
pub fn sanitize_surrogates(input: &str) -> String {
    let units: Vec<u16> = input.encode_utf16().collect();
    String::from_utf16(&sanitize_utf16_code_units(&units)).expect("sanitized UTF-16 must be valid Unicode")
}

/// Exposed for integration tests that construct lone surrogate code units.
#[doc(hidden)]
pub fn sanitize_utf16_code_units(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let cu = units[i];
        if (0xD800..=0xDBFF).contains(&cu) {
            if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                out.push(cu);
                out.push(units[i + 1]);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if (0xDC00..=0xDFFF).contains(&cu) {
            i += 1;
            continue;
        }
        out.push(cu);
        i += 1;
    }
    out
}
