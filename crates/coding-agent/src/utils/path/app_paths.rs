use std::path::PathBuf;

/// Common config/data path helpers shared by Elph applications.
pub trait AppPaths {
    fn config_dir(&self) -> &PathBuf;
    fn data_dir(&self) -> &PathBuf;

    fn settings_path(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }

    fn trust_path(&self) -> PathBuf {
        self.config_dir().join("trust.json")
    }

    fn bundled_dir(&self) -> PathBuf {
        self.config_dir().join("bundled")
    }

    fn bundled_manifest_path(&self) -> PathBuf {
        self.bundled_dir().join("manifest.json")
    }

    fn prompts_dir(&self) -> PathBuf {
        self.config_dir().join("prompts")
    }

    fn providers_dir(&self) -> PathBuf {
        self.config_dir().join("providers")
    }

    /// User-managed custom agents (`CONFIG_DIR/agents/`).
    fn agents_dir(&self) -> PathBuf {
        self.config_dir().join("agents")
    }

    /// User hooks (`CONFIG_DIR/hooks/`).
    fn hooks_dir(&self) -> PathBuf {
        self.config_dir().join("hooks")
    }

    /// Host-level MCP cache when no session is active (CLI `mcp` commands).
    fn host_mcp_cache_dir(&self) -> PathBuf {
        self.data_dir().join("mcp_cache")
    }

    /// Session root under APP_DATA: `sessions/<SESSION_ID>/` for artifacts
    /// (`mcp_cache`, `terminals`, `tool_outputs.jsonl`, `event_log.jsonl`).
    ///
    /// Also used by legacy multi-file `SessionDirStorage` (`sessions/<project_key>/<id>/`).
    fn sessions_dir(&self) -> PathBuf {
        self.data_dir().join("sessions")
    }

    fn mcp_config_path(&self) -> PathBuf {
        self.config_dir().join("mcp.json")
    }

    /// Shared OAuth / credential store file (default `auth.json` under config dir).
    ///
    /// Host-agnostic: elph → `CONFIG_DIR/auth.json`, other apps join their own `config_dir`.
    fn auth_store_path(&self) -> PathBuf {
        self.config_dir().join("auth.json")
    }

    fn skills_dir(&self) -> PathBuf {
        self.config_dir().join("skills")
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.data_dir().join("worktrees")
    }

    fn attachments_dir(&self) -> PathBuf {
        self.data_dir().join("attachments")
    }

    fn downloads_dir(&self) -> PathBuf {
        self.data_dir().join("downloads")
    }

    fn logs_dir(&self) -> PathBuf {
        self.data_dir().join("logs")
    }

    /// MCP tool stderr logs: `APP_DATA/logs/mcp/`.
    fn mcp_logs_dir(&self) -> PathBuf {
        self.logs_dir().join("mcp")
    }

    fn vendor_dir(&self) -> PathBuf {
        self.data_dir().join("vendor")
    }

    /// Local embedding model cache (embed_anything / Hugging Face downloads).
    fn models_dir(&self) -> PathBuf {
        self.data_dir().join("models")
    }

    fn version_path(&self) -> PathBuf {
        self.data_dir().join("version.json")
    }

    fn changelog_md_path(&self) -> PathBuf {
        self.data_dir().join("CHANGELOG.md")
    }

    fn changelog_json_path(&self) -> PathBuf {
        self.data_dir().join("CHANGELOG.json")
    }

    /// Per-session artifact directory: `APP_DATA/sessions/<SESSION_ID>/`.
    fn session_artifact_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(session_id)
    }

    fn session_mcp_cache_dir(&self, session_id: &str) -> PathBuf {
        self.session_artifact_dir(session_id).join("mcp_cache")
    }

    fn session_terminals_dir(&self, session_id: &str) -> PathBuf {
        self.session_artifact_dir(session_id).join("terminals")
    }

    fn session_tool_outputs_path(&self, session_id: &str) -> PathBuf {
        self.session_artifact_dir(session_id).join("tool_outputs.jsonl")
    }

    fn session_event_log_path(&self, session_id: &str) -> PathBuf {
        self.session_artifact_dir(session_id).join("event_log.jsonl")
    }

    /// `APP_DATA/logs/mcp/<MCP_NAME>/` — stderr / debug logs for a server.
    fn mcp_server_logs_dir(&self, mcp_name: &str) -> PathBuf {
        self.mcp_logs_dir().join(sanitize_path_segment(mcp_name))
    }

    /// `APP_DATA/logs/mcp/<MCP_NAME>/<TOOL_NAME>.stderr.log`
    fn mcp_tool_stderr_log_path(&self, mcp_name: &str, tool_name: &str) -> PathBuf {
        self.mcp_server_logs_dir(mcp_name)
            .join(format!("{}.stderr.log", sanitize_path_segment(tool_name)))
    }

    /// Ensure artifact subdirs for a session (mcp_cache, terminals).
    fn session_artifact_layout_dirs(&self, session_id: &str) -> [PathBuf; 2] {
        [
            self.session_mcp_cache_dir(session_id),
            self.session_terminals_dir(session_id),
        ]
    }

    fn bundled_content_dirs(&self) -> [PathBuf; 3] {
        let bundled = self.bundled_dir();
        [
            bundled.join("agents"),
            bundled.join("skills"),
            bundled.join("user-guide"),
        ]
    }

    fn standard_required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.bundled_content_dirs().into_iter().collect::<Vec<_>>();
        dirs.extend([
            self.agents_dir(),
            self.hooks_dir(),
            self.prompts_dir(),
            self.providers_dir(),
            self.host_mcp_cache_dir(),
            self.sessions_dir(),
            self.skills_dir(),
            self.worktrees_dir(),
            self.attachments_dir(),
            self.downloads_dir(),
            self.logs_dir(),
            self.mcp_logs_dir(),
            self.vendor_dir(),
            self.models_dir(),
        ]);
        dirs
    }
}

/// Make a filesystem-safe single path segment (MCP / tool names).
fn sanitize_path_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".into() } else { out }
}
