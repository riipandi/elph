use super::types::{EmbeddingStatus, MemoryCategory};

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
    let byte_len = v.len() * std::mem::size_of::<f32>();
    let mut buf = Vec::with_capacity(byte_len);
    // SAFETY: f32 is plain-old-data; layout matches LE byte sequence the Turso driver expects.
    unsafe {
        buf.set_len(byte_len);
        std::ptr::copy_nonoverlapping(v.as_ptr().cast(), buf.as_mut_ptr(), byte_len);
    }
    buf
}

pub fn embedding_status(byte_len: Option<i64>) -> EmbeddingStatus {
    match byte_len {
        None | Some(0) => EmbeddingStatus::Pending,
        Some(n) if n == VALID_EMBEDDING_BYTES as i64 => EmbeddingStatus::Ok,
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
