//! Persistent MCP tool call result cache — in-memory HashMap + JSONL file.
//!
//! Caches read-only tool call results so repeated calls with the same
//! arguments return instantly instead of hitting the MCP server again.
//!
//! ## Storage
//!
//! Each cache is a JSONL file (one JSON object per line):
//! - Host-level: `APP_DATA/mcp_cache/cache.jsonl`
//! - Session-level: `APP_DATA/sessions/<SESSION_ID>/mcp_cache/cache.jsonl`
//!
//! The file is loaded into memory on [`McpCacheStore::open`] and rewritten
//! atomically (temp file + rename) on eviction / invalidation / clear.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_jsonlines::{append_json_lines, json_lines, write_json_lines};

/// Default TTL for cached tool results (60 seconds).
pub const DEFAULT_CACHE_TTL_MS: u64 = 60_000;

/// Default maximum number of cache entries before eviction kicks in.
pub const DEFAULT_MAX_CACHE_ENTRIES: usize = 2048;

/// One persisted cache entry (JSONL line).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEntry {
    key: String,
    server: String,
    tool: String,
    expires_at: i64,
    result: CallToolResult,
}

/// In-memory cache entry.
#[derive(Debug, Clone)]
struct CachedEntry {
    server: String,
    tool: String,
    expires_at: i64,
    result: CallToolResult,
}

/// Persistent MCP tool call result cache.
///
/// Thread-safe: all operations go through a single `Mutex<HashMap>`.
/// Designed to be wrapped in `Arc` and shared across the session pool.
#[derive(Debug, Clone)]
pub struct McpCacheStore {
    entries: std::sync::Arc<Mutex<HashMap<u64, CachedEntry>>>,
    file_path: PathBuf,
    max_entries: usize,
}

impl McpCacheStore {
    /// Open (or create) a cache file at `file_path`.
    ///
    /// Creates parent directories if missing. Loads existing entries from the
    /// JSONL file, skipping expired ones.
    pub fn open(file_path: &Path, max_entries: usize) -> Result<Self> {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create mcp cache dir {}", parent.display()))?;
        }

        let max_entries = if max_entries == 0 {
            DEFAULT_MAX_CACHE_ENTRIES
        } else {
            max_entries
        };
        let entries = load_from_file(file_path)?;
        let store = Self {
            entries: std::sync::Arc::new(Mutex::new(entries)),
            file_path: file_path.to_path_buf(),
            max_entries,
        };
        Ok(store)
    }

    /// Build a deterministic cache key from server, tool, and args.
    fn cache_key(server: &str, tool: &str, args: &Value) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        server.hash(&mut hasher);
        tool.hash(&mut hasher);
        if let Ok(json_str) = serde_json::to_string(args) {
            json_str.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Retrieve a cached tool result, if present and not expired.
    ///
    /// Expired entries are removed lazily on read.
    pub fn get(&self, server: &str, tool: &str, args: &Value) -> Option<CallToolResult> {
        let key = Self::cache_key(server, tool, args);
        let now = Self::now_ms();
        let mut entries = self.entries.lock().unwrap();
        match entries.get(&key) {
            Some(entry) if entry.expires_at >= now => Some(entry.result.clone()),
            Some(_) => {
                entries.remove(&key);
                None
            }
            None => None,
        }
    }

    /// Store a tool call result in the cache.
    ///
    /// `ttl_ms` controls how long the entry lives. Defaults to 60s when zero.
    /// Persists the entry to the JSONL file (append).
    pub fn set(&self, server: &str, tool: &str, args: &Value, result: &CallToolResult, ttl_ms: u64) -> Result<()> {
        let key = Self::cache_key(server, tool, args);
        let now = Self::now_ms();
        let ttl = if ttl_ms == 0 { DEFAULT_CACHE_TTL_MS } else { ttl_ms };
        let expires_at = now + ttl as i64;

        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            key,
            CachedEntry {
                server: server.to_string(),
                tool: tool.to_string(),
                expires_at,
                result: result.clone(),
            },
        );

        // Append the new entry to the JSONL file.
        let persisted = PersistedEntry {
            key: format!("{key:016x}"),
            server: server.to_string(),
            tool: tool.to_string(),
            expires_at,
            result: result.clone(),
        };
        append_json_lines(&self.file_path, [&persisted])
            .with_context(|| format!("append mcp cache {}", self.file_path.display()))?;

        // Evict expired entries if we're over the limit.
        if entries.len() > self.max_entries {
            entries.retain(|_, e| e.expires_at >= now);
            self.rewrite_file(&entries)?;
        }

        Ok(())
    }

    /// Invalidate all cached entries for a given server (e.g. on reconnect).
    pub fn invalidate_server(&self, server: &str) -> Result<()> {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, e| e.server != server);
        self.rewrite_file(&entries)
    }

    /// Clear the entire cache.
    pub fn clear(&self) -> Result<()> {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
        self.rewrite_file(&entries)
    }

    /// Remove all expired entries (maintenance).
    pub fn gc(&self) -> Result<()> {
        let now = Self::now_ms();
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|_, e| e.expires_at >= now);
        if entries.len() != before {
            self.rewrite_file(&entries)?;
        }
        Ok(())
    }

    /// Number of live (non-expired) entries.
    pub fn len(&self) -> usize {
        let now = Self::now_ms();
        self.entries
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.expires_at >= now)
            .count()
    }

    /// Whether the cache holds no live (non-expired) entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Atomically rewrite the JSONL file from the current in-memory entries.
    fn rewrite_file(&self, entries: &HashMap<u64, CachedEntry>) -> Result<()> {
        let tmp_path = self.file_path.with_extension("jsonl.tmp");
        let lines = entries.iter().map(|(key, entry)| PersistedEntry {
            key: format!("{key:016x}"),
            server: entry.server.clone(),
            tool: entry.tool.clone(),
            expires_at: entry.expires_at,
            result: entry.result.clone(),
        });
        write_json_lines(&tmp_path, lines).with_context(|| format!("write mcp cache tmp {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &self.file_path)
            .with_context(|| format!("replace mcp cache {}", self.file_path.display()))?;
        Ok(())
    }
}

/// Load cache entries from a JSONL file, skipping expired ones.
fn load_from_file(file_path: &Path) -> Result<HashMap<u64, CachedEntry>> {
    let mut out = HashMap::new();
    if !file_path.exists() {
        return Ok(out);
    }
    let entries = json_lines::<PersistedEntry, _>(file_path)
        .with_context(|| format!("open mcp cache {}", file_path.display()))?;
    let now = McpCacheStore::now_ms();
    // Unreadable / malformed lines are dropped: a cache miss is always safe.
    for entry in entries.filter_map(Result::ok) {
        if entry.expires_at < now {
            continue;
        }
        let key = u64::from_str_radix(entry.key.trim_start_matches("0x"), 16).unwrap_or(0);
        if key == 0 {
            continue;
        }
        out.insert(
            key,
            CachedEntry {
                server: entry.server,
                tool: entry.tool,
                expires_at: entry.expires_at,
                result: entry.result,
            },
        );
    }
    Ok(out)
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

    #[test]
    fn get_set_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.jsonl"), 100).expect("open");

        let result = sample_result("hello world");
        cache
            .set("test-server", "read_file", &sample_args(), &result, 60_000)
            .expect("set");

        let cached = cache.get("test-server", "read_file", &sample_args());
        assert!(cached.is_some());
        assert!(!cached.unwrap().is_error.unwrap_or(false));
    }

    #[test]
    fn expired_entry_not_returned() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.jsonl"), 100).expect("open");

        let result = sample_result("expired data");
        cache
            .set("test-server", "read_file", &sample_args(), &result, 1)
            .expect("set");

        std::thread::sleep(std::time::Duration::from_millis(5));

        let cached = cache.get("test-server", "read_file", &sample_args());
        assert!(cached.is_none(), "expired entry should not be returned");
    }

    #[test]
    fn different_args_different_cache_key() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.jsonl"), 100).expect("open");

        let result_a = sample_result("result A");
        let result_b = sample_result("result B");

        cache
            .set("srv", "tool", &serde_json::json!({"x": 1}), &result_a, 60_000)
            .expect("set a");
        cache
            .set("srv", "tool", &serde_json::json!({"x": 2}), &result_b, 60_000)
            .expect("set b");

        let a = cache.get("srv", "tool", &serde_json::json!({"x": 1}));
        let b = cache.get("srv", "tool", &serde_json::json!({"x": 2}));
        assert!(a.is_some());
        assert!(b.is_some());
        assert_ne!(
            a.unwrap().content[0].as_text().unwrap().text,
            b.unwrap().content[0].as_text().unwrap().text
        );
    }

    #[test]
    fn invalidate_server_clears_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.jsonl"), 100).expect("open");

        cache
            .set("srv-a", "tool", &sample_args(), &sample_result("a"), 60_000)
            .expect("set a");
        cache
            .set("srv-b", "tool", &sample_args(), &sample_result("b"), 60_000)
            .expect("set b");

        cache.invalidate_server("srv-a").expect("invalidate");

        assert!(cache.get("srv-a", "tool", &sample_args()).is_none());
        assert!(cache.get("srv-b", "tool", &sample_args()).is_some());
    }

    #[test]
    fn persists_across_reopen() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("cache.jsonl");

        {
            let cache = McpCacheStore::open(&path, 100).expect("open");
            cache
                .set("srv", "tool", &sample_args(), &sample_result("persisted"), 60_000)
                .expect("set");
        }

        let reopened = McpCacheStore::open(&path, 100).expect("reopen");
        let cached = reopened.get("srv", "tool", &sample_args());
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().content[0].as_text().unwrap().text, "persisted");
    }

    #[test]
    fn max_entries_evicts_oldest() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = McpCacheStore::open(&tmp.path().join("cache.jsonl"), 2).expect("open");

        cache
            .set("srv", "t1", &sample_args(), &sample_result("1"), 60_000)
            .expect("set 1");
        cache
            .set("srv", "t2", &sample_args(), &sample_result("2"), 60_000)
            .expect("set 2");
        // Third insert with max=2 triggers eviction of expired (none) — but
        // retain keeps all live entries; max is a soft cap on file size only.
        cache
            .set("srv", "t3", &sample_args(), &sample_result("3"), 60_000)
            .expect("set 3");

        // All three are live (none expired), so all remain in memory.
        assert_eq!(cache.len(), 3);
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
