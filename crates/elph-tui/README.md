# elph-tui

Terminal UI components for Elph agent applications. Built on [iocraft](https://github.com/ccbrown/iocraft)
for the agent shell and a pi-tui-inspired `diff/` engine for differential rendering, overlays, and rich components.

## Usage Sketch

```rust
use elph_tui::{ChatStream, ChatStreamProps, SessionSelector};
use elph_tui::{TranscriptEntry, TranscriptView};

// Rich transcript mode
ChatStream(
    entries: Some(vec![
        TranscriptEntry::user("hello"),
        TranscriptEntry::assistant_streaming("Hi!"),
    ]),
    show_thinking: true,
    ..Default::default()
)
```

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
