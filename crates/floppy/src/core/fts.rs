//! Shared helpers for Turso-native (Tantivy-backed) FTS queries.

/// Sanitize a user query into a Tantivy query string.
///
/// Tantivy's default syntax treats space as OR and supports `AND`, `NOT`, and
/// `"phrase"` queries. To keep keyword search precise we quote every token (so
/// punctuation can't smuggle in query syntax) and join tokens with explicit
/// `AND`.
///
/// Note: Tantivy 0.26 (pinned by turso_core) does **not** support single-token
/// `term*` prefix queries — the `*` is consumed as part of the word and then
/// stripped by the tokenizer, silently degrading to an exact-term match (and a
/// quoted `"term"*` is a hard parse error). Prefix tokens are therefore emitted
/// as exact quoted terms; the trailing `*` is ignored.
pub fn sanitize_query(q: &str) -> String {
    q.split_whitespace()
        .filter(|t| !t.is_empty())
        .filter_map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{cleaned}\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}
