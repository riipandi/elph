//! TUI demo - basic layout
//!
//! ```bash
//! cargo run -p elph-tui --example basic_layout
//! ```

use anyhow::Result;
use chrono::Local;
use iocraft::prelude::*;
use std::time::Duration;

#[component]
fn MainShell(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(|| Local::now());
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            time.set(Local::now());
        }
    });

    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => match code {
                KeyCode::Char('q') => should_exit.set(true),
                _ => {}
            },
            _ => {}
        }
    });

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(
            width,
            height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexEnd,
        ) {
            View(
                width,
                height,
                background_color: Color::Reset,
                border_style: BorderStyle::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Baseline,
                justify_content: JustifyContent::FlexEnd,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 1,
                padding_right: 1,
            ) {
                Text(content: format!("1 Current Time: {} - Press \"q\" to quit.", time.get().format("%r")))
             }
            View(
                width,
                min_height: 3,
                border_style: BorderStyle::Round,
                border_color: Color::DarkGrey,
                align_items: AlignItems::Baseline,
                margin_bottom: 0,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 1,
                padding_right: 1,
            ) {
                // View(margin_top: -1, margin_left: 1) {
                //     Text(content: " Overlap Example ", wrap: TextWrap::NoWrap)
                // }
                // Text(content: format!("1 Current Time: {} - Press \"q\" to quit.", time.get().format("%r")))
             }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(MainShell).fullscreen().await?;
    Ok(())
}
