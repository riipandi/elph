use elph_tui::utils::*;

#[test]
fn wraps_words() {
    let lines = wrap_text("hello world foo", 8);
    assert_eq!(lines, vec!["hello", "world", "foo"]);
}

#[test]
fn truncates() {
    assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    assert_eq!(truncate_with_ellipsis("hello world", 8), "hello w…");
}

#[test]
fn wrap_preserves_paragraph_breaks() {
    let lines = wrap_text("line one\nline two", 20);
    assert_eq!(lines, vec!["line one", "line two"]);
}

#[test]
fn wrap_empty_string_returns_blank_line() {
    assert_eq!(wrap_text("", 10), vec![""]);
}

#[test]
fn wrap_splits_oversized_word_by_grapheme() {
    let lines = wrap_text("abcdefghij", 4);
    assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
}

#[test]
fn truncate_zero_width_is_empty() {
    assert_eq!(truncate_with_ellipsis("hello", 0), "");
}

#[test]
fn truncate_width_one_is_ellipsis_only() {
    assert_eq!(truncate_with_ellipsis("hello", 1), "…");
}

#[test]
fn wrap_single_word_fits() {
    assert_eq!(wrap_text("hello", 10), vec!["hello"]);
}
