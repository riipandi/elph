# Changelog

## 0.0.1

- Builder API: `Rendown` (`parse`, `write`, `render`, `plain`, `row_count`).
- Optional features (off by default): `stream` (incremental ANSI), `mermaid` (diagram render).
- Neutral IR (`RgbColor` / `FontWeight`). Mermaid fences always set `mermaid_source`.
- Theme builder: `MarkdownTheme::builder()`.
