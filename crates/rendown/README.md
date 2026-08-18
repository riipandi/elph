# rendown

CommonMark/markdown → ANSI for terminals.

Parse once into a cacheable document (GFM, optional syntect highlighting, OSC 8 links), then
write styled text. Incremental streaming and mermaid diagrams are optional features.

Headless ANSI wrap and a TUI toolkit’s wrap (for example iocraft) are **not** guaranteed to
match row-for-row. Tables and paragraph wrapping can differ; do not assert pixel parity.

## Usage

```toml
[dependencies]
rendown = "0.0.1"
# Optional:
# rendown = { version = "0.0.1", features = ["stream", "mermaid", "highlight"] }
```

```rust
use rendown::{ColorLevel, MarkdownTheme, Rendown};

let md = Rendown::new()
    .width(80)
    .theme(MarkdownTheme::dark())
    .color_level(ColorLevel::TrueColor);

let doc = md.parse("# Hello\n\n**world**");
let mut out = Vec::new();
md.write(&doc, &mut out)?;
```

Theme fields can be overridden with a builder:

```rust
use rendown::{MarkdownTheme, RgbColor};

let theme = MarkdownTheme::builder()
    .heading(RgbColor::new(0xff, 0xb3, 0x47))
    .build();
```

### Features

| Feature | Default | Adds |
| --- | --- | --- |
| *(none)* | yes | Parse + ANSI write (plain fenced code, no syntect) |
| `highlight` | off | syntect / two-face fence highlighting |
| `stream` | off | `Rendown::stream()` / `StreamRenderer` + `terminal_width()` |
| `mermaid` | off | One compact mermaid-text pass (`gaps` 2×1). Lines are clipped to the card width; measure/paint share `mermaid_display_shared` |

Layout helpers live under `rendown::layout` (insets, `ansi_row_count`, hanging wrap).
Link helpers live under `rendown::link`. Syntect helpers live under `rendown::syntax`.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
