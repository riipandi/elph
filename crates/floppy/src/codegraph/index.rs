//! File walk + incremental reindex.

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use turso::{Connection, params};

use super::chunk::{chunk_source, embed_text_for_chunk, lang_label_for_path};
use super::graph::{extract_import_targets, file_node_id, nodes_for_chunks};
use super::merkle::{merkle_root, sha256_hex};
use super::types::{IndexPhase, IndexProgress, ProgressFn, RawChunk, ScanStats};
use crate::core::embed::EmbedFn;
use crate::core::util::{drain_rows, is_zero, vec_buf};

const META_ROOT: &str = "merkle_root";
const META_LAST: &str = "last_indexed_at";
const META_DIR: &str = "root_dir";

pub struct Indexer<'a> {
    pub root: &'a Path,
    pub embed: &'a EmbedFn,
    pub max_chunk_lines: u32,
    pub max_file_bytes: u64,
    pub progress: Option<&'a ProgressFn>,
}

impl Indexer<'_> {
    fn report(&self, phase: IndexPhase, stats: &ScanStats, current: Option<&str>) {
        if let Some(cb) = self.progress {
            cb(IndexProgress {
                phase,
                files_walked: stats.files_walked,
                files_indexed: stats.files_indexed,
                current_path: current.map(str::to_string),
            });
        }
    }

    /// Full or incremental index. When `full` is true, rehash all files (still skips unchanged hashes).
    pub async fn scan(&self, conn: &Connection, full: bool) -> Result<ScanStats> {
        let _ = full;
        let mut stats = ScanStats::default();
        let mut live_paths: HashSet<String> = HashSet::new();
        let mut file_map: BTreeMap<String, String> = BTreeMap::new();

        self.report(IndexPhase::Starting, &stats, None);

        // Load existing hashes for skip
        let existing = load_file_hashes(conn).await?;
        self.report(IndexPhase::Scanning, &stats, None);

        let walk_start = Instant::now();
        let walker = WalkBuilder::new(self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

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
            let hash = sha256_hex(&bytes);
            live_paths.insert(rel.clone());
            file_map.insert(rel.clone(), hash.clone());

            if existing.get(&rel) == Some(&hash) {
                stats.files_unchanged += 1;
                continue;
            }

            let index_start = Instant::now();
            let source = String::from_utf8_lossy(&bytes).into_owned();
            let chunks = chunk_source(&rel, &source, self.max_chunk_lines);
            let lang = lang_label_for_path(path);
            self.report(IndexPhase::IndexingFile, &stats, Some(&rel));
            self.reindex_file(conn, &rel, &hash, lang, &source, &chunks, &mut stats)
                .await
                .with_context(|| format!("index {rel}"))?;
            stats.files_indexed += 1;
            // Chunk + embed + DB writes for this file.
            stats.reindex_ms += index_start.elapsed().as_millis() as u64;
            // Throttle UI: report every file when small, every 8th when large.
            if stats.files_indexed <= 20 || stats.files_indexed.is_multiple_of(8) {
                self.report(IndexPhase::IndexingFile, &stats, Some(&rel));
            }
        }
        // Walk time = everything in the loop except chunk/embed/DB work.
        stats.walk_ms = (walk_start.elapsed().as_millis() as u64).saturating_sub(stats.reindex_ms);

        self.report(IndexPhase::Finalizing, &stats, None);
        let finalize_start = Instant::now();

        // Remove deleted paths
        for old in existing.keys() {
            if !live_paths.contains(old) {
                delete_path(conn, old).await?;
            }
        }

        // Rebuild file_map from DB for accurate root (includes unchanged)
        let final_map = load_file_hashes(conn).await?;
        let root = merkle_root(&final_map.into_iter().collect());
        let now = now_secs();
        upsert_meta(conn, META_ROOT, &root).await?;
        upsert_meta(conn, META_LAST, &now.to_string()).await?;
        upsert_meta(conn, META_DIR, &self.root.display().to_string()).await?;
        stats.finalize_ms = finalize_start.elapsed().as_millis() as u64;

        self.report(IndexPhase::Done, &stats, None);
        Ok(stats)
    }

    /// Reindex only paths whose worktree hash differs from the store.
    pub async fn reindex_dirty(&self, conn: &Connection) -> Result<ScanStats> {
        self.scan(conn, false).await
    }

    // Internal helper with a single call site; each arg is a flat per-file DB
    // column, so a parameter struct would add ceremony without clarity. DeepWiki
    // (rust-clippy: too_many_arguments) sanctions #[allow] for such helpers.
    #[allow(clippy::too_many_arguments)]
    async fn reindex_file(
        &self,
        conn: &Connection,
        rel: &str,
        hash: &str,
        lang: &str,
        source: &str,
        chunks: &[RawChunk],
        stats: &mut ScanStats,
    ) -> Result<()> {
        delete_path(conn, rel).await?;

        let now = now_secs();
        conn.execute(
            "INSERT INTO cg_files(path, file_hash, lang, updated_at) VALUES (?, ?, ?, ?)",
            params![rel, hash, lang, now],
        )
        .await?;

        // Nodes + edges
        for (id, path, name, kind, start, end) in nodes_for_chunks(chunks) {
            conn.execute(
                "INSERT OR REPLACE INTO cg_nodes(id, path, name, kind, start_line, end_line)
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![id, path, name, kind, start as i64, end as i64],
            )
            .await?;
        }

        let src_node = file_node_id(rel);
        for target in extract_import_targets(rel, source) {
            let dst = format!("import:{target}");
            conn.execute(
                "INSERT OR IGNORE INTO cg_edges(src, dst, kind) VALUES (?, ?, 'imports')",
                params![src_node.clone(), dst],
            )
            .await?;
        }

        for chunk in chunks {
            // Compact embed text reduces model tokens / RAM; full body still stored for FTS.
            let embed_src = embed_text_for_chunk(chunk);
            let emb = (self.embed)(&embed_src).await?;
            let emb_blob = if is_zero(&emb) {
                None
            } else {
                stats.chunks_embedded += 1;
                Some(vec_buf(&emb))
            };

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
                    hash,
                    emb_blob.as_deref(),
                ],
            )
            .await?;
            stats.chunks_indexed += 1;
        }

        Ok(())
    }
}

async fn delete_path(conn: &Connection, path: &str) -> Result<()> {
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
    conn.execute("DELETE FROM cg_meta", ()).await?;
    // Rebuild empty FTS if present
    let _ = conn.execute("INSERT INTO cg_fts(cg_fts) VALUES('rebuild')", ()).await;
    Ok(())
}

#[allow(dead_code)]
pub fn pathbuf_root(root: &str) -> PathBuf {
    PathBuf::from(root)
}
