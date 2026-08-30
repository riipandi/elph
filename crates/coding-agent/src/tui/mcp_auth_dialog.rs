//! `/mcp auth` dialog — OAuth login for remote MCP servers.
//!
//! Flow mirrors `/provider connect` at a smaller scale:
//!   1. **SelectServer** — pick a remote MCP server (fuzzy filter)
//!   2. **WaitingBrowser** — browser OAuth (PKCE) in progress
//!   3. **Done** / **Failed** — result; main tick closes on `done`

use std::time::Instant;

use elph_agent::mcp::{McpOAuthFlowOptions, McpServerConfig, has_stored_credentials, run_oauth_flow};
use elph_tui::components::UiTheme;
use elph_tui::components::select::select_window_start_for_rows;
use iocraft::prelude::*;

use crate::platform::Paths;
use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};
use crate::tui::slash_palette::fuzzy::{field_score, max_score};
use crate::tui::slash_palette::list_viewport_cap;
use crate::utils::path::AppPaths;

// ── Types ────────────────────────────────────────────────────────────

/// Field currently edited by the quick MCP add dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAddField {
    Name,
    Source,
}

/// Step in the quick MCP add dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAddStep {
    Form,
    ConfirmUpdate,
    Done,
}

/// State for `/mcp add`, kept separate from the OAuth picker so opening one
/// dialog can never leak selection or filter state into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMcpAddDialog {
    pub step: McpAddStep,
    pub field: McpAddField,
    pub name: String,
    pub source: String,
    pub project_scope: bool,
    pub error: Option<String>,
    pub result: Option<String>,
    pub stashed_prompt_draft: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAuthStep {
    SelectServer,
    WaitingBrowser,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuthServerOption {
    pub name: String,
    pub url: String,
    pub transport: String,
    pub oauth_required: bool,
    pub has_credentials: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMcpAuthDialog {
    pub step: McpAuthStep,
    pub selected: usize,
    pub filter: String,
    /// Prefill from `/mcp auth <name>`.
    pub server_name: Option<String>,
    pub servers: Vec<McpAuthServerOption>,
    /// Status / auth URL / error text shown while waiting or after failure.
    pub status_message: String,
    pub stashed_prompt_draft: Option<String>,
    pub opened_at: Instant,
    /// When true, tick loop closes the dialog and restores focus.
    pub done: bool,
    /// Success notice for transcript (set with `done`).
    pub success_notice: Option<String>,
}

// ── Data helpers ─────────────────────────────────────────────────────

/// Remote MCP servers that can run OAuth (http/sse with a URL).
pub fn list_oauth_mcp_servers(paths: &Paths) -> Vec<McpAuthServerOption> {
    let auth_path = AppPaths::auth_store_path(paths);
    let config = crate::platform::mcp::load_config(paths).unwrap_or_default();
    let mut out = Vec::new();
    for (name, server) in &config.servers {
        if server.is_disabled() {
            continue;
        }
        let Some(url) = server.remote_url() else {
            continue;
        };
        let oauth_required = server.wants_oauth();
        let has_credentials = has_stored_credentials(&auth_path, name);
        out.push(McpAuthServerOption {
            name: name.clone(),
            url: url.to_string(),
            transport: server.kind_label().to_string(),
            oauth_required,
            has_credentials,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn server_score(query: &str, opt: &McpAuthServerOption) -> i32 {
    if query.is_empty() {
        return 1;
    }
    let mut best: Option<i32> = None;
    best = max_score(best, field_score(query, &opt.name, 4, false));
    best = max_score(best, field_score(query, &opt.url, 2, false));
    best = max_score(best, field_score(query, &opt.transport, 1, false));
    best.unwrap_or(0)
}

pub fn filtered_mcp_auth_servers<'a>(servers: &'a [McpAuthServerOption], filter: &str) -> Vec<&'a McpAuthServerOption> {
    let q = filter.trim();
    if q.is_empty() {
        return servers.iter().collect();
    }
    let mut scored: Vec<_> = servers
        .iter()
        .filter_map(|s| {
            let score = server_score(q, s);
            (score > 0).then_some((score, s))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().map(|(_, s)| s).collect()
}

pub fn get_filtered_mcp_server_at<'a>(
    servers: &'a [McpAuthServerOption],
    filter: &str,
    index: usize,
) -> Option<&'a McpAuthServerOption> {
    filtered_mcp_auth_servers(servers, filter).get(index).copied()
}

pub fn count_filtered_mcp_servers(servers: &[McpAuthServerOption], filter: &str) -> usize {
    filtered_mcp_auth_servers(servers, filter).len()
}

// ── Quick add ────────────────────────────────────────────────────────

pub struct OpenMcpAddDialogArgs<'a> {
    pub pending: &'a mut iocraft::hooks::Ref<Option<PendingMcpAddDialog>>,
    pub input: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut iocraft::hooks::Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub initial: String,
}

fn split_mcp_add_initial(initial: &str) -> (String, String) {
    let mut parts = initial.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default().trim().to_string();
    let rest = parts.next().unwrap_or_default().trim();
    let source = rest.strip_prefix("--").map(str::trim).unwrap_or(rest).to_string();
    (name, source)
}

pub fn open_mcp_add_dialog(args: OpenMcpAddDialogArgs<'_>) {
    let initial = args.initial.trim();
    let (name, source) = split_mcp_add_initial(initial);
    let field = if name.is_empty() {
        McpAddField::Name
    } else {
        McpAddField::Source
    };
    let field_value = match field {
        McpAddField::Name => name.clone(),
        McpAddField::Source => source.clone(),
    };
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    args.input.set(field_value);
    args.shell_focus.set(ShellFocus::StatusDialog);
    args.pending.set(Some(PendingMcpAddDialog {
        step: McpAddStep::Form,
        field,
        name,
        source,
        project_scope: false,
        error: None,
        result: None,
        stashed_prompt_draft: stashed,
    }));
}

pub fn close_mcp_add_dialog(
    pending: &mut iocraft::hooks::Ref<Option<PendingMcpAddDialog>>,
    input: &mut State<String>,
    draft: &mut State<String>,
    live_draft: &mut iocraft::hooks::Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    let stashed = pending.read().as_ref().and_then(|p| p.stashed_prompt_draft.clone());
    pending.set(None);
    input.set(String::new());
    if let Some(text) = stashed {
        draft.set(text.clone());
        live_draft.set(text);
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Commit the current field and advance the quick-add dialog.
///
/// Returns `true` when the dialog should close after a successful save.
pub fn submit_mcp_add(
    pending: &mut iocraft::hooks::Ref<Option<PendingMcpAddDialog>>,
    input: &mut State<String>,
    paths: &Paths,
) -> Result<bool, String> {
    let mut pending_state = pending.write();
    let Some(state) = pending_state.as_mut() else {
        return Ok(false);
    };
    if state.step == McpAddStep::Done {
        return Ok(true);
    }
    if state.step == McpAddStep::ConfirmUpdate {
        let scope = if state.project_scope {
            crate::platform::mcp::McpConfigScope::Project
        } else {
            crate::platform::mcp::McpConfigScope::Home
        };
        let source = state.source.trim();
        let server = if source.starts_with("http://") || source.starts_with("https://") {
            McpServerConfig::http(source)
        } else {
            let mut parts = source.split_whitespace();
            let Some(command) = parts.next() else {
                state.error = Some("Enter a command or URL before saving.".into());
                return Ok(false);
            };
            McpServerConfig::stdio(command, parts.map(str::to_string).collect())
        };
        crate::platform::mcp::upsert_server_in(paths, scope, &state.name, server).map_err(|error| error.to_string())?;
        state.step = McpAddStep::Done;
        state.error = None;
        state.result = Some(format!(
            "Saved '{}' to {} config.",
            state.name,
            if state.project_scope { "project" } else { "user" }
        ));
        return Ok(false);
    }

    match state.field {
        McpAddField::Name => {
            let name = input.read().trim().to_string();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                state.error = Some("Use a name containing only letters, numbers, '-' or '_'.".into());
                return Ok(false);
            }
            state.name = name;
            state.field = McpAddField::Source;
            input.set(state.source.clone());
            state.error = None;
        }
        McpAddField::Source => {
            let source = input.read().trim().to_string();
            if source.is_empty() {
                state.error = Some("Enter a command or http(s):// URL.".into());
                return Ok(false);
            }
            state.source = source;
            let scope = if state.project_scope {
                crate::platform::mcp::McpConfigScope::Project
            } else {
                crate::platform::mcp::McpConfigScope::Home
            };
            let existing = crate::platform::mcp::load_layer(paths, scope)
                .map_err(|error| error.to_string())?
                .servers
                .contains_key(&state.name);
            if existing {
                state.step = McpAddStep::ConfirmUpdate;
                state.error = None;
            } else {
                let _ = state;
                drop(pending_state);
                return submit_mcp_add(pending, input, paths);
            }
        }
    }
    Ok(false)
}

// ── Open / close ─────────────────────────────────────────────────────

pub struct OpenMcpAuthDialogArgs<'a> {
    pub pending: &'a mut iocraft::hooks::Ref<Option<PendingMcpAuthDialog>>,
    pub selected: &'a mut State<usize>,
    pub filter: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut iocraft::hooks::Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub paths: &'a Paths,
    pub server_name: Option<String>,
}

pub fn open_mcp_auth_dialog(args: OpenMcpAuthDialogArgs<'_>) {
    let servers = list_oauth_mcp_servers(args.paths);
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    args.filter.set(String::new());
    args.selected.set(0);
    args.shell_focus.set(ShellFocus::StatusDialog);

    // Prefill filter / selection when `/mcp auth <name>` was given.
    let mut selected = 0usize;
    let mut filter = String::new();
    if let Some(ref name) = args.server_name {
        filter = name.clone();
        args.filter.set(filter.clone());
        if let Some(idx) = filtered_mcp_auth_servers(&servers, &filter)
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
        {
            selected = idx;
            args.selected.set(selected);
        }
    }

    args.pending.set(Some(PendingMcpAuthDialog {
        step: McpAuthStep::SelectServer,
        selected,
        filter,
        server_name: args.server_name,
        servers,
        status_message: String::new(),
        stashed_prompt_draft: stashed,
        opened_at: Instant::now(),
        done: false,
        success_notice: None,
    }));
}

pub fn close_mcp_auth_dialog(
    pending: &mut iocraft::hooks::Ref<Option<PendingMcpAuthDialog>>,
    draft: &mut State<String>,
    live_draft: &mut iocraft::hooks::Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    let stashed = pending.read().as_ref().and_then(|p| p.stashed_prompt_draft.clone());
    pending.set(None);
    if let Some(text) = stashed {
        draft.set(text.clone());
        live_draft.set(text);
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Start browser OAuth for a server (updates pending; spawns async task).
///
/// `pending` is the same `Ref` held by the shell (Copy) so the task can write results.
pub fn start_mcp_oauth_for_server(
    mut pending: iocraft::hooks::Ref<Option<PendingMcpAuthDialog>>,
    paths: &Paths,
    server_name: &str,
) -> Result<(), String> {
    let config = crate::platform::mcp::load_config(paths).map_err(|e| e.to_string())?;
    let server = config
        .servers
        .get(server_name)
        .ok_or_else(|| format!("MCP server \"{server_name}\" not found in mcp.json"))?;
    let url = server
        .remote_url()
        .ok_or_else(|| format!("MCP server \"{server_name}\" is stdio; OAuth applies only to http/sse"))?
        .to_string();

    let mut options = server
        .oauth_meta()
        .map(|meta| McpOAuthFlowOptions::from_server_meta(&meta))
        .unwrap_or_default();
    options.open_browser = true;

    let auth_store_path = AppPaths::auth_store_path(paths);
    let name_owned = server_name.to_string();

    if let Some(ref mut p) = *pending.write() {
        p.step = McpAuthStep::WaitingBrowser;
        p.status_message = format!("Opening browser for MCP OAuth · {name_owned}…");
        p.success_notice = None;
        p.done = false;
    }

    let mut pending_ref = pending;
    tokio::spawn(async move {
        match run_oauth_flow(&name_owned, &url, &auth_store_path, options).await {
            Ok(result) => {
                log::info!(
                    "MCP OAuth complete: server={} client_id={}",
                    result.server_name,
                    result.client_id
                );
                if let Some(p) = pending_ref.write().as_mut() {
                    p.done = true;
                    p.success_notice = Some(format!(
                        "MCP OAuth complete for '{}' (client_id={}). Credentials saved to sealed auth.json.",
                        result.server_name, result.client_id
                    ));
                    p.status_message = format!("Authorized {}", result.server_name);
                }
            }
            Err(error) => {
                log::error!("MCP OAuth failed for {name_owned}: {error}");
                if let Some(p) = pending_ref.write().as_mut() {
                    p.step = McpAuthStep::Failed;
                    p.status_message = format!("OAuth failed: {error}");
                    p.done = false;
                }
            }
        }
    });
    Ok(())
}

/// Clear OAuth credentials for a server (CLI parity with `mcp logout`).
pub fn logout_mcp_server(paths: &Paths, server_name: &str) -> Result<String, String> {
    let auth_path = AppPaths::auth_store_path(paths);
    if !has_stored_credentials(&auth_path, server_name) {
        return Ok(format!("No OAuth credentials stored for MCP server '{server_name}'."));
    }
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(elph_agent::mcp::clear_credentials(&auth_path, server_name))
    });
    match result {
        Ok(true) => Ok(format!("Cleared OAuth credentials for MCP server '{server_name}'.")),
        Ok(false) => Ok(format!("No OAuth credentials stored for MCP server '{server_name}'.")),
        Err(e) => Err(e.to_string()),
    }
}

/// Format a short list of MCP servers for `/mcp list`.
pub fn mcp_list_slash_message(paths: &Paths) -> String {
    let config = match crate::platform::mcp::load_config(paths) {
        Ok(c) => c,
        Err(e) => return format!("Failed to load mcp.json: {e}"),
    };
    if config.servers.is_empty() {
        return "No MCP servers configured.\n\nUse /mcp add to configure a local command or remote HTTP URL.".into();
    }
    let auth_path = AppPaths::auth_store_path(paths);
    let mut lines = vec![
        "MCP servers".to_string(),
        "Merged from user and project config (project overrides user).".to_string(),
        String::new(),
    ];
    for (name, server) in &config.servers {
        let status = if server.is_disabled() { "disabled" } else { "enabled" };
        let auth = if server.remote_url().is_some() && has_stored_credentials(&auth_path, name) {
            "authorized"
        } else if server.wants_oauth() {
            "authorization needed"
        } else {
            "not required"
        };
        let lifecycle = match server.lifecycle_mode() {
            elph_agent::mcp::McpLifecycleMode::Auto => "auto",
            elph_agent::mcp::McpLifecycleMode::Legacy => "legacy",
            elph_agent::mcp::McpLifecycleMode::Discover => "discover",
        };
        lines.push(format!("  {name}  {}", server.kind_label()));
        lines.push(format!("    Status: {status} · Auth: {auth} · Lifecycle: {lifecycle}"));
        if let Some(url) = server.remote_url() {
            lines.push(format!("    URL: {url}"));
        }
        lines.push(String::new());
    }
    lines.push("Actions: /mcp add · /mcp auth [name] · /mcp logout <name>".to_string());
    lines.push(format!("Credential store: {}", auth_path.display()));
    lines.join("\n")
}

// ── Render ───────────────────────────────────────────────────────────

pub fn render_mcp_auth_dialog(
    screen_width: u16,
    screen_height: u16,
    has_focus: bool,
    selected: usize,
    filter: &str,
    pending: &PendingMcpAuthDialog,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let w = inline_body_width(screen_width);

    match pending.step {
        McpAuthStep::SelectServer => {
            let filtered = filtered_mcp_auth_servers(&pending.servers, filter);
            let title = "MCP OAuth · select server".to_string();
            let footer = "↑/↓ select · Enter authorize · Esc cancel · type to filter".to_string();
            if filtered.is_empty() {
                let empty = if pending.servers.is_empty() {
                    "No remote MCP servers in mcp.json.\nAdd one with type http/sse (e.g. Figma), then /mcp auth."
                        .to_string()
                } else {
                    format!("No servers match filter \"{filter}\".")
                };
                return element! {
                    InlineDialogShell(
                        screen_width: screen_width,
                        title: title,
                        has_focus: has_focus,
                        footer_hint: Some(footer),
                    ) {
                        View(width: w, flex_direction: FlexDirection::Column) {
                            Text(content: empty, color: theme.text_muted, wrap: TextWrap::Wrap)
                        }
                    }
                }
                .into();
            }

            let mut rows = String::new();
            let selected = selected.min(filtered.len().saturating_sub(1));
            let viewport_rows = list_viewport_cap(screen_height).max(2);
            let row_counts = vec![2usize; filtered.len()];
            let window_start = select_window_start_for_rows(selected, viewport_rows, &row_counts);
            let mut used_rows = 0usize;
            for (i, opt) in filtered.iter().enumerate().skip(window_start) {
                if used_rows.saturating_add(row_counts[i]) > viewport_rows {
                    break;
                }
                let marker = if i == selected { "›" } else { " " };
                let cred = if opt.has_credentials { "✓" } else { "·" };
                let auth = if opt.oauth_required {
                    if opt.has_credentials {
                        "authorized"
                    } else {
                        "authorization needed"
                    }
                } else {
                    "authorization optional"
                };
                rows.push_str(&format!(
                    "{marker} {cred} {}  [{}] {auth}\n    {}\n",
                    opt.name, opt.transport, opt.url
                ));
                used_rows += row_counts[i];
            }
            let filter_line = if filter.is_empty() {
                "Filter: _".to_string()
            } else {
                format!("Filter: {filter}_")
            };

            element! {
                InlineDialogShell(
                    screen_width: screen_width,
                    title: title,
                    has_focus: has_focus,
                    footer_hint: Some(footer),
                ) {
                    View(width: w, flex_direction: FlexDirection::Column, gap: 0) {
                        Text(content: filter_line, color: theme.text_secondary, wrap: TextWrap::NoWrap)
                        Text(content: rows, color: theme.text_primary, wrap: TextWrap::Wrap)
                    }
                }
            }
            .into()
        }
        McpAuthStep::WaitingBrowser => {
            let body = if pending.status_message.is_empty() {
                "Waiting for browser OAuth…".to_string()
            } else {
                pending.status_message.clone()
            };
            element! {
                InlineDialogShell(
                    screen_width: screen_width,
                    title: "MCP OAuth".to_string(),
                    has_focus: has_focus,
                    footer_hint: Some("Complete login in the browser · Esc cancel".to_string()),
                ) {
                    View(width: w, flex_direction: FlexDirection::Column) {
                        Text(content: body, color: theme.text_primary, wrap: TextWrap::Wrap)
                        Text(
                            content: "A browser window should open. After authorize, this dialog closes."
                                .to_string(),
                            color: theme.text_muted,
                            wrap: TextWrap::Wrap,
                        )
                    }
                }
            }
            .into()
        }
        McpAuthStep::Failed => {
            let body = pending.status_message.clone();
            element! {
                InlineDialogShell(
                    screen_width: screen_width,
                    title: "MCP OAuth failed".to_string(),
                    has_focus: has_focus,
                    footer_hint: Some("Enter retry · Esc close".to_string()),
                ) {
                    View(width: w, flex_direction: FlexDirection::Column) {
                        Text(content: body, color: theme.error, wrap: TextWrap::Wrap)
                    }
                }
            }
            .into()
        }
    }
}

pub fn render_mcp_add_dialog(
    screen_width: u16,
    has_focus: bool,
    input: &str,
    field: McpAddField,
    pending: &PendingMcpAddDialog,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let w = inline_body_width(screen_width);
    let title = match pending.step {
        McpAddStep::Form => "MCP · add server",
        McpAddStep::ConfirmUpdate => "MCP · update server",
        McpAddStep::Done => "MCP · server saved",
    };
    let footer = match pending.step {
        McpAddStep::Form => "Tab next field · Ctrl+P project scope · Enter continue · Esc cancel",
        McpAddStep::ConfirmUpdate => "Enter save · Esc cancel",
        McpAddStep::Done => "Enter close · Esc close",
    };
    let body = match pending.step {
        McpAddStep::Form => {
            let name = if field == McpAddField::Name {
                input.to_string()
            } else {
                pending.name.clone()
            };
            let source = if field == McpAddField::Source {
                input.to_string()
            } else {
                pending.source.clone()
            };
            let scope = if pending.project_scope { "project" } else { "user" };
            let error = pending
                .error
                .as_deref()
                .map(|error| format!("Error: {error}"))
                .unwrap_or_default();
            element! {
                View(width: w, flex_direction: FlexDirection::Column, gap: 0) {
                    Text(content: "Add a local stdio command or a remote HTTP URL.".to_string(), color: theme.text_muted, wrap: TextWrap::Wrap)
                    Text(content: format!("Name  {}{}", if field == McpAddField::Name { "› " } else { "  " }, name), color: theme.text_primary, wrap: TextWrap::NoWrap)
                    Text(content: format!("Source{}  {}", if field == McpAddField::Source { " ›" } else { "" }, source), color: theme.text_primary, wrap: TextWrap::Wrap)
                    Text(content: format!("Scope  {scope}"), color: theme.text_secondary, wrap: TextWrap::NoWrap)
                    Text(content: error, color: theme.error, wrap: TextWrap::Wrap)
                }
            }
        }
        McpAddStep::ConfirmUpdate => {
            let error = pending
                .error
                .as_deref()
                .map(|error| format!("Error: {error}"))
                .unwrap_or_default();
            element! {
                View(width: w, flex_direction: FlexDirection::Column) {
                    Text(content: format!("'{}' already exists in the {} config.", pending.name, if pending.project_scope { "project" } else { "user" }), color: theme.text_primary, wrap: TextWrap::Wrap)
                    Text(content: format!("Replace it with: {}", pending.source), color: theme.text_secondary, wrap: TextWrap::Wrap)
                    Text(content: error, color: theme.error, wrap: TextWrap::Wrap)
                }
            }
        }
        McpAddStep::Done => element! {
            View(width: w, flex_direction: FlexDirection::Column) {
                Text(content: pending.result.clone().unwrap_or_else(|| "MCP server saved.".into()), color: theme.success, wrap: TextWrap::Wrap)
                Text(content: "The new configuration will be available on the next MCP discovery.".to_string(), color: theme.text_muted, wrap: TextWrap::Wrap)
            }
        },
    };
    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: title.to_string(),
            has_focus: has_focus,
            footer_hint: Some(footer.to_string()),
        ) {
            #(body)
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_add_initial_supports_delimiter_and_short_form() {
        assert_eq!(
            split_mcp_add_initial("filesystem -- npx -y server-filesystem /tmp"),
            ("filesystem".into(), "npx -y server-filesystem /tmp".into())
        );
        assert_eq!(
            split_mcp_add_initial("filesystem npx -y server-filesystem"),
            ("filesystem".into(), "npx -y server-filesystem".into())
        );
        assert_eq!(
            split_mcp_add_initial("remote https://example.test/a--b"),
            ("remote".into(), "https://example.test/a--b".into())
        );
        assert_eq!(split_mcp_add_initial(""), ("".into(), "".into()));
    }

    #[test]
    fn filter_matches_name() {
        let servers = vec![McpAuthServerOption {
            name: "figma".into(),
            url: "https://mcp.figma.com/mcp".into(),
            transport: "http".into(),
            oauth_required: true,
            has_credentials: false,
        }];
        assert_eq!(count_filtered_mcp_servers(&servers, "fig"), 1);
        assert_eq!(count_filtered_mcp_servers(&servers, "xyz"), 0);
    }
}
