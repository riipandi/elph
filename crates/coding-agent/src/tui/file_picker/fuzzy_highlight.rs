//! Fuzzy match highlighting for `@` file picker rows.

use std::collections::BTreeSet;

/// One coalesced text run in a file picker row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePickerTextRun {
    pub text: String,
    pub matched: bool,
}

/// Character indices in `text` that fuzzy-match `query` (case-insensitive).
pub fn fuzzy_match_char_indices(query: &str, text: &str) -> Option<Vec<usize>> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let query_chars: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query_chars.is_empty() {
        return None;
    }

    let text_chars: Vec<char> = text.chars().collect();

    if text_chars.len() >= query_chars.len()
        && text_chars
            .iter()
            .zip(query_chars.iter())
            .take(query_chars.len())
            .all(|(text_ch, query_ch)| chars_eq_ignore_case(*text_ch, *query_ch))
    {
        return Some((0..query_chars.len()).collect());
    }

    let mut indices = Vec::with_capacity(query_chars.len());
    let mut query_idx = 0usize;
    for (text_idx, ch) in text_chars.iter().enumerate() {
        if query_idx >= query_chars.len() {
            break;
        }
        if chars_eq_ignore_case(*ch, query_chars[query_idx]) {
            indices.push(text_idx);
            query_idx += 1;
        }
    }

    if query_idx == query_chars.len() {
        Some(indices)
    } else {
        None
    }
}

/// Split a row into coalesced runs for foreground fuzzy highlighting.
pub fn file_picker_row_runs(prefix: &str, path: &str, query: &str) -> Vec<FilePickerTextRun> {
    let mut runs = Vec::new();
    if !prefix.is_empty() {
        runs.push(FilePickerTextRun {
            text: prefix.to_string(),
            matched: false,
        });
    }

    let match_set: BTreeSet<usize> = fuzzy_match_char_indices(query, path)
        .unwrap_or_default()
        .into_iter()
        .collect();

    if match_set.is_empty() {
        runs.push(FilePickerTextRun {
            text: path.to_string(),
            matched: false,
        });
        return runs;
    }

    let mut run = String::new();
    let mut run_match = false;
    for (index, ch) in path.chars().enumerate() {
        let matched = match_set.contains(&index);
        if run.is_empty() {
            run_match = matched;
            run.push(ch);
            continue;
        }
        if matched == run_match {
            run.push(ch);
            continue;
        }
        runs.push(FilePickerTextRun {
            text: run.clone(),
            matched: run_match,
        });
        run.clear();
        run_match = matched;
        run.push(ch);
    }
    if !run.is_empty() {
        runs.push(FilePickerTextRun {
            text: run,
            matched: run_match,
        });
    }

    runs
}

fn chars_eq_ignore_case(left: char, right: char) -> bool {
    left.to_lowercase().eq(right.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_match_highlights_leading_chars() {
        let indices = fuzzy_match_char_indices("src", "src/main.rs").expect("match");
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn subsequence_match_skips_separators() {
        let indices = fuzzy_match_char_indices("smr", "src/main.rs").expect("match");
        assert_eq!(indices, vec![0, 4, 9]);
    }

    #[test]
    fn case_insensitive_matching() {
        let indices = fuzzy_match_char_indices("MAIN", "src/main.rs").expect("match");
        assert!(indices.contains(&4));
        assert!(indices.contains(&5));
    }

    #[test]
    fn empty_query_returns_none() {
        assert!(fuzzy_match_char_indices("", "src/main.rs").is_none());
    }

    #[test]
    fn row_runs_mark_matched_segments() {
        let runs = file_picker_row_runs("❯ ", "src/main.rs", "main");
        assert!(runs.iter().any(|run| run.matched && run.text.contains("main")));
        assert!(runs.iter().any(|run| !run.matched && run.text == "❯ "));
    }
}
