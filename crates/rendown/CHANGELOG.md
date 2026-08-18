# Changelog

## 0.0.1

- Builder API: `Rendown` (`parse`, `write`, `render`, `plain`, `row_count`).
- Optional features (off by default): `stream` (incremental ANSI), `mermaid` (diagram render).
- Neutral IR (`RgbColor` / `FontWeight`). Mermaid fences always set `mermaid_source`.
- Theme builder: `MarkdownTheme::builder()`.
- Highlight IR always stores truecolor; `color_level` applies only at ANSI write.
- Optional `highlight` feature (syntect). Layout helpers under `rendown::layout` (`ansi_row_count`).
- Mermaid: one mermaid-text compaction pass (no forced Native/1×0, no row/line clip).
- Transcript: completing a stream invalidates the capped tail paint cache.
