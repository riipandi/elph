//! Skill slash-command argument requirements.

use serde_json::Value;

use crate::agent::harness::types::Skill;

/// Returns true when the hint contains required placeholders (`<name>`).
pub fn argument_hint_requires_args(hint: &str) -> bool {
    first_required_placeholder(hint).is_some()
}

/// Returns true when skill metadata explicitly marks arguments as required.
pub fn metadata_requires_arguments(metadata: &std::collections::HashMap<String, Value>) -> bool {
    for key in ["requires-arguments", "requires_arguments", "requiresArguments"] {
        if let Some(value) = metadata.get(key)
            && metadata_flag_truthy(value)
        {
            return true;
        }
    }
    false
}

/// Whether this skill expects user-provided slash arguments before invocation.
pub fn skill_requires_arguments(skill: &Skill) -> bool {
    if skill.argument_hint.as_deref().is_some_and(argument_hint_requires_args) {
        return true;
    }
    skill.metadata.as_ref().is_some_and(metadata_requires_arguments)
}

/// Transcript notice when a skill that requires args was invoked without them.
pub fn skill_args_validation_notice(skill: &Skill, args: &str) -> Option<String> {
    if !skill_requires_arguments(skill) || !args.trim().is_empty() {
        return None;
    }
    Some(format_skill_missing_args_notice(&skill.name, skill.argument_hint.as_deref()))
}

pub fn format_skill_missing_args_notice(skill_name: &str, hint: Option<&str>) -> String {
    let slash = format!("/skill:{skill_name}");
    match hint.filter(|text| !text.trim().is_empty()) {
        Some(hint) => {
            let example = first_required_placeholder(hint)
                .map(|placeholder| format!("{slash} {placeholder}"))
                .unwrap_or_else(|| format!("{slash} {hint}"));
            format!("Skill \"{skill_name}\" requires arguments: {hint}. Usage: {example}")
        }
        None => format!("Skill \"{skill_name}\" requires arguments. Usage: {slash} <arguments>"),
    }
}

fn first_required_placeholder(hint: &str) -> Option<String> {
    let start = hint.find('<')?;
    let after = &hint[start + 1..];
    let end = after.find('>')?;
    let inner = after[..end].trim();
    if inner.is_empty() {
        return None;
    }
    Some(format!("<{inner}>"))
}

fn metadata_flag_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(enabled) => *enabled,
        Value::String(text) => matches!(text.trim().to_ascii_lowercase().as_str(), "true" | "yes" | "1"),
        Value::Number(number) => number.as_i64() == Some(1),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_skill() -> Skill {
        Skill {
            name: "code-review".into(),
            description: "Review code".into(),
            content: "Review".into(),
            file_path: "/tmp/code-review/SKILL.md".into(),
            argument_hint: Some("<file-path>".into()),
            ..Default::default()
        }
    }

    #[test]
    fn detects_required_placeholders_in_hint() {
        assert!(argument_hint_requires_args("<file-path>"));
        assert!(argument_hint_requires_args("<file> [depth]"));
        assert!(!argument_hint_requires_args("[optional-only]"));
        assert!(!argument_hint_requires_args(""));
    }

    #[test]
    fn metadata_requires_arguments_flag() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("requires-arguments".into(), Value::Bool(true));
        let skill = Skill {
            metadata: Some(metadata),
            ..Default::default()
        };
        assert!(skill_requires_arguments(&skill));
    }

    #[test]
    fn notice_when_required_args_missing() {
        let skill = sample_skill();
        let notice = skill_args_validation_notice(&skill, "").expect("notice");
        assert!(notice.contains("code-review"));
        assert!(notice.contains("<file-path>"));
        assert!(notice.contains("/skill:code-review"));
    }

    #[test]
    fn no_notice_when_args_present() {
        let skill = sample_skill();
        assert!(skill_args_validation_notice(&skill, "src/main.rs").is_none());
    }

    #[test]
    fn no_notice_for_optional_only_hint() {
        let skill = Skill {
            argument_hint: Some("[focus-area]".into()),
            ..sample_skill()
        };
        assert!(skill_args_validation_notice(&skill, "").is_none());
    }
}
