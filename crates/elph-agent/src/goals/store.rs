//! Turso-backed goal persistence.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use turso::{Builder, Connection};

use super::types::{Goal, GoalStatus};

const GOAL_COLUMNS: &str = "id, goal_id, session_id, objective, completion_criterion, status,
    turns_used, tokens_used, wall_clock_ms, wall_clock_budget_ms,
    turn_budget, token_budget, created_at, completed_at";

#[derive(Clone)]
pub struct GoalStore {
    db_path: PathBuf,
}

impl GoalStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn connection(&self) -> Result<Connection> {
        let db = Builder::new_local(self.db_path.to_string_lossy().as_ref())
            .build()
            .await
            .with_context(|| format!("open goal database {}", self.db_path.display()))?;
        db.connect().context("connect goal database")
    }

    pub async fn get_active_goal(&self, session_id: &str) -> Result<Option<Goal>> {
        let conn = self.connection().await?;
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
    }

    pub async fn get_latest_goal(&self, session_id: &str) -> Result<Option<Goal>> {
        let conn = self.connection().await?;
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
    }

    pub async fn has_unfinished_goal(&self, session_id: &str) -> Result<bool> {
        let conn = self.connection().await?;
        let mut rows = conn
            .query(
                "SELECT 1 FROM goals
                 WHERE session_id = ? AND status NOT IN ('complete')
                 ORDER BY id DESC LIMIT 1",
                turso::params![session_id],
            )
            .await?;
        Ok(rows.next().await?.is_some())
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

        let goal_id = crate::session::id::create_tsid();
        let conn = self.connection().await?;
        conn.execute(
            "INSERT INTO goals (
                goal_id, session_id, objective, completion_criterion, status,
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

        self.get_active_goal(session_id)
            .await?
            .context("goal created but not found")
    }

    pub async fn update_goal_status(&self, session_id: &str, status: GoalStatus) -> Result<Goal> {
        let Some(goal) = self.get_active_goal(session_id).await? else {
            bail!("no active goal for this session");
        };

        let completed_at = if status.is_terminal() {
            Some(crate::messages::now_iso_timestamp())
        } else {
            None
        };

        let conn = self.connection().await?;
        conn.execute(
            "UPDATE goals SET status = ?, completed_at = ? WHERE id = ?",
            turso::params![status.as_str(), completed_at, goal.id],
        )
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

        let conn = self.connection().await?;
        conn.execute(
            "UPDATE goals SET status = ?, completed_at = ? WHERE id = ?",
            turso::params![status.as_str(), completed_at, goal.id],
        )
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

        let conn = self.connection().await?;
        conn.execute(
            "UPDATE goals SET status = 'active', completed_at = NULL WHERE id = ?",
            turso::params![goal.id],
        )
        .await?;

        self.get_active_goal(session_id)
            .await?
            .context("goal resumed but not found")
    }

    pub async fn clear_goal(&self, session_id: &str) -> Result<()> {
        let conn = self.connection().await?;
        conn.execute("DELETE FROM goals WHERE session_id = ?", turso::params![session_id])
            .await?;
        Ok(())
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

        let conn = self.connection().await?;
        conn.execute(
            "UPDATE goals
             SET token_budget = ?, turn_budget = ?, wall_clock_budget_ms = ?,
                 status = CASE WHEN status = 'budget_limited' THEN 'active' ELSE status END
             WHERE id = ?",
            turso::params![token_budget, turn_budget, wall_clock_budget_ms, goal.id],
        )
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

        let conn = self.connection().await?;
        conn.execute(
            "UPDATE goals
             SET tokens_used = ?, turns_used = ?, wall_clock_ms = ?,
                 status = ?, completed_at = COALESCE(?, completed_at)
             WHERE id = ?",
            turso::params![
                new_tokens,
                new_turns,
                new_wall,
                new_status.as_str(),
                completed_at,
                goal.id,
            ],
        )
        .await?;

        self.get_latest_goal(session_id).await
    }
}

fn row_to_goal(row: &turso::Row) -> Result<Goal> {
    let status_str: String = row.get(5)?;
    let status = GoalStatus::parse(&status_str).context("invalid goal status in database")?;
    Ok(Goal {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        session_id: row.get(2)?,
        objective: row.get(3)?,
        completion_criterion: row.get(4)?,
        status,
        turns_used: row.get(6)?,
        tokens_used: row.get(7)?,
        wall_clock_ms: row.get(8)?,
        wall_clock_budget_ms: row.get(9)?,
        turn_budget: row.get(10)?,
        token_budget: row.get(11)?,
        created_at: row.get(12)?,
        completed_at: row.get(13)?,
    })
}
