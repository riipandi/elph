//! Lowercase hex encode/decode for small fixed buffers (salts, checksums).

const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let raw = s.as_bytes();
    let mut out = Vec::with_capacity(raw.len() / 2);
    for chunk in raw.chunks_exact(2) {
        out.push((from_digit(chunk[0])? << 4) | from_digit(chunk[1])?);
    }
    Some(out)
}

fn from_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = [0x00, 0x0f, 0xa0, 0xff];
        assert_eq!(encode(bytes), "000fa0ff");
        assert_eq!(decode("000fa0ff").as_deref(), Some(bytes.as_slice()));
        assert_eq!(decode("000FA0FF").as_deref(), Some(bytes.as_slice()));
    }

    #[test]
    fn decode_rejects_odd_and_invalid() {
        assert!(decode("abc").is_none());
        assert!(decode("zz").is_none());
    }
}
