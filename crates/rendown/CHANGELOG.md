# Changelog

## 0.0.1

- Builder API: `Rendown` (`parse`, `write`, `render`, `plain`, `row_count`).
- Optional features (off by default): `stream` (incremental ANSI), `mermaid` (diagram render).
- Neutral IR (`RgbColor` / `FontWeight`). Mermaid fences always set `mermaid_source`.
- Theme builder: `MarkdownTheme::builder()`.
- Highlight IR always stores truecolor; `color_level` applies only at ANSI write.
- Optional `highlight` feature (syntect). Layout helpers under `rendown::layout` (`ansi_row_count`).
- Mermaid: `mermaid_display_shared` for shared measure/paint; LRU caches exact and clipped fits.
