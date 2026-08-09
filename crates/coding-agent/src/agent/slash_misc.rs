//! Plain-text formatters for slash commands that open scroll dialogs or print status.

use std::path::Path;
use std::sync::Arc;

use elph_agent::ForkEntriesOptions;

use super::CodingAgentSession;
use super::overlays::{list_session_select_items, list_tree_select_items};
use crate::platform::Paths;
use crate::utils::path::AppPaths;

pub const HOTKEYS_TEXT: &str = "\
Keyboard shortcuts
──────────────────
  Enter              Submit prompt
  Shift+Enter        Newline in prompt
  Esc                Cancel / close dialog
  Tab                Complete slash / cycle agent mode
  Ctrl+P             Cycle scoped models
  Ctrl+S             Toggle native text-select mode
  Shift+↑/↓          Scroll transcript
  PageUp / PageDown  Scroll transcript (faster)
  Ctrl+C / Ctrl+D    Interrupt / quit (context-dependent)
  /help              List slash commands
  /hotkeys           This list
  /workers           Live multi-worker peers
  /resume [id]       List or switch sessions
  /session           Current session info
  /tools             Active tools
  /system-prompt     Compiled system prompt
";

pub fn changelog_text() -> String {
    let candidates = [
        Path::new("CHANGELOG.md"),
        Path::new("docs/CHANGELOG.md"),
        Path::new("../CHANGELOG.md"),
    ];
    for path in candidates {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let max = 12_000usize;
                if trimmed.chars().count() > max {
                    let truncated: String = trimmed.chars().take(max).collect();
                    return format!("{truncated}\n\n… (truncated)");
                }
                return trimmed.to_string();
            }
        }
    }
    "Elph changelog\n──────────────\n\
     See CHANGELOG.md in the repository or project release notes.\n\
     Tip: elph --version"
        .into()
}

pub fn settings_slash_message(paths: &Paths) -> String {
    let home = paths.config_dir().join("settings.json");
    let project = paths.project_settings_path();
    format!(
        "Settings\n\
         ────────\n\
         Home:    {}\n\
         Project: {} {}\n\
         \n\
         Edit JSON with your editor, then `/reload` in the TUI (or restart).\n\
         Key groups: ui, models, memory, workers, session.retention, codegraph, notifications.\n\
         \n\
         Workers example:\n\
           \"workers\": {{ \"enabled\": true, \"name\": null, \"tuiShowPeers\": true }}\n\
         \n\
         Tip: project settings override home per key.",
        home.display(),
        project.display(),
        if project.exists() { "(present)" } else { "(optional)" },
    )
}

pub async fn workers_slash_message(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    let Some(session) = session else {
        return Err("Agent session required for /workers.".into());
    };
    let Some(name) = session.worker_name() else {
        return Ok(
            "Workers\n───────\nMulti-worker coordination is off or not registered for this session.\n\
             Enable with settings workers.enabled (default true) and restart the session."
                .into(),
        );
    };
    let live = session.worker_live_count();
    let peer_summary = match session.worker_runtime.as_ref() {
        Some(rt) => rt.peer_names_summary().await,
        None => String::new(),
    };
    let mut lines = vec![
        "Workers".into(),
        "───────".into(),
        format!("  Self              {name}"),
        format!("  Live count        {live}"),
    ];
    if peer_summary.is_empty() {
        lines.push("  Peers             (none — you are alone)".into());
    } else {
        lines.push(format!("  Peers             {peer_summary}"));
    }
    lines.push(String::new());
    lines.push("Tools: worker_list · worker_send · worker_ask · worker_get · worker_await".into());
    lines.push("Tip: peers share .elph/store.db; file claims protect parallel edits.".into());
    Ok(lines.join("\n"))
}

pub async fn resume_list_message(session: &CodingAgentSession) -> Result<String, String> {
    let sm = session.session_manager();
    let items = list_session_select_items(sm)
        .await
        .map_err(|e| format!("list sessions: {e:#}"))?;
    let current = session.session_id();
    let mut lines = vec![
        "Sessions (this project)".into(),
        "──────────────────────".into(),
        format!("  Current   {current}"),
        String::new(),
    ];
    if items.is_empty() {
        lines.push("  (no other sessions)".into());
    } else {
        for (i, item) in items.iter().take(40).enumerate() {
            let mark = if item.value == current { "●" } else { " " };
            let desc = item.description.as_deref().unwrap_or("");
            lines.push(format!(
                "{mark} {:2}. {}  {}",
                i + 1,
                item.label,
                if desc.is_empty() { "" } else { desc }
            ));
            if item.value != item.label {
                lines.push(format!("       id  {}", item.value));
            }
        }
        if items.len() > 40 {
            lines.push(format!("  … and {} more", items.len() - 40));
        }
    }
    lines.push(String::new());
    lines.push("Switch: /resume <session_id>   (reloads TUI on that session)".into());
    lines.push("CLI:    elph --resume <id> · elph --continue".into());
    Ok(lines.join("\n"))
}

pub async fn tree_list_message(session: &CodingAgentSession) -> Result<String, String> {
    let entries = session
        .branch_entries()
        .await
        .map_err(|e| format!("branch entries: {e:#}"))?;
    let items = list_tree_select_items(&entries);
    let mut lines = vec![
        "Session tree (current branch)".into(),
        "─────────────────────────────".into(),
        format!("  Entries  {}", items.len()),
        String::new(),
    ];
    if items.is_empty() {
        lines.push("  (empty branch)".into());
    } else {
        for (i, item) in items.iter().take(60).enumerate() {
            let desc = item.description.as_deref().unwrap_or("");
            lines.push(format!("{:3}. {}", i + 1, item.label));
            if !desc.is_empty() {
                lines.push(format!("      {desc}"));
            }
        }
        if items.len() > 60 {
            lines.push(format!("  … and {} more", items.len() - 60));
        }
    }
    lines.push(String::new());
    lines.push("Tip: the TUI follows the active leaf; use /resume to open another session.".into());
    Ok(lines.join("\n"))
}

pub async fn export_session_message(session: &CodingAgentSession, cwd: &Path, args: &str) -> Result<String, String> {
    let out = args.trim();
    let sid = session.session_id();
    let short = sid.chars().take(8).collect::<String>();
    let path = if out.is_empty() {
        cwd.join(format!("elph-session-{short}.jsonl"))
    } else {
        let p = Path::new(out);
        if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
    };
    let entries = session
        .branch_entries()
        .await
        .map_err(|e| format!("read branch: {e:#}"))?;
    let mut body = String::new();
    for entry in &entries {
        let line = serde_json::to_string(entry).map_err(|e| format!("serialize: {e}"))?;
        body.push_str(&line);
        body.push('\n');
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(format!(
        "Exported {} entries → {}\nTip: restore via store tools or /resume after import.",
        entries.len(),
        path.display()
    ))
}

pub fn import_slash_message(args: &str) -> String {
    let path = args.trim();
    if path.is_empty() {
        "Import session JSONL\n\
         ───────────────────\n\
         Usage: /import <path.jsonl>\n\
         \n\
         Mid-TUI import would replace the live session. Prefer exporting with\n\
         /export, then starting a forked session (/clone) or CLI resume after restore."
            .into()
    } else {
        format!(
            "Import from `{path}` is not applied mid-TUI (would replace the live session).\n\
             Current session is unchanged. Use /export, /clone, or elph --resume <id>."
        )
    }
}

pub fn trust_slash_message(cwd: &Path) -> Result<String, String> {
    let trust_dir = cwd.join(".elph");
    let trust_file = trust_dir.join("trusted");
    std::fs::create_dir_all(&trust_dir).map_err(|e| format!("mkdir .elph: {e}"))?;
    std::fs::write(&trust_file, b"1\n").map_err(|e| format!("write trust: {e}"))?;
    Ok(format!(
        "Project trusted\n\
         ───────────────\n\
         Wrote {}\n\
         Elph will treat this workspace as trusted for local automation defaults.",
        trust_file.display()
    ))
}

pub async fn fork_session_message(session: &CodingAgentSession) -> Result<String, String> {
    let sm = session.session_manager();
    let source_id = session.session_id().to_string();
    let forked = sm
        .fork_session(&source_id, ForkEntriesOptions::default())
        .await
        .map_err(|e| format!("fork failed: {e:#}"))?;
    use elph_agent::session::types::HasSessionId;
    let new_id = forked.metadata().await.session_id().to_string();
    Ok(format!(
        "Session forked\n\
         ─────────────\n\
         Source  {source_id}\n\
         New     {new_id}\n\
         \n\
         Switch: /resume {new_id}"
    ))
}

pub async fn clone_session_message(session: &CodingAgentSession) -> Result<String, String> {
    fork_session_message(session)
        .await
        .map(|s| s.replacen("forked", "cloned", 1))
}
