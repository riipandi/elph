//! TUI demo - progress bar
//!
//! ```bash
//! cargo run -p elph-tui --example progress_bar
//! ```

use anyhow::Result;
use iocraft::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

#[component]
fn ProgressBar(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut progress = hooks.use_state::<f32, _>(|| 0.0);

    hooks.use_future(async move {
        loop {
            sleep(Duration::from_millis(100)).await;
            progress.set((progress.get() + 2.0).min(100.0));
        }
    });

    if progress >= 100.0 {
        system.exit();
    }

    element! {
        View {
            View(border_style: BorderStyle::Round, border_color: Color::DarkGrey, width: 60) {
                View(width: Percent(progress.get()), height: 1, background_color: Color::DarkGreen)
            }
            View(padding: 1) {
                Text(content: format!("{:.0}%", progress))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(ProgressBar).render_loop().await?;
    println!("done!");
    Ok(())
}
