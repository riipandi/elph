//! Shared helpers (no domain types).

use anyhow::Result;
use turso::Rows;

/// Drain remaining rows so Turso releases statement resources.
///
/// Partial reads without draining can leak statement handles and block subsequent
/// DDL/DML on the same connection.
pub async fn drain_rows(rows: &mut Rows) -> Result<()> {
    while rows.next().await?.is_some() {}
    Ok(())
}

/// Dimensions for the default all-MiniLM-L6-v2 model.
pub const DEFAULT_EMBEDDING_DIMS: u32 = 384;

/// Valid f32 embedding blob size for 384-dim vectors (Turso `vector32`).
pub const VALID_EMBEDDING_BYTES: usize = (DEFAULT_EMBEDDING_DIMS as usize) * 4;

/// f32 vec -> raw LE bytes for Turso vector columns.
pub fn vec_buf(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// True when every component is approximately zero (noop embedder).
pub fn is_zero(v: &[f32]) -> bool {
    v.iter().all(|x| x.abs() < f32::EPSILON)
}
