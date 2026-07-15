//! iocraft-based TUI for Elph.
//!
//! Layout reference: `crates/elph-tui/examples/chat_layout.rs` (basic demo; not synced with production).
//! Zones (top → bottom): Header, Transcript, status row, Editor, Footer.

mod editor;
mod footer;
mod header;
mod labels;
mod prompt_chrome;
mod session_prefs;
mod shell;
mod status_row;
mod theme;
mod transcript;

use anyhow::Result;
use iocraft::prelude::*;

use crate::agent::agent_mode_from_setting;
use crate::platform::{Paths, Settings};
use crate::types::ThinkingLevel;

use labels::{model_footer_label, project_footer_label};
use shell::MainShell;

/// Launch options for the interactive TUI.
#[derive(Debug, Clone, Default)]
pub struct TuiOptions {
    pub resume_id: Option<String>,
}

/// Launch the Elph TUI.
pub async fn run_tui(options: TuiOptions) -> Result<()> {
    let paths = Paths::resolve()?;
    Settings::ensure(&paths)?;
    let settings = Settings::load(&paths)?;

    element!(MainShell(
        resume_id: options.resume_id,
        initial_agent_mode: agent_mode_from_setting(&settings.session.agent_mode),
        initial_thinking_level: ThinkingLevel::from_setting(&settings.session.thinking_level),
        model_label: model_footer_label(
            settings.session.provider_id.as_deref(),
            settings.session.model_id.as_deref(),
        ),
        project_label: project_footer_label(&paths),
        sticky_scroll: settings.sticky_scroll,
    ))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()
    .ignore_ctrl_c()
    .await?;
    Ok(())
}
