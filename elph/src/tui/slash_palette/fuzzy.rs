//! Fuzzy scoring for slash palette command filtering.

use crate::types::SlashCommand;

const PREFIX_MATCH_BASE: i32 = 10_000;
const NAME_WEIGHT: i32 = 4;
const SKILL_SUFFIX_WEIGHT: i32 = 5;
const DESCRIPTION_WEIGHT: i32 = 1;

/// Best fuzzy score for a command against a lowercase query, if any field matches.
pub fn command_match_score(cmd: &SlashCommand, query: &str) -> Option<i32> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }

    let name = cmd.name.to_ascii_lowercase();
    let mut best = field_score(query, &name, NAME_WEIGHT, true);

    if let Some(skill) = name.strip_prefix("skill:") {
        best = max_score(best, field_score(query, skill, SKILL_SUFFIX_WEIGHT, true));
    }

    let description = cmd.description.to_ascii_lowercase();
    best = max_score(best, field_score(query, &description, DESCRIPTION_WEIGHT, false));

    best
}

pub fn max_score(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub fn field_score(query: &str, text: &str, weight: i32, allow_prefix: bool) -> Option<i32> {
    let raw = if allow_prefix {
        prefix_score(query, text).or_else(|| subsequence_score(query, text))
    } else {
        subsequence_score(query, text)
    };
    raw.map(|score| score.saturating_mul(weight))
}

fn prefix_score(query: &str, text: &str) -> Option<i32> {
    if text.starts_with(query) {
        let length_bonus = 200i32.saturating_sub(text.len() as i32);
        Some(PREFIX_MATCH_BASE + length_bonus)
    } else {
        None
    }
}

fn subsequence_score(query: &str, text: &str) -> Option<i32> {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return Some(0);
    }

    let mut score = 0i32;
    let mut query_idx = 0usize;
    let mut prev_match: Option<usize> = None;
    let text_chars: Vec<char> = text.chars().collect();

    for (text_idx, &ch) in text_chars.iter().enumerate() {
        if query_idx >= query_chars.len() {
            break;
        }
        if ch != query_chars[query_idx] {
            continue;
        }

        score += 1;
        if query_idx == 0 {
            score += 15;
            if text_idx == 0 {
                score += 20;
            }
        }

        if let Some(prev) = prev_match {
            if text_idx == prev + 1 {
                score += 10;
            } else {
                score -= (text_idx - prev - 1) as i32;
            }
        }

        if text_idx > 0 && is_word_boundary(text_chars[text_idx - 1]) {
            score += 12;
        }

        prev_match = Some(text_idx);
        query_idx += 1;
    }

    if query_idx == query_chars.len() {
        score += 100i32.saturating_sub(text.len() as i32 / 2);
        Some(score)
    } else {
        None
    }
}

fn is_word_boundary(ch: char) -> bool {
    matches!(ch, ':' | '-' | '_' | ' ' | '/')
}

/// Filter and rank commands by fuzzy relevance to `query`.
pub fn filter_commands(commands: &[SlashCommand], query: &str) -> Vec<SlashCommand> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return commands.to_vec();
    }

    let mut scored: Vec<(SlashCommand, i32)> = commands
        .iter()
        .filter_map(|cmd| command_match_score(cmd, &query).map(|score| (cmd.clone(), score)))
        .collect();

    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.name.cmp(&right.0.name)));
    scored.into_iter().map(|(cmd, _)| cmd).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<SlashCommand> {
        vec![
            SlashCommand::new("compact", "Compact conversation history"),
            SlashCommand::new("goal", "Manage session goals"),
            SlashCommand::new("model", "Select model"),
            SlashCommand::new("reload", "Reload extensions and prompt templates"),
            SlashCommand::new("skill:rust-verify-harden", "Run make check/lint/test and audit Rust changes"),
            SlashCommand::new("skill:tui-design", "Guide terminal UI development with iocraft"),
        ]
    }

    #[test]
    fn prefix_match_ranks_above_subsequence() {
        let filtered = filter_commands(&commands(), "mod");
        assert_eq!(filtered.first().map(|cmd| cmd.name.as_str()), Some("model"));
    }

    #[test]
    fn subsequence_matches_non_prefix_queries() {
        let filtered = filter_commands(&commands(), "mdl");
        assert!(filtered.iter().any(|cmd| cmd.name == "model"));
    }

    #[test]
    fn matches_skill_suffix_without_prefix() {
        let filtered = filter_commands(&commands(), "tui");
        assert_eq!(filtered.first().map(|cmd| cmd.name.as_str()), Some("skill:tui-design"));
    }

    #[test]
    fn acronym_like_query_matches_hyphenated_skill() {
        let filtered = filter_commands(&commands(), "rvh");
        assert_eq!(filtered.first().map(|cmd| cmd.name.as_str()), Some("skill:rust-verify-harden"));
    }

    #[test]
    fn description_match_finds_command_by_hint_text() {
        let filtered = filter_commands(&commands(), "templates");
        assert!(filtered.iter().any(|cmd| cmd.name == "reload"));
    }

    #[test]
    fn empty_query_preserves_input_order() {
        let input = commands();
        let filtered = filter_commands(&input, "");
        assert_eq!(filtered, input);
    }

    #[test]
    fn case_insensitive_matching() {
        let filtered = filter_commands(&commands(), "GO");
        assert_eq!(filtered.first().map(|cmd| cmd.name.as_str()), Some("goal"));
    }
}
