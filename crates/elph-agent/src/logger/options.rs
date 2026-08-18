use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

/// How often (or on what trigger) log files are rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogRotation {
    Hourly,
    Daily,
    /// Rotate when the current file exceeds [`LoggingOptions::max_bytes`].
    Size,
}

impl LogRotation {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(|value| value.to_ascii_lowercase()) {
            Some(value) if value == "hourly" => Self::Hourly,
            Some(value) if value == "size" => Self::Size,
            Some(value) if value == "daily" || value.is_empty() => Self::Daily,
            None => Self::Daily,
            Some(_) => Self::Daily,
        }
    }
}

/// Optional logging knobs from `settings.json` (all fields optional; omitted keys stay unset).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<LogRotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<bool>,
}

/// Resolved logging configuration for an application to initialize its subscriber.
#[derive(Debug, Clone)]
pub struct LoggingOptions {
    pub app_name: &'static str,
    pub logs_dir: PathBuf,
    pub level: String,
    pub rotation: LogRotation,
    pub max_files: Option<usize>,
    pub max_bytes: Option<u64>,
    pub file_enabled: bool,
    pub console_enabled: bool,
    pub trace_enabled: bool,
}

impl LoggingOptions {
    pub fn builder() -> LoggingOptionsBuilder {
        LoggingOptionsBuilder::default()
    }
}

/// Builder for [`LoggingOptions`].
///
/// Merge order when [`LoggingOptionsBuilder::build`] runs:
/// defaults → [`LoggingSettings`] → `{PREFIX}_LOG_*` / `{PREFIX}_TRACE` (env wins).
#[derive(Debug, Clone)]
pub struct LoggingOptionsBuilder {
    app_name: &'static str,
    logs_dir: Option<PathBuf>,
    level: Option<String>,
    rotation: Option<LogRotation>,
    max_files: Option<usize>,
    max_bytes: Option<u64>,
    file_enabled: Option<bool>,
    console_enabled: bool,
    trace_enabled: Option<bool>,
    settings: LoggingSettings,
    env_prefix: Option<String>,
}

impl Default for LoggingOptionsBuilder {
    fn default() -> Self {
        Self {
            app_name: "elph",
            logs_dir: None,
            level: None,
            rotation: None,
            max_files: None,
            max_bytes: None,
            file_enabled: None,
            console_enabled: false,
            trace_enabled: None,
            settings: LoggingSettings::default(),
            env_prefix: None,
        }
    }
}

impl LoggingOptionsBuilder {
    pub fn app_name(mut self, name: &'static str) -> Self {
        self.app_name = name;
        self
    }

    pub fn logs_dir(mut self, dir: PathBuf) -> Self {
        self.logs_dir = Some(dir);
        self
    }

    pub fn logs_dir_opt(mut self, dir: Option<PathBuf>) -> Self {
        self.logs_dir = dir;
        self
    }

    pub fn level(mut self, level: impl Into<String>) -> Self {
        self.level = Some(level.into());
        self
    }

    pub fn rotation(mut self, rotation: LogRotation) -> Self {
        self.rotation = Some(rotation);
        self
    }

    pub fn max_files(mut self, max_files: usize) -> Self {
        self.max_files = Some(max_files);
        self
    }

    pub fn max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = Some(max_bytes);
        self
    }

    pub fn file_enabled(mut self, enabled: bool) -> Self {
        self.file_enabled = Some(enabled);
        self
    }

    pub fn console_enabled(mut self, enabled: bool) -> Self {
        self.console_enabled = enabled;
        self
    }

    pub fn trace_enabled(mut self, enabled: bool) -> Self {
        self.trace_enabled = Some(enabled);
        self
    }

    pub fn settings(mut self, settings: LoggingSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        if !prefix.is_empty() {
            self.env_prefix = Some(prefix);
        }
        self
    }

    pub fn build(self) -> LoggingOptions {
        let logs_dir = self.logs_dir.clone().unwrap_or_default();
        let has_logs_dir = self.logs_dir.is_some();

        let mut level = self.level.unwrap_or_else(|| "info".to_string());
        let mut rotation = self.rotation.unwrap_or(LogRotation::Daily);
        let mut max_files = self.max_files;
        let mut max_bytes = self.max_bytes;
        let mut file_enabled = self.file_enabled.unwrap_or(true);
        let mut console_enabled = self.console_enabled;
        let mut trace_enabled = self.trace_enabled.unwrap_or(true);

        if let Some(value) = self.settings.level {
            level = value;
        }
        if let Some(value) = self.settings.file {
            file_enabled = value;
        }
        if let Some(value) = self.settings.rotation {
            rotation = value;
        }
        if let Some(value) = self.settings.max_files {
            max_files = Some(value);
        }
        if let Some(value) = self.settings.max_bytes {
            max_bytes = Some(value);
        }
        if let Some(value) = self.settings.trace {
            trace_enabled = value;
        }

        if let Some(prefix) = self.env_prefix.as_deref() {
            if let Some(value) = env_nonempty(prefix, "LOG_LEVEL") {
                level = value;
            }
            if let Some(value) = env_nonempty(prefix, "LOG_FILE") {
                file_enabled = value != "0";
            }
            if let Some(value) = env_nonempty(prefix, "LOG_ROTATION") {
                rotation = LogRotation::parse(Some(&value));
            }
            if let Some(value) = env_nonempty(prefix, "LOG_MAX_FILES")
                && let Ok(parsed) = value.parse()
            {
                max_files = Some(parsed);
            }
            if let Some(value) = env_nonempty(prefix, "LOG_MAX_BYTES")
                && let Ok(parsed) = value.parse()
            {
                max_bytes = Some(parsed);
            }
            if let Some(value) = env_nonempty(prefix, "TRACE") {
                trace_enabled = parse_trace_enabled(Some(&value));
            }
            if let Some(value) = env_nonempty(prefix, "LOG_CONSOLE") {
                console_enabled =
                    value != "0" && !matches!(value.to_ascii_lowercase().as_str(), "false" | "no" | "off" | "disabled");
            }
        }

        if !has_logs_dir {
            file_enabled = false;
        }

        LoggingOptions {
            app_name: self.app_name,
            logs_dir,
            level,
            rotation,
            max_files,
            max_bytes,
            file_enabled,
            console_enabled,
            trace_enabled,
        }
    }
}

fn env_nonempty(prefix: &str, suffix: &str) -> Option<String> {
    let key = format!("{prefix}_{suffix}");
    std::env::var(&key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

/// Highest `log` crate filter implied by a rustlog spec (`info` or `elph_agent=debug,elph_ai=warn`).
pub fn max_level_from_spec(spec: &str) -> log::LevelFilter {
    let mut max = log::LevelFilter::Off;
    let mut saw = false;
    for part in spec.split(',') {
        let token = part.split('=').next_back().unwrap_or(part).trim();
        let Some(level) = parse_simple_level(token) else {
            continue;
        };
        saw = true;
        if level > max {
            max = level;
        }
    }
    if saw { max } else { log::LevelFilter::Info }
}

fn parse_simple_level(token: &str) -> Option<log::LevelFilter> {
    match token.to_ascii_lowercase().as_str() {
        "trace" => Some(log::LevelFilter::Trace),
        "debug" => Some(log::LevelFilter::Debug),
        "info" => Some(log::LevelFilter::Info),
        "warn" | "warning" => Some(log::LevelFilter::Warn),
        "error" => Some(log::LevelFilter::Error),
        "off" => Some(log::LevelFilter::Off),
        _ => None,
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
        assert_eq!(LogRotation::parse(Some("size")), LogRotation::Size);
        assert_eq!(LogRotation::parse(Some("weekly")), LogRotation::Daily);
        assert_eq!(LogRotation::parse(Some("unknown")), LogRotation::Daily);
    }

    #[test]
    fn trace_enabled_defaults_to_true() {
        assert!(parse_trace_enabled(None));
        assert!(parse_trace_enabled(Some("1")));
        assert!(parse_trace_enabled(Some("true")));
        assert!(parse_trace_enabled(Some("on")));
    }

    #[test]
    fn trace_disabled_when_env_is_zero() {
        for value in ["0", "false", "no", "off", "disabled", " FALSE "] {
            assert!(!parse_trace_enabled(Some(value)), "expected disabled for {value:?}");
        }
    }

    #[test]
    fn builder_defaults_without_logs_dir_disables_file() {
        let opts = LoggingOptions::builder()
            .app_name("elph")
            .console_enabled(false)
            .build();
        assert!(!opts.file_enabled);
        assert!(!opts.console_enabled);
        assert_eq!(opts.level, "info");
        assert!(opts.trace_enabled);
        assert_eq!(opts.rotation, LogRotation::Daily);
    }

    #[test]
    fn settings_overlay_then_explicit_builder_fields_lose_to_settings() {
        let opts = LoggingOptions::builder()
            .logs_dir(PathBuf::from("/tmp/elph-logs"))
            .level("warn")
            .settings(LoggingSettings {
                level: Some("debug".into()),
                file: Some(true),
                rotation: Some(LogRotation::Hourly),
                max_files: Some(7),
                max_bytes: None,
                trace: Some(false),
            })
            .build();
        assert_eq!(opts.level, "debug");
        assert_eq!(opts.rotation, LogRotation::Hourly);
        assert_eq!(opts.max_files, Some(7));
        assert!(!opts.trace_enabled);
        assert!(opts.file_enabled);
    }

    #[test]
    fn rustlog_directive_max_level() {
        assert_eq!(max_level_from_spec("info"), log::LevelFilter::Info);
        assert_eq!(max_level_from_spec("elph_agent=debug,elph_ai=warn"), log::LevelFilter::Debug);
        assert_eq!(max_level_from_spec("off"), log::LevelFilter::Off);
        assert_eq!(max_level_from_spec(""), log::LevelFilter::Info);
    }
}
