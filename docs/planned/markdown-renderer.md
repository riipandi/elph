# Plan: Headless pretty markdown via `rendown` (no transcript touch)

## Goal

Replace streamdown-based `--output=pretty` / `--output-format=pretty` with a **cloned** markdown pipeline from `elph-tui`’s transcript stack, hosted in **`crates/rendown`**, so headless visual parity can match TUI markdown **without** modifying coding-agent transcript code in this pass.

- **In scope:** `rendown` crate, headless `pretty` path, remove streamdown, wire workspace deps
- **Out of scope:** refactoring `coding-agent/src/tui/transcript/**`, sharing live code with `elph-tui` markdown (later PR can dedupe)
- **No legacy / compat:** drop streamdown completely; no dual path

---

## Why clone (not share yet)

Transcript + `elph-tui` markdown are tightly coupled to:

- `iocraft::Color` / `Weight`
- `UiTheme`
- iocraft paint (`render_markdown_block`, mermaid cards)
- worker/partition/cache in transcript

Cloning into `rendown` with a **neutral IR** lets headless ship pretty parity **now**, without a risky elph-tui/transcript migration. Later: elph-tui can re-export or call `rendown` and delete its copy.

---

## Target architecture

```text
LLM TextDelta
    │
    ▼
PrettyMarkdownSink (coding-agent headless)
    │ accumulate full source (stream)
    ▼
rendown::parse_document(source, theme)
    │
    ▼
MarkdownDocument { lines, spans }   // neutral colors
    │
    ▼
rendown::write_ansi(document, width, &mut stdout)
    │
    ▼
TTY (styled CommonMark) | non-TTY falls back to raw plain
```

Streaming strategy (match TUI spirit, simpler for CLI):

1. Buffer all deltas into one `String` source.
2. On each **newline** (and on `finish`): re-parse full source → document → emit **only new visual lines** since last paint (track `painted_line_count` / last painted source offset).
3. Open fences / incomplete tail: same as TUI `streaming_tail` / open-container behaviour (clone `parser_config` + parse logic).
4. Avoid full-screen redraw flicker: append-only line emission when possible; on rare layout regressions (table width change), accept re-print of tail or full re-emit for v1 if needed (document tradeoff).

**Plain** remains: raw tokens, **no** wait indicator (current behaviour).

---

## `rendown` crate design

### Workspace

- Add `crates/rendown` to workspace `members` (currently only coding-agent / elph-agent / elph-ai / floppy).
- Public crate `rendown` v0.0.1.

### Dependencies (headless-first, no iocraft)

| Dep                                                      | Role                                         |
| -------------------------------------------------------- | -------------------------------------------- |
| `pulldown-cmark`                                         | Same parser as elph-tui                      |
| `syntect` + `two-face` / tokyo-night asset               | Code highlight parity                        |
| `anstyle` (+ optional `anstyle-syntect`)                 | Span styles + ANSI write                     |
| `unicode-width`, `textwrap` / linebreak crates as needed | Wrap parity                                  |
| `linkify`, `url`                                         | Path/URL linkify                             |
| `crossterm` (optional, default on for width)             | Terminal width only — same family as iocraft |
| `supports-color`                                         | Color level (from colors.rs)                 |

**Do not** depend on iocraft, elph-tui, or mermaid for v1 (mermaid: render as fenced code / deferred plain source like simple code line).

### Module map (clone + adapt from elph-tui)

| Source (`elph-tui/.../markdown/`) | → `rendown`                 | Adaptation                                                                                            |
| --------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------------------- |
| `model.rs`                        | `model`                     | Replace `iocraft::Color/Weight` with neutral `RgbColor {r,g,b}` + `FontWeight { Normal, Bold }`       |
| `theme.rs`                        | `theme`                     | Default Ghostty-like palette hard-coded (values from `UiTheme::default()` mapping); no `UiTheme` type |
| `parse.rs`                        | `parse`                     | Clone; map theme colors via neutral type                                                              |
| `parser_config.rs`                | `parser_config`             | Clone open-container / streaming helpers                                                              |
| `highlight.rs` + `syntax.rs`      | `highlight`, `syntax`       | Clone; embed or `include_bytes!` tokyo-night theme asset                                              |
| `layout.rs`                       | `layout`                    | Clone wrap / row counts needed for ANSI line breaks                                                   |
| `table.rs`                        | `table`                     | Clone table formatting to plain styled lines (no iocraft)                                             |
| `linkify.rs`                      | `linkify`                   | Clone                                                                                                 |
| `blocks.rs`                       | `blocks`                    | Constants (insets)                                                                                    |
| `colors.rs`                       | `colors`                    | Keep anstyle conversion; **drop** `anstyle_color_to_iocraft` — emit `RgbColor` / anstyle directly     |
| `render.rs` (iocraft)             | **do not clone whole file** | New `ansi.rs`: walk `MarkdownDocument` → write ANSI lines                                             |

### Public API (minimal)

```rust
// parse
pub fn parse_markdown(source: &str) -> MarkdownDocument;
pub fn parse_markdown_with_theme(source: &str, theme: &MarkdownTheme) -> MarkdownDocument;

// paint
pub fn write_document_ansi(doc: &MarkdownDocument, width: u16, out: &mut impl Write) -> io::Result<()>;

// streaming helper for headless
pub struct StreamRenderer { /* source, last_emitted_lines, width, theme */ }
impl StreamRenderer {
    pub fn new(width: u16) -> Self;
    pub fn push(&mut self, delta: &str, out: &mut impl Write) -> io::Result<()>;
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()>;
}
```

Optional later: `to_plain_text(doc)` for tests.

### ANSI paint rules (v1)

- Map `StyledSpan` → anstyle `Style` (fg RGB, bold, italic, underline).
- OSC 8 hyperlinks when `href` set (same intent as TUI).
- Code blocks: optional dim background if truecolor; else dim/fg only.
- Horizontal rules: full-width `─` like TUI.
- Tables: monospaced borders from cloned `table.rs` text.
- Blank lines / continuation spacing: mirror layout gap semantics as line breaks only (no iocraft margin).

---

## Headless wiring (`coding-agent`)

| File                            | Change                                                                       |
| ------------------------------- | ---------------------------------------------------------------------------- |
| `Cargo.toml`                    | Depend on `rendown`; **remove** `streamdown-parser`, `streamdown-render`     |
| Workspace `Cargo.toml`          | Remove streamdown workspace deps; add `rendown` path dep; members += rendown |
| `agent/pretty_markdown.rs`      | Rewrite on `rendown::StreamRenderer` (delete streamdown types)               |
| `agent/run_mode.rs`             | Keep `OutputFormat::Pretty`; docs strings mention rendown not streamdown     |
| `cli/run.rs`                    | Help text: pretty = CommonMark via rendown                                   |
| `docs/planned/headless-mode.md` | Update library reference                                                     |

**Unchanged:** `agent/run_mode` plain (no indicator), footer, skills/templates, transcript/**.

---

## Cleanup streamdown

1. Remove from `crates/coding-agent/Cargo.toml` and root workspace deps.
2. Delete all `use streamdown_*` and tests that assert streamdown events.
3. `cargo tree -p elph -i streamdown` must be empty.
4. No feature flag dual path.

---

## Tests

| Level          | What                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------ |
| `rendown` unit | Parse heading/list/code/table golden sources; ANSI contains expected text + some SGR codes |
| `rendown` unit | Streaming: push partial fence, finish closes; multi-chunk heading                          |
| `coding-agent` | `OutputFormat::parse("pretty")`; pretty sink buffers newline (existing style tests)        |
| Manual         | `elph run --output=pretty "…"` vs TUI transcript same prompt — visual smoke                |

---

## Implementation order

1. **Workspace + empty API stubs** in `rendown` (members, deps).
2. **Clone model/theme/colors** (neutral types).
3. **Clone parse + parser_config + highlight/syntax** (asset path).
4. **Clone layout/table/linkify/blocks** as needed by ANSI.
5. **`ansi` writer + `StreamRenderer`**.
6. **Wire `pretty_markdown.rs` + remove streamdown**.
7. **Docs + tests + `cargo check/test -p rendown -p elph`**.

---

## Risks / decisions

| Decision                 | Choice                                                                                      |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| Share with elph-tui now? | **No** — clone only                                                                         |
| Mermaid                  | Code-fence / plain source v1 (no mermaid-text dep unless free)                              |
| Streaming re-parse cost  | Accept re-parse full source on each newline (matches TUI worker spirit; OK for CLI replies) |
| Color model              | Neutral RGB + weight; ANSI via anstyle                                                      |
| Crossterm                | Width detect only; no raw mode / mouse                                                      |
| Duplicate drift          | Accept until later “elph-tui → rendown” PR                                                  |

---

## Acceptance criteria

- [ ] `elph run --output=pretty` / `--output-format=pretty` renders via **rendown** only
- [ ] `streamdown-*` gone from workspace and elph dependency graph
- [ ] **No** edits under `coding-agent/src/tui/transcript/**`
- [ ] **No** edits required to elph-tui markdown (optional: none)
- [ ] Plain still: raw stream, no wait indicator
- [ ] Pretty TTY: headings/lists/code look closer to transcript than streamdown did
- [ ] Pretty non-TTY / `NO_COLOR`: raw fallback
- [ ] `cargo test -p rendown` and `cargo check -p elph` pass
