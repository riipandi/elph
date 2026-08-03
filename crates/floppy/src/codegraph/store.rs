//! Public [`CodegraphStore`] API.

use anyhow::Result;
use std::path::Path;
use std::sync::Mutex;

use super::index::{self, Indexer};
use super::migrations;
use super::search;
use super::types::{
    ChunkHit, CodegraphConfig, CodegraphStatus, ImpactNode, ProgressFn, ScanStats, SearchOptions,
};
use crate::core::db;
use crate::core::embed::EmbedFn;

pub struct CodegraphStore {
    db_path: String,
    root_dir: String,
    embed: EmbedFn,
    apply_migrations: bool,
    max_chunk_lines: u32,
    max_file_bytes: u64,
    initialized: Mutex<bool>,
}

impl CodegraphStore {
    pub fn new(config: CodegraphConfig) -> Self {
        Self {
            db_path: config.db_path,
            root_dir: config.root_dir,
            embed: config.embed,
            apply_migrations: config.apply_migrations,
            max_chunk_lines: config.max_chunk_lines,
            max_file_bytes: config.max_file_bytes,
            initialized: Mutex::new(false),
        }
    }

    pub async fn init(&self) -> Result<()> {
        if *self.initialized.lock().unwrap() {
            return Ok(());
        }
        let apply = self.apply_migrations;
        db::with_local_db(&self.db_path, move |conn| async move {
            if apply {
                migrations::apply(&conn).await?;
            }
            Ok(())
        })
        .await?;
        *self.initialized.lock().unwrap() = true;
        Ok(())
    }

    fn indexer<'a>(&'a self, progress: Option<&'a ProgressFn>) -> Indexer<'a> {
        Indexer {
            root: Path::new(&self.root_dir),
            embed: &self.embed,
            max_chunk_lines: self.max_chunk_lines,
            max_file_bytes: self.max_file_bytes,
            progress,
        }
    }

    /// Full project index (CLI `build`).
    pub async fn build(&self) -> Result<ScanStats> {
        self.build_with_progress(None).await
    }

    /// Full project index with optional progress callback.
    pub async fn build_with_progress(&self, progress: Option<ProgressFn>) -> Result<ScanStats> {
        self.init().await?;
        let progress_ref = progress.as_ref();
        let indexer = self.indexer(progress_ref);
        db::with_local_db(&self.db_path, |conn| async move { indexer.scan(&conn, true).await }).await
    }

    /// Incremental dirty reindex (CLI `update` / agent `code_reindex`).
    pub async fn update(&self) -> Result<ScanStats> {
        self.update_with_progress(None).await
    }

    pub async fn update_with_progress(&self, progress: Option<ProgressFn>) -> Result<ScanStats> {
        self.init().await?;
        let progress_ref = progress.as_ref();
        let indexer = self.indexer(progress_ref);
        db::with_local_db(&self.db_path, |conn| async move { indexer.reindex_dirty(&conn).await }).await
    }

    pub async fn status(&self) -> Result<CodegraphStatus> {
        self.init().await?;
        db::with_local_db(&self.db_path, |conn| async move {
            let (file_count, chunk_count, node_count, edge_count) = index::status_counts(&conn).await?;
            let merkle_root = index::load_meta(&conn, "merkle_root").await?;
            let last = index::load_meta(&conn, "last_indexed_at")
                .await?
                .and_then(|s| s.parse().ok());
            let root_dir = index::load_meta(&conn, "root_dir").await?;
            Ok(CodegraphStatus {
                file_count,
                chunk_count,
                node_count,
                edge_count,
                merkle_root,
                last_indexed_at: last,
                root_dir,
            })
        })
        .await
    }

    pub async fn purge(&self) -> Result<()> {
        self.init().await?;
        db::with_local_db(&self.db_path, |conn| async move { index::purge_all(&conn).await }).await
    }

    pub async fn search(&self, opts: SearchOptions) -> Result<Vec<ChunkHit>> {
        self.init().await?;
        if opts.refresh_dirty {
            let _ = self.update().await;
        }
        let embed = self.embed.clone();
        let q = opts;
        db::with_local_db(&self.db_path, move |conn| async move {
            search::hybrid_search(&conn, &embed, &q).await
        })
        .await
    }

    pub async fn impact(&self, target: &str, max_depth: u32, limit: u32) -> Result<Vec<ImpactNode>> {
        self.init().await?;
        let target = target.to_string();
        db::with_local_db(&self.db_path, move |conn| async move {
            search::impact(&conn, &target, max_depth.clamp(0, 4), limit.clamp(1, 100)).await
        })
        .await
    }
}
