use std::path::PathBuf;

/// How often log files are rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRotation {
    Hourly,
    Daily,
    Weekly,
}

impl LogRotation {
    pub fn from_env(prefix: &str) -> Self {
        let key = format!("{prefix}_LOG_ROTATION");
        Self::parse(std::env::var(&key).ok().as_deref())
    }

    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("hourly") => Self::Hourly,
            Some("weekly") => Self::Weekly,
            Some("daily") | None => Self::Daily,
            _ => Self::Daily,
        }
    }
}

/// Resolved logging configuration for an application to initialize its subscriber.
#[derive(Debug, Clone)]
pub struct LoggingOptions {
    pub app_name: &'static str,
    pub logs_dir: PathBuf,
    pub level: String,
    pub rotation: LogRotation,
    pub max_files: Option<usize>,
    pub file_enabled: bool,
    pub console_enabled: bool,
}

impl LoggingOptions {
    pub fn level_from_env(prefix: &str) -> String {
        let key = format!("{prefix}_LOG_LEVEL");
        match std::env::var(&key) {
            Ok(value) if matches!(value.as_str(), "trace" | "debug" | "info" | "warn" | "error") => value,
            _ => "info".to_string(),
        }
    }

    pub fn max_files_from_env(prefix: &str) -> Option<usize> {
        let key = format!("{prefix}_LOG_MAX_FILES");
        std::env::var(&key).ok().and_then(|value| value.parse().ok())
    }

    pub fn file_logging_enabled(prefix: &str) -> bool {
        let key = format!("{prefix}_LOG_FILE");
        std::env::var(&key).map(|value| value != "0").unwrap_or(true)
    }

    fn resolve(env_prefix: &str, app_name: &'static str, logs_dir: Option<PathBuf>, console_enabled: bool) -> Self {
        let file_enabled = logs_dir.is_some() && Self::file_logging_enabled(env_prefix);
        Self {
            app_name,
            logs_dir: logs_dir.unwrap_or_default(),
            level: Self::level_from_env(env_prefix),
            rotation: LogRotation::from_env(env_prefix),
            max_files: Self::max_files_from_env(env_prefix),
            file_enabled,
            console_enabled,
        }
    }
}

/// Output of [`AgentBuilder::build`].
#[derive(Debug, Clone)]
pub struct AgentInit {
    pub app_version: &'static str,
    pub quiet_env: Option<&'static str>,
    pub logging: LoggingOptions,
}

/// Builder for application initialization settings shared across Elph apps.
#[derive(Debug, Clone)]
pub struct AgentBuilder {
    app_version: &'static str,
    env_prefix: &'static str,
    app_name: &'static str,
    quiet_env: Option<&'static str>,
    logs_dir: Option<PathBuf>,
    console_enabled: bool,
}

impl AgentBuilder {
    pub fn new(app_version: &'static str) -> Self {
        Self {
            app_version,
            env_prefix: "",
            app_name: "",
            quiet_env: None,
            logs_dir: None,
            console_enabled: true,
        }
    }

    pub fn env_prefix(mut self, prefix: &'static str) -> Self {
        self.env_prefix = prefix;
        self
    }

    pub fn app_name(mut self, name: &'static str) -> Self {
        self.app_name = name;
        self
    }

    pub fn quiet_env(mut self, env: &'static str) -> Self {
        self.quiet_env = Some(env);
        self
    }

    pub fn logs_dir(mut self, dir: PathBuf) -> Self {
        self.logs_dir = Some(dir);
        self
    }

    pub fn console_enabled(mut self, enabled: bool) -> Self {
        self.console_enabled = enabled;
        self
    }

    pub fn build(self) -> AgentInit {
        AgentInit {
            app_version: self.app_version,
            quiet_env: self.quiet_env,
            logging: LoggingOptions::resolve(self.env_prefix, self.app_name, self.logs_dir, self.console_enabled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_daily_rotation() {
        assert_eq!(LogRotation::parse(None), LogRotation::Daily);
        assert_eq!(LogRotation::parse(Some("daily")), LogRotation::Daily);
    }

    #[test]
    fn parses_rotation_values() {
        assert_eq!(LogRotation::parse(Some("hourly")), LogRotation::Hourly);
        assert_eq!(LogRotation::parse(Some("weekly")), LogRotation::Weekly);
        assert_eq!(LogRotation::parse(Some("monthly")), LogRotation::Daily);
    }

    #[test]
    fn builder_resolves_logging_without_logs_dir() {
        let init = AgentBuilder::new("0.0.12-test")
            .env_prefix("ELPH")
            .app_name("elph")
            .console_enabled(false)
            .build();

        assert_eq!(init.app_version, "0.0.12-test");
        assert!(!init.logging.file_enabled);
        assert!(!init.logging.console_enabled);
        assert_eq!(init.logging.level, "info");
    }
}
