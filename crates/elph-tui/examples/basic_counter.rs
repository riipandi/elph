//! TUI demo - basic counter
//!
//! ```bash
//! cargo run -p elph-tui --example basic_counter
//! ```

use anyhow::Result;
use iocraft::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

#[component]
fn Counter(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0);

    // Put the terminal into raw mode so ctrl+c is delivered as a key event
    // and iocraft can run its drop handlers to restore the cursor on exit.
    hooks.use_terminal_events(|_| {});

    hooks.use_future(async move {
        loop {
            sleep(Duration::from_millis(100)).await;
            count += 1;
        }
    });

    element! {
        Text(color: Color::Blue, content: format!("Counter: {}", count))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(Counter).render_loop().await?;
    Ok(())
}
