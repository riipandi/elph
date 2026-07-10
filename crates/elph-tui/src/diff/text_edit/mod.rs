mod delete;
mod movement;

pub use delete::{
    delete_char_backward, delete_char_forward, delete_to_line_end, delete_to_line_start, delete_word_backward,
    delete_word_forward,
};
pub use movement::{char_left, char_right, line_end, line_start, word_left, word_right};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_and_end() {
        let text = "hello\nworld";
        assert_eq!(line_start(text, 7), 6);
        assert_eq!(line_end(text, 7), 11);
    }

    #[test]
    fn deletes_to_line_start() {
        let (next, cursor) = delete_to_line_start("hello world", 6);
        assert_eq!(next, "world");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn deletes_to_line_end() {
        let (next, cursor) = delete_to_line_end("hello world", 6);
        assert_eq!(next, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let (next, cursor) = delete_word_backward("hello world", 11);
        assert_eq!(next, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_forward_removes_next_word() {
        let (next, cursor) = delete_word_forward("hello world", 0);
        assert_eq!(next, "world");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn char_navigation_moves_by_scalar() {
        assert_eq!(char_left("héllo", 5), 4);
        assert_eq!(char_right("héllo", 3), 4);
    }

    #[test]
    fn word_left_skips_to_previous_word() {
        assert_eq!(word_left("hello world", 11), 6);
        assert_eq!(word_left("hello world", 6), 0);
    }

    #[test]
    fn word_right_skips_to_next_word() {
        assert_eq!(word_right("hello world", 0), 6);
        assert_eq!(word_right("hello world", 6), 11);
    }

    #[test]
    fn delete_to_line_start_on_empty_line_removes_newline() {
        let (next, cursor) = delete_to_line_start("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn delete_to_line_end_on_empty_line_removes_newline() {
        let (next, cursor) = delete_to_line_end("line1\n\n", 6);
        assert_eq!(next, "line1\n");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_backward_on_blank_line_removes_newline() {
        let (next, cursor) = delete_word_backward("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn delete_word_forward_on_blank_line_removes_newline() {
        let (next, cursor) = delete_word_forward("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn word_left_on_blank_line_moves_one_char() {
        assert_eq!(word_left("hello\n\n", 6), 5);
    }

    #[test]
    fn word_right_on_blank_line_moves_one_char() {
        assert_eq!(word_right("hello\n\nline", 6), 7);
    }

    #[test]
    fn delete_word_backward_on_whitespace_only_span() {
        let (next, cursor) = delete_word_backward("line1\n   \nline3", 8);
        assert_eq!(next, "line1\n  \nline3");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn delete_to_line_start_on_whitespace_only_line() {
        let (next, cursor) = delete_to_line_start("line1\n   \nline3", 9);
        assert_eq!(next, "line1\n\nline3");
        assert_eq!(cursor, 6);
    }
}
