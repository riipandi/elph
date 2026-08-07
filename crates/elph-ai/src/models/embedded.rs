//! Embedded provider catalog seed.
//!
//! `build.rs` compresses every `models/*.json` into an individual zstd frame and emits the
//! index below into `OUT_DIR`. Frames are decompressed on demand — nothing is parsed (or even
//! decompressed) until a provider is actually requested, and the binary stays self-contained.

/// One compressed provider catalog embedded in the binary.
pub(crate) struct EmbeddedCatalog {
    /// Provider id in kebab-case (`amazon-bedrock`).
    pub(crate) id: &'static str,
    /// Size of the decompressed JSON body, used to size the output buffer.
    pub(crate) raw_len: usize,
    /// zstd frame produced at build time.
    pub(crate) frame: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_catalogs.rs"));

/// Provider ids shipped with the binary (kebab-case, sorted).
pub fn embedded_provider_ids() -> &'static [&'static str] {
    EMBEDDED_CATALOG_IDS
}

/// Decompress the seed catalog JSON for `provider_id`.
///
/// Returns `None` for unknown providers or a corrupted frame (logged).
pub fn embedded_provider_json(provider_id: &str) -> Option<String> {
    let catalog = EMBEDDED_CATALOGS.iter().find(|c| c.id == provider_id)?;
    match zstd::bulk::decompress(catalog.frame, catalog.raw_len) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(json) => Some(json),
            Err(err) => {
                log::error!("embedded catalog {provider_id} is not valid UTF-8: {err}");
                None
            }
        },
        Err(err) => {
            log::error!("decompress embedded catalog {provider_id}: {err}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_kebab_case_and_sorted() {
        let ids = embedded_provider_ids();
        assert!(ids.len() > 10, "expected the full builtin catalog set");
        for id in ids {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "provider id must be kebab-case: {id}"
            );
        }
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted.as_slice(), ids);
    }

    #[test]
    fn frames_decompress_to_json_objects() {
        let json = embedded_provider_json("anthropic").expect("anthropic seed");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(value.as_object().is_some_and(|m| !m.is_empty()));
    }

    #[test]
    fn unknown_provider_has_no_seed() {
        assert!(embedded_provider_json("does-not-exist").is_none());
    }
}
