//! Editor keybinding matching for raw terminal input.

/// Editor actions mapped from pi-tui `tui.editor.*` bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorLineStart,
    CursorLineEnd,
    PageUp,
    PageDown,
    DeleteCharBackward,
    DeleteCharForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteToLineStart,
    DeleteToLineEnd,
    Yank,
    YankPop,
    Undo,
    NewLine,
    Submit,
    Tab,
    InsertText,
}

/// Maps a raw input chunk to an editor action when recognized.
pub fn match_editor_action(data: &str) -> Option<EditorAction> {
    match data {
        "\x1b[A" | "\x1bOA" => Some(EditorAction::CursorUp),
        "\x1b[B" | "\x1bOB" => Some(EditorAction::CursorDown),
        "\x1b[D" | "\x1bOD" => Some(EditorAction::CursorLeft),
        "\x1b[C" | "\x1bOC" => Some(EditorAction::CursorRight),
        "\x1b[1;5D" | "\x1b[1;3D" | "\x1b[1;9D" => Some(EditorAction::CursorWordLeft),
        "\x1b[1;5C" | "\x1b[1;3C" | "\x1b[1;9C" => Some(EditorAction::CursorWordRight),
        "\x01" | "\x1b[H" | "\x1b[1~" | "\x1bOH" => Some(EditorAction::CursorLineStart),
        "\x05" | "\x1b[F" | "\x1b[4~" | "\x1bOF" => Some(EditorAction::CursorLineEnd),
        "\x1b[5~" => Some(EditorAction::PageUp),
        "\x1b[6~" => Some(EditorAction::PageDown),
        "\x7f" | "\x08" => Some(EditorAction::DeleteCharBackward),
        "\x1b[3~" => Some(EditorAction::DeleteCharForward),
        "\x17" | "\x1b\x7f" => Some(EditorAction::DeleteWordBackward),
        "\x1b[3;5~" | "\x1b[1;3C\x7f" => Some(EditorAction::DeleteWordForward),
        "\x15" => Some(EditorAction::DeleteToLineStart),
        "\x0b" => Some(EditorAction::DeleteToLineEnd),
        "\x19" => Some(EditorAction::Yank),
        "\x1bY" | "\x1by" => Some(EditorAction::YankPop),
        "\x1f" | "\x1b-" | "\x1b\x1f" => Some(EditorAction::Undo),
        "\x18" => Some(EditorAction::NewLine),
        "\x1b\r" | "\x1b\n" | "\x1b[13;2~" | "\x1b[27;2;13~" => Some(EditorAction::NewLine),
        "\r" | "\n" => Some(EditorAction::Submit),
        "\t" => Some(EditorAction::Tab),
        _ if is_printable(data) => Some(EditorAction::InsertText),
        _ => None,
    }
}

fn is_printable(data: &str) -> bool {
    if data.is_empty() || data.starts_with('\x1b') {
        return false;
    }
    !data.chars().any(|c| c.is_control() && c != '\t')
}
