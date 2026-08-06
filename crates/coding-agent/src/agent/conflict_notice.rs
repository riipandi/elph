//! Shared formatting for resource name-conflict notices (skills, agents, templates).

use super::agents_load::AgentConflict;
use super::skills_load::SkillConflict;

/// A prompt template name defined in multiple directories; the later directory wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateConflict {
    pub name: String,
    pub overridden_label: String,
    pub winner_label: String,
}

/// Same slash name defined as both a skill and a prompt template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossKindConflict {
    pub name: String,
    /// Which kind wins for slash dispatch when both exist (prompt template before skill).
    pub slash_winner: &'static str,
}

/// User-facing multi-section notice for resource name conflicts.
///
/// Prefer sending via [`crate::agent::AgentUiEvent::TranscriptNotice`] so the card
/// is sticky in the transcript.
pub fn format_name_conflicts(
    skill_conflicts: &[SkillConflict],
    agent_conflicts: &[AgentConflict],
    template_conflicts: &[TemplateConflict],
    cross_kind_conflicts: &[CrossKindConflict],
) -> Option<String> {
    if skill_conflicts.is_empty()
        && agent_conflicts.is_empty()
        && template_conflicts.is_empty()
        && cross_kind_conflicts.is_empty()
    {
        return None;
    }

    let mut lines = vec!["Resource name conflicts resolved (higher-priority path wins):".to_string()];

    if !skill_conflicts.is_empty() {
        lines.push("Skills:".into());
        for c in skill_conflicts {
            lines.push(format!("  • {}: {} → {}", c.name, c.overridden_label, c.winner_label));
        }
    }
    if !template_conflicts.is_empty() {
        lines.push("Prompt templates:".into());
        for c in template_conflicts {
            lines.push(format!("  • {}: {} → {}", c.name, c.overridden_label, c.winner_label));
        }
    }
    if !agent_conflicts.is_empty() {
        lines.push("Agents:".into());
        for c in agent_conflicts {
            lines.push(format!("  • {}: {} → {}", c.name, c.overridden_label, c.winner_label));
        }
    }
    if !cross_kind_conflicts.is_empty() {
        lines.push("Same name as skill and prompt template (slash prefers prompt template):".into());
        for c in cross_kind_conflicts {
            lines.push(format!("  • /{} → {}", c.name, c.slash_winner));
        }
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_none() {
        assert!(format_name_conflicts(&[], &[], &[], &[]).is_none());
    }

    #[test]
    fn skills_only_section() {
        let text = format_name_conflicts(
            &[SkillConflict {
                name: "review".into(),
                overridden_label: "a".into(),
                winner_label: "b".into(),
            }],
            &[],
            &[],
            &[],
        )
        .expect("notice");
        assert!(text.contains("Skills:"));
        assert!(text.contains("review"));
        assert!(!text.contains("Agents:"));
    }
}
