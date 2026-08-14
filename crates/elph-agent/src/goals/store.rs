//! Turso-backed goal persistence.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, is_lock_err, with_conn, with_write_transaction};

use super::types::{Goal, GoalStatus};

const GOAL_COLUMNS: &str = "id, session_id, objective, completion_criterion, status,
    turns_used, tokens_used, wall_clock_ms, wall_clock_budget_ms,
    turn_budget, token_budget, created_at, completed_at";

#[derive(Clone)]
pub struct GoalStore {
    db_path: PathBuf,
    /// Shared database handle injected by the host. When present, the store
    /// connects from this handle instead of opening `db_path` — the host owns
    /// the open/apply-migrations lifetime.
    database: Option<Arc<turso::Database>>,
}

impl GoalStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    /// Attach a shared, already-open database handle. When set, the store
    /// connects from this handle on each operation instead of opening
    /// [`db_path`] — the host is responsible for opening the database and
    /// applying migrations.
    pub fn with_database(mut self, database: Arc<turso::Database>) -> Self {
        self.database = Some(database);
        self
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn with_conn<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match &self.database {
            Some(db) => {
                let conn = connect(db).await?;
                f(conn).await
            }
            None => with_conn(&self.db_path, f)
                .await
                .with_context(|| format!("open goal database {}", self.db_path.display())),
        }
    }

    pub async fn get_active_goal(&self, session_id: &str) -> Result<Option<Goal>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {GOAL_COLUMNS} FROM goals
                         WHERE session_id = ? AND status = 'active'
                         ORDER BY id DESC LIMIT 1"
                    ),
                    turso::params![session_id],
                )
                .await?;
            if let Some(row) = rows.next().await? {
                return Ok(Some(row_to_goal(&row)?));
            }
            Ok(None)
        })
        .await
    }

    pub async fn get_latest_goal(&self, session_id: &str) -> Result<Option<Goal>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {GOAL_COLUMNS} FROM goals
                         WHERE session_id = ?
                         ORDER BY id DESC LIMIT 1"
                    ),
                    turso::params![session_id],
                )
                .await?;
            if let Some(row) = rows.next().await? {
                return Ok(Some(row_to_goal(&row)?));
            }
            Ok(None)
        })
        .await
    }

    pub async fn has_unfinished_goal(&self, session_id: &str) -> Result<bool> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT 1 FROM goals
                     WHERE session_id = ? AND status NOT IN ('complete')
                     ORDER BY id DESC LIMIT 1",
                    turso::params![session_id],
                )
                .await?;
            Ok(rows.next().await?.is_some())
        })
        .await
    }

    pub async fn create_goal(
        &self,
        session_id: &str,
        objective: &str,
        completion_criterion: Option<&str>,
        token_budget: i64,
        turn_budget: i64,
        wall_clock_budget_ms: i64,
    ) -> Result<Goal> {
        if objective.trim().is_empty() {
            bail!("objective must not be empty");
        }
        if self.has_unfinished_goal(session_id).await? {
            bail!("an unfinished goal already exists for this session");
        }
        if token_budget < 0 || turn_budget < 0 || wall_clock_budget_ms < 0 {
            bail!("budgets must be non-negative");
        }

        let goal_id = crate::session::id::create_goal_id();
        self.with_conn(|conn| async move {
            with_write_transaction(&conn, || async {
                let mut rows = conn
                    .query(
                        "SELECT 1 FROM goals WHERE session_id = ? AND status NOT IN ('complete') LIMIT 1",
                        turso::params![session_id],
                    )
                    .await?;
                if rows.next().await?.is_some() {
                    bail!("an unfinished goal already exists for this session");
                }
                conn.execute(
                    "INSERT INTO goals (
                        id, session_id, objective, completion_criterion, status,
                        token_budget, turn_budget, wall_clock_budget_ms
                     ) VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
                    turso::params![
                        goal_id.as_str(),
                        session_id,
                        objective.trim(),
                        completion_criterion,
                        token_budget,
                        turn_budget,
                        wall_clock_budget_ms,
                    ],
                )
                .await?;
                Ok::<(), anyhow::Error>(())
            })
            .await
        })
        .await?;

        self.get_active_goal(session_id)
            .await?
            .context("goal created but not found")
    }

    pub async fn update_goal_status(&self, session_id: &str, status: GoalStatus) -> Result<Goal> {
        let Some(goal) = self.get_active_goal(session_id).await? else {
            bail!("no active goal for this session");
        };
        let completed_at = status.is_terminal().then(crate::messages::now_iso_timestamp);
        self.with_conn(|conn| async move {
            with_write_transaction(&conn, || async {
                let changed = conn
                    .execute(
                        "UPDATE goals SET status = ?, completed_at = ? WHERE id = ? AND status = 'active'",
                        turso::params![status.as_str(), completed_at.clone(), goal.id.as_str()],
                    )
                    .await?;
                if changed == 0 {
                    bail!("goal was changed by another process");
                }
                Ok::<(), anyhow::Error>(())
            })
            .await
        })
        .await?;
        self.get_latest_goal(session_id)
            .await?
            .context("goal updated but not found")
    }

    pub async fn set_status(&self, session_id: &str, status: GoalStatus) -> Result<Goal> {
        let goal = self
            .get_latest_goal(session_id)
            .await?
            .context("no goal for this session")?;
        if goal.status.is_terminal() {
            bail!("cannot change a completed goal");
        }

        let completed_at = if status.is_terminal() {
            Some(crate::messages::now_iso_timestamp())
        } else {
            None
        };

        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE goals SET status = ?, completed_at = ? WHERE id = ?",
                turso::params![status.as_str(), completed_at.clone(), goal.id.as_str()],
            )
            .await?;
            Ok(())
        })
        .await?;

        self.get_latest_goal(session_id)
            .await?
            .context("goal status updated but not found")
    }

    pub async fn resume_goal(&self, session_id: &str) -> Result<Goal> {
        let goal = self
            .get_latest_goal(session_id)
            .await?
            .context("no goal for this session")?;
        if !matches!(
            goal.status,
            GoalStatus::Paused | GoalStatus::Blocked | GoalStatus::BudgetLimited
        ) {
            bail!("goal is not paused, blocked, or budget-limited");
        }
        if self.get_active_goal(session_id).await?.is_some() {
            bail!("another active goal exists");
        }

        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE goals SET status = 'active', completed_at = NULL WHERE id = ?",
                turso::params![goal.id.as_str()],
            )
            .await?;
            Ok(())
        })
        .await?;

        self.get_active_goal(session_id)
            .await?
            .context("goal resumed but not found")
    }

    pub async fn clear_goal(&self, session_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute("DELETE FROM goals WHERE session_id = ?", turso::params![session_id])
                .await?;
            Ok(())
        })
        .await
        .map_err(|e| {
            // If database is locked during clear (common during cancel), suppress the error
            // since the goal will be orphaned anyway and user explicitly requested cancel
            if is_lock_err(&e.to_string()) {
                log::warn!("Goal database locked during clear, ignoring: {e}");
                anyhow::anyhow!("Goal cancelled (database busy)")
            } else {
                e
            }
        })
    }

    pub async fn replace_goal(
        &self,
        session_id: &str,
        objective: &str,
        completion_criterion: Option<&str>,
        token_budget: i64,
        turn_budget: i64,
        wall_clock_budget_ms: i64,
    ) -> Result<Goal> {
        self.clear_goal(session_id).await?;
        self.create_goal(
            session_id,
            objective,
            completion_criterion,
            token_budget,
            turn_budget,
            wall_clock_budget_ms,
        )
        .await
    }

    pub async fn set_goal_budget(
        &self,
        session_id: &str,
        token_budget: Option<i64>,
        turn_budget: Option<i64>,
        wall_clock_budget_ms: Option<i64>,
    ) -> Result<Goal> {
        let Some(goal) = self.get_active_goal(session_id).await? else {
            bail!("no active goal for this session");
        };
        if token_budget.is_none() && turn_budget.is_none() && wall_clock_budget_ms.is_none() {
            bail!("at least one budget field must be provided");
        }

        let token_budget = token_budget.unwrap_or(goal.token_budget);
        let turn_budget = turn_budget.unwrap_or(goal.turn_budget);
        let wall_clock_budget_ms = wall_clock_budget_ms.unwrap_or(goal.wall_clock_budget_ms);

        if token_budget < 0 || turn_budget < 0 || wall_clock_budget_ms < 0 {
            bail!("budgets must be non-negative");
        }

        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE goals
                 SET token_budget = ?, turn_budget = ?, wall_clock_budget_ms = ?,
                     status = CASE WHEN status = 'budget_limited' THEN 'active' ELSE status END
                 WHERE id = ?",
                turso::params![token_budget, turn_budget, wall_clock_budget_ms, goal.id.as_str()],
            )
            .await?;
            Ok(())
        })
        .await?;

        if let Some(goal) = self.get_active_goal(session_id).await? {
            return Ok(goal);
        }
        self.get_latest_goal(session_id)
            .await?
            .context("goal budget updated but not found")
    }

    pub async fn record_usage(
        &self,
        session_id: &str,
        token_delta: i64,
        turn_delta: i64,
        wall_delta_ms: i64,
    ) -> Result<Option<Goal>> {
        let Some(goal) = self.get_active_goal(session_id).await? else {
            return Ok(None);
        };

        let new_tokens = goal.tokens_used.saturating_add(token_delta);
        let new_turns = goal.turns_used.saturating_add(turn_delta);
        let new_wall = goal.wall_clock_ms.saturating_add(wall_delta_ms);

        let mut new_status = goal.status;
        let budget_exceeded = (goal.token_budget > 0 && new_tokens >= goal.token_budget)
            || (goal.turn_budget > 0 && new_turns >= goal.turn_budget)
            || (goal.wall_clock_budget_ms > 0 && new_wall >= goal.wall_clock_budget_ms);
        if budget_exceeded {
            new_status = GoalStatus::BudgetLimited;
        }

        let completed_at = if new_status.is_terminal() {
            Some(crate::messages::now_iso_timestamp())
        } else {
            None
        };

        let id = goal.id.clone();
        self.with_conn(|conn| async move {
            with_write_transaction(&conn, || async {
                let changed = conn
                    .execute(
                        "UPDATE goals
                         SET tokens_used = ?, turns_used = ?, wall_clock_ms = ?,
                             status = ?, completed_at = COALESCE(?, completed_at)
                         WHERE id = ? AND status = 'active'",
                        turso::params![
                            new_tokens,
                            new_turns,
                            new_wall,
                            new_status.as_str(),
                            completed_at.clone(),
                            id.as_str(),
                        ],
                    )
                    .await?;
                if changed == 0 {
                    bail!("goal was changed or completed by another process");
                }
                Ok::<(), anyhow::Error>(())
            })
            .await
        })
        .await?;

        self.get_latest_goal(session_id).await
    }
}

fn row_to_goal(row: &turso::Row) -> Result<Goal> {
    let status_str: String = row.get(4)?;
    let status = GoalStatus::parse(&status_str).context("invalid goal status in database")?;
    Ok(Goal {
        id: row.get(0)?,
        session_id: row.get(1)?,
        objective: row.get(2)?,
        completion_criterion: row.get(3)?,
        status,
        turns_used: row.get(5)?,
        tokens_used: row.get(6)?,
        wall_clock_ms: row.get(7)?,
        wall_clock_budget_ms: row.get(8)?,
        turn_budget: row.get(9)?,
        token_budget: row.get(10)?,
        created_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clear_goal_handles_lock_error_gracefully() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("test.db");
        let store = GoalStore::new(db_path.clone());

        // Initialize database with goals table
        crate::datastore::with_conn(&db_path, |conn| async move {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS goals (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    objective TEXT NOT NULL,
                    completion_criterion TEXT,
                    status TEXT NOT NULL DEFAULT 'active',
                    turns_used INTEGER NOT NULL DEFAULT 0,
                    tokens_used INTEGER NOT NULL DEFAULT 0,
                    wall_clock_ms INTEGER NOT NULL DEFAULT 0,
                    wall_clock_budget_ms INTEGER NOT NULL DEFAULT 0,
                    turn_budget INTEGER NOT NULL DEFAULT 0,
                    token_budget INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    completed_at TEXT
                ) STRICT",
                (),
            )
            .await
            .map_err(|e| anyhow::anyhow!("create goals table: {e}"))
        })
        .await
        .expect("create goals table");

        // Create a goal
        let session_id = "test-session";
        store
            .create_goal(session_id, "test objective", None, 100, 10, 60000)
            .await
            .expect("create goal");

        // Verify goal exists
        let goal = store.get_latest_goal(session_id).await.expect("get goal");
        assert!(goal.is_some(), "goal should exist");

        // Test that lock error detection works
        let lock_error = anyhow::anyhow!("database is locked");
        let is_lock = is_lock_err(&lock_error.to_string());
        assert!(is_lock, "should detect lock error");

        // Test that clear_goal works normally (no lock)
        let result = store.clear_goal(session_id).await;
        assert!(result.is_ok(), "clear should succeed when no lock");

        // Verify goal is cleared
        let goal = store.get_latest_goal(session_id).await.expect("get goal");
        assert!(goal.is_none(), "goal should be cleared");
    }
}
