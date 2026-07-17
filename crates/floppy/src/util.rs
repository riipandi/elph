use anyhow::Result;
use turso::Rows;

use super::types::{EmbeddingStatus, MemoryCategory};

/// Drain remaining rows so Turso releases statement resources.
///
/// Partial reads without draining can leak statement handles and block subsequent
/// DDL/DML on the same connection (see `migrations::run`).
pub async fn drain_rows(rows: &mut Rows) -> Result<()> {
    while rows.next().await?.is_some() {}
    Ok(())
}

/// Dimensions for the default all-MiniLM-L6-v2 model.
pub const DEFAULT_EMBEDDING_DIMS: u32 = 384;

/// Valid f32 embedding blob size for 384-dim vectors (Turso `vector32`).
pub const VALID_EMBEDDING_BYTES: usize = (DEFAULT_EMBEDDING_DIMS as usize) * 4;

pub fn category_str(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Correction => "correction",
        MemoryCategory::Insight => "insight",
        MemoryCategory::User => "user",
        MemoryCategory::Consolidated => "consolidated",
        MemoryCategory::Discovery => "discovery",
    }
}

pub fn category_from_str(s: &str) -> MemoryCategory {
    match s {
        "correction" => MemoryCategory::Correction,
        "insight" => MemoryCategory::Insight,
        "user" => MemoryCategory::User,
        "consolidated" => MemoryCategory::Consolidated,
        "discovery" => MemoryCategory::Discovery,
        _ => MemoryCategory::Discovery,
    }
}

/// f32 vec -> raw LE bytes. Mirrors TS vecBuf: preserve float32 binary layout for the driver.
pub fn vec_buf(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
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
