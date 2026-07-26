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
    pub trace_enabled: bool,
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

    pub fn trace_enabled(prefix: &str) -> bool {
        let key = format!("{prefix}_TRACE");
        Self::parse_trace_enabled(std::env::var(&key).ok().as_deref())
    }

    pub fn parse_trace_enabled(value: Option<&str>) -> bool {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None => true,
            Some(value) => {
                let normalized = value.to_ascii_lowercase();
                !matches!(normalized.as_str(), "0" | "false" | "no" | "off" | "disabled")
            }
        }
    }

    pub fn resolve(env_prefix: &str, app_name: &'static str, logs_dir: Option<PathBuf>, console_enabled: bool) -> Self {
        let file_enabled = logs_dir.is_some() && Self::file_logging_enabled(env_prefix);
        Self {
            app_name,
            logs_dir: logs_dir.unwrap_or_default(),
            level: Self::level_from_env(env_prefix),
            rotation: LogRotation::from_env(env_prefix),
            max_files: Self::max_files_from_env(env_prefix),
            file_enabled,
            console_enabled,
            trace_enabled: Self::trace_enabled(env_prefix),
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
    fn trace_enabled_defaults_to_true() {
        assert!(LoggingOptions::parse_trace_enabled(None));
        assert!(LoggingOptions::parse_trace_enabled(Some("1")));
        assert!(LoggingOptions::parse_trace_enabled(Some("true")));
        assert!(LoggingOptions::parse_trace_enabled(Some("on")));
    }

    #[test]
    fn trace_disabled_when_env_is_zero() {
        for value in ["0", "false", "no", "off", "disabled", " FALSE "] {
            assert!(
                !LoggingOptions::parse_trace_enabled(Some(value)),
                "expected disabled for {value:?}"
            );
        }
    }

    #[test]
    fn parse_trace_enabled_edge_cases() {
        assert!(LoggingOptions::parse_trace_enabled(Some("")));
        assert!(LoggingOptions::parse_trace_enabled(Some("  ")));
        assert!(!LoggingOptions::parse_trace_enabled(Some("FALSE")));
    }

    #[test]
    fn rotation_parse_unknown_defaults_daily() {
        assert_eq!(LogRotation::parse(Some("unknown")), LogRotation::Daily);
    }

    #[test]
    fn level_from_env_defaults_to_info() {
        let level = LoggingOptions::level_from_env("NONEXISTENT_PREFIX_XYZ");
        assert_eq!(level, "info");
    }

    #[test]
    fn max_files_from_env_returns_none_for_missing() {
        assert!(LoggingOptions::max_files_from_env("NONEXISTENT_PREFIX_XYZ").is_none());
    }

    #[test]
    fn file_logging_enabled_defaults_to_true() {
        assert!(LoggingOptions::file_logging_enabled("NONEXISTENT_PREFIX_XYZ"));
    }

    #[test]
    fn trace_enabled_from_env_defaults_to_true() {
        assert!(LoggingOptions::trace_enabled("NONEXISTENT_PREFIX_XYZ"));
    }
}
