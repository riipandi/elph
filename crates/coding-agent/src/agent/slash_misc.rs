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
  :q / :q!           Quit (force-quit mid-turn)
  /help              List slash commands
  /aside <question>  Side question (does not interrupt the main turn)
  /hotkeys           This list
  /intercom [peer]   Open threaded worker chat (Alt+M)
  /workers           Live multi-worker peers
  /resume [id]       Interactive session picker (or switch by id)
  /tree [id]         Interactive tree navigate (or jump by entry id)
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
         Key groups: ui, models, resources, memory, workers, session, notifications.\n\
         Also top-level: defaultTools, shellPath, httpProxy, quietStartup.\n\
         MCP cache lives in mcp.json; project trust in trust.json.\n\
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
    lines.push(
        "Tools: worker_list · worker_send · worker_reply · worker_ask · worker_get · worker_await · worker_pending"
            .into(),
    );
    lines.push("Chat: Alt+M or /intercom — threaded 1:1 messaging (answers in parallel with your turn).".into());
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

/// List the full session DAG (Pi `/tree` inspect view) and how to jump.
pub async fn tree_list_message(session: &CodingAgentSession) -> Result<String, String> {
    let entries = session
        .session_tree_entries()
        .await
        .map_err(|e| format!("session entries: {e:#}"))?;
    let leaf = session.leaf_id().await.map_err(|e| format!("leaf: {e:#}"))?;
    let mut lines = vec![
        "Session tree".into(),
        "────────────".into(),
        format!("  Leaf     {}", leaf.as_deref().unwrap_or("(root)")),
        format!("  Entries  {}", entries.len()),
        String::new(),
    ];
    if entries.is_empty() {
        lines.push("  (empty session)".into());
    } else {
        let rendered = render_session_tree_lines(&entries, leaf.as_deref(), 80);
        lines.extend(rendered);
    }
    lines.push(String::new());
    lines.push("Navigate (Pi-style):".into());
    lines.push("  /tree <entry_id>           jump leaf to that entry".into());
    lines.push("  /tree <entry_id> --summary jump and summarize abandoned branch".into());
    lines.push("  /tree --branch             list only the active branch path".into());
    Ok(lines.join("\n"))
}

/// `/tree` with optional args: list, jump, jump+summary, or branch-only list.
pub async fn tree_slash_message(session: &CodingAgentSession, args: &str) -> Result<String, String> {
    let args = args.trim();
    if args.is_empty() {
        return tree_list_message(session).await;
    }
    if args == "--branch" || args == "branch" {
        return tree_branch_only_message(session).await;
    }

    let mut summarize = false;
    let mut target = None::<String>;
    for tok in args.split_whitespace() {
        match tok {
            "--summary" | "-s" | "summary" => summarize = true,
            t if t.starts_with('-') => {
                return Err(format!("unknown /tree flag: {t}"));
            }
            t => {
                if target.is_some() {
                    return Err("usage: /tree [entry_id] [--summary] | /tree --branch".into());
                }
                target = Some(t.to_string());
            }
        }
    }
    let Some(entry_id) = target else {
        return tree_list_message(session).await;
    };

    session
        .navigate_tree_to_with_options(&entry_id, summarize)
        .await
        .map_err(|e| format!("navigate failed: {e:#}"))?;

    Ok(format!(
        "Navigated to entry\n\
         ─────────────────\n\
         Target   {entry_id}\n\
         Summary  {}\n\
         \n\
         Context now follows this leaf. Continue chatting, or /tree to inspect.",
        if summarize { "requested" } else { "skipped" }
    ))
}

async fn tree_branch_only_message(session: &CodingAgentSession) -> Result<String, String> {
    let entries = session
        .branch_entries()
        .await
        .map_err(|e| format!("branch entries: {e:#}"))?;
    let items = list_tree_select_items(&entries);
    let mut lines = vec![
        "Active branch path".into(),
        "──────────────────".into(),
        format!("  Entries  {}", items.len()),
        String::new(),
    ];
    if items.is_empty() {
        lines.push("  (empty branch)".into());
    } else {
        for (i, item) in items.iter().take(80).enumerate() {
            let desc = item.description.as_deref().unwrap_or("");
            lines.push(format!("{:3}. {}  ({})", i + 1, item.label, item.value));
            if !desc.is_empty() {
                lines.push(format!("      {desc}"));
            }
        }
        if items.len() > 80 {
            lines.push(format!("  … and {} more", items.len() - 80));
        }
    }
    lines.push(String::new());
    lines.push("Jump: /tree <entry_id>   Full tree: /tree".into());
    Ok(lines.join("\n"))
}

fn render_session_tree_lines(
    entries: &[elph_agent::SessionTreeEntry],
    leaf_id: Option<&str>,
    max_rows: usize,
) -> Vec<String> {
    use std::collections::HashMap;
    let mut children: HashMap<Option<String>, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        children.entry(e.parent_id().map(str::to_string)).or_default().push(i);
    }
    let mut out = Vec::new();
    fn walk(
        entries: &[elph_agent::SessionTreeEntry],
        children: &HashMap<Option<String>, Vec<usize>>,
        parent: Option<&str>,
        prefix: &str,
        leaf_id: Option<&str>,
        out: &mut Vec<String>,
        max_rows: usize,
    ) {
        if out.len() >= max_rows {
            return;
        }
        let key = parent.map(str::to_string);
        let Some(idxs) = children.get(&key) else {
            return;
        };
        for (pos, &i) in idxs.iter().enumerate() {
            if out.len() >= max_rows {
                out.push(format!("  … truncated ({max_rows} rows)"));
                return;
            }
            let e = &entries[i];
            let is_last = pos + 1 == idxs.len();
            let branch = if is_last { "└─ " } else { "├─ " };
            let mark = if leaf_id == Some(e.id()) { "●" } else { " " };
            let label = tree_entry_label(e);
            let short_id: String = e.id().chars().take(8).collect();
            out.push(format!("{prefix}{branch}{mark} {short_id}  {label}"));
            let child_prefix = format!("{prefix}{}", if is_last { "   " } else { "│  " });
            walk(entries, children, Some(e.id()), &child_prefix, leaf_id, out, max_rows);
        }
    }
    walk(entries, &children, None, "", leaf_id, &mut out, max_rows);
    if out.is_empty() {
        // Orphan-only / missing roots — fall back to flat list.
        for e in entries.iter().take(max_rows) {
            let mark = if leaf_id == Some(e.id()) { "●" } else { " " };
            let short_id: String = e.id().chars().take(8).collect();
            out.push(format!("  {mark} {short_id}  {}", tree_entry_label(e)));
        }
    }
    out
}

fn tree_entry_label(entry: &elph_agent::SessionTreeEntry) -> String {
    use elph_agent::SessionTreeEntry;
    match entry {
        SessionTreeEntry::Message { message, .. } => {
            let role = message.role();
            let preview = match message {
                elph_agent::AgentMessage::Llm(msg) => match msg.as_ref() {
                    elph_ai::Message::User { content, .. } => match content {
                        elph_ai::UserContent::Text(t) => t.chars().take(48).collect::<String>(),
                        elph_ai::UserContent::Blocks(blocks) => blocks
                            .iter()
                            .filter_map(|b| match b {
                                elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                            .chars()
                            .take(48)
                            .collect(),
                    },
                    elph_ai::Message::Assistant(a) => a
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            elph_ai::AssistantContentBlock::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(48)
                        .collect(),
                    elph_ai::Message::ToolResult { tool_name, .. } => tool_name.clone(),
                },
                other => format!("{other:?}").chars().take(48).collect(),
            };
            let preview = preview.replace('\n', " ");
            format!("{role}: {preview}")
        }
        SessionTreeEntry::BranchSummary { summary, .. } => {
            format!("branch: {}", summary.chars().take(48).collect::<String>())
        }
        SessionTreeEntry::Compaction { .. } => "compaction".into(),
        SessionTreeEntry::ModelChange { provider, model_id, .. } => {
            format!("model: {provider}/{model_id}")
        }
        SessionTreeEntry::ThinkingLevelChange { thinking_level, .. } => {
            format!("thinking: {thinking_level}")
        }
        SessionTreeEntry::Label { label, .. } => {
            format!("label: {}", label.as_deref().unwrap_or("(cleared)"))
        }
        SessionTreeEntry::SessionInfo { name, .. } => {
            format!("session: {}", name.as_deref().unwrap_or("(unnamed)"))
        }
        SessionTreeEntry::Leaf { .. } => "leaf".into(),
        SessionTreeEntry::Custom { custom_type, .. } => format!("custom:{custom_type}"),
        SessionTreeEntry::CustomMessage { custom_type, .. } => format!("msg:{custom_type}"),
        SessionTreeEntry::ActiveToolsChange { .. } => "tools".into(),
        SessionTreeEntry::CollaborationModeChange { mode, .. } => format!("mode: {mode:?}"),
    }
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
    // Full session DAG (not only the active branch) so /import restores forks.
    let entries = session
        .session_tree_entries()
        .await
        .map_err(|e| format!("read entries: {e:#}"))?;
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
        "Exported {} entries → {}\nRestore: /import {}  (or: elph import {})",
        entries.len(),
        path.display(),
        path.display(),
        path.display()
    ))
}

/// Parse `/import` / `/export` path args like Pi (`"quoted paths"`, unquoted first token).
pub fn path_command_argument(args: &str) -> Option<String> {
    let args = args.trim_start();
    if args.is_empty() {
        return None;
    }
    let first = args.chars().next()?;
    if first == '"' || first == '\'' {
        let rest = &args[first.len_utf8()..];
        let end = rest.find(first)?;
        return Some(rest[..end].to_string());
    }
    let end = args.find(char::is_whitespace).unwrap_or(args.len());
    Some(args[..end].to_string())
}

fn resolve_import_path(cwd: &Path, input: &str) -> std::path::PathBuf {
    let p = Path::new(input);
    if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
}

/// Import JSONL into a new Turso session (Pi `/import` intent).
///
/// Returns `(status_message, new_session_id)` so the TUI can switch via resume.
pub async fn import_session_from_jsonl(
    session: &CodingAgentSession,
    cwd: &Path,
    args: &str,
) -> Result<(String, String), String> {
    let Some(input) = path_command_argument(args) else {
        return Err("Usage: /import <path.jsonl>\n\
             Imports exported session entries into a new session, then switch with /resume."
            .into());
    };
    let path = resolve_import_path(cwd, &input);
    if !path.is_file() {
        return Err(format!("Import file not found: {}", path.display()));
    }
    let sm = session.session_manager();
    let (new_id, n) = sm
        .import_from_jsonl(&path)
        .await
        .map_err(|e| format!("import failed: {e:#}"))?;
    Ok((
        format!(
            "Session imported\n\
             ───────────────\n\
             File     {}\n\
             Entries  {n}\n\
             New id   {new_id}\n\
             \n\
             Switching to the imported session…",
            path.display()
        ),
        new_id,
    ))
}

/// Usage / help when `/import` has no path (and non-async call sites).
pub fn import_slash_message(args: &str) -> String {
    if path_command_argument(args).is_none() {
        "Import session JSONL\n\
         ───────────────────\n\
         Usage: /import <path.jsonl>\n\
         \n\
         Loads an `/export` (or compatible) JSONL file into a **new** session for this\n\
         project, then switches the TUI to it (Pi-style import + resume).\n\
         Paths may be quoted when they contain spaces."
            .into()
    } else {
        // Handlers should call `import_session_from_jsonl`; keep a clear fallback.
        format!("Use /import with a live agent session to import `{}`.", args.trim())
    }
}

/// Trust the project cwd in `CONFIG_DIR/trust.json` (see docs/archive/configuration.md).
pub fn trust_slash_message(paths: &Paths, cwd: &Path) -> Result<String, String> {
    use crate::platform::scaffold::TrustStore;
    let key = TrustStore::trust_directory(paths, cwd).map_err(|e| format!("trust failed: {e:#}"))?;
    Ok(format!(
        "Project trusted\n\
         ───────────────\n\
         Directory  {key}\n\
         Store      {}\n\
         \n\
         Project-local extensions under .elph/extensions/ may load in trusted workspaces.",
        paths.trust_path().display()
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
