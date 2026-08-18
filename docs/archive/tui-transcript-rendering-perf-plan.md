# Plan: Fix `elph` TUI Transcript Rendering (laggy scroll + partial-blank content + memory usage)

**Repo:** `riipandi/elph`.
**Scope:** `crates/coding-agent/src/tui/transcript/*` (panel, layout, card/builder, markdown/*, archive),
`crates/elph-tui/src/components/scroll_box.rs` and related scroll primitives,
`crates/coding-agent/src/platform/settings.rs` (new config section).
**Constraints:** no backward-compat concerns — internal APIs, cache structures, and settings
schema may change freely. Do not change the on-disk session snapshot format
(`ArchivedTranscriptMessage` / `TranscriptSnapshot`) without a version bump in `version` field —
that one _is_ persisted across sessions and needs to stay loadable.

**Symptoms reported:** laggy scrolling, and terminal content becoming partially blank during
scroll. Both are explained by concrete, evidence-backed root causes below — not speculation.

---

## Architecture context (read this before making changes)

The transcript uses **windowed virtualization**: only messages intersecting the viewport (+
overscan + a "live tail") are actually mounted as iocraft elements; everything else becomes a
fixed-height blank `View` spacer (`card/builder.rs::build_transcript_bubbles_windowed`). This
keeps the render tree at O(viewport) instead of O(full history) — good design, and not itself the
problem.

The catch: spacer height comes from a **separately maintained row-count estimate**
(`layout.rs::message_row_count` / `markdown/layout.rs::assistant_row_count`), not from actually
measuring what gets painted. If the estimate and the real painted height ever diverge for a given
message, the spacer is the wrong size and the viewport shows either a gap (extra blank space) or a
clipped card (missing top/bottom line) — this is a **known, previously-hit bug class**, not a
hypothesis: `markdown/layout.rs:17-20` has this exact comment already in the code:

> "Summing the stable and tail row counts independently missed the inter-segment gap at the
> stable↔tail boundary, so the measured height ran one row short and the scroll viewport clipped
> the first line of the following paragraph."

And `layout.rs:374-378` (test doc comment) references _"the auto-scroll viewport pins its bottom
to the measured total. Any divergence … shifts the window so cards appear clipped mid-line — the
`/tools list` fragment bug."_ — i.e. this exact failure mode (partial-blank content on scroll) has
already occurred at least twice in this codebase and been patched reactively. The fix in this plan
is to close off the whole bug class, not add a third patch for a third specific case.

---

## Phase 1 — Close the measure/paint divergence bug class (fixes: partial-blank content on scroll)

### 1.1 Understand the specific divergence risk

`markdown/layout.rs:13-26` (`assistant_row_count`):

```rust
pub fn assistant_row_count(content: &str, markdown: Option<&AssistantMarkdownBuffer>, wrap_width: u16) -> u16 {
    let Some(md) = markdown else {
        return wrapped_text_row_count(content, wrap_width as usize)...  // PRE-parse estimate
    };
    let document = build_assistant_markdown_document(md, content, Color::Reset);
    ...
    markdown_document_row_count(&document, wrap_width)  // POST-parse estimate
}
```

Markdown parsing is debounced and asynchronous (`panel.rs:98-147`, 120ms idle / 400ms streaming
debounce, `spawn_blocking` worker). A message transitions from `markdown: None` → `markdown:
Some(...)` on a frame _after_ the worker finishes, driven by `markdown_layout_revision` bumping
(`panel.rs:143-145`). Plain-text wrap row count and parsed-markdown row count (which accounts for
headers, fenced code blocks, tables, blank-line block separators) are **not required to produce
the same number** for the same content — they're two different measurement code paths over two
different representations. The stable↔tail segment-boundary bug quoted above is a concrete
instance of exactly this class of problem.

### 1.2 Fix: eliminate the dual-implementation by measuring, not estimating

Replace hand-maintained row-count formulas with an actual off-screen layout measurement, memoized
per `(content_hash, style, detail_expanded, wrap_width)`. iocraft already supports this pattern —
the existing tests literally do it (`layout.rs:308-319`, `element! {...}.to_string().lines().count()`)
to _verify_ the formula is correct; promote that pattern from "test assertion" to "the actual
implementation":

1. In `layout.rs`, replace `message_row_count()` (and `assistant_row_count` /
   `markdown_document_row_count` in `markdown/layout.rs`) with a function that:
    - Builds the single card's real element tree the same way `card/builder.rs::transcript_message_bubble`
      does for that message (reuse the exact same card-construction function — do not
      reimplement/duplicate the branch-per-style logic a second time).
    - Renders it off-screen at the target `wrap_width` and counts painted lines
      (`element! { View(width: wrap_width) { #(bubble) } }.to_string().lines().count()`).
    - This _is_ more expensive per call than the current arithmetic estimate, which is exactly why
      memoization (next step) matters.
2. Memoize by content fingerprint: extend `IncrementalLayoutCache` (`layout.rs:13-21`) — it
   already fingerprints per message and skips unchanged prefixes — so the _expensive_ measured
   path only runs for messages whose fingerprint actually changed (new message appended, markdown
   parse just landed, card expanded/collapsed, width changed). This should be a small, bounded set
   per frame in the common case (the streaming tail message, plus whatever the user just toggled),
   not the whole history — same amortized cost model as today, just correct.
3. Delete `wrapped_text_row_count`'s role as an independent estimator for assistant messages —
   it can stay for genuinely plain-text card kinds (status lines, etc.) that don't have a markdown
   parse step and thus have no divergence risk, but assistant/markdown content must go through the
   measure-don't-estimate path exclusively.
4. Keep the existing `all_card_kinds_measure_matches_paint` and
   `slash_flow_full_and_windowed_heights_match_measure` tests — they should now pass _trivially_
   (measured value compared against itself) rather than by coincidence of two formulas agreeing.
   Add a new test that intentionally exercises the pre-parse → post-parse transition frame (render
   with `markdown: None`, then simulate parse landing, assert the windowed spacer/card boundary is
   still correct on the very next frame) since that's the specific seam that broke before.

### 1.3 Guard the pre-parse frame explicitly

Even with 1.2, there's an unavoidable brief window where content has streamed in but not yet been
parsed (`markdown: None`). During that window the "measured" height (now: real off-screen render
of plain content) will legitimately be smaller than what it becomes after parsing adds block
structure. This is _expected_, not a bug — but make sure `render_cache` invalidation
(`panel.rs:173-178`) fires on the exact frame `markdown_layout_revision` changes, so the mismatch
window is exactly one frame's width of streamed-but-unparsed content, never a stale multi-frame gap.
Add a regression test that streams content across several debounce ticks and asserts the windowed
view never shows a spacer/card boundary error beyond that one-frame tolerance.

---

## Phase 2 — Replace O(n) window-boundary scan with O(log n) (fixes: laggy scroll on long sessions)

`card/builder.rs:63-73` (`build_transcript_bubbles_windowed`) does a **full linear scan over every
message's `row_layouts` entry on every single render frame**:

```rust
for (index, layout) in row_layouts.iter().enumerate() {
    let msg_end = layout.start_row.saturating_add(layout.row_count);
    let intersects = msg_end > view_start_row && layout.start_row < view_end_row;
    ...
}
```

`row_layouts` is populated by cumulative `start_row` (`layout.rs:126-133`, `cursor` only ever
increases), so it is **sorted ascending by `start_row`** — this is a binary-search-friendly
structure being scanned linearly. For long agent sessions (thousands of messages, which is
realistic for the kind of multi-hour agent sessions this tool is built for) this scan runs on
every scroll key-repeat, every mouse-wheel tick, and every general re-render, and is the most
likely direct cause of "laggy scroll."

### 2.1 Implementation

1. Replace the linear scan with two binary searches using `slice::partition_point`:
    - `first_visible` candidate: smallest index where `start_row + row_count > view_start_row`.
    - `last_visible` candidate: largest index where `start_row < view_end_row`, i.e.
      `row_layouts.partition_point(|l| l.start_row < view_end_row).saturating_sub(1)`.
2. Keep the existing tail-inclusion logic (`tail_start`, lines 61/68/77-78) and the flush-pair
   expansion (lines 96-109) exactly as-is — those are O(1)/O(small-constant) already and correct;
   only the O(n) boundary scan is being replaced.
3. Keep all the existing defensive clamps (lines 75-93) — they're cheap and guard against edge
   cases (empty intersection, out-of-range indices); no need to remove them just because the
   search got faster.
4. Add a `#[cfg(test)]` benchmark-style test (or a `criterion` bench if the workspace already has
   one — check `Cargo.toml` for a `[[bench]]` section before adding a new dependency) that builds
   a synthetic transcript of 10,000 messages and asserts `build_transcript_bubbles_windowed`
   returns in well under 1ms — this makes the O(log n) property a regression-tested guarantee, not
   just an implementation note that can silently regress back to O(n) later.

---

## Phase 3 — Bound live transcript memory (fixes: inefficient / growing memory usage)

`crates/coding-agent/src/tui/transcript/archive.rs` only handles **persisting** the transcript to disk for
session resume (`build_snapshot_data`) — it does not evict anything from the **live** in-memory
`Vec<TranscriptMessage>` (`panel.rs:33`, held via `State<Vec<TranscriptMessage>>` or
`Arc<RwLock<Vec<TranscriptMessage>>>`). For long-running agent sessions this vec only grows:

- Full raw `content` string per message (tool outputs, diffs, long file reads can be large).
- A parsed `AssistantMarkdownBuffer`/`MarkdownDocument` retained per assistant message
  indefinitely, even for messages scrolled far off-screen and never revisited.
- Nothing currently prunes either, for the lifetime of the TUI process.

### 3.1 Add a live-message cap with lazy on-demand history loading

1. Introduce a cap, e.g. `transcript.maxLiveMessages` (see Phase 4 for the settings entry),
   default **2000** messages.
2. When the live `Vec<TranscriptMessage>` exceeds the cap, move the oldest excess messages out:
    - Serialize them using the **existing** `ArchivedTranscriptMessage::from` /
      `build_snapshot_data` machinery (already tested, already round-trips correctly per
      `archive.rs`'s `snapshot_round_trip_preserves_tool_diff_and_duration` test) into an
      append-only on-disk "history overflow" store (reuse the session-tree custom-entry mechanism
      already used for snapshots, or a dedicated append file — pick whichever fits the existing
      session storage layer with the least new surface area; check how `TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE`
      entries are currently read/written for the pattern to follow).
    - Replace the evicted range in the live vec with a single lightweight placeholder message
      (new `TranscriptStyle` variant, e.g. `HistoryPlaceholder { count: usize }`, or reuse `Meta`
      style with a synthetic content string) that renders as `"— N earlier messages, press Home to
load —"`.
3. On `Home` key (already a bound key, `panel.rs:487`) or on scrolling the placeholder into view,
   lazily load the archived range back into the live vec (paginated — e.g. load 500 at a time, not
   the entire remaining history at once, to avoid trading a memory spike for the eviction you just
   did).
4. This must not change `should_archive_message` filtering semantics (`archive.rs:115-133`) — the
   overflow store and the resume-snapshot are conceptually separate; do not conflate "evicted for
   live memory" with "excluded from resume snapshot" (a message can be both live-evicted _and_
   snapshot-eligible).

### 3.2 Lower-risk incremental first step (do this even if 3.1 is deferred)

Independent of full pagination: once a tool-call message has **settled** (`duration_secs.is_some()`,
turn complete) and its row range is more than, say, `2 * scroll_zone` rows above the current
scroll top (already computable from `row_layouts` + `effective_scroll_offset`, see `panel.rs:290-296`),
truncate large stored fields (tool `output`, `old_text`/`new_text` diff bodies) to a short summary
in the live struct, keeping the full text only in whatever gets written to the resume snapshot.
Reload full content on-demand if the user expands that specific card again (re-fetch from the
snapshot/archive store, same mechanism as 3.1's lazy load). This bounds the single biggest memory
cost (large diffs/command output sitting in RAM forever) without needing the full placeholder/pagination
UI of 3.1.

### 3.3 Audit for redundant retention

Confirm `AssistantMarkdownBuffer` does not keep both the fully-parsed `MarkdownDocument` AND a
separately duplicated stable/tail text buffer for messages that are no longer streaming (i.e. once
`mark_stream_complete()` has been called, per `archive.rs:100`, any streaming-only scratch state
used during incremental parsing should be dropped, keeping only the final parsed document). If
there is duplicated state, apply 3.2's settle+compact treatment to it too.

---

## Phase 4 — User-configurable settings (`settings.json`)

Follow the same convention already used for `EmbedSettings` in
`crates/coding-agent/src/platform/settings.rs` — `#[serde(default = "fn_name")]` per field, camelCase JSON,
doc comment per field. Add a new `TranscriptSettings` struct (nested under the existing `ui`
section, i.e. `UiSettings.transcript: TranscriptSettings`, since transcript rendering is a UI
concern — check `UiSettings`'s existing sub-sections at `settings.rs:126+` for the nesting
convention to match).

| JSON key                           | Rust field                             | Type    | Recommended default | Controls                                                                           | Why user-tunable                                                                                                                                                                                                                                                                                                                                   |
| ---------------------------------- | -------------------------------------- | ------- | ------------------- | ---------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `windowOverscanRows`               | `window_overscan_rows`                 | `u32`   | `24`                | Extra rows mounted above/below viewport (`card/builder.rs::WINDOW_OVERSCAN_ROWS`). | Users on slow terminals/SSH links may want a smaller overscan (less to paint per frame); users who scroll in large jumps (PageUp/PageDown spam, fast mouse wheel) may want it larger to avoid ever seeing a spacer at the edge before the next frame settles.                                                                                      |
| `windowMinTailMessages`            | `window_min_tail_messages`             | `usize` | `12`                | Minimum trailing messages always fully mounted (`WINDOW_MIN_TAIL_MESSAGES`).       | Sessions with very large individual messages (big diffs) near the tail may want a smaller count to reduce always-mounted render cost; sessions with many tiny status-line messages may want it larger so the whole "recent activity" cluster stays interactive.                                                                                    |
| `maxLiveMessages`                  | `max_live_messages`                    | `usize` | `2000`              | Live in-memory message cap before overflow-to-disk eviction (Phase 3.1).           | Users on memory-constrained machines (or running very long unattended sessions) may want a lower cap; users who frequently scroll far back into history may want it higher to avoid frequent lazy-reload pagination. `0` = disabled (no cap, old unbounded behavior) — support this explicitly for users who'd rather trade memory for simplicity. |
| `markdownParseDebounceMs`          | `markdown_parse_debounce_ms`           | `u64`   | `120`               | Idle markdown re-parse debounce (`panel.rs::MARKDOWN_DEBOUNCE_MS`).                | Faster machines can lower this for snappier markdown formatting after typing/streaming pauses; slower machines can raise it to reduce parse-triggered re-render frequency.                                                                                                                                                                         |
| `markdownParseDebounceStreamingMs` | `markdown_parse_debounce_streaming_ms` | `u64`   | `400`               | Debounce while actively streaming (`MARKDOWN_STREAMING_DEBOUNCE_MS`).              | Same tradeoff as above, separated because streaming already re-renders frequently for other reasons (token-by-token content growth) — this is the knob users would reach for first if they report choppiness specifically _while the agent is responding_.                                                                                         |

### 4.1 Implementation steps

1. Add `TranscriptSettings` struct + `default_*()` functions in `settings.rs`, nested as
   `UiSettings.transcript`.
2. Thread the four constants (`WINDOW_OVERSCAN_ROWS`, `WINDOW_MIN_TAIL_MESSAGES`,
   `MARKDOWN_DEBOUNCE_MS`, `MARKDOWN_STREAMING_DEBOUNCE_MS`) out of their current hardcoded
   `const` locations in `card/builder.rs` and `transcript/panel.rs`, into `TranscriptPanelProps`
   (add fields there) or a settings-derived struct passed down from wherever `TranscriptPanel` is
   mounted (check the shell/view layer, likely `crates/coding-agent/src/tui/shell/view.rs`) — populate from the
   loaded `Settings` at that call site.
3. Add `max_live_messages` plumbing per Phase 3.1/3.2, read at the point where messages are
   appended (wherever `messages_state.write().push(...)` / `messages_arc.write().push(...)`
   currently happens — locate via `rg -n "messages_arc.write\(\)|messages_state.write\(\)"`).
4. Validate at settings-load time: clamp `windowOverscanRows`/`windowMinTailMessages` to sane
   minimums (e.g. overscan ≥ 0, tail ≥ 1) so a bad hand-edited `settings.json` can't produce a
   degenerate always-empty or always-full window.
5. Document all five fields in the user-guide (`assets/user-guide/05-configuration.md`), same as
   the other settings groups.

---

## Phase 5 — Verification

1. **Correctness (Phase 1):** run the existing + new measure/paint parity tests
   (`cargo test -p elph --lib tui::transcript`). All must pass with the new measure-based
   implementation. Specifically re-run `all_card_kinds_measure_matches_paint` and
   `slash_flow_full_and_windowed_heights_match_measure` — these should still pass, now because
   the values are measured rather than because two formulas happen to agree.
2. **Manual repro checklist** (do all of these before/after, on a synthetic long session — script
   or replay a saved transcript with 1000+ mixed-type messages):
    - Hold PageDown, then PageUp, rapidly several times in a row — check for any single-frame blank
      flash.
    - Trigger a long assistant response mid-scroll (scrolled up, not auto-following) and watch the
      exact frame where markdown parsing lands — check for clipped/blank content at that transition.
    - Resize the terminal while scrolled to the middle of a long transcript.
    - Resume a session from a snapshot with 1000+ archived messages, then immediately scroll up.
    - Expand/collapse a card (Ctrl+O) while actively auto-scrolling near the bottom.
3. **Performance (Phase 2):** the new `criterion`/bench-style test from 2.1 — confirm near-constant
   time for `build_transcript_bubbles_windowed` regardless of total message count (10 vs 10,000
   messages), where before the fix it should scale linearly (verify the _old_ code actually
   regresses in the same benchmark, as a sanity check that the benchmark is meaningful, before
   removing the old linear-scan code).
4. **Memory (Phase 3):** run a synthetic session that pushes messages until the live vec would
   exceed `maxLiveMessages` several times over (e.g. 20,000 messages with realistic content sizes,
   including some large synthetic diffs/tool outputs) and confirm process RSS plateaus after the
   cap kicks in, instead of growing linearly with message count. Confirm `Home` + scroll-to-top
   correctly lazy-loads evicted history back without data loss (round-trip test: evict → reload →
   compare against the original in-memory content).
5. Run the full existing TUI test suite (`cargo test -p elph-tui`, `cargo test -p elph
--features tui` or whatever the correct feature/package invocation is per the repo's CI config
   — check `.github/workflows/ci-elph-tui.yml.example`) and fix any fallout from the
   `TranscriptStyle`/settings/API changes in this plan.

## Explicitly out of scope for this plan

- Changing the overall windowing _strategy_ (viewport + overscan + tail spacer approach) — it's
  architecturally sound; only the height-estimation-vs-paint divergence (Phase 1) and the O(n)
  boundary search (Phase 2) are actual bugs/inefficiencies within it.
- Migrating off `iocraft` to a different TUI rendering framework — out of scope, not indicated by
  anything found in this analysis.
- Per-frame syntax highlighting cost — checked and ruled out: `highlight_code_block`
  (`crates/elph-tui/src/components/markdown/highlight.rs`) runs once during the background
  markdown parse (`spawn_blocking` worker) and the result is cached in the parsed
  `MarkdownDocument`; per-frame rendering only converts the already-highlighted cached document
  into iocraft elements (`markdown/render.rs`'s "fast paint path"), not re-running `syntect`. No
  action needed here.
