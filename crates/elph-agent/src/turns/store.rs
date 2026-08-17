//! Turso-backed session turn persistence and session rollups.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;
use crate::session::id::create_turn_id;

use super::types::{TurnRecord, TurnStatus, TurnUsage};

const TURN_COLUMNS: &str = "id, session_id, turn_index, status, operation_id,
    started_at, finished_at, wall_clock_ms, provider_id, model_id, thinking_level, agent_mode,
    input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, cost,
    user_entry_id, assistant_entry_id, error_message";

#[derive(Clone)]
pub struct TurnStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl TurnStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

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
                .with_context(|| format!("open turn database {}", self.db_path.display())),
        }
    }

    /// Start a new turn; returns the turn id and index.
    pub async fn start_turn(
        &self,
        session_id: &str,
        operation_id: Option<&str>,
        provider_id: Option<&str>,
        model_id: Option<&str>,
        thinking_level: Option<&str>,
    ) -> Result<TurnRecord> {
        let started_at = now_iso_timestamp();
        let id = create_turn_id();

        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT COALESCE(MAX(turn_index), -1) + 1 FROM session_turns WHERE session_id = ?",
                    turso::params![session_id],
                )
                .await?;
            let turn_index: i64 = if let Some(row) = rows.next().await? {
                row.get(0)?
            } else {
                0
            };
            while rows.next().await?.is_some() {}

            conn.execute(
                "INSERT INTO session_turns (
                    id, session_id, turn_index, status, operation_id,
                    started_at, finished_at, wall_clock_ms, provider_id, model_id, thinking_level, agent_mode,
                    input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, cost,
                    user_entry_id, assistant_entry_id, error_message
                 ) VALUES (?, ?, ?, 'started', ?, ?, NULL, 0, ?, ?, ?, NULL, 0, 0, 0, 0, 0, 0, NULL, NULL, NULL)",
                turso::params![
                    id.as_str(),
                    session_id,
                    turn_index,
                    operation_id,
                    started_at.as_str(),
                    provider_id,
                    model_id,
                    thinking_level,
                ],
            )
            .await?;

            Ok(TurnRecord {
                id,
                session_id: session_id.to_string(),
                turn_index,
                status: TurnStatus::Started,
                operation_id: operation_id.map(str::to_string),
                started_at,
                finished_at: None,
                wall_clock_ms: 0,
                provider_id: provider_id.map(str::to_string),
                model_id: model_id.map(str::to_string),
                thinking_level: thinking_level.map(str::to_string),
                agent_mode: None,
                usage: TurnUsage::default(),
                user_entry_id: None,
                assistant_entry_id: None,
                error_message: None,
            })
        })
        .await
    }

    /// Finish a turn and update session rollups when completed successfully.
    #[allow(clippy::too_many_arguments)] // turn completion fields map to columns
    pub async fn finish_turn(
        &self,
        turn_id: &str,
        status: TurnStatus,
        usage: TurnUsage,
        wall_clock_ms: i64,
        user_entry_id: Option<&str>,
        assistant_entry_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<TurnRecord> {
        let finished_at = now_iso_timestamp();
        self.with_conn(|conn| async move {
            conn.execute("BEGIN IMMEDIATE", ()).await?;
            let outcome = async {
                conn.execute(
                    "UPDATE session_turns SET
                        status = ?, finished_at = ?, wall_clock_ms = ?,
                        input_tokens = ?, output_tokens = ?, cache_read_tokens = ?,
                        cache_write_tokens = ?, total_tokens = ?, cost = ?,
                        user_entry_id = COALESCE(?, user_entry_id),
                        assistant_entry_id = COALESCE(?, assistant_entry_id),
                        error_message = ?
                     WHERE id = ?",
                    turso::params![
                        status.as_str(),
                        finished_at.as_str(),
                        wall_clock_ms,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_read_tokens,
                        usage.cache_write_tokens,
                        usage.total_tokens,
                        usage.cost,
                        user_entry_id,
                        assistant_entry_id,
                        error_message,
                        turn_id,
                    ],
                )
                .await?;

                if status == TurnStatus::Completed {
                    // Roll up into sessions from this turn's usage delta.
                    conn.execute(
                        "UPDATE sessions SET
                            turn_count = turn_count + 1,
                            total_input_tokens = total_input_tokens + ?,
                            total_output_tokens = total_output_tokens + ?,
                            total_cache_read_tokens = total_cache_read_tokens + ?,
                            total_cache_write_tokens = total_cache_write_tokens + ?,
                            total_tokens = total_tokens + ?,
                            total_cost = total_cost + ?,
                            last_turn_at = ?,
                            updated_at = ?
                         WHERE id = (SELECT session_id FROM session_turns WHERE id = ?)",
                        turso::params![
                            usage.input_tokens,
                            usage.output_tokens,
                            usage.cache_read_tokens,
                            usage.cache_write_tokens,
                            usage.total_tokens,
                            usage.cost,
                            finished_at.as_str(),
                            finished_at.as_str(),
                            turn_id,
                        ],
                    )
                    .await?;
                } else {
                    conn.execute(
                        "UPDATE sessions SET last_turn_at = ?, updated_at = ?
                         WHERE id = (SELECT session_id FROM session_turns WHERE id = ?)",
                        turso::params![finished_at.as_str(), finished_at.as_str(), turn_id],
                    )
                    .await?;
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            match outcome {
                Ok(()) => {
                    conn.execute("COMMIT", ()).await?;
                }
                Err(err) => {
                    let _ = conn.execute("ROLLBACK", ()).await;
                    return Err(err);
                }
            }
            load_turn(&conn, turn_id).await
        })
        .await
    }

    pub async fn get_turn(&self, turn_id: &str) -> Result<Option<TurnRecord>> {
        self.with_conn(|conn| async move {
            match load_turn(&conn, turn_id).await {
                Ok(t) => Ok(Some(t)),
                Err(_) => Ok(None),
            }
        })
        .await
    }

    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnRecord>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {TURN_COLUMNS} FROM session_turns
                         WHERE session_id = ?
                         ORDER BY turn_index ASC"
                    ),
                    turso::params![session_id],
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row_to_turn(&row)?);
            }
            Ok(out)
        })
        .await
    }

    /// Latest turn record for a session (highest `turn_index`), when any exists.
    pub async fn latest_turn(&self, session_id: &str) -> Result<Option<TurnRecord>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {TURN_COLUMNS} FROM session_turns
                         WHERE session_id = ?
                         ORDER BY turn_index DESC LIMIT 1"
                    ),
                    turso::params![session_id],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            let turn = row_to_turn(&row)?;
            while rows.next().await?.is_some() {}
            Ok(Some(turn))
        })
        .await
    }
}

async fn load_turn(conn: &Connection, turn_id: &str) -> Result<TurnRecord> {
    let mut rows = conn
        .query(
            &format!("SELECT {TURN_COLUMNS} FROM session_turns WHERE id = ?"),
            turso::params![turn_id],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        anyhow::bail!("turn not found: {turn_id}");
    };
    let turn = row_to_turn(&row)?;
    while rows.next().await?.is_some() {}
    Ok(turn)
}

fn row_to_turn(row: &turso::Row) -> Result<TurnRecord> {
    Ok(TurnRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        turn_index: row.get(2)?,
        status: TurnStatus::parse(&row.get::<String>(3)?).unwrap_or(TurnStatus::Started),
        operation_id: row.get(4)?,
        started_at: row.get(5)?,
        finished_at: row.get(6)?,
        wall_clock_ms: row.get(7)?,
        provider_id: row.get(8)?,
        model_id: row.get(9)?,
        thinking_level: row.get(10)?,
        agent_mode: row.get(11)?,
        usage: TurnUsage {
            input_tokens: row.get(12)?,
            output_tokens: row.get(13)?,
            cache_read_tokens: row.get(14)?,
            cache_write_tokens: row.get(15)?,
            total_tokens: row.get(16)?,
            cost: row.get(17)?,
        },
        user_entry_id: row.get(18)?,
        assistant_entry_id: row.get(19)?,
        error_message: row.get(20)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::ensure_database;
    use crate::session::migrations::SESSION_TREE_MIGRATIONS;

    #[tokio::test]
    async fn start_finish_updates_rollup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("store.db");
        ensure_database(&db, &SESSION_TREE_MIGRATIONS).await.expect("migrate");
        let open = crate::datastore::open_local(&db).await.expect("open");
        let c = crate::datastore::connect(&open).await.expect("connect");
        c.execute(
            "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES ('s1', 't', 't', '/tmp')",
            (),
        )
        .await
        .expect("session");

        let store = TurnStore::new(&db);
        let started = store
            .start_turn("s1", Some("op1"), Some("openai"), Some("gpt"), Some("high"))
            .await
            .expect("start");
        assert_eq!(started.turn_index, 0);
        assert_eq!(started.status, TurnStatus::Started);

        let finished = store
            .finish_turn(
                &started.id,
                TurnStatus::Completed,
                TurnUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cache_read_tokens: 1,
                    cache_write_tokens: 0,
                    total_tokens: 16,
                    cost: 0.01,
                },
                100,
                None,
                None,
                None,
            )
            .await
            .expect("finish");
        assert_eq!(finished.status, TurnStatus::Completed);
        assert_eq!(finished.usage.total_tokens, 16);

        let mut rows = c
            .query("SELECT turn_count, total_tokens, total_cost FROM sessions WHERE id = 's1'", ())
            .await
            .expect("q");
        let row = rows.next().await.expect("n").expect("r");
        let turn_count: i64 = row.get(0).expect("tc");
        let total_tokens: i64 = row.get(1).expect("tt");
        let total_cost: f64 = row.get(2).expect("cost");
        assert_eq!(turn_count, 1);
        assert_eq!(total_tokens, 16);
        assert!((total_cost - 0.01).abs() < 1e-9);

        let latest = store.latest_turn("s1").await.expect("latest");
        let latest = latest.expect("record");
        assert_eq!(latest.id, finished.id);
        assert_eq!(latest.usage.input_tokens, 10);
        assert_eq!(latest.usage.cache_read_tokens, 1);
        assert_eq!(latest.provider_id.as_deref(), Some("openai"));
        assert_eq!(latest.model_id.as_deref(), Some("gpt"));
        assert_eq!(store.latest_turn("no-such-session").await.expect("none"), None);
    }
}
