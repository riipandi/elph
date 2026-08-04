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
        let mut live_paths: HashSet<String> = HashSet::new();
        let mut file_map: BTreeMap<String, String> = BTreeMap::new();

        self.report(IndexPhase::Starting, &stats, None, None, None);

        // Load existing hashes for skip
        let existing = load_file_hashes(conn).await?;
        self.report(IndexPhase::Scanning, &stats, None, None, None);

        let walk_start = Instant::now();

        // Phase 1: Walk and collect files (sequential - filesystem operations)
        let walker = WalkBuilder::new(self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let mut files_to_process: Vec<FileToProcess> = Vec::new();

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
            // Estimate: assume ~0.5 seconds per file (CPU) or ~0.1 seconds per file (GPU)
            // Use a conservative estimate for CPU
            let seconds_per_file = if self
                .gpu_acceleration
                .as_ref()
                .is_some_and(|s| s.contains("metal") || s.contains("cuda"))
            {
                0.1 // GPU is faster
            } else {
                0.5 // CPU is slower
            };
            Some((files_to_index_count as f64 * seconds_per_file).ceil() as u64)
        } else {
            None
        };

        self.report(
            IndexPhase::IndexingFile,
            &stats,
            None,
            Some(files_to_index_count),
            estimated_seconds,
        );

        // Phase 2: Parallel chunking with rayon (CPU-bound)
        let chunk_start = Instant::now();
        let root_path = self.root.to_path_buf();
        let max_chunk_lines = self.max_chunk_lines;

        let chunked_files: Vec<(String, String, String, String, Vec<RawChunk>)> = files_to_process
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
            .collect();

        stats.reindex_ms = chunk_start.elapsed().as_millis() as u64;

        // Phase 3: Flatten all chunks into one batch, embed once (or in fixed-size
        // sub-batches), then scatter results back to their owning file. This is the
        // dominant indexing speedup: dozens of batched embedder calls instead of
        // thousands of unbatched single-item calls.
        let embed_start = Instant::now();
        let embed_fn = self.embed;
        let _concurrency = self.embed_concurrency.max(1); // reserved: concurrent dispatch not yet enabled

        let mut flat_texts: Vec<String> = Vec::new();
        let mut owner: Vec<(usize, usize)> = Vec::new();
        for (file_idx, (_, _, _, _, chunks)) in chunked_files.iter().enumerate() {
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                flat_texts.push(embed_text_for_chunk(chunk));
                owner.push((file_idx, chunk_idx));
            }
        }

        let batch_size = self.embed_batch_size.max(1);
        let mut flat_embeddings: Vec<Vec<f32>> = Vec::with_capacity(flat_texts.len());
        for batch in flat_texts.chunks(batch_size) {
            let batch_vec: Vec<String> = batch.to_vec();
            let result = embed_fn(&batch_vec)
                .await
                .unwrap_or_else(|_| vec![Vec::new(); batch_vec.len()]);
            flat_embeddings.extend(result);
        }

        let mut final_processed: Vec<ProcessedFile> = chunked_files
            .into_iter()
            .map(|(rel, hash, lang, source, chunks)| {
                let embeddings = vec![Vec::new(); chunks.len()];
                ProcessedFile {
                    rel,
                    hash,
                    lang,
                    source,
                    chunks,
                    embeddings,
                }
            })
            .collect();

        for ((file_idx, chunk_idx), emb) in owner.into_iter().zip(flat_embeddings) {
            final_processed[file_idx].embeddings[chunk_idx] = emb;
        }

        stats.chunks_embedded = final_processed
            .iter()
            .flat_map(|f| f.embeddings.iter())
            .filter(|e| !is_zero(e))
            .count() as u32;
        stats.reindex_ms += embed_start.elapsed().as_millis() as u64;

        // Phase 4: Batch DB writes
        let db_start = Instant::now();

        // Delete old paths first
        for old in existing.keys() {
            if !live_paths.contains(old) {
                delete_path(conn, old).await?;
            }
        }

        // Batch insert all files inside one (or a few) transaction(s) so we pay a
        // WAL commit per batch of files rather than one commit per file.
        let txn_batch = self.db_commit_batch_files.max(1);
        for chunk in final_processed.chunks(txn_batch) {
            conn.execute("BEGIN TRANSACTION", ()).await?;
            for processed in chunk {
                self.report(
                    IndexPhase::IndexingFile,
                    &stats,
                    Some(&processed.rel),
                    Some(files_to_index_count),
                    estimated_seconds,
                );
                self.batch_insert_file(conn, processed, &mut stats)
                    .await
                    .with_context(|| format!("index {}", processed.rel))?;
                stats.files_indexed += 1;

                if stats.files_indexed <= 20 || stats.files_indexed.is_multiple_of(8) {
                    self.report(
                        IndexPhase::IndexingFile,
                        &stats,
                        Some(&processed.rel),
                        Some(files_to_index_count),
                        estimated_seconds,
                    );
                }
            }
            conn.execute("COMMIT", ()).await?;
        }

        stats.reindex_ms += db_start.elapsed().as_millis() as u64;

        self.report(
            IndexPhase::Finalizing,
            &stats,
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

        self.report(IndexPhase::Done, &stats, None, Some(files_to_index_count), estimated_seconds);
        Ok(stats)
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

        // Insert file record
        let now = now_secs();
        conn.execute(
            "INSERT INTO cg_files(path, file_hash, lang, updated_at) VALUES (?, ?, ?, ?)",
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
            for (id, path, name, kind, start, end) in nodes {
                conn.execute(
                    "INSERT OR REPLACE INTO cg_nodes(id, path, name, kind, start_line, end_line)
                     VALUES (?, ?, ?, ?, ?, ?)",
                    params![id, path, name, kind, start as i64, end as i64],
                )
                .await?;
            }
        }

        // Batch insert edges
        let src_node = file_node_id(&processed.rel);
        let targets = extract_import_targets(&processed.rel, &processed.source);
        if !targets.is_empty() {
            for target in targets {
                let dst = format!("import:{target}");
                conn.execute(
                    "INSERT OR IGNORE INTO cg_edges(src, dst, kind) VALUES (?, ?, 'imports')",
                    params![src_node.clone(), dst],
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

fn should_skip_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let skip_dirs = [
        "/.git/",
        "/target/",
        "/node_modules/",
        "/.elph/",
        "/dist/",
        "/build/",
        "/.next/",
        "/vendor/",
        "/__pycache__/",
        "/.venv/",
        "/venv/",
        "/.cargo/",
        "/.idea/",
        "/.vscode/",
        "/coverage/",
        "/.turbo/",
        "/.cache/",
        "/Pods/",
        "/.gradle/",
        "/out/",
        "/site-packages/",
        "/third_party/",
        "/third-party/",
        "/.svelte-kit/",
        "/.nuxt/",
        "/.output/",
    ];
    if skip_dirs.iter().any(|d| s.contains(d)) {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || matches!(
            name.as_str(),
            "package-lock.json"
                | "yarn.lock"
                | "pnpm-lock.yaml"
                | "cargo.lock"
                | "composer.lock"
                | "go.sum"
                | "poetry.lock"
        )
    {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "pdf"
            | "zip"
            | "gz"
            | "tar"
            | "woff"
            | "woff2"
            | "ttf"
            | "eot"
            | "mp4"
            | "mp3"
            | "wasm"
            | "so"
            | "dylib"
            | "a"
            | "o"
            | "class"
            | "jar"
            | "exe"
            | "dll"
            | "bin"
            | "lock"
            | "rlib"
            | "rmeta"
            | "pyc"
            | "pyo"
            | "db"
            | "sqlite"
            | "parquet"
    )
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
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
