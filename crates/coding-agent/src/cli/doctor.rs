//! `elph doctor` — environment health + a secret-free snapshot for bug reports.

use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clap::Args;
use serde::Serialize;

use super::style::{self, CliStyle, S_ERR, S_MUTED, S_OK, S_WARN};
use super::version::{self, BuildIdentity};
use crate::platform::hooks::HookHost;
use crate::platform::scaffold::TrustStore;
use crate::platform::{self, EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings};
use crate::utils::path::AppPaths;
use elph_agent::runtime::LocalExecutionEnv;

const SAFE_ENV: &[&str] = &[
    "ELPH_HOME",
    "ELPH_DATA_DIR",
    "ELPH_PROJECT_DIR",
    "ELPH_LOG_LEVEL",
    "ELPH_LOG_FILE",
    "ELPH_LOG_ROTATION",
    "ELPH_LOG_MAX_FILES",
    "ELPH_LOG_MAX_BYTES",
    "ELPH_LOG_CONSOLE",
    "ELPH_TRACE",
    "ELPH_QUIET",
    "ELPH_PROVIDER",
    "ELPH_MODEL",
    "ELPH_PROMPT_ENCODING",
    "NO_COLOR",
    "TERM",
    "TERM_PROGRAM",
    "COLORTERM",
    "LANG",
    "LC_ALL",
    "SHELL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON (safe to attach to a bug report)
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    id: String,
    status: CheckStatus,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorReport {
    schema_version: &'static str,
    generated_at: String,
    identity: IdentitySection,
    terminal: TerminalSection,
    clipboard: ClipboardSection,
    paths: PathsSection,
    env: EnvSection,
    settings: SettingsSection,
    auth: AuthSection,
    mcp: McpSection,
    resources: ResourcesSection,
    store: StoreSection,
    logging: LoggingSection,
    git: GitSection,
    checks: Vec<Check>,
    counts: Counts,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    ok: usize,
    warnings: usize,
    failures: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalSection {
    name: String,
    term: Option<String>,
    term_program: Option<String>,
    multiplexer: String,
    ssh: bool,
    color: String,
    no_color: bool,
    stdout_tty: bool,
    stderr_tty: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardSection {
    available: bool,
    backend: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentitySection {
    version: String,
    version_line: String,
    profile: String,
    target: String,
    os_arch: String,
    git_sha: String,
    build_date: String,
    os: String,
    arch: String,
    family: String,
    rustc: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathsSection {
    cwd: String,
    project_dir: String,
    config_dir: String,
    data_dir: String,
    logs_dir: String,
    settings_home: String,
    settings_project: String,
    auth: String,
    mcp_home: String,
    mcp_project: String,
    trust: String,
    store_db: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvSection {
    overrides: Vec<Kv>,
    provider_key_env_set: Vec<String>,
    http_proxy: Option<String>,
    https_proxy: Option<String>,
}

#[derive(Debug, Serialize)]
struct Kv {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSection {
    home_exists: bool,
    project_exists: bool,
    loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    default_model: Option<String>,
    default_thinking_level: Option<String>,
    memory_enabled: Option<bool>,
    quiet_startup: Option<bool>,
    http_proxy_configured: bool,
    logging_level: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthSection {
    path: String,
    exists: bool,
    parsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    providers: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpSection {
    home_exists: bool,
    project_exists: bool,
    home_servers: usize,
    project_servers: usize,
    merged_servers: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourcesSection {
    skills: usize,
    prompt_templates: usize,
    agents: usize,
    skill_conflicts: usize,
    template_conflicts: usize,
    agent_conflicts: usize,
    project_hooks_allowed: bool,
    active_hooks: usize,
    hook_diagnostics: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreSection {
    path: String,
    exists: bool,
    bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoggingSection {
    dir: String,
    dir_exists: bool,
    jsonl: Option<FileStat>,
    traces: Option<FileStat>,
    crash_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FileStat {
    path: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSection {
    inside_work_tree: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    head: Option<String>,
    dirty: Option<bool>,
}

pub fn handle(args: &DoctorArgs) -> ExitCode {
    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            super::help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };

    let report = collect(&paths);
    if args.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => {
                println!("{s}");
            }
            Err(err) => {
                super::help::cli_error(format!("json: {err}"));
                return EXIT_ERROR;
            }
        }
    } else {
        print!("{}", format_human(&report));
    }

    if report.checks.iter().any(|c| c.status == CheckStatus::Fail) {
        EXIT_ERROR
    } else {
        EXIT_SUCCESS
    }
}

fn collect(paths: &Paths) -> DoctorReport {
    let mut checks = Vec::new();
    let identity = identity_section();
    let path_sec = paths_section(paths);
    check_writable(&mut checks, "paths.config_writable", paths.config_dir());
    check_writable(&mut checks, "paths.data_writable", paths.data_dir());
    check_writable(&mut checks, "paths.logs_writable", &paths.logs_dir());
    check_writable(&mut checks, "paths.project_elph_writable", &paths.project_elph_dir());

    let settings = settings_section(paths, &mut checks);
    let auth = auth_section(paths, &mut checks);
    let mcp = mcp_section(paths, &mut checks);
    let resources = resources_section(paths, &mut checks);
    let store = store_section(paths, &mut checks);
    let logging = logging_section(paths, &mut checks);
    let env = env_section();
    let git = git_section(paths.project_dir());

    check_default_model(&mut checks, &settings, &auth, &env);

    if !auth.exists && auth.providers.is_empty() && env.provider_key_env_set.is_empty() {
        checks.push(Check {
            id: "auth.none".into(),
            status: CheckStatus::Warn,
            summary: "No auth.json and no provider API key env vars".into(),
            detail: Some("Models will fail until a credential exists.".into()),
            remediation: Some("Run `elph provider` or set the provider API key env var.".into()),
        });
    }

    let terminal = terminal_section();
    let clipboard = clipboard_section();
    collect_terminal_findings(&mut checks, &terminal, &clipboard);

    checks.sort_by(|a, b| a.id.cmp(&b.id));
    let counts = Counts {
        ok: checks.iter().filter(|c| c.status == CheckStatus::Ok).count(),
        warnings: checks.iter().filter(|c| c.status == CheckStatus::Warn).count(),
        failures: checks.iter().filter(|c| c.status == CheckStatus::Fail).count(),
    };

    DoctorReport {
        schema_version: "1",
        generated_at: chrono_utc_now(),
        identity,
        terminal,
        clipboard,
        paths: path_sec,
        env,
        settings,
        auth,
        mcp,
        resources,
        store,
        logging,
        git,
        checks,
        counts,
    }
}

fn terminal_section() -> TerminalSection {
    let term = std::env::var("TERM").ok().filter(|s| !s.is_empty());
    let term_program = std::env::var("TERM_PROGRAM").ok().filter(|s| !s.is_empty());
    let name = term_program
        .clone()
        .or_else(|| term.clone())
        .unwrap_or_else(|| "unknown".into());
    let multiplexer = if std::env::var_os("TMUX").is_some() {
        "tmux"
    } else if std::env::var_os("ZELLIJ").is_some() {
        "zellij"
    } else if std::env::var_os("STY").is_some() {
        "screen"
    } else {
        "none"
    };
    let ssh = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|k| std::env::var_os(k).is_some());
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let color = if no_color {
        "none"
    } else if std::env::var("COLORTERM")
        .map(|v| v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit"))
        .unwrap_or(false)
    {
        "truecolor"
    } else if term.as_deref().is_some_and(|t| t.contains("256")) {
        "256"
    } else if std::io::stdout().is_terminal() {
        "ansi"
    } else {
        "none"
    };
    TerminalSection {
        name,
        term,
        term_program,
        multiplexer: multiplexer.into(),
        ssh,
        color: color.into(),
        no_color,
        stdout_tty: std::io::stdout().is_terminal(),
        stderr_tty: std::io::stderr().is_terminal(),
    }
}

fn clipboard_section() -> ClipboardSection {
    ClipboardSection {
        available: elph_tui::clipboard::clipboard_available(),
        backend: elph_tui::clipboard::clipboard_backend().to_string(),
    }
}

fn collect_terminal_findings(checks: &mut Vec<Check>, term: &TerminalSection, clip: &ClipboardSection) {
    if term.no_color {
        checks.push(Check {
            id: "terminal.limited-color".into(),
            status: CheckStatus::Warn,
            summary: "Colors are off because `NO_COLOR` is set".into(),
            detail: None,
            remediation: Some("Unset `NO_COLOR`, then restart Elph.".into()),
        });
    } else {
        checks.push(Check {
            id: "terminal.color".into(),
            status: CheckStatus::Ok,
            summary: format!("color={}", term.color),
            detail: None,
            remediation: None,
        });
    }
    checks.push(Check {
        id: "terminal.env".into(),
        status: CheckStatus::Ok,
        summary: format!("terminal={} multiplexer={} ssh={}", term.name, term.multiplexer, term.ssh),
        detail: None,
        remediation: None,
    });
    if clip.available {
        checks.push(Check {
            id: "clipboard.native".into(),
            status: CheckStatus::Ok,
            summary: format!("clipboard {}", clip.backend),
            detail: None,
            remediation: None,
        });
    } else {
        checks.push(Check {
            id: "clipboard.native".into(),
            status: CheckStatus::Warn,
            summary: "system clipboard is not available".into(),
            detail: Some(clip.backend.clone()),
            remediation: Some(
                "Check clipboard permissions (macOS: Terminal / Ghostty in Privacy → Accessibility).".into(),
            ),
        });
    }
}

fn identity_section() -> IdentitySection {
    let build: BuildIdentity = version::build_identity();
    IdentitySection {
        version: format!("{}{}", build.version, build.suffix),
        version_line: version::version_line(),
        profile: build.profile,
        target: build.target,
        os_arch: build.os_arch,
        git_sha: build.git_sha,
        build_date: build.build_date,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        family: std::env::consts::FAMILY.to_string(),
        rustc: option_env!("ELPH_RUSTC_VERSION").unwrap_or("").to_string(),
    }
}

fn paths_section(paths: &Paths) -> PathsSection {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<unreadable>".into());
    PathsSection {
        cwd,
        project_dir: paths.project_dir().display().to_string(),
        config_dir: paths.config_dir().display().to_string(),
        data_dir: paths.data_dir().display().to_string(),
        logs_dir: paths.logs_dir().display().to_string(),
        settings_home: paths.settings_path().display().to_string(),
        settings_project: paths.project_settings_path().display().to_string(),
        auth: paths.auth_store_path().display().to_string(),
        mcp_home: paths.mcp_config_path().display().to_string(),
        mcp_project: paths.project_mcp_config_path().display().to_string(),
        trust: paths.trust_path().display().to_string(),
        store_db: paths.memory_db_path().display().to_string(),
    }
}

fn env_section() -> EnvSection {
    let mut overrides = Vec::new();
    for name in SAFE_ENV {
        if let Ok(value) = std::env::var(name)
            && !value.is_empty()
        {
            overrides.push(Kv {
                name: (*name).to_string(),
                value,
            });
        }
    }
    let mut provider_key_env_set = Vec::new();
    for id in elph_ai::embedded_provider_ids() {
        if let Some(var) = crate::agent::provider_api_key_env(id)
            && std::env::var_os(var).is_some_and(|v| !v.is_empty())
        {
            provider_key_env_set.push(var.to_string());
        }
    }
    provider_key_env_set.sort();
    provider_key_env_set.dedup();

    EnvSection {
        overrides,
        provider_key_env_set,
        http_proxy: proxy_summary("HTTP_PROXY").or_else(|| proxy_summary("http_proxy")),
        https_proxy: proxy_summary("HTTPS_PROXY").or_else(|| proxy_summary("https_proxy")),
    }
}

fn proxy_summary(var: &str) -> Option<String> {
    let raw = std::env::var(var).ok().filter(|s| !s.trim().is_empty())?;
    Some(proxy_host(&raw))
}

fn proxy_host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    format!("set host={host}")
}

fn settings_section(paths: &Paths, checks: &mut Vec<Check>) -> SettingsSection {
    let home_exists = paths.settings_path().is_file();
    let project_exists = paths.project_settings_path().is_file();
    match Settings::load(paths) {
        Ok(s) => {
            checks.push(Check {
                id: "settings.load".into(),
                status: CheckStatus::Ok,
                summary: if s.project_layer_loaded {
                    "settings.json loaded (home + project)".into()
                } else {
                    "settings.json loaded (home)".into()
                },
                detail: None,
                remediation: None,
            });
            SettingsSection {
                home_exists,
                project_exists,
                loaded: true,
                error: None,
                default_model: s.models.default_model.clone(),
                default_thinking_level: Some(s.models.default_thinking_level.clone()),
                memory_enabled: Some(s.memory.enabled),
                quiet_startup: Some(s.quiet_startup),
                http_proxy_configured: s.http_proxy.as_deref().is_some_and(|u| !u.trim().is_empty()),
                logging_level: s.logging.level.clone(),
            }
        }
        Err(err) => {
            checks.push(Check {
                id: "settings.load".into(),
                status: CheckStatus::Fail,
                summary: "settings.json failed to parse".into(),
                detail: Some(err.to_string()),
                remediation: Some("Fix JSON syntax in settings.json (home or project).".into()),
            });
            SettingsSection {
                home_exists,
                project_exists,
                loaded: false,
                error: Some(err.to_string()),
                default_model: None,
                default_thinking_level: None,
                memory_enabled: None,
                quiet_startup: None,
                http_proxy_configured: false,
                logging_level: None,
            }
        }
    }
}

fn auth_section(paths: &Paths, checks: &mut Vec<Check>) -> AuthSection {
    let path = paths.auth_store_path();
    let exists = path.is_file();
    if !exists {
        checks.push(Check {
            id: "auth.file".into(),
            status: CheckStatus::Warn,
            summary: "auth.json missing".into(),
            detail: Some(path.display().to_string()),
            remediation: Some("Run `elph provider` to create auth.json.".into()),
        });
        return AuthSection {
            path: path.display().to_string(),
            exists: false,
            parsed: false,
            error: None,
            providers: Vec::new(),
        };
    }
    let providers = crate::tui::provider_credential_store::list_providers_with_credentials(&path);
    let parsed = elph_agent::mcp::AuthStoreFile::load_from_path_sync(&path).is_ok() || !providers.is_empty();
    if parsed {
        checks.push(Check {
            id: "auth.file".into(),
            status: CheckStatus::Ok,
            summary: format!("auth.json readable ({} provider credential(s))", providers.len()),
            detail: None,
            remediation: None,
        });
        AuthSection {
            path: path.display().to_string(),
            exists: true,
            parsed: true,
            error: None,
            providers,
        }
    } else {
        checks.push(Check {
            id: "auth.file".into(),
            status: CheckStatus::Fail,
            summary: "auth.json present but unreadable".into(),
            detail: Some(path.display().to_string()),
            remediation: Some("Restore or recreate auth.json; do not paste secrets into a ticket.".into()),
        });
        AuthSection {
            path: path.display().to_string(),
            exists: true,
            parsed: false,
            error: Some("could not parse sealed or plain auth store".into()),
            providers,
        }
    }
}

fn mcp_section(paths: &Paths, checks: &mut Vec<Check>) -> McpSection {
    let home_exists = paths.mcp_config_path().is_file();
    let project_exists = paths.project_mcp_config_path().is_file();
    match platform::mcp::load_config(paths) {
        Ok(cfg) => {
            let home = platform::mcp::load_layer(paths, platform::mcp::McpConfigScope::Home).unwrap_or_default();
            let project = platform::mcp::load_layer(paths, platform::mcp::McpConfigScope::Project).unwrap_or_default();
            checks.push(Check {
                id: "mcp.config".into(),
                status: CheckStatus::Ok,
                summary: format!("mcp.json merged ({} server(s))", cfg.server_count()),
                detail: None,
                remediation: None,
            });
            McpSection {
                home_exists,
                project_exists,
                home_servers: home.server_count(),
                project_servers: project.server_count(),
                merged_servers: cfg.server_count(),
                error: None,
            }
        }
        Err(err) => {
            checks.push(Check {
                id: "mcp.config".into(),
                status: CheckStatus::Fail,
                summary: "mcp.json failed to parse".into(),
                detail: Some(err.to_string()),
                remediation: Some("Fix JSON in CONFIG_DIR/mcp.json or .elph/mcp.json.".into()),
            });
            McpSection {
                home_exists,
                project_exists,
                home_servers: 0,
                project_servers: 0,
                merged_servers: 0,
                error: Some(err.to_string()),
            }
        }
    }
}

fn resources_section(paths: &Paths, checks: &mut Vec<Check>) -> ResourcesSection {
    let project_hooks_allowed = TrustStore::project_hooks_allowed(paths, paths.project_dir()).unwrap_or(false);
    let hook_host = HookHost::new();
    let (active_hooks, hook_diagnostics) = match hook_host.reload(paths) {
        Ok(()) => {
            let status = hook_host.status();
            (status.active.len(), status.diagnostics.len())
        }
        Err(error) => {
            checks.push(Check {
                id: "hooks.load".into(),
                status: CheckStatus::Warn,
                summary: "hook configuration load failed".into(),
                detail: Some(error.to_string()),
                remediation: Some("Fix JSON in CONFIG_DIR/hooks.json or .elph/hooks.json.".into()),
            });
            (0, 1)
        }
    };
    if hook_diagnostics > 0 {
        checks.push(Check {
            id: "hooks.config".into(),
            status: CheckStatus::Warn,
            summary: format!("{hook_diagnostics} hook configuration issue(s)"),
            detail: None,
            remediation: Some("Inspect hook paths and validate hooks.json against schemas/hooks-schema.json.".into()),
        });
    }
    let settings = Settings::load(paths).unwrap_or_else(|_| Settings::defaults());
    let env = LocalExecutionEnv::new(paths.project_dir());
    match elph_agent::runtime::try_block_on(crate::agent::load_resources(paths, paths.project_dir(), &env, &settings)) {
        Ok(loaded) => {
            let agents = crate::agent::load_workspace_agents(paths);
            let skill_conflicts = loaded.skill_conflicts.len();
            let template_conflicts = loaded.template_conflicts.len();
            let agent_conflicts = agents.conflicts.len();
            let status = if skill_conflicts + template_conflicts + agent_conflicts > 0 {
                CheckStatus::Warn
            } else {
                CheckStatus::Ok
            };
            checks.push(Check {
                id: "resources.load".into(),
                status,
                summary: format!(
                    "{} skill(s), {} template(s), {} agent(s)",
                    loaded.skill_count(),
                    loaded.template_count(),
                    agents.agents.len()
                ),
                detail: if status == CheckStatus::Warn {
                    Some(format!(
                        "name conflicts: skills={skill_conflicts} templates={template_conflicts} agents={agent_conflicts}"
                    ))
                } else {
                    None
                },
                remediation: if status == CheckStatus::Warn {
                    Some("Same name in two different directories; later path wins.".into())
                } else {
                    None
                },
            });
            ResourcesSection {
                skills: loaded.skill_count(),
                prompt_templates: loaded.template_count(),
                agents: agents.agents.len(),
                skill_conflicts,
                template_conflicts,
                agent_conflicts,
                project_hooks_allowed,
                active_hooks,
                hook_diagnostics,
                load_error: None,
            }
        }
        Err(err) => {
            checks.push(Check {
                id: "resources.load".into(),
                status: CheckStatus::Warn,
                summary: "resource load runtime failed".into(),
                detail: Some(err.to_string()),
                remediation: None,
            });
            ResourcesSection {
                skills: 0,
                prompt_templates: 0,
                agents: 0,
                skill_conflicts: 0,
                template_conflicts: 0,
                agent_conflicts: 0,
                project_hooks_allowed,
                active_hooks,
                hook_diagnostics,
                load_error: Some(err.to_string()),
            }
        }
    }
}

fn store_section(paths: &Paths, checks: &mut Vec<Check>) -> StoreSection {
    let path = paths.memory_db_path();
    if !path.exists() {
        checks.push(Check {
            id: "store.db".into(),
            status: CheckStatus::Ok,
            summary: "store.db not created yet (normal before first session)".into(),
            detail: None,
            remediation: None,
        });
        return StoreSection {
            path: path.display().to_string(),
            exists: false,
            bytes: None,
        };
    }
    let bytes = std::fs::metadata(&path).ok().map(|m| m.len());
    checks.push(Check {
        id: "store.db".into(),
        status: CheckStatus::Ok,
        summary: format!(
            "store.db present ({})",
            bytes.map(style::fmt_bytes).unwrap_or_else(|| "size unknown".into())
        ),
        detail: None,
        remediation: None,
    });
    StoreSection {
        path: path.display().to_string(),
        exists: true,
        bytes,
    }
}

fn logging_section(paths: &Paths, checks: &mut Vec<Check>) -> LoggingSection {
    let dir = paths.logs_dir();
    let dir_exists = dir.is_dir();
    if !dir_exists {
        checks.push(Check {
            id: "logging.dir".into(),
            status: CheckStatus::Warn,
            summary: "logs directory missing".into(),
            detail: Some(dir.display().to_string()),
            remediation: Some("Elph should create APP_DATA/logs on first run; check data-dir permissions.".into()),
        });
    } else {
        checks.push(Check {
            id: "logging.dir".into(),
            status: CheckStatus::Ok,
            summary: "logs directory present".into(),
            detail: None,
            remediation: None,
        });
    }
    let jsonl = file_stat(&dir.join("elph.jsonl"));
    let traces = file_stat(&dir.join("elph-traces.jsonl"));
    let mut crash_files = list_prefix_files(&dir, "crash-");
    crash_files.sort();
    crash_files.reverse();
    crash_files.truncate(5);
    LoggingSection {
        dir: dir.display().to_string(),
        dir_exists,
        jsonl,
        traces,
        crash_files,
    }
}

fn file_stat(path: &Path) -> Option<FileStat> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(FileStat {
        path: path.display().to_string(),
        bytes: meta.len(),
    })
}

fn list_prefix_files(dir: &Path, prefix: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(prefix).then_some(name)
        })
        .collect()
}

fn git_section(project: &Path) -> GitSection {
    let inside = git_out(project, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|s| s.trim() == "true");
    if !inside {
        return GitSection {
            inside_work_tree: false,
            root: None,
            branch: None,
            head: None,
            dirty: None,
        };
    }
    let root = git_out(project, &["rev-parse", "--show-toplevel"]);
    let branch = git_out(project, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let head = git_out(project, &["rev-parse", "--short", "HEAD"]);
    let dirty = git_out(project, &["status", "--porcelain"]).map(|s| !s.trim().is_empty());
    GitSection {
        inside_work_tree: true,
        root,
        branch,
        head,
        dirty,
    }
}

fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn check_writable(checks: &mut Vec<Check>, id: &str, dir: &Path) {
    if let Err(err) = std::fs::create_dir_all(dir) {
        checks.push(Check {
            id: id.into(),
            status: CheckStatus::Fail,
            summary: format!("cannot create {}", dir.display()),
            detail: Some(err.to_string()),
            remediation: Some("Check disk permissions on the config/data directory.".into()),
        });
        return;
    }
    let probe = dir.join(".elph-doctor-write");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            checks.push(Check {
                id: id.into(),
                status: CheckStatus::Ok,
                summary: format!("writable {}", dir.display()),
                detail: None,
                remediation: None,
            });
        }
        Err(err) => {
            checks.push(Check {
                id: id.into(),
                status: CheckStatus::Fail,
                summary: format!("not writable {}", dir.display()),
                detail: Some(err.to_string()),
                remediation: Some("Check disk permissions on the config/data directory.".into()),
            });
        }
    }
}

fn check_default_model(checks: &mut Vec<Check>, settings: &SettingsSection, auth: &AuthSection, env: &EnvSection) {
    let Some(model) = settings.default_model.as_deref() else {
        return;
    };
    let provider = model.split('/').next().unwrap_or(model);
    let in_auth = auth.providers.iter().any(|p| p == provider);
    let env_var = crate::agent::provider_api_key_env(provider);
    let in_env = env_var.is_some_and(|v| env.provider_key_env_set.iter().any(|s| s == v));
    if in_auth || in_env {
        checks.push(Check {
            id: "models.default".into(),
            status: CheckStatus::Ok,
            summary: format!("default model {model} has a credential source"),
            detail: None,
            remediation: None,
        });
    } else {
        checks.push(Check {
            id: "models.default".into(),
            status: CheckStatus::Warn,
            summary: format!("default model {model} has no stored or env credential"),
            detail: Some("Connect the provider or pick another default.".into()),
            remediation: Some("Run `elph provider` or unset models.defaultModel.".into()),
        });
    }
}

fn format_human(report: &DoctorReport) -> String {
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Elph Doctor");
    style::kv(&mut out, sty, "Version", &report.identity.version_line);
    style::kv(
        &mut out,
        sty,
        "Summary",
        format!(
            "{} ok, {} warning(s), {} failure(s)",
            report.counts.ok, report.counts.warnings, report.counts.failures
        ),
    );
    let _ = writeln!(out);

    style::section(&mut out, sty, "Environment");
    style::kv(&mut out, sty, "Terminal", &report.terminal.name);
    if let Some(t) = &report.terminal.term {
        style::kv(&mut out, sty, "TERM", t);
    }
    style::kv(&mut out, sty, "Multiplexer", &report.terminal.multiplexer);
    style::kv(&mut out, sty, "SSH", yn(report.terminal.ssh));
    style::kv(&mut out, sty, "Color", &report.terminal.color);
    style::kv(
        &mut out,
        sty,
        "TTY",
        format!(
            "stdout={} stderr={}",
            yn(report.terminal.stdout_tty),
            yn(report.terminal.stderr_tty)
        ),
    );

    let _ = writeln!(out);
    style::section(&mut out, sty, "Clipboard");
    style::kv(
        &mut out,
        sty,
        "Native",
        if report.clipboard.available {
            "available"
        } else {
            "unavailable"
        },
    );
    style::kv(&mut out, sty, "Backend", &report.clipboard.backend);

    let findings: Vec<&Check> = report.checks.iter().filter(|c| c.status != CheckStatus::Ok).collect();
    let _ = writeln!(out);
    style::section(&mut out, sty, "Findings");
    if findings.is_empty() {
        style::success(&mut out, sty, "No issues.");
    } else {
        for check in findings {
            let (tag, style) = match check.status {
                CheckStatus::Warn => ("!", S_WARN),
                CheckStatus::Fail => ("x", S_ERR),
                CheckStatus::Ok => ("·", S_OK),
            };
            let _ = writeln!(
                out,
                "  {} {} {}",
                sty.paint(style, tag),
                sty.paint(S_MUTED, format!("{:<28}", check.id)),
                check.summary
            );
            if let Some(detail) = &check.detail {
                style::tip(&mut out, sty, detail);
            }
            if let Some(fix) = &check.remediation {
                style::tip(&mut out, sty, format!("→ {fix}"));
            }
        }
    }

    let _ = writeln!(out);
    style::section(&mut out, sty, "Identity");
    style::kv(&mut out, sty, "OS", format!("{}/{}", report.identity.os, report.identity.arch));
    style::kv(&mut out, sty, "Target", &report.identity.target);
    style::kv(&mut out, sty, "Profile", &report.identity.profile);
    if !report.identity.git_sha.is_empty() {
        style::kv(&mut out, sty, "Git SHA", &report.identity.git_sha);
    }

    let _ = writeln!(out);
    style::section(&mut out, sty, "Paths");
    style::kv(&mut out, sty, "CWD", &report.paths.cwd);
    style::kv(&mut out, sty, "Project", &report.paths.project_dir);
    style::kv(&mut out, sty, "Config", &report.paths.config_dir);
    style::kv(&mut out, sty, "Data", &report.paths.data_dir);
    style::kv(&mut out, sty, "Logs", &report.paths.logs_dir);
    style::kv(&mut out, sty, "Store", &report.paths.store_db);

    let _ = writeln!(out);
    style::section(&mut out, sty, "Overrides");
    if report.env.overrides.is_empty() {
        style::info(&mut out, sty, sty.paint(S_MUTED, "No ELPH_* / terminal overrides set."));
    } else {
        for kv in &report.env.overrides {
            style::kv(&mut out, sty, &kv.name, &kv.value);
        }
    }
    if !report.env.provider_key_env_set.is_empty() {
        style::kv(&mut out, sty, "API key env", report.env.provider_key_env_set.join(", "));
    }
    if let Some(p) = &report.env.http_proxy {
        style::kv(&mut out, sty, "HTTP_PROXY", p);
    }
    if let Some(p) = &report.env.https_proxy {
        style::kv(&mut out, sty, "HTTPS_PROXY", p);
    }

    let _ = writeln!(out);
    style::section(&mut out, sty, "Settings");
    style::kv(&mut out, sty, "Home file", yn(report.settings.home_exists));
    style::kv(&mut out, sty, "Project file", yn(report.settings.project_exists));
    if let Some(m) = &report.settings.default_model {
        style::kv(&mut out, sty, "Default model", m);
    } else {
        style::kv(&mut out, sty, "Default model", "(unset)");
    }
    if let Some(t) = &report.settings.default_thinking_level {
        style::kv(&mut out, sty, "Thinking", t);
    }
    if let Some(m) = report.settings.memory_enabled {
        style::kv(&mut out, sty, "Memory", yn(m));
    }

    let _ = writeln!(out);
    style::section(&mut out, sty, "Auth");
    style::kv(&mut out, sty, "File", &report.auth.path);
    if report.auth.providers.is_empty() {
        style::kv(&mut out, sty, "Providers", "(none)");
    } else {
        style::kv(&mut out, sty, "Providers", report.auth.providers.join(", "));
    }
    style::tip(&mut out, sty, "Credential values are never printed.");

    let _ = writeln!(out);
    style::section(&mut out, sty, "MCP");
    style::kv(&mut out, sty, "Home servers", report.mcp.home_servers);
    style::kv(&mut out, sty, "Project servers", report.mcp.project_servers);
    style::kv(&mut out, sty, "Merged", report.mcp.merged_servers);
    style::tip(&mut out, sty, "Connectivity: elph mcp doctor");

    let _ = writeln!(out);
    style::section(&mut out, sty, "Resources");
    style::kv(&mut out, sty, "Skills", report.resources.skills);
    style::kv(&mut out, sty, "Templates", report.resources.prompt_templates);
    style::kv(&mut out, sty, "Agents", report.resources.agents);
    style::kv(&mut out, sty, "Project hooks", yn(report.resources.project_hooks_allowed));
    style::kv(&mut out, sty, "Active hooks", report.resources.active_hooks);
    style::kv(&mut out, sty, "Hook diagnostics", report.resources.hook_diagnostics);

    let _ = writeln!(out);
    style::section(&mut out, sty, "Store & logs");
    style::kv(
        &mut out,
        sty,
        "store.db",
        match report.store.bytes {
            Some(b) => format!("yes ({})", style::fmt_bytes(b)),
            None => {
                if report.store.exists {
                    "yes".into()
                } else {
                    "no".into()
                }
            }
        },
    );
    if let Some(f) = &report.logging.jsonl {
        style::kv(&mut out, sty, "elph.jsonl", style::fmt_bytes(f.bytes));
    }
    if let Some(f) = &report.logging.traces {
        style::kv(&mut out, sty, "traces", style::fmt_bytes(f.bytes));
    }
    if !report.logging.crash_files.is_empty() {
        style::kv(&mut out, sty, "Crash logs", report.logging.crash_files.join(", "));
    }

    if report.git.inside_work_tree {
        let _ = writeln!(out);
        style::section(&mut out, sty, "Git");
        if let Some(b) = &report.git.branch {
            style::kv(&mut out, sty, "Branch", b);
        }
        if let Some(h) = &report.git.head {
            style::kv(&mut out, sty, "HEAD", h);
        }
        if let Some(d) = report.git.dirty {
            style::kv(&mut out, sty, "Dirty", yn(d));
        }
    }

    let _ = writeln!(out);
    style::section(&mut out, sty, "Bug reports");
    style::info(&mut out, sty, "Attach `elph doctor --json` (this output is secret-free).");
    style::info(
        &mut out,
        sty,
        format!(
            "Also useful: {} and crash-*.jsonl under {}",
            PathBuf::from(&report.logging.dir).join("elph.jsonl").display(),
            report.logging.dir
        ),
    );
    style::tip(&mut out, sty, "Do not attach auth.json, mcp tokens, or prompt transcripts.");
    out
}

fn yn(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

fn chrono_utc_now() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    format_rfc3339(secs, millis)
}

fn format_rfc3339(secs: u64, millis: u32) -> String {
    let days = secs / 86400;
    let tod = secs % 86400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

/// Howard Hinnant civil_from_days (UTC).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_host_strips_userinfo() {
        assert_eq!(
            proxy_host("http://user:secret@proxy.example:8080/path"),
            "set host=proxy.example:8080"
        );
    }

    #[test]
    fn rfc3339_epoch() {
        assert_eq!(format_rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_rfc3339(1_704_067_200, 123), "2024-01-01T00:00:00.123Z");
    }

    #[test]
    fn version_line_mentions_elph() {
        assert!(version::version_line().starts_with("elph "));
    }

    #[test]
    fn terminal_section_detects_no_color() {
        // Function reads process env; just assert it returns a non-empty name.
        let t = terminal_section();
        assert!(!t.name.is_empty());
        assert!(!t.color.is_empty());
    }
}
