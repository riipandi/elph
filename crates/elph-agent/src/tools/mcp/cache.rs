//! Persistent MCP tool call result cache backed by Turso SQLite.
//!
//! Caches read-only tool call results so repeated calls with the same
//! arguments return instantly instead of hitting the MCP server again.
//!
//! ## Storage
//!
//! Each cache database is a standalone Turso file:
//! - Host-level: `APP_DATA/mcp_cache/cache.db`
//! - Session-level: `APP_DATA/sessions/<SESSION_ID>/mcp_cache/cache.db`
//!
//! ## Schema
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS mcp_cache (
//!     cache_key  TEXT PRIMARY KEY,
//!     server     TEXT NOT NULL,
//!     tool       TEXT NOT NULL,
//!     args_hash  TEXT NOT NULL,
//!     result     BLOB NOT NULL,
//!     is_error   INTEGER NOT NULL,
//!     created_at INTEGER NOT NULL,
//!     expires_at INTEGER NOT NULL
//! ) STRICT;
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rmcp::model::CallToolResult;
use serde_json::Value;
use turso::{Builder, Connection};

/// Default TTL for cached tool results (60 seconds).
const DEFAULT_CACHE_TTL_MS: u64 = 60_000;

/// Maximum number of cache entries before eviction kicks in.
const MAX_CACHE_ENTRIES: usize = 2048;

/// Eviction batch: when over max, remove this many expired entries.
const EVICTION_BATCH: usize = 512;

/// Schema DDL (idempotent).
const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS mcp_cache (
    cache_key  TEXT PRIMARY KEY,
    server     TEXT NOT NULL,
    tool       TEXT NOT NULL,
    args_hash  TEXT NOT NULL,
    result     BLOB NOT NULL,
    is_error   INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS idx_mcp_cache_expires ON mcp_cache(expires_at);
CREATE INDEX IF NOT EXISTS idx_mcp_cache_server ON mcp_cache(server);
"#;

/// Persistent MCP tool call result cache.
///
/// Thread-safe: all operations go through a single Turso connection.
/// Designed to be wrapped in `Arc` and shared across the session pool.
#[derive(Debug, Clone)]
pub struct McpCacheStore {
    db: Arc<turso::Database>,
}

impl McpCacheStore {
    /// Open or create a cache database at `db_path`.
    ///
    /// Creates parent directories if missing. Applies schema on first open.
    pub async fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create mcp cache dir {}", parent.display()))?;
        }

        let db = Builder::new_local(db_path.to_string_lossy().as_ref())
            .experimental_multiprocess_wal(true)
            .build()
            .await
            .with_context(|| format!("open mcp cache at {}", db_path.display()))?;

        let conn = db.connect().context("connect mcp cache")?;
        conn.execute_batch(SCHEMA_SQL).await.context("apply mcp cache schema")?;

        Ok(Self { db: Arc::new(db) })
    }

    fn conn(&self) -> Result<Connection> {
        self.db.connect().context("mcp cache connect")
    }

    /// Build a deterministic cache key from server, tool, and args.
    fn cache_key(server: &str, tool: &str, args: &Value) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        server.hash(&mut hasher);
        tool.hash(&mut hasher);
        // Serialize args to a canonical JSON string for hashing.
        if let Ok(json_str) = serde_json::to_string(args) {
            json_str.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Retrieve a cached tool result, if present and not expired.
    ///
    /// Expired entries are deleted lazily on read.
    pub async fn get(&self, server: &str, tool: &str, args: &Value) -> Result<Option<CallToolResult>> {
        let conn = self.conn()?;
        let key = Self::cache_key(server, tool, args);
        let now = Self::now_ms();

        // Garbage-collect expired entries on every read (lazy eviction).
        conn.execute("DELETE FROM mcp_cache WHERE expires_at < ?", (now,))
            .await?;

        let mut rows = conn
            .query(
                "SELECT result, is_error FROM mcp_cache WHERE cache_key = ? AND expires_at >= ?",
                (key.as_str(), now),
            )
            .await?;

        match rows.next().await? {
            Some(row) => {
                let blob: Vec<u8> = row.get(0)?;
                let is_error: i64 = row.get(1)?;
                let mut result: CallToolResult =
                    serde_json::from_slice(&blob).context("deserialize cached MCP result")?;
                result.is_error = Some(is_error != 0);
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// Store a tool call result in the cache.
    ///
    /// `ttl_ms` controls how long the entry lives. Defaults to 60s when zero.
    pub async fn set(
        &self,
        server: &str,
        tool: &str,
        args: &Value,
        result: &CallToolResult,
        ttl_ms: u64,
    ) -> Result<()> {
        let conn = self.conn()?;
        let key = Self::cache_key(server, tool, args);
        let now = Self::now_ms();
        let ttl = if ttl_ms == 0 { DEFAULT_CACHE_TTL_MS } else { ttl_ms };
        let expires_at = now + ttl as i64;
        let args_hash = Self::cache_key(server, tool, args);

        let blob = serde_json::to_vec(result).context("serialize MCP result for cache")?;

        conn.execute(
            "INSERT OR REPLACE INTO mcp_cache (cache_key, server, tool, args_hash, result, is_error, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                key.as_str(),
                server,
                tool,
                &args_hash[..8],
                blob.as_slice(),
                result.is_error.unwrap_or(false) as i64,
                now,
                expires_at,
            ),
        )
        .await?;

        // Evict oldest expired entries if we're over the limit.
        let mut rows = conn.query("SELECT COUNT(*) FROM mcp_cache", ()).await?;
        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            if count > MAX_CACHE_ENTRIES as i64 {
                conn.execute(
                    "DELETE FROM mcp_cache WHERE cache_key IN (
                        SELECT cache_key FROM mcp_cache ORDER BY expires_at ASC LIMIT ?
                    )",
                    (EVICTION_BATCH as i64,),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Invalidate all cached entries for a given server (e.g. on reconnect).
    pub async fn invalidate_server(&self, server: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM mcp_cache WHERE server = ?", (server,))
            .await?;
        Ok(())
    }

    /// Clear the entire cache.
    pub async fn clear(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM mcp_cache", ()).await?;
        Ok(())
    }

    /// Remove all expired entries (maintenance).
    pub async fn gc(&self) -> Result<()> {
        let conn = self.conn()?;
        let now = Self::now_ms();
        conn.execute("DELETE FROM mcp_cache WHERE expires_at < ?", (now,))
            .await?;
        Ok(())
    }
}

/// Heuristic: treat a tool as read-only (cacheable) when its name does not
/// contain mutation-related keywords.
pub fn is_read_only_tool(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    !(lower.contains("write")
        || lower.contains("create")
        || lower.contains("delete")
        || lower.contains("update")
        || lower.contains("edit")
        || lower.contains("set")
        || lower.contains("add")
        || lower.contains("remove")
        || lower.contains("rename")
        || lower.contains("move")
        || lower.contains("copy")
        || lower.contains("mkdir")
        || lower.contains("upload")
        || lower.contains("insert")
        || lower.contains("patch")
        || lower.contains("put")
        || lower.contains("post"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolResult, ContentBlock, TextContent};

    fn sample_result(text: &str) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::Text(TextContent::new(text))])
    }

    fn sample_args() -> Value {
        serde_json::json!({"path": "/tmp/test.txt", "limit": 10})
    }

    #[tokio::test]
    async fn get_set_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.db")).await.expect("open");

        let result = sample_result("hello world");
        cache
            .set("test-server", "read_file", &sample_args(), &result, 60_000)
            .await
            .expect("set");

        let cached = cache
            .get("test-server", "read_file", &sample_args())
            .await
            .expect("get");
        assert!(cached.is_some());
        assert!(!cached.unwrap().is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn expired_entry_not_returned() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.db")).await.expect("open");

        let result = sample_result("expired data");
        cache
            .set("test-server", "read_file", &sample_args(), &result, 1)
            .await
            .expect("set");

        // Wait for TTL to expire.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let cached = cache
            .get("test-server", "read_file", &sample_args())
            .await
            .expect("get");
        assert!(cached.is_none(), "expired entry should not be returned");
    }

    #[tokio::test]
    async fn different_args_different_cache_key() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.db")).await.expect("open");

        let result_a = sample_result("result A");
        let result_b = sample_result("result B");

        cache
            .set("srv", "tool", &serde_json::json!({"x": 1}), &result_a, 60_000)
            .await
            .expect("set a");
        cache
            .set("srv", "tool", &serde_json::json!({"x": 2}), &result_b, 60_000)
            .await
            .expect("set b");

        let a = cache
            .get("srv", "tool", &serde_json::json!({"x": 1}))
            .await
            .expect("get a");
        let b = cache
            .get("srv", "tool", &serde_json::json!({"x": 2}))
            .await
            .expect("get b");
        assert!(a.is_some());
        assert!(b.is_some());
        assert_ne!(
            a.unwrap().content[0].as_text().unwrap().text,
            b.unwrap().content[0].as_text().unwrap().text
        );
    }

    #[tokio::test]
    async fn invalidate_server_clears_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.db")).await.expect("open");

        cache
            .set("srv-a", "tool", &sample_args(), &sample_result("a"), 60_000)
            .await
            .expect("set a");
        cache
            .set("srv-b", "tool", &sample_args(), &sample_result("b"), 60_000)
            .await
            .expect("set b");

        cache.invalidate_server("srv-a").await.expect("invalidate");

        assert!(
            cache
                .get("srv-a", "tool", &sample_args())
                .await
                .expect("get a")
                .is_none()
        );
        assert!(
            cache
                .get("srv-b", "tool", &sample_args())
                .await
                .expect("get b")
                .is_some()
        );
    }

    #[test]
    fn read_only_tool_detection() {
        assert!(is_read_only_tool("read_file"));
        assert!(is_read_only_tool("grep"));
        assert!(is_read_only_tool("list_dir"));
        assert!(is_read_only_tool("web_search"));
        assert!(!is_read_only_tool("write_file"));
        assert!(!is_read_only_tool("create_dir"));
        assert!(!is_read_only_tool("delete_path"));
        assert!(!is_read_only_tool("edit_file"));
        assert!(!is_read_only_tool("rename"));
        assert!(!is_read_only_tool("upload_file"));
    }
}
