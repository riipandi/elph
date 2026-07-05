use std::path::PathBuf;

use elph_core::logger::LoggingOptions;

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
