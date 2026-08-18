//! Cross-session retention / GC for the shared project store.
//!
//! Host maps `settings.json` `session` into [`RetentionPolicy`] and
//! calls [`run_session_gc`]. Never deletes pinned sessions or the optional
//! protected "latest per cwd" / currently open session id.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use turso::Database;

use crate::datastore::connect;

/// Policy for automatic session garbage collection.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub enabled: bool,
    pub max_sessions_per_cwd: u32,
    pub max_session_age_days: u32,
    pub max_store_db_bytes: u64,
    pub protect_latest_per_cwd: bool,
    /// Session id that must never be deleted (current process session).
    pub protect_session_id: Option<String>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_sessions_per_cwd: 40,
            max_session_age_days: 30,
            max_store_db_bytes: 512 * 1024 * 1024,
            protect_latest_per_cwd: true,
            protect_session_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionGcCandidate {
    pub id: String,
    pub cwd: String,
    pub updated_at: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionGcReport {
    pub examined: usize,
    pub deleted_ids: Vec<String>,
    pub skipped_pinned: usize,
    pub skipped_protected: usize,
    pub dry_run: bool,
}

/// List sessions eligible for GC inspection (all rows).
pub async fn list_session_gc_rows(database: &Database) -> Result<Vec<SessionGcCandidate>> {
    let conn = connect(database).await?;
    let mut rows = conn
        .query(
            "SELECT id, COALESCE(cwd, ''), COALESCE(updated_at, created_at), COALESCE(pinned, 0)
             FROM sessions
             ORDER BY updated_at DESC",
            (),
        )
        .await
        .context("list sessions for gc")?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(SessionGcCandidate {
            id: row.get(0)?,
            cwd: row.get(1)?,
            updated_at: row.get(2)?,
            pinned: row.get::<i64>(3).unwrap_or(0) != 0,
        });
    }
    Ok(out)
}

/// Select session ids to delete under `policy` (does not mutate DB).
///
/// `extra_protect` — additional session ids that must not be deleted (e.g. active
/// multi-worker `session_leases` holders).
pub fn plan_session_gc(
    candidates: &[SessionGcCandidate],
    policy: &RetentionPolicy,
    extra_protect: &HashSet<String>,
) -> Vec<String> {
    if !policy.enabled {
        return Vec::new();
    }

    let mut protect: HashSet<&str> = policy.protect_session_id.as_deref().into_iter().collect();
    for id in extra_protect {
        protect.insert(id.as_str());
    }

    let mut latest_per_cwd: HashMap<&str, &str> = HashMap::new();
    if policy.protect_latest_per_cwd {
        for c in candidates {
            latest_per_cwd.entry(c.cwd.as_str()).or_insert(c.id.as_str());
        }
    }

    let age_cutoff_secs = if policy.max_session_age_days > 0 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Some(now - (policy.max_session_age_days as i64) * 86_400)
    } else {
        None
    };

    let mut doomed: HashSet<String> = HashSet::new();

    if let Some(cutoff) = age_cutoff_secs {
        for c in candidates {
            if c.pinned || protect.contains(c.id.as_str()) {
                continue;
            }
            if policy.protect_latest_per_cwd && latest_per_cwd.get(c.cwd.as_str()).copied() == Some(c.id.as_str()) {
                continue;
            }
            if let Some(ts) = parse_timestamp_secs(&c.updated_at)
                && ts < cutoff
            {
                doomed.insert(c.id.clone());
            }
        }
    }

    if policy.max_sessions_per_cwd > 0 {
        let mut per_cwd: HashMap<&str, u32> = HashMap::new();
        for c in candidates {
            if c.pinned || protect.contains(c.id.as_str()) {
                continue;
            }
            if doomed.contains(&c.id) {
                continue;
            }
            let count = per_cwd.entry(c.cwd.as_str()).or_insert(0);
            *count += 1;
            if *count > policy.max_sessions_per_cwd {
                doomed.insert(c.id.clone());
            }
        }
    }

    let mut doomed_list: Vec<String> = doomed.into_iter().collect();
    doomed_list.sort_by(|a, b| {
        let ta = candidates
            .iter()
            .find(|c| &c.id == a)
            .map(|c| c.updated_at.as_str())
            .unwrap_or("");
        let tb = candidates
            .iter()
            .find(|c| &c.id == b)
            .map(|c| c.updated_at.as_str())
            .unwrap_or("");
        ta.cmp(tb)
    });
    doomed_list
}

/// Delete sessions by id. Child rows cascade via FK (`ON DELETE CASCADE`).
pub async fn delete_sessions(database: &Database, session_ids: &[String]) -> Result<usize> {
    if session_ids.is_empty() {
        return Ok(0);
    }
    let conn = connect(database).await?;
    conn.execute("BEGIN IMMEDIATE", ()).await?;
    let outcome = async {
        let mut n = 0usize;
        for id in session_ids {
            // Spawn edges reference two sessions; clear either side before/with parent delete.
            let _ = conn
                .execute(
                    "DELETE FROM agent_spawn_edges WHERE parent_session_id = ? OR child_session_id = ?",
                    turso::params![id.as_str(), id.as_str()],
                )
                .await;
            conn.execute("DELETE FROM sessions WHERE id = ?", turso::params![id.as_str()])
                .await?;
            n += 1;
        }
        Ok::<usize, anyhow::Error>(n)
    }
    .await;
    match outcome {
        Ok(n) => {
            conn.execute("COMMIT", ()).await?;
            let _ = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", ()).await;
            Ok(n)
        }
        Err(err) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(err)
        }
    }
}

/// Set `sessions.pinned` for a session.
pub async fn set_session_pinned(database: &Database, session_id: &str, pinned: bool) -> Result<()> {
    let conn = connect(database).await?;
    let n = conn
        .execute(
            "UPDATE sessions SET pinned = ? WHERE id = ?",
            turso::params![if pinned { 1i64 } else { 0i64 }, session_id],
        )
        .await?;
    if n == 0 {
        anyhow::bail!("session not found: {session_id}");
    }
    Ok(())
}

/// Session ids that currently hold a multi-worker exclusive lease (must not GC).
pub async fn list_leased_session_ids(database: &Database) -> Result<HashSet<String>> {
    let conn = connect(database).await?;
    // Table may be missing on very old DBs; treat as empty.
    let mut rows = match conn.query("SELECT session_id FROM session_leases", ()).await {
        Ok(r) => r,
        Err(_) => return Ok(HashSet::new()),
    };
    let mut out = HashSet::new();
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        out.insert(id);
    }
    Ok(out)
}

/// Run GC: plan + delete. When `dry_run`, only report candidates.
pub async fn run_session_gc(database: &Database, policy: &RetentionPolicy, dry_run: bool) -> Result<SessionGcReport> {
    let candidates = list_session_gc_rows(database).await?;
    let leased = list_leased_session_ids(database).await.unwrap_or_default();
    let mut report = SessionGcReport {
        examined: candidates.len(),
        dry_run,
        ..Default::default()
    };
    for c in &candidates {
        if c.pinned {
            report.skipped_pinned += 1;
        }
    }
    if let Some(id) = &policy.protect_session_id
        && candidates.iter().any(|c| &c.id == id)
    {
        report.skipped_protected += 1;
    }
    report.skipped_protected += leased
        .iter()
        .filter(|id| candidates.iter().any(|c| c.id == **id))
        .count();

    let to_delete = plan_session_gc(&candidates, policy, &leased);

    if dry_run {
        report.deleted_ids = to_delete;
        return Ok(report);
    }

    delete_sessions(database, &to_delete).await?;
    report.deleted_ids = to_delete;
    Ok(report)
}

/// Expand delete set until store file is under budget (or only protected remain).
pub async fn expand_gc_for_size(
    database: &Database,
    db_path: &Path,
    policy: &RetentionPolicy,
    mut already: Vec<String>,
) -> Result<Vec<String>> {
    if policy.max_store_db_bytes == 0 {
        return Ok(already);
    }
    let mut size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    if size <= policy.max_store_db_bytes {
        return Ok(already);
    }

    let candidates = list_session_gc_rows(database).await?;
    let leased = list_leased_session_ids(database).await.unwrap_or_default();
    let mut protect: HashSet<&str> = policy.protect_session_id.as_deref().into_iter().collect();
    for id in &leased {
        protect.insert(id.as_str());
    }
    let mut latest_per_cwd: HashMap<&str, &str> = HashMap::new();
    if policy.protect_latest_per_cwd {
        for c in &candidates {
            latest_per_cwd.entry(c.cwd.as_str()).or_insert(c.id.as_str());
        }
    }
    let already_set: HashSet<String> = already.iter().cloned().collect();

    let mut remaining: Vec<&SessionGcCandidate> = candidates
        .iter()
        .filter(|c| !already_set.contains(&c.id))
        .filter(|c| !c.pinned)
        .filter(|c| !protect.contains(c.id.as_str()))
        .filter(|c| {
            !(policy.protect_latest_per_cwd && latest_per_cwd.get(c.cwd.as_str()).copied() == Some(c.id.as_str()))
        })
        .collect();
    remaining.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));

    for c in remaining {
        if size <= policy.max_store_db_bytes {
            break;
        }
        already.push(c.id.clone());
        delete_sessions(database, std::slice::from_ref(&c.id)).await?;
        size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(size);
    }
    Ok(already)
}

/// Remove orphan artifact dirs under `sessions_root` whose names are not session ids.
pub fn prune_orphan_artifact_dirs(sessions_root: &Path, live_session_ids: &HashSet<String>) -> Result<usize> {
    if !sessions_root.is_dir() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in std::fs::read_dir(sessions_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if live_session_ids.contains(&name) {
            continue;
        }
        let _ = std::fs::remove_dir_all(entry.path());
        removed += 1;
    }
    Ok(removed)
}

/// Parse ISO-ish timestamps to unix seconds (best-effort).
fn parse_timestamp_secs(s: &str) -> Option<i64> {
    // RFC3339 / ISO with T: take first 19 chars "YYYY-MM-DDTHH:MM:SS"
    let normalized = s.replace('T', " ");
    let head = normalized.get(..19)?;
    let parts: Vec<&str> = head.split([' ', '-', ':']).collect();
    if parts.len() < 6 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let mo: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    let h: i64 = parts[3].parse().ok()?;
    let mi: i64 = parts[4].parse().ok()?;
    let se: i64 = parts[5].parse().ok()?;
    // Approximate civil → unix (good enough for retention days).
    let days = days_from_civil(y, mo, d)?;
    Some(days * 86_400 + h * 3600 + mi * 60 + se)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    // Howard Hinnant civil_from_days inverse (approx for GC).
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// Convenience: open path and run GC with optional size expansion + artifact cleanup.
pub async fn run_full_session_gc(
    database: Arc<Database>,
    db_path: PathBuf,
    sessions_artifact_root: Option<PathBuf>,
    policy: RetentionPolicy,
    dry_run: bool,
) -> Result<SessionGcReport> {
    let mut report = run_session_gc(&database, &policy, dry_run).await?;
    if dry_run {
        return Ok(report);
    }
    if policy.max_store_db_bytes > 0 {
        let expanded = expand_gc_for_size(&database, &db_path, &policy, report.deleted_ids.clone()).await?;
        report.deleted_ids = expanded;
    }
    if let Some(root) = sessions_artifact_root {
        let live: HashSet<String> = list_session_gc_rows(&database)
            .await?
            .into_iter()
            .map(|c| c.id)
            .collect();
        for id in &report.deleted_ids {
            let dir = root.join(id);
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        let _ = prune_orphan_artifact_dirs(&root, &live);
    }
    let _ = Duration::from_secs(0); // silence unused if optimized
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, cwd: &str, updated: &str, pinned: bool) -> SessionGcCandidate {
        SessionGcCandidate {
            id: id.into(),
            cwd: cwd.into(),
            updated_at: updated.into(),
            pinned,
        }
    }

    #[test]
    fn plan_respects_pin_and_max_per_cwd() {
        let c = vec![
            cand("s1", "/a", "2026-08-09T00:00:00Z", false),
            cand("s2", "/a", "2026-08-08T00:00:00Z", false),
            cand("s3", "/a", "2026-08-07T00:00:00Z", true),
            cand("s4", "/a", "2026-08-06T00:00:00Z", false),
        ];
        let policy = RetentionPolicy {
            enabled: true,
            max_sessions_per_cwd: 2,
            max_session_age_days: 0,
            max_store_db_bytes: 0,
            protect_latest_per_cwd: true,
            protect_session_id: None,
        };
        let plan = plan_session_gc(&c, &policy, &HashSet::new());
        assert!(plan.contains(&"s4".to_string()));
        assert!(!plan.contains(&"s1".to_string()));
        assert!(!plan.contains(&"s3".to_string()));
    }
}
