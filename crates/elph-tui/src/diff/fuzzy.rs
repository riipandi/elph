//! Fuzzy matching utilities (pi-tui `fuzzy.ts`).

/// Result of a fuzzy match attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzzyMatch {
    pub matches: bool,
    pub score: f64,
}

/// Returns true when all query characters appear in order within `text`.
/// Lower score is a better match.
pub fn fuzzy_match(query: &str, text: &str) -> FuzzyMatch {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();

    let primary = match_query(&query_lower, &text_lower);
    if primary.matches {
        return primary;
    }

    let swapped = swap_alpha_numeric(&query_lower);
    if let Some(swapped_query) = swapped {
        let swapped_match = match_query(&swapped_query, &text_lower);
        if swapped_match.matches {
            return FuzzyMatch {
                matches: true,
                score: swapped_match.score + 5.0,
            };
        }
    }

    primary
}

fn match_query(query: &str, text: &str) -> FuzzyMatch {
    if query.is_empty() {
        return FuzzyMatch {
            matches: true,
            score: 0.0,
        };
    }
    if query.len() > text.len() {
        return FuzzyMatch {
            matches: false,
            score: 0.0,
        };
    }

    let chars: Vec<char> = text.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    let mut query_index = 0usize;
    let mut score = 0.0f64;
    let mut last_match_index: Option<usize> = None;
    let mut consecutive_matches = 0usize;

    for (i, &ch) in chars.iter().enumerate() {
        if query_index >= query_chars.len() {
            break;
        }
        if ch == query_chars[query_index] {
            let is_boundary = i == 0
                || chars
                    .get(i - 1)
                    .is_some_and(|prev| prev.is_ascii_whitespace() || matches!(prev, '-' | '_' | '.' | '/' | ':'));

            if i > 0 && last_match_index == Some(i - 1) {
                consecutive_matches += 1;
                score -= (consecutive_matches * 5) as f64;
            } else {
                consecutive_matches = 0;
                if let Some(last) = last_match_index
                    && i > last + 1
                {
                    score += ((i - last - 1) * 2) as f64;
                }
            }

            if is_boundary {
                score -= 10.0;
            }
            score += i as f64 * 0.1;
            last_match_index = Some(i);
            query_index += 1;
        }
    }

    if query_index < query_chars.len() {
        return FuzzyMatch {
            matches: false,
            score: 0.0,
        };
    }

    if query == text {
        score -= 100.0;
    }

    FuzzyMatch { matches: true, score }
}

fn swap_alpha_numeric(query: &str) -> Option<String> {
    if let Some(caps) = regex_like_alpha_num(query) {
        return Some(format!("{}{}", caps.1, caps.0));
    }
    None
}

fn regex_like_alpha_num(query: &str) -> Option<(&str, &str)> {
    let bytes = query.as_bytes();
    let mut split = None;
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        let prev_alpha = bytes[i - 1].is_ascii_alphabetic();
        let prev_digit = bytes[i - 1].is_ascii_digit();
        let curr_alpha = b.is_ascii_alphabetic();
        let curr_digit = b.is_ascii_digit();
        if (prev_alpha && curr_digit) || (prev_digit && curr_alpha) {
            split = Some(i);
            break;
        }
    }
    let idx = split?;
    let (left, right) = query.split_at(idx);
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left, right))
}

/// Filters and sorts items by fuzzy match quality (best matches first).
/// Whitespace- or slash-separated tokens must all match.
pub fn fuzzy_filter<T>(items: &[T], query: &str, get_text: impl Fn(&T) -> String) -> Vec<T>
where
    T: Clone,
{
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return items.to_vec();
    }

    let tokens: Vec<&str> = trimmed
        .split(|c: char| c.is_whitespace() || c == '/')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return items.to_vec();
    }

    let mut results = Vec::new();
    for item in items {
        let text = get_text(item);
        let mut total_score = 0.0;
        let mut all_match = true;
        for token in &tokens {
            let m = fuzzy_match(token, &text);
            if m.matches {
                total_score += m.score;
            } else {
                all_match = false;
                break;
            }
        }
        if all_match {
            results.push((item.clone(), total_score));
        }
    }

    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    results.into_iter().map(|(item, _)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequence() {
        let m = fuzzy_match("hl", "hello");
        assert!(m.matches);
    }

    #[test]
    fn filters_with_multiple_tokens() {
        let items = vec!["git status", "git commit", "cargo test"];
        let out = fuzzy_filter(&items, "git st", |s| (*s).to_string());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "git status");
    }
}
