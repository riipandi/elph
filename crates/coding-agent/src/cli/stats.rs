use clap::Args;

use super::style::{self, CliStyle, S_MUTED};
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings};

#[derive(Args, Default)]
pub struct StatsArgs {
    /// Filter statistics to a specific session
    #[arg(long, value_name = "SESSION_ID")]
    pub session: Option<String>,

    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &StatsArgs) -> ExitCode {
    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            super::help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };
    if let Err(err) = crate::platform::ensure_datastore_blocking(&paths) {
        super::help::cli_error(format!("init datastore: {err}"));
        return EXIT_ERROR;
    }

    match elph_agent::block_on(collect_stats(&paths, args.session.as_deref())) {
        Ok(report) => {
            if args.json {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => {
                        println!("{s}");
                        EXIT_SUCCESS
                    }
                    Err(err) => {
                        super::help::cli_error(format!("json: {err}"));
                        EXIT_ERROR
                    }
                }
            } else {
                print_human(&report);
                EXIT_SUCCESS
            }
        }
        Err(err) => {
            super::help::cli_error(format!("stats: {err}"));
            EXIT_ERROR
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct StatsReport {
    store_db: String,
    store_bytes: u64,
    sessions: usize,
    turns: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_tokens: i64,
    total_cost: f64,
    session_filter: Option<String>,
    per_session: Vec<SessionStatRow>,
}

#[derive(Debug, serde::Serialize)]
struct SessionStatRow {
    id: String,
    name: Option<String>,
    cwd: String,
    turn_count: i64,
    total_tokens: i64,
    total_cost: f64,
    last_turn_at: Option<String>,
}

async fn collect_stats(paths: &Paths, session_filter: Option<&str>) -> anyhow::Result<StatsReport> {
    let db_path = paths.memory_db_path();
    let store_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let database = crate::platform::datastore::ensure_database(paths).await?;
    let conn = elph_agent::datastore::connect(&database).await?;

    let mut sql = "SELECT id, name, COALESCE(cwd,''), turn_count, total_tokens, total_cost, last_turn_at
                   FROM sessions"
        .to_string();
    if session_filter.is_some() {
        sql.push_str(" WHERE id = ?");
    }
    sql.push_str(" ORDER BY updated_at DESC LIMIT 50");

    let mut rows = if let Some(id) = session_filter {
        conn.query(&sql, turso::params![id]).await?
    } else {
        conn.query(&sql, ()).await?
    };

    let mut per_session = Vec::new();
    let mut total_input = 0i64;
    let mut total_output = 0i64;
    let mut total_tokens = 0i64;
    let mut total_cost = 0.0f64;
    let mut turns = 0i64;

    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let name: Option<String> = row.get(1)?;
        let cwd: String = row.get(2)?;
        let turn_count: i64 = row.get(3).unwrap_or(0);
        let tok: i64 = row.get(4).unwrap_or(0);
        let cost: f64 = row.get(5).unwrap_or(0.0);
        let last_turn_at: Option<String> = row.get(6)?;
        turns += turn_count;
        total_tokens += tok;
        total_cost += cost;
        per_session.push(SessionStatRow {
            id,
            name,
            cwd,
            turn_count,
            total_tokens: tok,
            total_cost: cost,
            last_turn_at,
        });
    }

    // Prefer summing session_turns for input/output when available.
    let mut turn_q = if let Some(id) = session_filter {
        conn.query(
            "SELECT COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(total_tokens),0), COALESCE(SUM(cost),0), COUNT(*)
             FROM session_turns WHERE session_id = ?",
            turso::params![id],
        )
        .await?
    } else {
        conn.query(
            "SELECT COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(total_tokens),0), COALESCE(SUM(cost),0), COUNT(*)
             FROM session_turns",
            (),
        )
        .await?
    };
    if let Some(row) = turn_q.next().await? {
        total_input = row.get(0).unwrap_or(0);
        total_output = row.get(1).unwrap_or(0);
        let tt: i64 = row.get(2).unwrap_or(0);
        if tt > 0 {
            total_tokens = tt;
        }
        let tc: f64 = row.get(3).unwrap_or(0.0);
        if tc > 0.0 {
            total_cost = tc;
        }
        let tc_count: i64 = row.get(4).unwrap_or(0);
        if tc_count > 0 {
            turns = tc_count;
        }
    }

    let _ = Settings::load(paths); // ensure settings readable (future filters)

    Ok(StatsReport {
        store_db: db_path.display().to_string(),
        store_bytes,
        sessions: per_session.len(),
        turns,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_tokens,
        total_cost,
        session_filter: session_filter.map(str::to_string),
        per_session,
    })
}

fn print_human(report: &StatsReport) {
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Statistics");
    style::kv(&mut out, sty, "Store", &report.store_db);
    style::kv(&mut out, sty, "Store size", format_bytes(report.store_bytes));
    if let Some(id) = &report.session_filter {
        style::kv(&mut out, sty, "Session filter", id);
    }
    style::kv(&mut out, sty, "Sessions (listed)", report.sessions.to_string());
    style::kv(&mut out, sty, "Turns", report.turns.to_string());
    style::kv(&mut out, sty, "Input tokens", report.total_input_tokens.to_string());
    style::kv(&mut out, sty, "Output tokens", report.total_output_tokens.to_string());
    style::kv(&mut out, sty, "Total tokens", report.total_tokens.to_string());
    style::kv(&mut out, sty, "Total cost", format!("{:.6}", report.total_cost));
    if report.per_session.is_empty() {
        style::info(&mut out, sty, sty.paint(S_MUTED, "No sessions yet."));
    } else {
        style::section(&mut out, sty, "Recent sessions");
        for s in &report.per_session {
            let label = s.name.as_deref().unwrap_or(&s.id);
            style::info(
                &mut out,
                sty,
                format!(
                    "{label}  turns={}  tokens={}  cost={:.4}",
                    s.turn_count, s.total_tokens, s.total_cost
                ),
            );
        }
    }
    print!("{out}");
}

fn format_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let n = n as f64;
    if n < K {
        format!("{n:.0} B")
    } else if n < K * K {
        format!("{:.1} KiB", n / K)
    } else if n < K * K * K {
        format!("{:.1} MiB", n / (K * K))
    } else {
        format!("{:.2} GiB", n / (K * K * K))
    }
}
