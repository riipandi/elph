# rendown

Streaming CommonMark/markdown → ANSI for terminals.

Cloned from `elph-tui`’s markdown pipeline (pulldown-cmark + syntect tokyo-night) with
neutral RGB colors (no iocraft). Used by headless `elph run --output=pretty`.

## API

```rust
use rendown::{parse_markdown, write_document_ansi, StreamRenderer, MarkdownTheme};

let doc = parse_markdown("# Hello\n\n**world**");
let mut out = Vec::new();
write_document_ansi(&doc, 80, &MarkdownTheme::default(), &mut out)?;

let mut stream = StreamRenderer::new(80);
stream.push("# Hel", &mut out)?;
stream.push("lo\n", &mut out)?;
stream.finish(&mut out)?;
```

## Notes

- Mermaid fences render as plain source in v1 (no `mermaid-text`).
- Transcript / elph-tui still use their own copy; a later PR can dedupe.
