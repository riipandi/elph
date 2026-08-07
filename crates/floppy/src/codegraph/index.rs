//! File walk + incremental reindex.

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use turso::{Connection, params};

use super::chunk::{chunk_source, embed_text_for_chunk, lang_label_for_path};
use super::graph::{extract_import_targets, file_node_id, nodes_for_chunks};
use super::merkle::{fast_hash, merkle_root, sha256_hex};
use super::types::{IndexPhase, IndexProgress, ProgressFn, RawChunk, ScanStats};
use super::walk::{looks_binary, should_skip_path};
use crate::core::embed::EmbedFn;
use crate::core::util::{drain_rows, is_zero, vec_buf};

const META_ROOT: &str = "merkle_root";
const META_LAST: &str = "last_indexed_at";
const META_DIR: &str = "root_dir";

/// File data collected during walk phase for parallel processing.
struct FileToProcess {
    rel: String,
    bytes: Vec<u8>,
    hash: String,
}

/// Result of parallel file processing.
struct ProcessedFile {
    rel: String,
    hash: String,
    lang: String,
    source: String,
    chunks: Vec<RawChunk>,
    embeddings: Vec<Vec<f32>>,
}

pub struct Indexer<'a> {
    pub root: &'a Path,
    pub embed: &'a EmbedFn,
    pub max_chunk_lines: u32,
    pub max_file_bytes: u64,
    /// Number of chunk texts sent to the embedder per batched call (Phase 1).
    pub embed_batch_size: usize,
    /// Number of files committed per DB transaction (Phase 3).
    pub db_commit_batch_files: usize,
    /// Concurrent embedding batches (advanced; default 1 = sequential, Phase 5).
    pub embed_concurrency: usize,
    pub progress: Option<&'a ProgressFn>,
    pub gpu_acceleration: Option<String>,
}

impl Indexer<'_> {
    fn report(
        &self,
        phase: IndexPhase,
        stats: &ScanStats,
        current: Option<&str>,
        files_to_index: Option<u32>,
        estimated_seconds: Option<u64>,
    ) {
        if let Some(cb) = self.progress {
            cb(IndexProgress {
                phase,
                files_walked: stats.files_walked,
                files_indexed: stats.files_indexed,
                current_path: current.map(str::to_string),
                files_to_index,
                estimated_seconds,
            });
        }
    }

    /// Full or incremental index. When `full` is true, rehash all files (still skips unchanged hashes).
    pub async fn scan(&self, conn: &Connection, full: bool) -> Result<ScanStats> {
        let _ = full;
        let mut stats = ScanStats {
            gpu_acceleration: self.gpu_acceleration.clone(),
            ..Default::default()
        };

        self.report(IndexPhase::Starting, &stats, None, None, None);

        // Load existing hashes so we can skip unchanged files and prune deleted ones.
        let existing = load_file_hashes(conn).await?;
        self.report(IndexPhase::Scanning, &stats, None, None, None);

        let (files_to_process, live_paths, files_to_index_count, estimated_seconds) =
            self.walk_phase(&mut stats, &existing);

        let chunked_files = self.chunk_phase(&mut stats, &files_to_process);
        let final_processed = self.embed_phase(&mut stats, chunked_files).await;

        self.write_phase(
            conn,
            &mut stats,
            &existing,
            &live_paths,
            &final_processed,
            files_to_index_count,
            estimated_seconds,
        )
        .await?;

        self.finalize_phase(conn, &mut stats, files_to_index_count, estimated_seconds)
            .await?;

        log::info!(
            target: "codegraph",
            "codegraph index complete: files_walked={} files_indexed={} chunks_embedded={} \
             walk_ms={} reindex_ms={} finalize_ms={} total_ms={}",
            stats.files_walked,
            stats.files_indexed,
            stats.chunks_embedded,
            stats.walk_ms,
            stats.reindex_ms,
            stats.finalize_ms,
            stats.walk_ms + stats.reindex_ms + stats.finalize_ms,
        );

        self.report(IndexPhase::Done, &stats, None, Some(files_to_index_count), estimated_seconds);
        Ok(stats)
    }

    /// Phase 1: walk the tree, read + hash files, and drop unchanged/skipped ones.
    /// Returns the files to (re)index, the set of live paths, and the UI progress
    /// estimates derived from the work count.
    fn walk_phase(
        &self,
        stats: &mut ScanStats,
        existing: &BTreeMap<String, String>,
    ) -> (Vec<FileToProcess>, HashSet<String>, u32, Option<u64>) {
        let walk_start = Instant::now();

        let walker = WalkBuilder::new(self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let mut files_to_process: Vec<FileToProcess> = Vec::new();
        let mut live_paths: HashSet<String> = HashSet::new();
        // Retained for behavior parity with the original walk (built, not read).
        let mut file_map: BTreeMap<String, String> = BTreeMap::new();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    stats.files_skipped += 1;
                    continue;
                }
            };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            stats.files_walked += 1;
            let path = entry.path();
            if should_skip_path(path) {
                stats.files_skipped += 1;
                continue;
            }
            let meta = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => {
                    stats.files_skipped += 1;
                    continue;
                }
            };
            if meta.len() > self.max_file_bytes {
                stats.files_skipped += 1;
                continue;
            }
            if meta.len() == 0 {
                stats.files_skipped += 1;
                continue;
            }

            let rel = match path.strip_prefix(self.root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => {
                    stats.files_skipped += 1;
                    continue;
                }
            };

            let bytes = match std::fs::read(path) {
                Ok(b) if !looks_binary(&b) => b,
                _ => {
                    stats.files_skipped += 1;
                    continue;
                }
            };
            stats.bytes_read += bytes.len() as u64;

            // Use fast FxHasher64 (non-crypto) instead of SHA-256 for content comparison
            let hash = fast_hash(&bytes);
            live_paths.insert(rel.clone());
            file_map.insert(rel.clone(), hash.clone());

            if existing.get(&rel) == Some(&hash) {
                stats.files_unchanged += 1;
                continue;
            }

            files_to_process.push(FileToProcess {
                rel: rel.clone(),
                bytes,
                hash,
            });
        }

        stats.walk_ms = walk_start.elapsed().as_millis() as u64;

        // Calculate estimation after walk phase
        let files_to_index_count = files_to_process.len() as u32;
        let estimated_seconds = if files_to_index_count > 0 {
            // Improved estimation: account for chunk count and actual GPU/CPU speed
            // Base: GPU ~0.08s per chunk, CPU ~0.25s per chunk (measured from typical runs)
            let chunks_estimate = files_to_index_count as f64 * 2.5; // ~2.5 chunks per file average

            let seconds_per_chunk = if self
                .gpu_acceleration
                .as_ref()
                .is_some_and(|s| s.contains("metal") || s.contains("cuda"))
            {
                0.08 // GPU: faster, Metal typically ~0.08s per chunk
            } else {
                0.25 // CPU: slower, ~0.25s per chunk
            };

            // Add walk time overhead (usually small but non-zero)
            let walk_overhead = stats.walk_ms as f64 / 1000.0 * 0.1; // 10% of walk time as overhead

            let total_estimate = (chunks_estimate * seconds_per_chunk) + walk_overhead;
            Some(total_estimate.ceil() as u64)
        } else {
            None
        };

        self.report(
            IndexPhase::IndexingFile,
            stats,
            None,
            Some(files_to_index_count),
            estimated_seconds,
        );

        (files_to_process, live_paths, files_to_index_count, estimated_seconds)
    }

    /// Phase 2: parallel AST chunking (CPU-bound, via rayon).
    fn chunk_phase(
        &self,
        stats: &mut ScanStats,
        files_to_process: &[FileToProcess],
    ) -> Vec<(String, String, String, String, Vec<RawChunk>)> {
        let chunk_start = Instant::now();
        let root_path = self.root.to_path_buf();
        let max_chunk_lines = self.max_chunk_lines;

        stats.reindex_ms = chunk_start.elapsed().as_millis() as u64;
        files_to_process
            .par_iter()
            .map(|file| {
                let rel = &file.rel;
                let bytes = &file.bytes;
                let hash = &file.hash;

                let source = String::from_utf8_lossy(bytes).into_owned();
                let chunks = chunk_source(rel, &source, max_chunk_lines);
                let lang = lang_label_for_path(root_path.join(rel).as_path()).to_string();

                (rel.clone(), hash.clone(), lang, source, chunks)
            })
            .collect()
    }

    /// Phase 3: flatten all chunks, embed them in batches, scatter results back.
    /// This is the dominant indexing speedup: dozens of batched embedder calls
    /// instead of thousands of unbatched single-item calls.
    async fn embed_phase(
        &self,
        stats: &mut ScanStats,
        chunked_files: Vec<(String, String, String, String, Vec<RawChunk>)>,
    ) -> Vec<ProcessedFile> {
        let embed_start = Instant::now();
        let embed_fn = self.embed;
        let _concurrency = self.embed_concurrency.max(1); // reserved: concurrent dispatch not yet enabled

        // Flatten -> embed in sub-batches -> scatter back. Extracted into free
        // functions so the order/index bookkeeping is unit-testable in isolation.
        let (flat_texts, owner, chunked_files) = flatten_chunk_texts(chunked_files);

        let batch_size = self.embed_batch_size.max(1);
        let mut flat_embeddings: Vec<Vec<f32>> = Vec::with_capacity(flat_texts.len());
        for batch in flat_texts.chunks(batch_size) {
            let batch_vec: Vec<String> = batch.to_vec();
            let result = embed_fn(&batch_vec)
                .await
                .unwrap_or_else(|_| vec![Vec::new(); batch_vec.len()]);
            flat_embeddings.extend(result);
        }

        let final_processed = scatter_embeddings(&owner, flat_embeddings, chunked_files);

        stats.chunks_embedded = final_processed
            .iter()
            .flat_map(|f| f.embeddings.iter())
            .filter(|e| !is_zero(e))
            .count() as u32;
        stats.reindex_ms += embed_start.elapsed().as_millis() as u64;
        final_processed
    }

    /// Phase 4: prune deleted paths, then batch-insert all files inside one (or a
    /// few) transaction(s) so we pay a WAL commit per batch of files rather than
    /// one commit per file.
    #[allow(clippy::too_many_arguments)]
    async fn write_phase(
        &self,
        conn: &Connection,
        stats: &mut ScanStats,
        existing: &BTreeMap<String, String>,
        live_paths: &HashSet<String>,
        final_processed: &[ProcessedFile],
        files_to_index_count: u32,
        estimated_seconds: Option<u64>,
    ) -> Result<()> {
        let db_start = Instant::now();
        let phase_start = Instant::now();

        // Delete old paths first
        for old in existing.keys() {
            if !live_paths.contains(old) {
                delete_path(conn, old).await?;
            }
        }

        // Batch insert all files inside one (or a few) transaction(s).
        let txn_batch = self.db_commit_batch_files.max(1);
        for chunk in final_processed.chunks(txn_batch) {
            conn.execute("BEGIN TRANSACTION", ()).await?;
            for processed in chunk {
                // Dynamic estimation update based on actual progress
                let dynamic_estimate = if stats.files_indexed > 0 && files_to_index_count > 0 {
                    let elapsed = phase_start.elapsed().as_secs_f64();
                    let files_done = stats.files_indexed as f64;
                    let files_remaining = (files_to_index_count - stats.files_indexed) as f64;
                    if files_done > 0.0 {
                        let avg_time_per_file = elapsed / files_done;
                        let remaining = avg_time_per_file * files_remaining;
                        Some(remaining.ceil() as u64)
                    } else {
                        estimated_seconds
                    }
                } else {
                    estimated_seconds
                };

                self.report(
                    IndexPhase::IndexingFile,
                    stats,
                    Some(&processed.rel),
                    Some(files_to_index_count),
                    dynamic_estimate,
                );
                self.batch_insert_file(conn, processed, stats)
                    .await
                    .with_context(|| format!("index {}", processed.rel))?;
                stats.files_indexed += 1;

                if stats.files_indexed <= 20 || stats.files_indexed.is_multiple_of(8) {
                    self.report(
                        IndexPhase::IndexingFile,
                        stats,
                        Some(&processed.rel),
                        Some(files_to_index_count),
                        dynamic_estimate,
                    );
                }
            }
            conn.execute("COMMIT", ()).await?;
        }

        stats.reindex_ms += db_start.elapsed().as_millis() as u64;
        Ok(())
    }

    /// Phase 5: rebuild the Merkle root and persist index metadata.
    async fn finalize_phase(
        &self,
        conn: &Connection,
        stats: &mut ScanStats,
        files_to_index_count: u32,
        estimated_seconds: Option<u64>,
    ) -> Result<()> {
        self.report(
            IndexPhase::Finalizing,
            stats,
            None,
            Some(files_to_index_count),
            estimated_seconds,
        );
        let finalize_start = Instant::now();

        // Rebuild file_map from DB for accurate root (includes unchanged)
        let final_map = load_file_hashes(conn).await?;
        let root = merkle_root(&final_map.into_iter().collect());
        let now = now_secs();
        upsert_meta(conn, META_ROOT, &root).await?;
        upsert_meta(conn, META_LAST, &now.to_string()).await?;
        upsert_meta(conn, META_DIR, &self.root.display().to_string()).await?;
        stats.finalize_ms = finalize_start.elapsed().as_millis() as u64;
        Ok(())
    }

    /// Batch insert a processed file to reduce DB roundtrips.
    async fn batch_insert_file(
        &self,
        conn: &Connection,
        processed: &ProcessedFile,
        stats: &mut ScanStats,
    ) -> Result<()> {
        // The surrounding call in `scan` opens the enclosing transaction; we just
        // insert this file's rows within it.

        // Delete existing data for this path first (for updates)
        let path = processed.rel.as_str();
        let file_node = file_node_id(path);

        conn.execute("DELETE FROM cg_chunks WHERE path = ?", params![path])
            .await?;
        conn.execute("DELETE FROM cg_nodes WHERE path = ?", params![path])
            .await?;
        conn.execute(
            "DELETE FROM cg_edges WHERE src = ? OR dst = ?",
            params![file_node.as_str(), file_node.as_str()],
        )
        .await?;

        // Insert file record (use INSERT OR REPLACE to handle updates)
        let now = now_secs();
        conn.execute(
            "INSERT OR REPLACE INTO cg_files(path, file_hash, lang, updated_at) VALUES (?, ?, ?, ?)",
            params![
                processed.rel.as_str(),
                processed.hash.as_str(),
                processed.lang.as_str(),
                now
            ],
        )
        .await?;

        // Batch insert nodes
        let nodes = nodes_for_chunks(&processed.chunks);
        if !nodes.is_empty() {
            for (id, node_path, name, kind, start, end) in nodes {
                conn.execute(
                    "INSERT OR REPLACE INTO cg_nodes(id, path, name, kind, start_line, end_line)
                     VALUES (?, ?, ?, ?, ?, ?)",
                    params![id, node_path, name, kind, start as i64, end as i64],
                )
                .await?;
            }
        }

        // Batch insert edges
        let targets = extract_import_targets(&processed.rel, &processed.source);
        if !targets.is_empty() {
            for target in targets {
                let dst = format!("import:{target}");
                conn.execute(
                    "INSERT OR IGNORE INTO cg_edges(src, dst, kind) VALUES (?, ?, 'imports')",
                    params![file_node.as_str(), dst],
                )
                .await?;
            }
        }

        // Batch insert chunks with embeddings
        for (i, chunk) in processed.chunks.iter().enumerate() {
            let embedding = processed
                .embeddings
                .get(i)
                .and_then(|e| if is_zero(e) { None } else { Some(e.as_slice()) });
            let emb_blob = embedding.map(vec_buf);

            conn.execute(
                "INSERT INTO cg_chunks(path, kind, name, start_line, end_line, content, file_hash, embedding)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    chunk.path.as_str(),
                    chunk.kind.as_str(),
                    chunk.name.as_deref(),
                    chunk.start_line as i64,
                    chunk.end_line as i64,
                    chunk.content.as_str(),
                    processed.hash.as_str(),
                    emb_blob.as_deref(),
                ],
            )
            .await?;
            stats.chunks_indexed += 1;
        }

        Ok(())
    }

    /// Reindex only paths whose worktree hash differs from the store.
    pub async fn reindex_dirty(&self, conn: &Connection) -> Result<ScanStats> {
        self.scan(conn, false).await
    }
}

/// Flatten every chunk across all files into a single ordered list of embed
/// inputs, recording `(file_idx, chunk_idx)` ownership so the resulting
/// embeddings can be scattered back onto the correct file/chunk. The order of
/// `flat_texts` is guaranteed to match `owner`.
#[allow(clippy::type_complexity)]
fn flatten_chunk_texts(
    chunked_files: Vec<(String, String, String, String, Vec<RawChunk>)>,
) -> (
    Vec<String>,
    Vec<(usize, usize)>,
    Vec<(String, String, String, String, Vec<RawChunk>)>,
) {
    let mut flat_texts = Vec::new();
    let mut owner = Vec::new();
    for (file_idx, (_, _, _, _, chunks)) in chunked_files.iter().enumerate() {
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            flat_texts.push(embed_text_for_chunk(chunk));
            owner.push((file_idx, chunk_idx));
        }
    }
    (flat_texts, owner, chunked_files)
}

/// Scatter a flat, order-preserving list of embeddings back onto the file/chunk
/// they came from. `owner[k]` must pair with `flat_embeddings[k]`.
fn scatter_embeddings(
    owner: &[(usize, usize)],
    flat_embeddings: Vec<Vec<f32>>,
    chunked_files: Vec<(String, String, String, String, Vec<RawChunk>)>,
) -> Vec<ProcessedFile> {
    let mut final_processed: Vec<ProcessedFile> = chunked_files
        .into_iter()
        .map(|(rel, hash, lang, source, chunks)| {
            let n = chunks.len();
            ProcessedFile {
                rel,
                hash,
                lang,
                source,
                chunks,
                embeddings: vec![Vec::new(); n],
            }
        })
        .collect();
    for ((file_idx, chunk_idx), emb) in owner.iter().zip(flat_embeddings) {
        final_processed[*file_idx].embeddings[*chunk_idx] = emb;
    }
    final_processed
}

async fn delete_path(conn: &Connection, path: &str) -> Result<()> {
    // Batch delete: use transaction for atomicity and efficiency
    conn.execute("BEGIN TRANSACTION", ()).await?;

    // Edges for file node
    let node = file_node_id(path);
    conn.execute("DELETE FROM cg_edges WHERE src = ? OR dst = ?", params![node.clone(), node])
        .await?;
    conn.execute("DELETE FROM cg_nodes WHERE path = ?", params![path])
        .await?;
    conn.execute("DELETE FROM cg_chunks WHERE path = ?", params![path])
        .await?;
    conn.execute("DELETE FROM cg_files WHERE path = ?", params![path])
        .await?;

    conn.execute("COMMIT", ()).await?;
    Ok(())
}

async fn load_file_hashes(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut rows = conn.query("SELECT path, file_hash FROM cg_files", ()).await?;
    let mut map = BTreeMap::new();
    while let Some(row) = rows.next().await? {
        map.insert(row.get::<String>(0)?, row.get::<String>(1)?);
    }
    drain_rows(&mut rows).await?;
    Ok(map)
}

async fn upsert_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO cg_meta(key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .await?;
    Ok(())
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[allow(dead_code)]
pub fn worktree_file_hash(root: &Path, rel: &str) -> Result<Option<String>> {
    let path = root.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    Ok(Some(sha256_hex(&bytes)))
}

pub async fn load_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut rows = conn
        .query("SELECT value FROM cg_meta WHERE key = ?", params![key])
        .await?;
    let v = if let Some(row) = rows.next().await? {
        Some(row.get::<String>(0)?)
    } else {
        None
    };
    drain_rows(&mut rows).await?;
    Ok(v)
}

pub async fn status_counts(conn: &Connection) -> Result<(u32, u32, u32, u32)> {
    async fn count(conn: &Connection, sql: &str) -> Result<u32> {
        let mut rows = conn.query(sql, ()).await?;
        let n = if let Some(row) = rows.next().await? {
            row.get::<i64>(0)? as u32
        } else {
            0
        };
        drain_rows(&mut rows).await?;
        Ok(n)
    }
    Ok((
        count(conn, "SELECT COUNT(*) FROM cg_files").await?,
        count(conn, "SELECT COUNT(*) FROM cg_chunks").await?,
        count(conn, "SELECT COUNT(*) FROM cg_nodes").await?,
        count(conn, "SELECT COUNT(*) FROM cg_edges").await?,
    ))
}

pub async fn purge_all(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM cg_edges", ()).await?;
    conn.execute("DELETE FROM cg_nodes", ()).await?;
    conn.execute("DELETE FROM cg_chunks", ()).await?;
    conn.execute("DELETE FROM cg_files", ()).await?;
    // Keep capability flags: Turso FTS indexes self-maintain on INSERT/DELETE,
    // so there is no external rebuild step; only drop the rest of cg_meta.
    conn.execute("DELETE FROM cg_meta WHERE key != 'fts_available'", ())
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub fn pathbuf_root(root: &str) -> PathBuf {
    PathBuf::from(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Deterministic mock embedder: each input text maps to a fixed 4-dim vector,
    /// so a test can assert exact correspondence between an input text and its
    /// embedding and thus detect off-by-one / index bugs in the scatter step.
    fn embed_of(text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; 4];
        for (i, b) in text.bytes().enumerate() {
            vec[i % 4] += (b as f32 + 1.0) * 0.01;
        }
        vec
    }

    fn mock_embedder() -> EmbedFn {
        Arc::new(|texts: &[String]| {
            let out: Vec<Vec<f32>> = texts.iter().map(|t| embed_of(t)).collect();
            Box::pin(async move { Ok(out) })
        })
    }

    #[tokio::test]
    async fn batch_embedding_scatter_preserves_order_and_index() {
        // Two files with unequal chunk counts exercise the index bookkeeping.
        let chunked_files: Vec<(String, String, String, String, Vec<RawChunk>)> = vec![
            (
                "a.rs".into(),
                "h1".into(),
                "rust".into(),
                "fn a() {}".into(),
                vec![
                    RawChunk {
                        path: "a.rs".into(),
                        kind: "fn".into(),
                        name: Some("a".into()),
                        start_line: 1,
                        end_line: 2,
                        content: "alpha".into(),
                    },
                    RawChunk {
                        path: "a.rs".into(),
                        kind: "fn".into(),
                        name: Some("b".into()),
                        start_line: 3,
                        end_line: 4,
                        content: "beta".into(),
                    },
                ],
            ),
            (
                "b.rs".into(),
                "h2".into(),
                "rust".into(),
                "fn b() {}".into(),
                vec![RawChunk {
                    path: "b.rs".into(),
                    kind: "fn".into(),
                    name: Some("c".into()),
                    start_line: 1,
                    end_line: 2,
                    content: "gamma".into(),
                }],
            ),
        ];

        let embed_fn = mock_embedder();
        let (flat_texts, owner, cf) = flatten_chunk_texts(chunked_files);

        // Order of flat_texts must match owner: a.rs[0], a.rs[1], b.rs[0].
        assert_eq!(owner, vec![(0usize, 0usize), (0, 1), (1, 0)]);
        assert_eq!(flat_texts.len(), 3);

        let flat_embeddings = embed_fn(&flat_texts).await.unwrap();
        assert_eq!(flat_embeddings.len(), 3);

        let final_processed = scatter_embeddings(&owner, flat_embeddings, cf);

        // Every (file, chunk) embedding must equal the mock output for that chunk's
        // embedding text — independent of any chunk-count balancing.
        for pf in final_processed.iter() {
            for ci in 0..pf.chunks.len() {
                let expected = embed_of(&embed_text_for_chunk(&pf.chunks[ci]));
                assert_eq!(pf.embeddings[ci], expected, "mismatch in {}", pf.rel);
            }
        }
        // Distinct chunks must not collapse onto one another (no off-by-one).
        assert_ne!(final_processed[0].embeddings[0], final_processed[0].embeddings[1]);
    }

    #[tokio::test]
    async fn batch_embedding_subbatch_preserves_order() {
        // Embedding in sub-batches must still return results in input order, which
        // is exactly what scan() relies on when scattering flat_embeddings back by
        // index. A divergence here would mean the whole scatter step is misaligned.
        let texts: Vec<String> = (0..7).map(|i| format!("chunk-{i}")).collect();
        let embed_fn = mock_embedder();
        let mut all: Vec<Vec<f32>> = Vec::new();
        for batch in texts.chunks(3) {
            let b: Vec<String> = batch.to_vec();
            all.extend(embed_fn(&b).await.unwrap());
        }
        assert_eq!(all.len(), 7);
        for (i, t) in texts.iter().enumerate() {
            assert_eq!(all[i], embed_of(t), "order broken at index {i}");
        }
    }
}
