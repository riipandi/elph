//! TUI demo - basic chat layout
//!
//! ```bash
//! cargo run -p elph-tui --example chat_layout
//! ```

use anyhow::Result;
use chrono::Local;
use iocraft::prelude::*;
use std::time::Duration;

const LOREM_IPSUM: &str = "Lorem ipsum odor amet, consectetuer adipiscing elit. \
Lobortis hendrerit nec ipsum dapibus quam. Donec malesuada tincidunt elementum \
mollis vehicula quisque purus. Est volutpat integer, donec sagittis placerat \
fermentum phasellus ipsum sollicitudin. Tempus laoreet ad tempus aptent proin \
per donec lectus. Quisque auctor urna; phasellus urna tortor ligula. Class \
pharetra bibendum tristique, quisque consectetur placerat potenti. Imperdiet ut \
torquent vestibulum eleifend bibendum et. Dictumst vulputate interdum iaculis \
at conubia venenatis.";

#[component]
fn MainShell(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
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
            width: screen_width,
            height: screen_height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            margin: 0,
            padding: 0,
        ) {
            View(
                width: screen_width,
                background_color: Color::Reset,
                border_style: BorderStyle::Single,
                border_edges: Edges::Top,
                border_color: Color::Rgb { r: (88), g: (88), b: (88) },
                position: Position::Relative,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding_left: 1,
                padding_right: 1,
                margin_bottom: 0,
            ) {
                Text(color: Color::DarkGrey, content: format!("Current Time: {}", time.get().format("%r")))
                Text(color: Color::DarkGrey, content: "Press \"q\" to quit.")
            }

            View(
                width: screen_width,
                height: screen_height,
                background_color: Color::Reset,
                border_style: BorderStyle::Single,
                border_edges: Edges::Top,
                border_color: Color::Rgb { r: (88), g: (88), b: (88) },
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Baseline,
                overflow: Overflow::Scroll,
                padding_top: 1,
                padding_bottom: 1,
                padding_left: 1,
                padding_right: 1,
                gap: 1,
            ) {
                // Content stream (chat transcription)
                View(
                    width: screen_width - 2,
                    background_color: Color::Rgb { r: (48), g: (48), b: (48) },
                    margin_bottom: 0,
                    padding: 1,
                ) {
                    Text(color: Color::White, content: LOREM_IPSUM)
                }
                View(
                    width: screen_width - 2,
                    background_color: Color::Rgb { r: (48), g: (48), b: (48) },
                    margin_bottom: 0,
                    padding: 1,
                ) {
                    Text(color: Color::DarkGrey, content: LOREM_IPSUM)
                }
                View(
                    width: screen_width - 2,
                    background_color: Color::Reset,
                    margin_bottom: 0,
                    padding: 0,
                ) {
                    Text(color: Color::Green, content: LOREM_IPSUM)
                }
            }

            View(
                width: screen_width,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding_left: 1,
                padding_right: 1,
                margin_bottom: 0,
            ) {
                Text(color: Color::DarkGrey, content: format!("Current Time: {}", time.get().format("%r")))
                Text(color: Color::DarkGrey, content: "Press \"q\" to quit.")
             }

             View(
                width: screen_width,
                border_style: BorderStyle::None,
                align_items: AlignItems::Baseline,
                flex_direction: FlexDirection::Column,
                margin_bottom: 0,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 0,
                padding_right: 0,
             ) {
                View(
                    width: screen_width,
                    min_height: 3,
                    border_style: BorderStyle::Round,
                    border_color: Color::Rgb { r: (108), g: (108), b: (108) },
                    align_items: AlignItems::Baseline,
                    margin_bottom: 0,
                    padding_top: 0,
                    padding_bottom: 0,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    // View(width, margin_top: -1, margin_left: 1, position: Position::Absolute) {
                    //     Text(content: "Overlap Example", wrap: TextWrap::NoWrap)
                    // }
                    TextInput(
                        has_focus: true,
                        multiline: true,
                        color: Color::Grey,
                        cursor_color: Color::DarkGrey,
                        value: "Ask anything... \"Fix broken tests\"",
                    )
                }
                View(
                    width: screen_width,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    Text(color: Color::DarkGrey, content: "STATUSBAR_TOP_LEFT")
                    Text(color: Color::DarkGrey, content: "STATUSBAR_TOP_RIGHT")
                }
                // View(
                //     width: screen_width,
                //     align_items: AlignItems::Center,
                //     justify_content: JustifyContent::SpaceBetween,
                //     padding_left: 1,
                //     padding_right: 1,
                // ) {
                //     Text(color: Color::DarkGrey, content: "STATUSBAR_BOTTOM_LEFT")
                //     Text(color: Color::DarkGrey, content: "STATUSBAR_BOTTOM_RIGHT")
                // }
             }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(MainShell).fullscreen().await?;
    Ok(())
}
