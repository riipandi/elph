use slt::Context;

/// Gap between major shell regions (status / chat / input / footer).
pub fn shell_section_gap(ui: &Context) -> u32 {
    ui.spacing().xs()
}

/// Gap inside the input stack (activity → palette → prompt).
pub fn shell_input_gap(ui: &Context, composer: bool) -> u32 {
    if composer { 0 } else { ui.spacing().xs() }
}

/// Padding inside bordered panels (slash palette, compact tool hints).
pub fn shell_panel_pad(ui: &Context) -> u32 {
    ui.spacing().xs()
}

/// Padding inside bordered transcript cards (user turns, expanded tool output).
pub fn shell_chat_card_pad(ui: &Context) -> u32 {
    ui.spacing().xs().saturating_add(1)
}

/// Horizontal inset for the scrollable chat transcript.
pub fn shell_chat_pad_x(ui: &Context) -> u32 {
    ui.spacing().xs().saturating_add(1)
}

/// Vertical inset inside the chat scroll viewport (top/bottom breathing room).
pub fn shell_chat_pad_y(ui: &Context) -> u32 {
    ui.spacing().xs()
}

/// Gap between transcript blocks in composer layout.
pub fn shell_chat_block_gap(ui: &Context) -> u32 {
    ui.spacing().xs().saturating_add(1)
}

/// Vertical padding inside the prompt border.
pub fn shell_prompt_pad_y(ui: &Context) -> u32 {
    shell_prompt_pad(ui)
}

/// Horizontal padding inside the prompt border (left/right).
pub fn shell_prompt_pad_x(ui: &Context) -> u32 {
    let _ = ui;
    1
}

/// Tight vertical padding for the prompt chrome.
pub fn shell_prompt_pad(ui: &Context) -> u32 {
    let _ = ui;
    0
}
