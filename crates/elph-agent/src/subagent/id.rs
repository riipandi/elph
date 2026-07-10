//! Human-readable agent identifiers.

use memorable_ids::{GenerateOptions, generate};

pub(crate) const MAX_NAME_ATTEMPTS: usize = 8;

/// Generates a memorable agent name with underscore separators.
///
/// Example: `agent_quick_fox`
pub fn generate_agent_name() -> String {
    let core = generate(GenerateOptions {
        components: 2,
        separator: "_".to_string(),
        suffix: None,
        ..Default::default()
    })
    .expect("valid memorable id options");
    format!("agent_{core}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_agent_name_uses_underscores_without_suffix() {
        let name = generate_agent_name();
        assert!(name.starts_with("agent_"), "expected agent_ prefix, got {name}");
        let core = name.strip_prefix("agent_").expect("core words");
        let parts: Vec<&str> = core.split('_').collect();
        assert_eq!(parts.len(), 2, "expected adjective_noun core, got {name}");
        assert!(parts[0].chars().all(|c| c.is_ascii_alphabetic()));
        assert!(parts[1].chars().all(|c| c.is_ascii_alphabetic()));
        assert!(
            !name
                .rsplit('_')
                .next()
                .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())),
            "agent id should not end with a numeric suffix, got {name}"
        );
    }
}
