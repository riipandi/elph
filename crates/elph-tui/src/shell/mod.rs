mod layout;
mod run_config;
mod spacing;

pub use layout::{ShellChrome, ShellRegion, ShellTier, layout_pad, render_agent_shell, render_inline_shell};
pub use run_config::{OWLY_INLINE_HEIGHT, default_activity_spinner, default_run_config, inline_static_run_config};
pub use spacing::{
    shell_chat_block_gap, shell_chat_card_pad, shell_chat_pad_x, shell_chat_pad_y, shell_input_gap, shell_panel_pad,
    shell_prompt_pad, shell_prompt_pad_x, shell_prompt_pad_y, shell_section_gap,
};
