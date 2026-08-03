//! File-only Merkle fingerprint for the code index.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Compute a stable root hash over sorted `(path, file_hash)` pairs.
pub fn merkle_root(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(hash.as_bytes());
        hasher.update(*b"\n");
    }
    hex_encode(hasher.finalize())
}

/// Fast non-cryptographic hash for file content comparison.
/// Uses xxHash3 which is ~10x faster than SHA-256.
pub fn fast_hash(bytes: &[u8]) -> String {
    let hash = xxhash_rust::xxh3::xxh3_64(bytes);
    format!("{:x}", hash)
}

/// Cryptographic hash for secure fingerprinting (used for merkle root).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(hasher.finalize())
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_order_independent_via_btree() {
        let mut a = BTreeMap::new();
        a.insert("b.rs".into(), "11".into());
        a.insert("a.rs".into(), "22".into());
        let mut b = BTreeMap::new();
        b.insert("a.rs".into(), "22".into());
        b.insert("b.rs".into(), "11".into());
        assert_eq!(merkle_root(&a), merkle_root(&b));
    }
}
