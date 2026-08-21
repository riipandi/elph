//! Host-side maps from [`Settings`] into session/runtime options.

use std::path::{Path, PathBuf};

use elph_agent::harness::{PromptTemplate, Skill};
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

    /// Filter discovered prompt templates by `resources.disabledPrompts` and `resources.prompts` `!` / `-` patterns.
    pub fn filter_prompts(&self, templates: Vec<PromptTemplate>) -> Vec<PromptTemplate> {
        templates
            .into_iter()
            .filter(|t| !name_denied(&self.resources.disabled_prompts, &t.name))
            .filter(|t| !path_or_name_excluded(&self.resources.prompts, &t.name, &t.file_path))
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
        let rest = expand_user_pattern(rest);
        crate::platform::settings::patterns::matches_any(std::slice::from_ref(&rest), name)
            || path_matches_pattern(&rest, path)
    })
}

/// True when `pat` excludes `path`: as an absolute path, as a parent directory of `path`,
/// or as a relative suffix of `path` (so `!.agents/skills` / `!.agents/skills/*` exclude
/// project skills regardless of the project's absolute location).
fn path_matches_pattern(pat: &str, path: &str) -> bool {
    if path_under_dir(pat, path) {
        return true;
    }
    if crate::platform::settings::patterns::matches_any(&[pat.to_string()], path) {
        return true;
    }
    for suffix in path_suffixes(path) {
        if crate::platform::settings::patterns::matches_any(&[pat.to_string()], &suffix) || path_under_dir(pat, &suffix)
        {
            return true;
        }
    }
    false
}

/// Trailing path suffixes starting at each separator boundary, longest first.
///
/// `/a/b/.agents/skills/demo` yields `b/.agents/skills/demo`, `.agents/skills/demo`, …
fn path_suffixes(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, ch) in path.char_indices() {
        if ch == '/' || ch == '\\' {
            out.push(path[i + 1..].to_string());
        }
    }
    out
}

/// True when `dir` (expanded, no glob) is `path` itself or a parent directory of `path`.
///
/// Lets a bare `!~/.agents/skills` exclude every skill under that directory, not just an
/// exact-path match. Accepts both `/` and `\` separators so patterns work on any platform.
fn path_under_dir(dir: &str, path: &str) -> bool {
    if dir.is_empty() || dir.contains('*') {
        return false;
    }
    if path == dir {
        return true;
    }
    for sep in ['/', '\\'] {
        if path.starts_with(&format!("{dir}{sep}")) {
            return true;
        }
    }
    false
}

/// Expand a leading `~/` in an exclude pattern so it matches absolute file paths.
fn expand_user_pattern(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        return PathBuf::from(home).join(rest).to_string_lossy().into_owned();
    }
    s.to_string()
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

    #[test]
    fn filter_prompts_disabled_names_and_path_excludes() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .expect("HOME");
        let mut settings = Settings::defaults();
        settings.resources.disabled_prompts = vec!["legacy-*".into()];
        settings.resources.prompts = vec!["!~/.agents/prompts/*".into()];

        let templates = vec![
            PromptTemplate {
                name: "ship".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: format!("{}/.agents/prompts/ship.md", home.display()),
            },
            PromptTemplate {
                name: "legacy-x".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: format!("{}/.config/elph/prompts/legacy-x.md", home.display()),
            },
            PromptTemplate {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: format!("{}/.config/elph/prompts/keep.md", home.display()),
            },
        ];
        let out = settings.filter_prompts(templates);
        let names: Vec<_> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }

    #[test]
    fn filter_skills_tilde_exclude_matches_absolute_path() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .expect("HOME");
        let mut settings = Settings::defaults();
        settings.resources.skills = vec!["!~/.agents/skills/*".into()];

        let skills = vec![
            Skill {
                name: "demo".into(),
                description: String::new(),
                content: String::new(),
                file_path: format!("{}/.agents/skills/demo/SKILL.md", home.display()),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
            Skill {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                file_path: format!("{}/.config/elph/skills/keep/SKILL.md", home.display()),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
        ];
        let out = settings.filter_skills(skills);
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }

    #[test]
    fn filter_skills_bare_tilde_dir_excludes_whole_directory() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .expect("HOME");
        let mut settings = Settings::defaults();
        settings.resources.skills = vec!["!~/.agents/skills".into()];

        let skills = vec![
            Skill {
                name: "demo".into(),
                description: String::new(),
                content: String::new(),
                file_path: format!("{}/.agents/skills/demo/SKILL.md", home.display()),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
            Skill {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                file_path: format!("{}/.config/elph/skills/keep/SKILL.md", home.display()),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
        ];
        let out = settings.filter_skills(skills);
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }

    #[test]
    fn filter_prompts_bare_tilde_dir_excludes_whole_directory() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .expect("HOME");
        let mut settings = Settings::defaults();
        settings.resources.prompts = vec!["!~/.agents/prompts".into()];

        let templates = vec![
            PromptTemplate {
                name: "ship".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: format!("{}/.agents/prompts/ship.md", home.display()),
            },
            PromptTemplate {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: format!("{}/.config/elph/prompts/keep.md", home.display()),
            },
        ];
        let out = settings.filter_prompts(templates);
        let names: Vec<_> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }

    #[test]
    fn filter_skills_relative_agents_dir_excludes_project_skills() {
        let mut settings = Settings::defaults();
        settings.resources.skills = vec!["!.agents/skills".into(), "!.agents/skills/*".into()];

        let skills = vec![
            Skill {
                name: "demo".into(),
                description: String::new(),
                content: String::new(),
                file_path: "/home/user/proj/.agents/skills/demo/SKILL.md".into(),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
            Skill {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                file_path: "/home/user/proj/.elph/skills/keep/SKILL.md".into(),
                disable_model_invocation: false,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                argument_hint: None,
            },
        ];
        let out = settings.filter_skills(skills);
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }

    #[test]
    fn filter_prompts_relative_agents_dir_excludes_project_prompts() {
        let mut settings = Settings::defaults();
        settings.resources.prompts = vec!["!.agents/prompts".into(), "!.agents/prompts/*".into()];

        let templates = vec![
            PromptTemplate {
                name: "ship".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: "/home/user/proj/.agents/prompts/ship.md".into(),
            },
            PromptTemplate {
                name: "keep".into(),
                description: String::new(),
                content: String::new(),
                argument_hint: None,
                file_path: "/home/user/proj/.elph/prompts/keep.md".into(),
            },
        ];
        let out = settings.filter_prompts(templates);
        let names: Vec<_> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["keep"]);
    }
}
