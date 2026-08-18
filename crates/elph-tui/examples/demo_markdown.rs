//! Demo: MarkdownView component.
//!
//! ```sh
//! cargo run -p elph-tui --example demo_markdown
//! ```

use anyhow::Result;
use elph_tui::prelude::*;

const SAMPLE: &str =
    "# Elph TUI\n\nMarkdown rendering via **rendown**.\n\n- Item one\n- Item two\n\n```rust\nfn demo() {}\n```\n";

#[component]
fn Demo(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut exit = hooks.use_state(|| false);

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
            return;
        };
        if kind != KeyEventKind::Release && matches!(code, KeyCode::Char('q')) {
            exit.set(true);
        }
    });

    if exit.get() {
        system.exit();
    }

    element! {
        View(padding: 2u16, flex_direction: FlexDirection::Column, gap: 1u16) {
            StyledText(content: "MarkdownView demo — q to quit".to_string(), color: Color::DarkGrey)
            MarkdownView(width: 70u16, height: 16u16, source: SAMPLE.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(Demo).render_loop().await?;
    Ok(())
}
