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

    fn projects_dir(&self) -> PathBuf {
        self.config_dir().join("projects")
    }

    fn sessions_dir(&self) -> PathBuf {
        self.config_dir().join("sessions")
    }

    fn mcp_config_path(&self) -> PathBuf {
        self.config_dir().join("mcp.json")
    }

    fn skills_dir(&self) -> PathBuf {
        self.config_dir().join("skills")
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.config_dir().join("worktrees")
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

    fn vendor_dir(&self) -> PathBuf {
        self.data_dir().join("vendor")
    }

    /// Local ONNX embedding model cache (fastembed / Hugging Face downloads).
    fn models_dir(&self) -> PathBuf {
        self.data_dir().join("models")
    }

    fn metadata_db_path(&self) -> PathBuf {
        self.data_dir().join("metadata.db")
    }

    fn version_path(&self) -> PathBuf {
        self.data_dir().join("version.json")
    }

    fn bundled_content_dirs(&self) -> [PathBuf; 4] {
        let bundled = self.bundled_dir();
        [
            bundled.join("agents"),
            bundled.join("personas"),
            bundled.join("skills"),
            bundled.join("user-guide"),
        ]
    }

    fn standard_required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.bundled_content_dirs().into_iter().collect::<Vec<_>>();
        dirs.extend([
            self.prompts_dir(),
            self.providers_dir(),
            self.projects_dir(),
            self.sessions_dir(),
            self.skills_dir(),
            self.worktrees_dir(),
            self.attachments_dir(),
            self.downloads_dir(),
            self.logs_dir(),
            self.vendor_dir(),
            self.models_dir(),
        ]);
        dirs
    }
}
