# rendown

CommonMark/markdown → ANSI for terminals.

Parse once into a cacheable document (GFM, syntect highlighting, OSC 8 links), then write
styled text. Incremental streaming and mermaid diagrams are optional features.

## Usage

```toml
[dependencies]
rendown = "0.0.1"
# Optional:
# rendown = { version = "0.0.1", features = ["stream", "mermaid"] }
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
| *(none)* | yes | Parse + ANSI write |
| `stream` | off | `Rendown::stream()` / `StreamRenderer` + `terminal_width()` (crossterm) |
| `mermaid` | off | `render_mermaid_at_width` (mermaid-text). Fences always store `mermaid_source` in the IR; without this feature, ANSI prints the source as a code card. Only exact (strict-width) diagrams are cached — clipped fallbacks are not, so a bad first frame cannot stick until restart |

```rust
#[cfg(feature = "stream")]
{
    use rendown::Rendown;
    let mut stream = Rendown::new().width(80).stream();
    stream.push("# Hel", &mut out)?;
    stream.finish(&mut out)?;
}
```

## Notes

- Colors are neutral RGB (`RgbColor`, `FontWeight`). No TUI toolkit dependency.
- `NO_COLOR` and `supports-color` control auto color level unless you call `.color_level(...)`.
- Mermaid fences are deferred at parse time so width-aware rendering can happen later.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
