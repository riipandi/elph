# elph-tui

Terminal UI components for Elph agent applications. Built on [SuperLightTUI](https://github.com/subinium/SuperLightTUI)
for the agent shell and a pi-tui-inspired `diff/` engine for differential rendering, overlays, and rich components.

## Usage Sketch

```rust
use elph_tui::{ChatStreamState, PromptState, Theme, render_chat_stream, render_prompt};
use slt::run;

let mut chat = ChatStreamState::with_messages(vec!["hello".into()]);
let mut prompt = PromptState::new("claude-sonnet");
let theme = Theme::detect();

run(|ui| {
    render_chat_stream(ui, &mut chat, theme);
    render_prompt(ui, &mut prompt, theme);
})?;
```

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).