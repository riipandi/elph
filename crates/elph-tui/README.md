# elph-tui

Terminal UI components for Elph agent applications. The interactive shell is built
on [iocraft](https://crates.io/crates/iocraft) with a diff engine for differential
rendering, overlays, and rich components.

## Examples

Run any example with `cargo run -p elph-tui --example <name>`.

Build all elph-tui examples: `cargo build -p elph-tui --examples`

### Apps (multi-zone simulators)

| Example        | Notes                                                               |
| -------------- | ------------------------------------------------------------------- |
| `coding_agent` | Chat shell + slash palette + dialog overlays (full agent simulator) |
| `chat_layout`  | Four-zone chat layout mirroring the production elph shell           |

### Component demos (`demo_*`)

| Example                   | Notes                                                     |
| ------------------------- | --------------------------------------------------------- |
| `demo_text_card`          | StyledText and Card                                       |
| `demo_input`              | Input and Textarea                                        |
| `demo_scroll`             | ScrollBox, VerticalScrollbar, and ScrollIndicator         |
| `demo_theme`              | UiThemeProvider, per-component theme overrides, on_change |
| `demo_select`             | SelectList, TabSelect, and Slider                         |
| `demo_code`               | CodeBlock and LineNumbers                                 |
| `demo_markdown`           | MarkdownView                                              |
| `demo_diff`               | DiffView                                                  |
| `demo_special`            | AsciiText, QrCodeView, and FrameBufferView                |
| `demo_loading_indicator`  | Braille spinner and KITT scanner widgets                  |
| `demo_progress_indicator` | CLI stepped bar and fullscreen init simulation            |
| `demo_dialog_shell`       | Dialog shell gallery — presets and header variants        |
| `demo_dialog_choices`     | User-question dialogs — single, multi, input, and confirm |

### iocraft basics (`basic_*`)

| Example           | Notes             |
| ----------------- | ----------------- |
| `basic_context`   | Context hooks     |
| `basic_counter`   | Stateful counter  |
| `basic_form`      | Form inputs       |
| `basic_input`     | User input        |
| `basic_layout`    | Flex layout       |
| `basic_output`    | Text output       |
| `basic_overlap`   | Overlapping views |
| `basic_scrolling` | Scroll regions    |
| `basic_table`     | Table layout      |

### Other

| Example        | Notes                                     |
| -------------- | ----------------------------------------- |
| `calculator`   | Simple calculator                         |
| `progress_bar` | Progress bar widget                       |
| `weather`      | Async remote data loading from user input |

### Shared code

`examples/common/` is imported by complex apps via `#[path]` — not a runnable example.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
