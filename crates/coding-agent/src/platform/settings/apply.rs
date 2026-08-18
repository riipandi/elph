//! Host-side maps from [`Settings`] into session/runtime options.

use std::path::{Path, PathBuf};

use elph_agent::harness::Skill;
use elph_agent::plugins::ExtensionsSettings;

use super::Settings;
use super::patterns::model_matches;

impl Settings {
    /// Extra + disabled extension names for the WASM host.
    pub fn extensions_settings(&self) -> ExtensionsSettings {
        ExtensionsSettings {
            disabled: self.resources.disabled_extensions.clone(),
            extra_paths: self
                .resources
                .extensions
                .iter()
                .filter(|p| !p.starts_with('!') && !p.starts_with('-'))
                .map(|p| expand_user_path(p))
                .collect(),
        }
    }

    /// Project skill/prompt directories are always scanned (same as settings merge).
    pub fn include_project_resources(&self) -> bool {
        true
    }

    /// Project WASM extensions require `/trust` or `trust.json` `defaultProjectTrust: always`.
    pub fn include_project_extensions(&self, paths: &crate::platform::Paths) -> bool {
        crate::platform::scaffold::TrustStore::project_extensions_allowed(paths, paths.project_dir()).unwrap_or(false)
    }

    /// Filter discovered skills by `resources.disabledSkills` and `resources.skills` `!` / `-` patterns.
    pub fn filter_skills(&self, skills: Vec<Skill>) -> Vec<Skill> {
        skills
            .into_iter()
            .filter(|s| !name_denied(&self.resources.disabled_skills, &s.name))
            .filter(|s| !path_or_name_excluded(&self.resources.skills, &s.name, &s.file_path))
            .collect()
    }

    /// Extra skill directory/file paths (include-only entries).
    pub fn extra_skill_paths(&self) -> Vec<String> {
        extra_include_paths(&self.resources.skills)
    }

    /// Extra prompt template paths.
    pub fn extra_prompt_paths(&self) -> Vec<String> {
        extra_include_paths(&self.resources.prompts)
    }

    /// Whether `provider/model_id` passes `models.enabled`.
    pub fn model_is_enabled(&self, provider: &str, model_id: &str) -> bool {
        model_matches(&self.models.enabled, provider, model_id)
    }

    /// Apply HTTP proxy env from settings when the process has no proxy vars yet.
    pub fn apply_http_proxy_env(&self) {
        let Some(url) = self.http_proxy.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
            return;
        };
        if std::env::var_os("HTTP_PROXY").is_none() && std::env::var_os("http_proxy").is_none() {
            unsafe { std::env::set_var("HTTP_PROXY", url) };
        }
        if std::env::var_os("HTTPS_PROXY").is_none() && std::env::var_os("https_proxy").is_none() {
            unsafe { std::env::set_var("HTTPS_PROXY", url) };
        }
    }

    /// Honor `ui.quietStartup` when `ELPH_QUIET` is unset.
    pub fn apply_quiet_startup_env(&self) {
        if !self.quiet_startup {
            return;
        }
        if std::env::var_os("ELPH_QUIET").is_none() {
            unsafe { std::env::set_var("ELPH_QUIET", "1") };
        }
    }
}

fn extra_include_paths(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .filter(|p| {
            let t = p.trim();
            !t.is_empty() && !t.starts_with('!') && !t.starts_with('-')
        })
        .map(|p| {
            let t = p.trim();
            let t = t.strip_prefix('+').unwrap_or(t);
            expand_user_path(t).to_string_lossy().into_owned()
        })
        .collect()
}

fn name_denied(deny: &[String], name: &str) -> bool {
    deny.iter().any(|pat| {
        let rest = pat.trim().trim_start_matches(['!', '-']);
        !rest.is_empty() && crate::platform::settings::patterns::matches_any(&[rest.to_string()], name)
    })
}

fn path_or_name_excluded(skill_patterns: &[String], name: &str, path: &str) -> bool {
    skill_patterns.iter().any(|pat| {
        let t = pat.trim();
        let rest = if let Some(r) = t.strip_prefix('!') {
            r
        } else if let Some(r) = t.strip_prefix('-') {
            r
        } else {
            return false;
        };
        crate::platform::settings::patterns::matches_any(&[rest.to_string()], name)
            || crate::platform::settings::patterns::matches_any(&[rest.to_string()], path)
    })
}

fn expand_user_path(raw: &str) -> PathBuf {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    } else if s == "~"
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        return PathBuf::from(home);
    }
    Path::new(s).to_path_buf()
}

/// Filter builtin tool names. Meta tools are always kept.
pub fn filter_default_tools(all_names: &[String], allowlist: Option<&[String]>) -> Vec<String> {
    const META: &[&str] = &["list_available_tools", "list_skills"];
    let Some(allow) = allowlist else {
        return all_names.to_vec();
    };
    all_names
        .iter()
        .filter(|name| META.contains(&name.as_str()) || allow.iter().any(|a| a == *name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_default_tools_none_keeps_all() {
        let all = vec!["read_file".into(), "write_file".into(), "list_skills".into()];
        assert_eq!(filter_default_tools(&all, None), all);
    }

    #[test]
    fn filter_default_tools_empty_keeps_meta() {
        let all = vec![
            "read_file".into(),
            "write_file".into(),
            "list_skills".into(),
            "list_available_tools".into(),
        ];
        let out = filter_default_tools(&all, Some(&[]));
        assert_eq!(out, vec!["list_skills".to_string(), "list_available_tools".to_string()]);
    }

    #[test]
    fn filter_default_tools_allowlist() {
        let all = vec![
            "read_file".into(),
            "write_file".into(),
            "grep".into(),
            "list_skills".into(),
        ];
        let out = filter_default_tools(&all, Some(&["read_file".into(), "grep".into()]));
        assert_eq!(
            out,
            vec!["read_file".to_string(), "grep".to_string(), "list_skills".to_string()]
        );
    }
}
