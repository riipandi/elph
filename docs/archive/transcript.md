# Transcript System

Architecture, performance optimizations, and disk caching for the transcript panel.

## Overview

The transcript panel renders a scrollable, windowed list of conversation messages (user prompts,
assistant responses, tool calls, status lines). It supports streaming content, sticky user prompts,
collapsible detail blocks, markdown rendering, and disk-backed archival.

## Message Types

Every row in the transcript is a [`TranscriptMessage`] with a [`TranscriptStyle`]:

| Style                                              | Usage                                         |
| -------------------------------------------------- | --------------------------------------------- |
| `User`                                             | Submitted user input (tinted bubble)          |
| `SkillPrompt`                                      | Slash command invoking a skill prompt         |
| `Thinking`                                         | Model reasoning (streaming, then collapsible) |
| `Assistant`                                        | Model response (streaming log line)           |
| `ToolRunning` / `ToolSuccess` / `ToolFailed`       | Tool call card (status-colored tint)          |
| `Meta`                                             | Plain metadata line                           |
| `Error`                                            | API / provider failure (red tint)             |
| `StatusRunning` / `StatusSuccess` / `StatusFailed` | Process log (flush, foreground-only)          |

Model **Assistant responses** render as plain log lines: no `✓ Response · 000ms` phase
header and no expand/collapse toggle. Elapsed time is still recorded internally
(`duration_secs`) for archival, it is just not displayed. Thinking and tool cards keep
their collapsible headers.

### Error Cards and Retry

Transient provider/stream failures (stream cutoff, 5xx, rate limits) are auto-retried by the
session before they surface. The retry submits a Continue-style recovery prompt
([`RETRY_CONTINUE_PROMPT`]) instead of re-sending the original text, so tool calls that
already completed are not duplicated, and the status row shows a spinner with a
"Retrying…" label while it runs. Because that recovery prompt is an internal resumption
message — not something the user typed — it is **not** rendered as a user bubble card. The
shell pushes a slim sticky meta label (`Continuing tasks…`) into the transcript instead, and it is
kept out of Arrow-Up history.

The same recovery prompt can be submitted manually with the `/continue` slash command
(`/cont`) after a turn stalls, and it renders the identical `Continuing tasks…` meta line
rather than a user prompt card.

If a turn still fails, the shell emits a **retryable** error card: its message ends with a
`Press Ctrl+R to retry this prompt.` hint (the `RETRY_HINT` marker in `api_error_display.rs`).
Retryable cards render a dedicated "Press `Ctrl+R` to retry this prompt" hint row below the
error body, and pressing Ctrl+R re-submits the recovery prompt without re-typing — the prompt
is stashed by the tick loop on `AgentUiEvent::RetryablePrompt` and consumed by the key
handler, which also renders the `Continuing tasks…` meta label rather than a user bubble. Non-transient
errors render without the hint row.

## State ↔ Arc Sync

The shell keeps two views of the active transcript:

- **`messages_arc_inner`** (`Arc<RwLock<Vec<TranscriptMessage>>>`) — written by the agent
  event applier (streaming, tool calls) and by bootstrap events.
- **`messages` State** — the render source the panel reads from.

Every tick, when an agent event changed the transcript, the shell copies the arc into the
State (`*messages.write() = messages_arc_inner.read()...`). Because that copy **overwrites**
the State, any message written only to the State would disappear on the next agent event.

To prevent that loss, every State write must be mirrored into the arc first:

- `push_transcript_message_synced()` — pushes to the shared arc _and_ the State. Used for
  slash-command output, status notices, and error lines.
- Bootstrap messages are synced from State back to the arc once the bootstrap event loop
  finishes, so the first agent event cannot wipe them.
- Pre-echoed user prompts (`pre_echoed_user_prompts`) push to the arc directly before
  writing the State, following the same rule.

The one exception is ephemeral notices and in-place mutations (collapse toggles, approval
row updates) that only touch the State; they are intentionally not mirrored back because
copying the State into the arc mid-stream could clobber un-synced streaming content.

## Performance Optimizations

### Render Cache (panel.rs)

The panel uses a hierarchical cache to avoid recomputation on every frame:

```
(messages_revision, markdown_layout_revision, screen_width, streaming_content_fp)
```

- **`messages_revision`** — bumped by the shell at the transcript publish interval (~100ms).
- **`streaming_content_fp`** — O(1) fingerprint of the last streaming message's `content.len()`,
  added so the cache invalidates when a streaming appends occurs between publish ticks.
  Without this, the layout cache stays stale until the next publish tick, causing incorrect
  windowing and "empty transcript" when scrolling up during streaming.
- **`markdown_layout_revision`** — bumped by the background markdown parsing future.

### Incremental Layout Cache (layout.rs)

[`IncrementalLayoutCache`] stores per-message metadata so unchanged messages are skipped:

| Field          | Type       | Purpose                                             |
| -------------- | ---------- | --------------------------------------------------- |
| `fingerprints` | `Vec<u64>` | Content hash (samples start/end for streaming)      |
| `row_counts`   | `Vec<u32>` | Bubble height (content + padding), excluding margin |
| `start_rows`   | `Vec<u32>` | Cached `start_row` for forward-walk resumption      |

**Algorithm (`layout_transcript_rows_cached`):**

1. **Backward walk** — find the first changed message (streaming appends at the tail).
2. **Early return** — if nothing changed, reuse cached `start_rows` and `row_counts`.
3. **Forward walk** — resume from `first_changed - 1` (margin dependency), recomputing only
   the changed suffix.

This reduces recomputation from O(n) to O(changed_suffix) for streaming workloads.

### Scroll Hysteresis (panel.rs)

The "near bottom" threshold was lowered from **10 → 6** rows to match the scroll step (3 rows).
Previously, pressing Up required 4 presses to unpin auto-scroll; now 2 presses suffice.

### Measure–Paint Wrap Parity (word_wrap.rs)

Transcript row measurement must predict exactly what iocraft paints, because auto-scroll pins
the viewport bottom to the _measured_ height. Text rendered with `TextWrap::Wrap` (markdown
paragraphs/list items, plain-text cards, tool param values) is word-wrapped by iocraft on
Unicode line-break opportunities; older measurement used a character-wrap layout that packs
more characters per row, under-counting rows at narrow widths (e.g. width 36 → 55 measured vs
62 painted). The clipped painted tail (often the `…` of a truncated line) fell outside the
viewport, so long local slash output (e.g. `/tools`, which now opens a wider, color-coded
scrollable dialog) appeared cut mid-line.

`elph_tui::wrapped_text_row_count` replicates the single-segment case of iocraft's
`SegmentedString::wrap` — same `unicode-linebreak` tables (Unicode 15.0.0) and zero-width
control characters — so measurement matches paint at every width. It is used by:

- Markdown paragraphs / list items and the raw-source fallback (`elph-tui markdown/layout.rs`).
- Plain-text cards: thinking body, tool output, status (`transcript/layout.rs`).
- Streaming markdown tail and markdown part fallback (`transcript/markdown/layout.rs`).
- Tool param values and approval summaries (`tool_params.rs`).
- Dialog/select descriptions (`elph-tui` select + dialog_shell).

Assistant markdown rows (`assistant_row_count`) are measured from the **exact same merged
document** the renderer paints: `build_assistant_markdown_document` concatenates the cached
stable parts with the streaming tail (same fence/tail caps as `render_markdown_buffer`),
preserving the inter-block gap at the stable↔tail boundary. Measuring the stable and tail
segments independently under-counted that gap by one row, so the auto-scroll viewport pinned
one row short and clipped the first line of the following paragraph (the words at the start of
a sentence/block looked cut off mid-stream).

Pre-wrapped text rendered with `TextWrap::NoWrap` (tables, code blocks, user-input cards,
sticky chrome) intentionally stays on the character-wrap path via `wrapped_transcript_row_count`.

### Row-Gap Parity (card chrome)

Two card renderers paint blank rows between sections that measurement must mirror:

- **Thinking cards** paint a 1-row gap between the phase header and the body
  (`phase_card_shell` gap). `message_row_count` adds 1 for thinking bodies that have content.
- **Tool cards** pad the args block 1 row below the header (`padding_top: 1` in
  `tool_call_card`). `ToolCardDetail::layout_text` emits the matching blank line.

Regression coverage: `all_card_kinds_measure_matches_paint` in `transcript/layout.rs` renders
every card kind (thinking streaming/expanded/collapsed, assistant, user, status, tool
running/expanded/collapsed, ask-user, inline diff, and the Thinking+Assistant flush pair) and
asserts measured rows + margins equal painted rows at widths 36/60/120.

### Sticky Prompt vs Auto-Scroll (panel.rs)

The sticky user-prompt bar is only shown while the user has scrolled away from the bottom.
While the viewport is auto-scroll pinned, showing the bar would shrink the scroll view by
`sticky_rows`, shifting the latest card up behind it — the top rows of the newest card
(heading, first bullet lines) would be permanently hidden behind the bar and the card would
look clipped mid-line (e.g. an orphaned `…ing glob patterns…` line as the first visible row).
`sticky_scroll` now defers to `near_bottom`: pinned → no bar, latest card fully visible;
scroll up → bar appears for orientation and disappears again once the prompt re-enters the
viewport or the user returns to the bottom.

### Scroll Height on Collapse (panel.rs + vendored ScrollView)

Toggling a card open/closed changes the content height, but the vendored `ScrollView` keeps
a _peak_ height while scrolled (its translated pane under-reports its measured size ≈
viewport), so after a collapse the old peak could leave the scroll offset past the real
tail — an empty/blank viewport and a stale scrollbar. The panel now passes the authoritative
layout-measured content height to `ScrollView` via `content_height_override`, which replaces
the stale peak (yielding to a larger live measure during streaming growth). The panel also
reads the scroll handle directly each frame (no separate override mirror), clamps a stale
offset to the fresh `max_offset`, and snaps to the bottom when it was clamped. The scrollbar
thumb and offset clamping therefore track the collapsed height immediately.

### Smart Scroll on Expand (panel.rs + vendored ScrollView)

Auto-scroll follows the bottom while the user is pinned there. Expanding a collapsible card
(Ctrl+O for the newest block, or clicking any process header) to read its detail must **not**
yank the view back down as more content streams below it. The panel tracks each collapsible
card's `detail_expanded` state across frames and, on a real collapsed→expanded toggle at a
stable index, calls the vendored `ScrollViewHandle::pause_auto_scroll()` (which sets
`user_scrolled_up = true` without moving the offset) and drops the near-bottom hysteresis
(`near_bottom_sticky = false`). The result: the expanded card stays exactly where it was,
streaming output keeps appending below it, and the view only re-pins to the bottom once the
user scrolls back down (End, arrow/Page-Down, or mouse wheel to the bottom). Newly streamed
cards start expanded but are excluded by `is_collapsible_detail()`, and a shrinking message
count (e.g. an archive reload) is skipped, so the pause fires only on genuine user expands.

### Streaming Content Cap (agent_bridge.rs)

Assistant streaming content is now capped at **200 KB** (similar to `TOOL_OUTPUT_STREAM_CAP` at
100 KB). When the cap is exceeded, the oldest content is dropped from the front with a
`[...stream truncated...]` marker, preserving the tail.

```
ASSISTANT_STREAM_CAP: usize = 200 * 1024  // agent_bridge.rs
TOOL_OUTPUT_STREAM_CAP: usize = 100 * 1024
```

## Tool Labels (tool_params.rs)

Tool call headers in the transcript use descriptive labels. MCP tools include the server name:

| Tool name                  | Transcript label            |
| -------------------------- | --------------------------- |
| `read_file`                | `Read`                      |
| `edit_file`                | `Edit`                      |
| `spawn_agent`              | `Spawning agent`            |
| `mcp_deepwiki__read_wiki`  | `[MCP:deepwiki] Read Wiki`  |
| `mcp_context7__query_docs` | `[MCP:context7] Query Docs` |

The server name is extracted from the exposed tool name format `mcp_{server}__{tool}` via
[`parse_exposed_tool_name`] in `tool_params.rs`.

## Memory Retention (retention.rs)

The transcript is the user's scrollback: **every row stays mounted for the whole session**.
Retention never drops messages — it only sheds *derived* or *bounded* payloads that can be
rebuilt, so memory stays flat without changing what is on screen.

Previously, `run_completed` drained the oldest messages out of `messages_arc_inner` once the
count passed a threshold. That silently deleted earlier scrollback mid-session: the rows were
gone from the panel but still present in `session_entries`, so they reappeared only after
quitting and resuming (which reconstructs the transcript from the tree). Retention replaces
that drain.

`apply_transcript_retention` runs at turn completion (`run_completed`) and walks the messages
**newest-first**, shedding two payloads:

| Payload                          | Rule                                          | Recovery                                  |
| -------------------------------- | --------------------------------------------- | ----------------------------------------- |
| Parsed `MarkdownDocument` caches | Released outside the trailing 40-message window | Re-derived on paint from `content`        |
| Tool diff text (`old_text`/`new_text`) | Stripped past an 8 MB total budget       | Card renders collapsed (same as on resume) |

```rust
const MARKDOWN_DOCUMENT_WINDOW: usize = 40;         // retention.rs
const DIFF_TEXT_BUDGET_BYTES: usize = 8 * 1024 * 1024;
```

Retained `AssistantMarkdownBuffer` metadata (`stable_end`, `stream_complete`, `wrap_width`,
per-part `row_count`) is untouched, so `layout_transcript_rows_cached` keeps measuring released
rows at their correct height and scroll geometry does not shift when a document is freed.

Two details keep this cheap and stable:

- **`without_documents()` instead of `Arc::make_mut`.** The buffer is shared with the render
  snapshot, so `make_mut` would deep-clone every cached document *before* dropping it — a
  memory spike at exactly the moment memory is being reclaimed. `without_documents` builds a
  document-free copy directly.
- **The parse worker honors the same window.** `collect_markdown_parse_jobs` is bounded to the
  trailing `MARKDOWN_DOCUMENT_WINDOW` messages. Without that bound the worker would re-parse
  precisely the rows retention releases each turn, and the two would ping-pong every tick.

Rows scrolled back into view re-derive their document through `build_cached_document`, which
memoizes into the global `BUILT_DOC_CACHE` (64 entries), so revisiting old scrollback parses
at most once.

## Disk Snapshots with Turso

Snapshots persist the transcript for `--resume` / `--continue`. They are unrelated to live
memory retention above.

```
Event Loop                          Panel
    │                                  │
    ▼                                  ▼
┌─────────────────────┐    ┌──────────────────────┐
│  messages_arc_inner │    │  messages State      │
│  (full session Vec) │◄──►│  (synced each tick)  │
│  never drained      │    │  panel reads from    │
└──────────┬──────────┘    └──────────────────────┘
           │
           │ retention at run_completed
           │ (sheds caches, keeps rows)
           ▼
┌─────────────────────┐
│  TranscriptCache    │  ← turso SQLite (overwrite)
└─────────────────────┘
```

### Database Schema

```sql
CREATE TABLE IF NOT EXISTS transcript_messages (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    style       TEXT NOT NULL,
    content     TEXT NOT NULL,
    tool_name   TEXT,
    tool_args   TEXT,
    tool_output TEXT,
    tool_old    TEXT,
    tool_new    TEXT,
    tool_path   TEXT,
    duration    REAL,
    expanded    INTEGER NOT NULL DEFAULT 1,
    pinned      INTEGER NOT NULL DEFAULT 0,
    status      TEXT,
    indent      INTEGER NOT NULL DEFAULT 0,
    tree        TEXT,
    model       TEXT,
    agent       TEXT,
    user_shell  INTEGER NOT NULL DEFAULT 0,
    slash_resp  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(session_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_transcript_msg_session_seq
    ON transcript_messages(session_id, seq);
```

### Snapshot Semantics

`transcript_snapshot` holds **one row per session** (`INSERT OR REPLACE` on `session_id`), so
saving does not accumulate copies. Snapshots previously went into the append-only session tree
as `elph.transcript.snapshot` custom entries, where they grew to 600+ MB; `TranscriptCache::open`
prunes those legacy entries and checkpoints the WAL once, idempotently.

`build_snapshot_data` strips tool diff text (`old_text` / `new_text`) before serializing —
the same payload live retention drops — so snapshot size stays bounded. On resume the tool
card renders collapsed without its inline diff.

### Core API (`TranscriptCache`)

| Method                      | Async | Purpose                                             |
| --------------------------- | ----- | --------------------------------------------------- |
| `open(db_path, session_id)` | ✅    | Open DB, prune legacy tree snapshots, checkpoint WAL |
| `save_snapshot(json)`       | ✅    | Overwrite this session's snapshot (single row)      |
| `load_snapshot()`           | ✅    | Read the session's snapshot, if any                 |
| `push_batch(batch)`         | ✅    | Insert batch (individual `INSERT OR IGNORE`)        |

### Database Location

```
<project>/.elph/store.db       // floppy memory (no transcript tables)
```

floppy `store.db` (memory). It is not merged into `store.db`, and it is unrelated
which stays a separate file.

The transcript schema is created with idempotent DDL (`CREATE TABLE IF NOT EXISTS` +
`CREATE INDEX IF NOT EXISTS`) on open — there is no migration version band and no
`PRAGMA user_version` involvement. The path is resolved via [`Paths::transcript_db_path()`]
in `platform/paths.rs`.

## File Map

| File                                                     | Role                                                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `crates/coding-agent/src/tui/transcript/cache.rs`        | `TranscriptCache` (SQLite snapshot store)                                                              |
| `crates/coding-agent/src/tui/transcript/retention.rs`    | `apply_transcript_retention` (sheds caches, never drops rows)                                          |
| `crates/coding-agent/src/tui/transcript/mod.rs`          | Module exports                                                                                         |
| `crates/coding-agent/src/tui/transcript/panel.rs`        | TranscriptPanel component + render cache                                                               |
| `crates/coding-agent/src/tui/transcript/layout.rs`       | `IncrementalLayoutCache` + row layout                                                                  |
| `crates/coding-agent/src/tui/transcript/types.rs`        | `TranscriptMessage`, `TranscriptStyle`, `ToolCardDetail`                                               |
| `crates/coding-agent/src/tui/transcript/card/builder.rs` | Bubble building + windowing                                                                            |
| `crates/coding-agent/src/tui/transcript/markdown/`       | Streaming markdown buffer + parse workers                                                              |
| `crates/coding-agent/src/tui/tool_params.rs`             | Tool display labels + MCP server names                                                                 |
| `crates/coding-agent/src/tui/agent_bridge.rs`            | Event applier + streaming content caps                                                                 |
| `crates/coding-agent/src/tui/shell/`                     | Shell component (`mod.rs`), event loop (`tick.rs`), key handling (`keys.rs`), view builder (`view.rs`) |
| `crates/coding-agent/src/platform/paths.rs`              | `transcript_db_path()`                                                                                 |

[`TranscriptMessage`]: https://github.com/riipandi/elph/blob/main/crates/coding-agent/src/tui/transcript/types.rs
[`TranscriptStyle`]: https://github.com/riipandi/elph/blob/main/crates/coding-agent/src/tui/transcript/types.rs
[`IncrementalLayoutCache`]: https://github.com/riipandi/elph/blob/main/crates/coding-agent/src/tui/transcript/layout.rs
[`parse_exposed_tool_name`]: https://github.com/riipandi/elph/blob/main/crates/coding-agent/src/tui/tool_params.rs
[`Paths::transcript_db_path()`]: https://github.com/riipandi/elph/blob/main/crates/coding-agent/src/platform/paths.rs
