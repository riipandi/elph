use std::path::PathBuf;

use clap::Args;

use super::style::{self, CliStyle, S_MUTED, S_OK};
use crate::agent::{SessionManager, load_session_tree_jsonl};
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};

#[derive(Args)]
pub struct ImportArgs {
    /// Path to session JSONL (`/export` format: one SessionTreeEntry per line)
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// List entry count / preview without importing
    #[arg(long)]
    pub list: bool,

    /// Emit NDJSON summary to stdout
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &ImportArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let Some(file) = args.file.as_deref() else {
        let mut out = String::new();
        style::section(&mut out, sty, "Import");
        style::info(
            &mut out,
            sty,
            sty.paint(
                S_MUTED,
                "Usage: elph import <path.jsonl> [--list] [--json]\n\
                 Imports Elph session JSONL (from /export) into a new session for the current project.",
            ),
        );
        print!("{out}");
        return EXIT_ERROR;
    };

    let path = PathBuf::from(file);
    if !path.is_file() {
        let mut out = String::new();
        style::section(&mut out, sty, "Import");
        style::warn(&mut out, sty, &format!("file not found: {}", path.display()));
        eprint!("{out}");
        return EXIT_ERROR;
    }

    let entries = match load_session_tree_jsonl(&path) {
        Ok(e) => e,
        Err(e) => {
            let mut out = String::new();
            style::section(&mut out, sty, "Import");
            style::warn(&mut out, sty, &format!("{e:#}"));
            eprint!("{out}");
            return EXIT_ERROR;
        }
    };

    if args.list {
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "file": path.display().to_string(),
                    "entries": entries.len(),
                })
            );
        } else {
            let mut out = String::new();
            style::section(&mut out, sty, "Import (list)");
            style::kv(&mut out, sty, "File", &path.display().to_string());
            style::kv(&mut out, sty, "Entries", &entries.len().to_string());
            print!("{out}");
        }
        return EXIT_SUCCESS;
    }

    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(e) => {
            let mut out = String::new();
            style::warn(&mut out, sty, &format!("paths: {e:#}"));
            eprint!("{out}");
            return EXIT_ERROR;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| paths.project_dir().clone());
    let manager = match SessionManager::new(&paths, &cwd) {
        Ok(m) => m,
        Err(e) => {
            let mut out = String::new();
            style::warn(&mut out, sty, &format!("session manager: {e:#}"));
            eprint!("{out}");
            return EXIT_ERROR;
        }
    };

    let result = elph_agent::try_block_on(async { manager.import_from_jsonl(&path).await });
    match result {
        Ok(Ok((id, n))) => {
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "session_id": id,
                        "entries": n,
                        "file": path.display().to_string(),
                    })
                );
            } else {
                let mut out = String::new();
                style::section(&mut out, sty, "Import");
                style::info(&mut out, sty, sty.paint(S_OK, "Session imported"));
                style::kv(&mut out, sty, "File", &path.display().to_string());
                style::kv(&mut out, sty, "Entries", &n.to_string());
                style::kv(&mut out, sty, "Session", &id);
                style::info(&mut out, sty, sty.paint(S_MUTED, &format!("Resume: elph --resume {id}")));
                print!("{out}");
            }
            EXIT_SUCCESS
        }
        Ok(Err(e)) => {
            let mut out = String::new();
            style::warn(&mut out, sty, &format!("import failed: {e:#}"));
            eprint!("{out}");
            EXIT_ERROR
        }
        Err(e) => {
            let mut out = String::new();
            style::warn(&mut out, sty, &format!("import failed: {e}"));
            eprint!("{out}");
            EXIT_ERROR
        }
    }
}
