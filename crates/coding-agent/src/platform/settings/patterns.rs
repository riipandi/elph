//! Glob / include-exclude patterns for settings filters (Pi resource-array intent).

/// Whether `candidate` matches any of `patterns`.
///
/// Empty `patterns` → `true` (no filter).
/// Otherwise: include patterns first (`*` wildcards), then `!` / `-` excludes,
/// then `+` force-includes. A candidate that only matches excludes is out.
pub fn matches_any(patterns: &[String], candidate: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }
    classify(patterns, candidate) != PatternDecision::Exclude
}

/// Filter `items` by `patterns` using [`item_key`].
pub fn filter_owned<T, F>(items: Vec<T>, patterns: &[String], item_key: F) -> Vec<T>
where
    F: Fn(&T) -> String,
{
    if patterns.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| matches_any(patterns, &item_key(item)))
        .collect()
}

/// Model id match: `provider/model_id` **or** bare `model_id`.
pub fn model_matches(patterns: &[String], provider: &str, model_id: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let qualified = format!("{provider}/{model_id}");
    classify(patterns, &qualified) != PatternDecision::Exclude
        || classify(patterns, model_id) != PatternDecision::Exclude
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternDecision {
    Include,
    Exclude,
}

fn classify(patterns: &[String], candidate: &str) -> PatternDecision {
    let mut included = false;
    let mut saw_include = false;
    let mut excluded = false;
    let mut forced_in = false;

    for raw in patterns {
        let pat = raw.trim();
        if pat.is_empty() {
            continue;
        }
        if let Some(rest) = pat.strip_prefix('+') {
            if exact_or_glob(rest, candidate) {
                forced_in = true;
            }
            continue;
        }
        if let Some(rest) = pat.strip_prefix('-') {
            if exact_or_glob(rest, candidate) {
                excluded = true;
            }
            continue;
        }
        if let Some(rest) = pat.strip_prefix('!') {
            if glob_match(rest, candidate) {
                excluded = true;
            }
            continue;
        }
        saw_include = true;
        if glob_match(pat, candidate) {
            included = true;
        }
    }

    if forced_in {
        return PatternDecision::Include;
    }
    if excluded {
        return PatternDecision::Exclude;
    }
    if !saw_include || included {
        PatternDecision::Include
    } else {
        PatternDecision::Exclude
    }
}

fn exact_or_glob(pat: &str, candidate: &str) -> bool {
    if pat.contains('*') {
        glob_match(pat, candidate)
    } else {
        pat == candidate
    }
}

/// `*` matches any run of characters (including `/`). No `**` / `?` / character classes.
fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star = None::<(usize, usize)>;
    while ti < text.len() {
        if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
            continue;
        }
        if pi < pattern.len() && pattern[pi] == text[ti] {
            pi += 1;
            ti += 1;
            continue;
        }
        if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, ti));
            continue;
        }
        return false;
    }
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }
    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_patterns_allow_all() {
        assert!(matches_any(&[], "anything"));
        assert!(model_matches(&[], "openai", "gpt-5"));
    }

    #[test]
    fn glob_star() {
        assert!(glob_match("openai/*", "openai/gpt-5.6-luna"));
        assert!(glob_match("claude-*", "claude-sonnet-4"));
        assert!(!glob_match("openai/*", "anthropic/claude"));
        assert!(glob_match("*", "x"));
        assert!(glob_match("a*c", "abc"));
        assert!(glob_match("a*c", "ac"));
        assert!(!glob_match("a*c", "ab"));
    }

    #[test]
    fn exclude_bang() {
        let pats = vec!["openai/*".into(), "!openai/*preview*".into()];
        assert!(model_matches(&pats, "openai", "gpt-5"));
        assert!(!model_matches(&pats, "openai", "gpt-preview-1"));
        assert!(!model_matches(&pats, "anthropic", "claude"));
    }

    #[test]
    fn plus_force_include() {
        let pats = vec!["openai/*".into(), "+anthropic/claude-sonnet-4".into()];
        assert!(model_matches(&pats, "anthropic", "claude-sonnet-4"));
        assert!(!model_matches(&pats, "anthropic", "claude-opus"));
    }

    #[test]
    fn minus_force_exclude() {
        let pats = vec!["*".into(), "-secret".into()];
        assert!(matches_any(&pats, "ok"));
        assert!(!matches_any(&pats, "secret"));
    }

    #[test]
    fn filter_owned_by_name() {
        let items = vec!["a".to_string(), "b".to_string(), "legacy-x".to_string()];
        let pats = vec!["*".into(), "!legacy-*".into()];
        let out = filter_owned(items, &pats, |s| s.clone());
        assert_eq!(out, vec!["a", "b"]);
    }
}
