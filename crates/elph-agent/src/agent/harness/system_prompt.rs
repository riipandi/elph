//! System prompt formatting — elph-agent module.

use std::path::Path;

use crate::agent::harness::types::Skill;

/// Scope metadata key (soft-typed, parsed from `SKILL.md` frontmatter).
///
/// A skill authored with `scope: project` is only advertised while the current
/// session works inside the directory tree containing that skill's `SKILL.md`.
/// `scope: global` (or no scope metadata at all) keeps the skill always visible.
pub const SKILL_SCOPE_METADATA_KEY: &str = "scope";
const SKILL_SCOPE_PROJECT: &str = "project";
const SKILL_SCOPE_GLOBAL: &str = "global";

/// Return the scope tag from a skill's `metadata` map, if set and well-formed.
fn skill_scope(skill: &Skill) -> Option<&str> {
    let value = skill.metadata.as_ref()?.get(SKILL_SCOPE_METADATA_KEY)?;
    match value {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim()),
        _ => None,
    }
}

/// Directory containing a skill's `SKILL.md` (its "project" home).
fn skill_project_dir(skill: &Skill) -> Option<&Path> {
    // A project-scoped skill lives at `<project>/.agents/skills/<name>/SKILL.md`
    // (or `<project>/.elph/skills/...`). The project root is the fourth level up
    // from SKILL.md (SKILL.md → <name> → skills → .agents → <project>).
    Path::new(&skill.file_path).parent()?.parent()?.parent()?.parent()
}

/// Component-wise path prefix check (`/r` must NOT match `/r9`).
fn path_starts_with(path: &Path, dir: &Path) -> bool {
    let mut comps = path.components();
    for dir_comp in dir.components() {
        if comps.next() != Some(dir_comp) {
            return false;
        }
    }
    true
}

/// Filter skills to those relevant to the current working directory.
///
/// Rules (backward compatible — a skill without scope metadata keeps its current
/// always-visible behavior):
/// - no scope metadata  → always included
/// - `scope: global`    → always included
/// - `scope: project`   → included only when this skill's directory is an
///   ancestor of (or equal to) `cwd`
///
/// Unknown scope values default to visible (treat as unset) so a typo never
/// silently hides a skill.
pub fn filter_skills_for_context<'a>(skills: &'a [Skill], cwd: &Path) -> Vec<&'a Skill> {
    let mut out = Vec::new();
    for skill in skills {
        let visible = match skill_scope(skill) {
            Some(SKILL_SCOPE_PROJECT) => {
                let root = match skill_project_dir(skill) {
                    Some(root) => root,
                    None => continue,
                };
                path_starts_with(cwd, root)
            }
            Some(SKILL_SCOPE_GLOBAL) | None => true,
            Some(_) => true, // unknown scope → visible
        };
        if visible {
            out.push(skill);
        }
    }
    out
}

/// Format model-visible skills for the system prompt with XML escaping.
pub fn format_skills_for_system_prompt(skills: &[Skill]) -> String {
    let visible_skills: Vec<_> = skills.iter().filter(|skill| !skill.disable_model_invocation).collect();
    if visible_skills.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "Use a matching skill; read its full file first and resolve relative references from the skill directory."
            .to_string(),
        String::new(),
        "<available_skills>".to_string(),
    ];

    for skill in visible_skills {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!("    <description>{}</description>", escape_xml(&skill.description)));
        lines.push(format!("    <location>{}</location>", escape_xml(&skill.file_path)));
        lines.push("  </skill>".to_string());
    }

    lines.push("</available_skills>".to_string());
    lines.join("\n")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn skill(name: &str, file_path: &str, scope: Option<&str>) -> Skill {
        let metadata = scope.map(|value| {
            let mut map = std::collections::HashMap::new();
            map.insert(SKILL_SCOPE_METADATA_KEY.to_string(), json!(value));
            map
        });
        Skill {
            name: name.to_string(),
            description: format!("{name} description"),
            content: "# body".to_string(),
            file_path: file_path.to_string(),
            disable_model_invocation: false,
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: None,
            argument_hint: None,
        }
    }

    #[test]
    fn escapes_xml_entities() {
        assert_eq!(escape_xml("a&b<c>\"d'"), "a&amp;b&lt;c&gt;&quot;d&apos;");
    }

    #[test]
    fn unset_scope_stays_visible() {
        let skills = vec![skill("plain", "/r/.agents/skills/plain/SKILL.md", None)];
        let kept = filter_skills_for_context(&skills, Path::new("/elsewhere"));
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn global_scope_always_visible() {
        let skills = vec![skill("glob", "/home/u/.agents/skills/glob/SKILL.md", Some("global"))];
        for cwd in ["/", "/repo", "/elsewhere"] {
            assert_eq!(filter_skills_for_context(&skills, Path::new(cwd)).len(), 1, "cwd={cwd}");
        }
    }

    #[test]
    fn project_scope_matches_only_inside_skill_dir() {
        let dir = std::env::temp_dir().join("elph_skill_scope_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo/.agents/skills/rust-lean")).unwrap();
        std::fs::create_dir_all(dir.join("repo2/.agents/skills/a")).unwrap();
        std::fs::create_dir_all(dir.join("repo/.r2/skills/p2")).unwrap();

        let mut make = |name: &str, path: &Path, scope: &str| {
            let mut s = skill(name, &path.to_string_lossy(), Some(scope));
            s.file_path = path.to_string_lossy().into_owned();
            s
        };
        let skills = vec![
            make("rust-lean", &dir.join("repo/.agents/skills/rust-lean/SKILL.md"), "project"),
            make("a", &dir.join("repo2/.agents/skills/a/SKILL.md"), "project"),
            make("p2", &dir.join("repo/.r2/skills/p2/SKILL.md"), "project"),
        ];
        // NOTE: dir.join("repo") path root = <temp>/elph_skill_scope_test/repo.
        // Skill project root (three up from SKILL.md) for rust-lean is <dir>/repo.
        let root = dir.join("repo");
        let r2 = dir.join("repo2");
        let names = |p: &Path| {
            filter_skills_for_context(&skills, p)
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        };
        // rust-lean's project root is <dir>/repo; p2's is also <dir>/repo (custom
        // layout) so it matches inside root too — assert both.
        assert_eq!(names(&root), vec!["rust-lean", "p2"], "got {:?}", names(&root));
        assert_eq!(names(&root.join("src")), vec!["rust-lean", "p2"]);
        // Sibling project must only expose its own skill, never rust-lean.
        assert_eq!(names(&r2), vec!["a"]);
        assert_eq!(filter_skills_for_context(&skills, &dir.join("other")).len(), 0);
        // Inside `.r2/skills/p2` both project skills rooted at <dir>/repo match.
        let nested = filter_skills_for_context(&skills, &root.join(".r2/skills/p2"));
        assert_eq!(nested.len(), 2);
    }

    #[test]
    fn unknown_scope_value_defaults_to_visible() {
        let skills = vec![skill("odd", "/r/.agents/skills/odd/SKILL.md", Some("sometimes"))];
        assert_eq!(filter_skills_for_context(&skills, Path::new("/anywhere")).len(), 1);
    }

    #[test]
    fn project_scoping_filters_but_keeps_unscoped() {
        let dir = std::env::temp_dir().join("elph_skill_scope_filters");
        let _ = std::fs::remove_dir_all(&dir);
        for (name, scope) in [
            ("rust-local2", Some("project")),
            ("global-skill", Some("global")),
            ("legacy", None),
            ("other-repo", Some("project")),
        ] {
            let p = dir
                .join(if name == "other-repo" { "elsewhere" } else { "repo8" })
                .join(".agents/skills")
                .join(name)
                .join("SKILL.md");
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        }
        let skills = vec![
            skill(
                "rust-local2",
                &format!("{}/repo8/.agents/skills/rust-local2/SKILL.md", dir.display()),
                Some("project"),
            ),
            skill("global-skill", "/home/u/.agents/skills/global-skill/SKILL.md", Some("global")),
            skill("legacy", "/home/u/.agents/skills/legacy/SKILL.md", None),
            skill(
                "other-repo",
                &format!("{}/elsewhere/.agents/skills/other-repo/SKILL.md", dir.display()),
                Some("project"),
            ),
        ];
        let cwd = dir.join("repo8").join("src");
        let kept = filter_skills_for_context(&skills, &cwd);
        let names: String = kept.iter().map(|s| format!("{},", s.name)).collect();
        assert!(names.contains("rust-local2"), "{names}");
        assert!(names.contains("global-skill"));
        assert!(names.contains("legacy"));
        assert!(!names.contains("other-repo"));
    }

    #[test]
    fn projects_are_not_mixed_by_prefix() {
        let dir = std::env::temp_dir().join("elph_skill_prefix_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo2/.agents/skills/a")).unwrap();
        std::fs::create_dir_all(dir.join("repo/.agents/skills/nested")).unwrap();
        let skills = vec![
            skill(
                "a",
                &format!("{}/repo2/.agents/skills/a/SKILL.md", dir.display()).as_str(),
                Some("project"),
            ),
            skill(
                "nested",
                &format!("{}/repo/.agents/skills/nested/SKILL.md", dir.display()),
                Some("project"),
            ),
        ];
        // "/repo" is a strict prefix of "/repo2" — they must not match each other.
        assert_eq!(filter_skills_for_context(&skills, &dir.join("repo9")).len(), 0);
        let r2 = filter_skills_for_context(&skills, &dir.join("repo2"));
        assert_eq!(r2.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["a"]);
        let r = filter_skills_for_context(&skills, &dir.join("repo"));
        assert_eq!(r.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["nested"]);
    }

    #[test]
    fn disable_model_invocation_still_respected() {
        let mut s = skill("hidden", "/r/.agents/skills/hidden/SKILL.md", None);
        s.disable_model_invocation = true;
        let rendered = format_skills_for_system_prompt(&[s]);
        assert!(rendered.is_empty());
    }
}
