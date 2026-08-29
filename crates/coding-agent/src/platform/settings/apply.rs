//! Host-side maps from [`Settings`] into session/runtime options.

use std::path::{Path, PathBuf};

use elph_agent::harness::{PromptTemplate, Skill};

use super::Settings;
use super::patterns::model_matches;

impl Settings {
    /// Project skill/prompt directories are always scanned (same as settings merge).
    pub fn include_project_resources(&self) -> bool {
        true
    }

    /// Filter discovered skills by disabled names and ordered `resources.skills` patterns.
    pub fn filter_skills(&self, skills: Vec<Skill>) -> Vec<Skill> {
        self.filter_skills_with_base(skills, None)
    }

    /// Filter skills using the project directory to resolve relative resource paths.
    pub(crate) fn filter_skills_for_project(&self, skills: Vec<Skill>, project_dir: &Path) -> Vec<Skill> {
        self.filter_skills_with_base(skills, Some(project_dir))
    }

    fn filter_skills_with_base(&self, skills: Vec<Skill>, base_dir: Option<&Path>) -> Vec<Skill> {
        skills
            .into_iter()
            .filter(|s| !name_denied(&self.resources.disabled_skills, &s.name))
            .filter(|s| !path_or_name_excluded(&self.resources.skills, &s.name, &s.file_path, base_dir))
            .collect()
    }

    /// Filter discovered prompt templates by `resources.disabledPrompts` and `resources.prompts` `!` / `-` patterns.
    pub fn filter_prompts(&self, templates: Vec<PromptTemplate>) -> Vec<PromptTemplate> {
        templates
            .into_iter()
            .filter(|t| !name_denied(&self.resources.disabled_prompts, &t.name))
            .filter(|t| !path_or_name_excluded(&self.resources.prompts, &t.name, &t.file_path, None))
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

fn path_or_name_excluded(resource_patterns: &[String], name: &str, path: &str, base_dir: Option<&Path>) -> bool {
    let mut excluded = false;
    for pat in resource_patterns {
        let t = pat.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix('!').or_else(|| t.strip_prefix('-')) {
            if resource_pattern_matches(rest, name, path, base_dir, t.starts_with('-')) {
                excluded = true;
            }
        } else {
            let rest = t.strip_prefix('+').unwrap_or(t);
            if resource_pattern_matches(rest, name, path, base_dir, t.starts_with('+')) {
                excluded = false;
            }
        }
    }
    excluded
}

fn resource_pattern_matches(raw_pattern: &str, name: &str, path: &str, base_dir: Option<&Path>, exact: bool) -> bool {
    let pattern = raw_pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    let path_like = is_path_pattern(pattern);
    let name_matches = if exact {
        pattern == name
    } else {
        crate::platform::settings::patterns::matches_any(&[pattern.to_string()], name)
    };
    if name_matches {
        return true;
    }
    path_like && path_matches_pattern(&expand_resource_pattern(pattern, base_dir), path)
}

fn is_path_pattern(pattern: &str) -> bool {
    pattern == "~"
        || pattern.starts_with("~/")
        || pattern.starts_with("./")
        || pattern.starts_with("../")
        || pattern.contains('/')
        || pattern.contains('\\')
        || Path::new(pattern).is_absolute()
}

fn expand_resource_pattern(pattern: &str, base_dir: Option<&Path>) -> String {
    let expanded = expand_user_pattern(pattern);
    if expanded == pattern
        && !Path::new(pattern).is_absolute()
        && let Some(base_dir) = base_dir
    {
        return base_dir.join(pattern).to_string_lossy().into_owned();
    }
    expanded
}

/// True when `pat` excludes `path`: as an absolute path, as a parent directory of `path`,
/// or as a relative suffix of `path` (so `!.agents/skills` / `!.agents/skills/*` exclude
/// project skills regardless of the project's absolute location).
fn path_matches_pattern(pat: &str, path: &str) -> bool {
    // Normalize separators so `~`-expanded patterns (platform separators, e.g. `\` on
    // Windows) still match paths that may use `/` or `\` interchangeably.
    let pat = normalize_separators(pat);
    let path = normalize_separators(path);
    if path_under_dir(&pat, &path) {
        return true;
    }
    if crate::platform::settings::patterns::matches_any(std::slice::from_ref(&pat), &path) {
        return true;
    }
    for suffix in path_suffixes(&path) {
        if crate::platform::settings::patterns::matches_any(std::slice::from_ref(&pat), &suffix)
            || path_under_dir(&pat, &suffix)
        {
            return true;
        }
    }
    false
}

/// Replace `\` with `/` so path matching is separator-agnostic across platforms.
fn normalize_separators(s: &str) -> String {
    s.replace('\\', "/")
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
    fn path_matches_pattern_normalizes_windows_separators() {
        // On Windows `~`-expansion yields `\` separators while paths may use `/`.
        let pat = "C:\\Users\\foo\\.agents\\skills\\*";
        let path = "C:/Users/foo/.agents/skills/demo/SKILL.md";
        assert!(path_matches_pattern(pat, path));
        assert!(path_matches_pattern(pat, "C:\\Users\\foo\\.agents\\skills\\demo\\SKILL.md"));
        assert!(!path_matches_pattern(pat, "C/Users/foo/.agents/sks/demo/SKILL.md"));
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
    fn filter_skills_reincludes_explicit_paths_after_directory_exclude() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .expect("HOME");
        let project = tempfile::tempdir().expect("project");
        let project_skill = project.path().join(".agents/skills/project/SKILL.md");
        let skill = |name: &str, file_path: std::path::PathBuf| Skill {
            name: name.into(),
            description: String::new(),
            content: String::new(),
            file_path: file_path.to_string_lossy().into_owned(),
            disable_model_invocation: false,
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
            argument_hint: None,
        };
        let mut settings = Settings::defaults();
        settings.resources.skills = vec![
            ".agents/skills".into(),
            "!~/.agents/skills/*".into(),
            "~/.agents/skills/commit-only".into(),
            "~/.agents/skills/identify".into(),
        ];

        let skills = vec![
            skill("project", project_skill),
            skill("commit-only", home.join(".agents/skills/commit-only/SKILL.md")),
            skill("identify", home.join(".agents/skills/identify/SKILL.md")),
            skill("other", home.join(".agents/skills/other/SKILL.md")),
        ];
        let out = settings.filter_skills_for_project(skills, project.path());
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["project", "commit-only", "identify"]);
    }

    #[test]
    fn filter_skills_keeps_name_patterns_relative_to_project() {
        let mut settings = Settings::defaults();
        settings.resources.skills = vec!["!legacy-*".into(), "legacy-keep".into()];

        let skill = |name: &str| Skill {
            name: name.into(),
            description: String::new(),
            content: String::new(),
            file_path: format!("/home/user/project/.agents/skills/{name}/SKILL.md"),
            disable_model_invocation: false,
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
            argument_hint: None,
        };
        let out = settings.filter_skills_for_project(
            vec![skill("legacy-drop"), skill("legacy-keep"), skill("current")],
            Path::new("/home/user/project"),
        );
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["legacy-keep", "current"]);
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
