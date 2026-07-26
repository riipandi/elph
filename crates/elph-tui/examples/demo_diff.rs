//! Demo: DiffView component with syntax highlighting and hunk-aware rendering.
//!
//! ```sh
//! cargo run -p elph-tui --example demo_diff
//! ```

use anyhow::Result;
use elph_tui::prelude::*;

fn rust_old() -> &'static str {
    r#"use std::collections::HashMap;

/// Calculate the factorial of n.
fn factorial(n: u32) -> u32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

fn main() {
    let mut cache = HashMap::new();
    let result = fibonacci(10, &mut cache);
    println!("fib(10) = {result}");

    // Print some factorials
    for i in 0..=5 {
        println!("{i}! = {}", factorial(i));
    }
}
"#
}

fn rust_new() -> &'static str {
    r#"use std::collections::HashMap;

/// Calculate the factorial of n.
fn factorial(n: u32) -> u32 {
    match n {
        0 | 1 => 1,
        _ => n * factorial(n - 1),
    }
}

/// Compute the nth Fibonacci number with memoization.
fn fibonacci(n: u32, cache: &mut HashMap<u32, u32>) -> u32 {
    if let Some(&result) = cache.get(&n) {
        return result;
    }
    let result = match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1, cache) + fibonacci(n - 2, cache),
    };
    cache.insert(n, result);
    result
}

fn main() {
    let mut cache = HashMap::new();
    let result = fibonacci(10, &mut cache);
    println!("fib(10) = {result}");

    // Print some factorials
    for i in 0..=10 {
        println!("{i}! = {}", factorial(i));
    }
}
"#
}

#[component]
fn Demo(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut exit = hooks.use_state(|| false);
    let mut mode = hooks.use_state(|| DiffMode::Unified);
    let mut syntax = hooks.use_state(|| true);
    let mut numbers = hooks.use_state(|| true);

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }
        match code {
            KeyCode::Char('q') => exit.set(true),
            KeyCode::Char('s') => mode.set(DiffMode::SideBySide),
            KeyCode::Char('u') => mode.set(DiffMode::Unified),
            KeyCode::Char('h') => syntax.set(!syntax.get()),
            KeyCode::Char('n') => numbers.set(!numbers.get()),
            _ => {}
        }
    });

    if exit.get() {
        system.exit();
    }

    let key_hint = |key: &str, desc: &str| -> String { format!("[{key}] {desc}") };
    let hints = [
        key_hint("u", "unified"),
        key_hint("s", "side-by-side"),
        key_hint("h", if syntax.get() { "plain" } else { "highlight" }),
        key_hint("n", if numbers.get() { "hide #" } else { "show #" }),
        key_hint("q", "quit"),
    ];

    element! {
        View(padding: 2u16, flex_direction: FlexDirection::Column, gap: 1u16) {
            StyledText(
                content: format!("DiffView  —  {}", hints.join("  ·  ")),
                color: Color::DarkGrey,
            )
            DiffView(
                width: 78u16,
                height: 24u16,
                old_text: rust_old().to_string(),
                new_text: rust_new().to_string(),
                mode: mode.get(),
                file_path: Some("src/main.rs".to_string()),
                syntax_highlight: syntax.get(),
                show_line_numbers: numbers.get(),
                show_file_header: true,
                show_hunk_header: true,
                context_lines: 2usize,
            )
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(Demo).render_loop().await?;
    Ok(())
}
