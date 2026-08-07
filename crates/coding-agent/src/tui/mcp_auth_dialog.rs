//! `/mcp auth` dialog — OAuth login for remote MCP servers.
//!
//! Flow mirrors `/provider connect` at a smaller scale:
//!   1. **SelectServer** — pick a remote MCP server (fuzzy filter)
//!   2. **WaitingBrowser** — browser OAuth (PKCE) in progress
//!   3. **Done** / **Failed** — result; main tick closes on `done`

use std::time::Instant;

use elph_agent::{McpOAuthFlowOptions, has_stored_credentials, run_oauth_flow};
use elph_tui::components::UiTheme;
use iocraft::prelude::*;

use crate::platform::Paths;
use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};
use crate::tui::slash_palette::fuzzy::{field_score, max_score};
use crate::utils::path::AppPaths;

// ── Types ────────────────────────────────────────────────────────────

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
        tokio::runtime::Handle::current().block_on(elph_agent::clear_credentials(&auth_path, server_name))
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
        return "No MCP servers configured. Add one with `elph mcp add` or edit mcp.json.".into();
    }
    let auth_path = AppPaths::auth_store_path(paths);
    let mut lines = vec!["MCP servers (merged home + project):".to_string()];
    for (name, server) in &config.servers {
        let disabled = if server.is_disabled() { " [disabled]" } else { "" };
        let oauth = if server.wants_oauth() { " oauth" } else { "" };
        let creds = if server.remote_url().is_some() && has_stored_credentials(&auth_path, name) {
            " auth=stored"
        } else if server.wants_oauth() {
            " auth=needed"
        } else {
            ""
        };
        let lifecycle = match server.lifecycle_mode() {
            elph_agent::McpLifecycleMode::Auto => "auto",
            elph_agent::McpLifecycleMode::Legacy => "legacy",
            elph_agent::McpLifecycleMode::Discover => "discover",
        };
        let url = server.remote_url().unwrap_or("-");
        lines.push(format!(
            "  {name}: {}{disabled}{oauth}{creds} lifecycle={lifecycle}  {url}",
            server.kind_label()
        ));
    }
    lines.push(format!(
        "OAuth: /mcp auth [name] · logout: /mcp logout <name> · store: {}",
        auth_path.display()
    ));
    lines.join("\n")
}

// ── Render ───────────────────────────────────────────────────────────

pub fn render_mcp_auth_dialog(
    screen_width: u16,
    _screen_height: u16,
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
            for (i, opt) in filtered.iter().enumerate() {
                let marker = if i == selected { "›" } else { " " };
                let cred = if opt.has_credentials { "✓" } else { "·" };
                let oauth = if opt.oauth_required { "oauth" } else { "optional" };
                rows.push_str(&format!(
                    "{marker} {cred} {}  [{}] {oauth}\n    {}\n",
                    opt.name, opt.transport, opt.url
                ));
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

#[cfg(test)]
mod tests {
    use super::*;

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
