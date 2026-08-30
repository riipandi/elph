use std::env;

use clap::{Parser, Subcommand};

use super::help;
use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_MUTED};
use crate::agent::SessionManager;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};
use crate::utils::path::AppPaths;

#[derive(Parser, Default)]
#[command(
    name = "session",
    about = "Manage coding-agent sessions",
    color = clap::ColorChoice::Auto
)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: Option<SessionCommands>,
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List recent sessions (same as search with no query)
    List,
    /// Search sessions by keyword
    Search {
        /// Search query to filter sessions
        query: Option<String>,
    },
    /// Permanently delete a session from history
    Delete {
        /// Session ID to delete
        id: String,
    },
    /// Pin a session so automatic retention GC will not delete it
    Pin {
        /// Session ID to pin
        id: String,
    },
    /// Remove pin from a session
    Unpin {
        /// Session ID to unpin
        id: String,
    },
    /// Run session retention GC using settings (`session`)
    Prune {
        /// Report candidates only; do not delete
        #[arg(long)]
        dry_run: bool,
        /// Prune every policy-eligible session, including the latest session of
        /// each project (by default the most recent session per project is kept).
        #[arg(long)]
        all: bool,
    },
}

pub fn handle(args: &SessionArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<SessionArgs>();
    };

    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };
    if let Err(err) = crate::platform::ensure_datastore_blocking(&paths) {
        help::cli_error(format!("init datastore: {err}"));
        return EXIT_ERROR;
    }
    let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
    let manager = match SessionManager::new(&paths, &cwd) {
        Ok(manager) => manager,
        Err(err) => {
            help::cli_error(format!("init session manager: {err}"));
            return EXIT_ERROR;
        }
    };

    match cmd {
        SessionCommands::List => list_sessions(&manager, &cwd, None),
        SessionCommands::Search { query } => list_sessions(&manager, &cwd, query.as_deref()),
        SessionCommands::Delete { id } => {
            match elph_agent::runtime::block_on(async {
                let sessions = manager.list().await?;
                let meta = sessions
                    .into_iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
                manager.delete(&meta).await
            }) {
                Ok(()) => {
                    log::info!("session deleted id={id}");
                    let mut out = String::new();
                    style::success(&mut out, CliStyle::auto(), format!("Deleted session {id}"));
                    print!("{out}");
                    EXIT_SUCCESS
                }
                Err(err) => {
                    help::cli_error(format!("delete session: {err}"));
                    EXIT_ERROR
                }
            }
        }
        SessionCommands::Pin { id } => pin_session(&paths, id, true),
        SessionCommands::Unpin { id } => pin_session(&paths, id, false),
        SessionCommands::Prune { dry_run, all } => prune_sessions(&paths, *dry_run, *all),
    }
}

fn pin_session(paths: &Paths, id: &str, pinned: bool) -> ExitCode {
    match elph_agent::runtime::block_on(async {
        let db = crate::platform::datastore::ensure_database(paths).await?;
        elph_agent::session::set_session_pinned(&db, id, pinned).await
    }) {
        Ok(()) => {
            let mut out = String::new();
            let verb = if pinned { "Pinned" } else { "Unpinned" };
            style::success(&mut out, CliStyle::auto(), format!("{verb} session {id}"));
            print!("{out}");
            log::info!("session {} id={id}", if pinned { "pinned" } else { "unpinned" });
            EXIT_SUCCESS
        }
        Err(err) => {
            help::cli_error(format!("pin session: {err}"));
            EXIT_ERROR
        }
    }
}

fn prune_sessions(paths: &Paths, dry_run: bool, all: bool) -> ExitCode {
    match elph_agent::runtime::block_on(async {
        let settings = crate::platform::Settings::load(paths)?;
        let r = &settings.session;
        if !r.enabled && !dry_run {
            anyhow::bail!("session.enabled is false (enable in settings.json or use --dry-run)");
        }
        let db = std::sync::Arc::new(crate::platform::datastore::ensure_database(paths).await?);
        let policy = elph_agent::session::RetentionPolicy {
            enabled: true, // CLI prune always plans; dry_run controls delete
            max_sessions_per_cwd: r.max_sessions_per_cwd,
            max_session_age_days: r.max_session_age_days,
            max_store_db_bytes: r.max_store_db_bytes,
            // `--all` also prunes the most recent session of each project
            // (by default the latest per cwd is kept).
            protect_latest_per_cwd: r.protect_latest_per_cwd && !all,
            protect_session_id: None,
        };
        elph_agent::session::run_full_session_gc(
            db,
            paths.memory_db_path(),
            Some(paths.data_dir().join("sessions")),
            policy,
            dry_run,
        )
        .await
    }) {
        Ok(report) => {
            let sty = CliStyle::auto();
            let mut out = String::new();
            style::section(
                &mut out,
                sty,
                if dry_run {
                    "Session prune (dry-run)"
                } else {
                    "Session prune"
                },
            );
            style::kv(&mut out, sty, "Examined", report.examined.to_string());
            style::kv(&mut out, sty, "Deleted", report.deleted_ids.len().to_string());
            style::kv(&mut out, sty, "Skipped pinned", report.skipped_pinned.to_string());
            for id in &report.deleted_ids {
                style::info(&mut out, sty, format!("  - {id}"));
            }
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(err) => {
            help::cli_error(format!("prune sessions: {err}"));
            EXIT_ERROR
        }
    }
}

fn list_sessions(manager: &SessionManager, cwd: &std::path::Path, query: Option<&str>) -> ExitCode {
    match elph_agent::runtime::block_on(manager.list()) {
        Ok(sessions) => {
            let sessions: Vec<_> = match query {
                Some(q) if !q.trim().is_empty() => {
                    let q = q.to_lowercase();
                    sessions
                        .into_iter()
                        .filter(|s| {
                            s.id.to_lowercase().contains(&q)
                                || s.cwd.to_lowercase().contains(&q)
                                || s.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&q))
                        })
                        .collect()
                }
                _ => sessions,
            };
            let sty = CliStyle::auto();
            let mut out = String::new();
            if sessions.is_empty() {
                style::info(
                    &mut out,
                    sty,
                    sty.paint(S_MUTED, format!("No sessions found for {}", cwd.display())),
                );
                style::tip(
                    &mut out,
                    sty,
                    "Sessions are created automatically when you start a conversation.",
                );
            } else {
                style::section(&mut out, sty, &format!("Sessions ({})", sessions.len()));
                use std::fmt::Write;
                let _ = writeln!(out);
                for meta in &sessions {
                    let name = meta.name.as_deref().unwrap_or("(untitled)");
                    let _ = writeln!(
                        out,
                        "  {}  {}",
                        sty.paint(S_ACCENT, &meta.id[..8.min(meta.id.len())]),
                        sty.paint(S_BODY, name),
                    );
                    let _ = writeln!(
                        out,
                        "   {}  created {}  ·  updated {}  ·  {}",
                        sty.paint(S_MUTED, "·"),
                        sty.paint(S_MUTED, &meta.created_at[..10.min(meta.created_at.len())]),
                        sty.paint(S_MUTED, &meta.updated_at[..10.min(meta.updated_at.len())]),
                        sty.paint(S_MUTED, &meta.cwd),
                    );
                }
            }
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(err) => {
            help::cli_error(format!("list sessions: {err}"));
            EXIT_ERROR
        }
    }
}
