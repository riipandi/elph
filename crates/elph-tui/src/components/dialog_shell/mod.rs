//! Reusable dialog shell: bordered frame, header variants, and preset bodies.

mod chrome;
mod content;
mod frame;
mod header;
mod overlay;

pub use chrome::{
    DIALOG_SELECT_AUTO_HEIGHT, DialogChrome, dialog_body_min_height, dialog_choice_list_height, dialog_divider_line,
    dialog_header_title_fit, dialog_max_content_height, dialog_select_body_plan, dialog_select_fixed_rows,
    dialog_shell_body_height, dialog_shell_chrome_rows, dialog_shell_estimated_height, dialog_shell_outer_height,
    dialog_text_rows, dialog_todo_list_content_rows, select_list_chrome_rows,
};
pub use content::{
    ConfirmButtonAction, ConfirmButtonFocus, DialogConfirmButtonsContent, DialogConfirmButtonsContentProps,
    DialogConfirmContent, DialogConfirmContentProps, DialogModeSelectContent, DialogModeSelectContentProps,
    DialogMultiChoiceContent, DialogMultiChoiceContentProps, DialogQuestionContent, DialogQuestionContentProps,
    DialogTodoListContent, DialogTodoListContentProps, DialogTodoProgressContent, DialogTodoProgressContentProps,
    DialogUserInputContent, DialogUserInputContentProps, MultiChoiceAction, TODO_GLYPH_DONE, TODO_GLYPH_PENDING,
    TODO_GLYPH_RUNNING, TODO_PANEL_DEFAULT_MAX_ROWS, TODO_PANEL_HEADER_PREFIX, TodoPanelRow, TodoProgressPanel,
    TodoProgressPanelProps, build_todo_panel_rows, confirm_button_key_action, dialog_mode_accent,
    dialog_mode_from_index, dialog_mode_select_options, multi_choice_key_action, multi_choice_selected_indices,
    multi_choice_toggle, progress_row_glyph, todo_panel_header_line, todo_row_line, todo_row_prefix, todo_status_glyph,
    todo_status_word,
};
pub use frame::{DialogShell, DialogShellProps};
pub use header::{
    DialogHeader, DialogHeaderRow, DialogHeaderRowProps, DialogHeaderSearch, DialogHeaderSearchProps, DialogHeaderTabs,
    DialogHeaderTabsProps, DialogHeaderTitle, DialogHeaderTitleProps,
};
pub use overlay::{DialogShellOverlay, DialogShellOverlayProps, dialog_overlay_left, dialog_overlay_top};
