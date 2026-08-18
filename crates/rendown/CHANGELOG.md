# Changelog

## 0.0.1

- Builder API: `Rendown` (`parse`, `write`, `render`, `plain`, `row_count`).
- Optional features (off by default): `stream` (incremental ANSI), `mermaid` (diagram render).
- Neutral IR (`RgbColor` / `FontWeight`). Mermaid fences always set `mermaid_source`.
- Theme builder: `MarkdownTheme::builder()`.
- Highlight IR always stores truecolor; `color_level` applies only at ANSI write.
- Mermaid: at most two mermaid-text pipelines; LRU cache (16 / 256 KiB) stores exact fits only.
