//! Memory-domain helpers (categories, retrieval SQL).

use super::types::{EmbeddingStatus, MemoryCategory};

pub fn category_str(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Correction => "correction",
        MemoryCategory::Insight => "insight",
        MemoryCategory::User => "user",
        MemoryCategory::Consolidated => "consolidated",
        MemoryCategory::Discovery => "discovery",
        MemoryCategory::Work => "work",
    }
}

pub fn category_from_str(s: &str) -> MemoryCategory {
    match s {
        "correction" => MemoryCategory::Correction,
        "insight" => MemoryCategory::Insight,
        "user" => MemoryCategory::User,
        "consolidated" => MemoryCategory::Consolidated,
        "discovery" => MemoryCategory::Discovery,
        "work" => MemoryCategory::Work,
        _ => MemoryCategory::Discovery,
    }
}

pub fn embedding_status(byte_len: Option<i64>, dimensions: u32) -> EmbeddingStatus {
    let expected_bytes = (dimensions as usize) * std::mem::size_of::<f32>();
    match byte_len {
        None | Some(0) => EmbeddingStatus::Pending,
        Some(n) if n == expected_bytes as i64 => EmbeddingStatus::Ok,
        Some(_) => EmbeddingStatus::Truncated,
    }
}

pub fn retrieval_sql(vfn: &str) -> String {
    format!(
        r#"
        SELECT
          id, content, category, weight, created_at, retrieval_count,
          vector_distance_cos({vfn}(embedding), {vfn}(?)) AS distance
        FROM memories
        WHERE embedding IS NOT NULL
        ORDER BY
          (1.0 - vector_distance_cos({vfn}(embedding), {vfn}(?)))
          * POWER(?, (CAST(? AS REAL) - COALESCE(last_retrieved, created_at)) / 86400.0)
        DESC
        LIMIT ?
        "#
    )
}
