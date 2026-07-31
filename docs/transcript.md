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
| `Assistant`                                        | Model response (streaming, then collapsible)  |
| `ToolRunning` / `ToolSuccess` / `ToolFailed`       | Tool call card (status-colored tint)          |
| `Meta`                                             | Plain metadata line                           |
| `Error`                                            | API / provider failure (red tint)             |
| `StatusRunning` / `StatusSuccess` / `StatusFailed` | Process log (flush, foreground-only)          |

## State ↔ Arc Sync

The shell keeps two views of the active transcript:

- **`messages_arc_inner`** (`Arc<RwLock<Vec<TranscriptMessage>>>`) — written by the agent
  event applier (streaming, tool calls) and by bootstrap events.
- **`messages` State** — the render source the panel reads from.

Every tick, when an agent event changed the transcript, the shell copies the arc into the
State (`*messages.write() = messages_arc_inner.read()...`). Because that copy **overwrites**
the State, any message written only to the State would disappear on the next agent event.

To prevent that loss, every State write must be mirrored into the arc first:

- `push_transcript_message_synced()` — pushes to the shared arc *and* the State. Used for
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
the viewport bottom to the *measured* height. Text rendered with `TextWrap::Wrap` (markdown
paragraphs/list items, plain-text cards, tool param values) is word-wrapped by iocraft on
Unicode line-break opportunities; older measurement used a character-wrap layout that packs
more characters per row, under-counting rows at narrow widths (e.g. width 36 → 55 measured vs
62 painted). The clipped painted tail (often the `…` of a truncated line) fell outside the
viewport, so `/tools list` output appeared cut mid-line.

`elph_tui::wrapped_text_row_count` replicates the single-segment case of iocraft's
`SegmentedString::wrap` — same `unicode-linebreak` tables (Unicode 15.0.0) and zero-width
control characters — so measurement matches paint at every width. It is used by:

- Markdown paragraphs / list items and the raw-source fallback (`elph-tui markdown/layout.rs`).
- Plain-text cards: thinking body, tool output, status (`transcript/layout.rs`).
- Streaming markdown tail and markdown part fallback (`transcript/markdown/layout.rs`).
- Tool param values and approval summaries (`tool_params.rs`).
- Dialog/select descriptions (`elph-tui` select + dialog_shell).

Pre-wrapped text rendered with `TextWrap::NoWrap` (tables, code blocks, user-input cards,
sticky chrome) intentionally stays on the character-wrap path via `wrapped_transcript_row_count`.

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

## Disk Caching with Turso

### Architecture

The transcript cache uses **libsql** (Turso) local SQLite to archive old messages, keeping
memory usage bounded.

```
Event Loop                          Panel
    │                                  │
    ▼                                  ▼
┌─────────────────────┐    ┌──────────────────────┐
│  messages_arc_inner │    │  messages State      │
│  (active Vec)       │◄──►│  (synced each tick)  │
│  200-500 messages   │    │  panel reads from    │
└──────────┬──────────┘    └──────────────────────┘
           │
           │ drain oldest saat run_completed
           ▼
┌─────────────────────┐
│  TranscriptCache    │  ← turso SQLite
│  push_batch()       │    project/.elph/transcript.db
│  load_range()       │
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

### Archive Trigger

Archival happens **at turn completion** (`run_completed`) when the message count exceeds 500:

```rust
const MAX_MESSAGES_BEFORE_ARCHIVE: usize = 500;  // trigger threshold
const KEEP_MESSAGES: usize = 200;                // keep after truncation
```

1. Clone the first 300 messages from `messages_arc_inner`.
2. Spawn a background `tokio::spawn` to push them to SQLite via `push_batch()`.
3. Drain the first 300 from `messages_arc_inner` in-place.
4. Set `transcript_changed = true` so the next sync propagates the truncation.

### Core API (`TranscriptCache`)

| Method                           | Async | Purpose                                      |
| -------------------------------- | ----- | -------------------------------------------- |
| `open(db_path, session_id)`      | ✅    | Open/create DB, run migrations               |
| `push_batch(batch)`              | ✅    | Insert batch (individual `INSERT OR IGNORE`) |
| `load_range(start_seq, end_seq)` | ✅    | Load archived messages by seq range          |
| `archived_count()`               | ✅    | Count archived messages for this session     |
| `clear_session()`                | ✅    | Delete all data for this session             |

### Hybrid In-Memory Cache (`CachedTranscript`)

A sliding-window wrapper for future use (not yet wired as the primary message store):

- `active: Vec<TranscriptMessage>` — in-memory window (max 200 messages)
- `base_seq: usize` — global seq offset of `active[0]`
- `total: usize` — total messages ever pushed (active + archived)
- Auto-archives when `active.len() > 200` by draining 100 to `pending_archive`
- `flush()` flushes pending archive rows to SQLite
- `suppress_archival()` / `resume_archival()` — used during bootstrap

### Database Location

```
<project>/.elph/transcript.db    // per-project transcript cache
<project>/.elph/store.db          // existing floppy memory store
```

The path is resolved via [`Paths::transcript_db_path()`] in `platform/paths.rs`.

## File Map

| File                                      | Role                                                     |
| ----------------------------------------- | -------------------------------------------------------- |
| `elph/src/tui/transcript/cache.rs`        | `TranscriptCache` + `CachedTranscript`                   |
| `elph/src/tui/transcript/mod.rs`          | Module exports                                           |
| `elph/src/tui/transcript/panel.rs`        | TranscriptPanel component + render cache                 |
| `elph/src/tui/transcript/layout.rs`       | `IncrementalLayoutCache` + row layout                    |
| `elph/src/tui/transcript/types.rs`        | `TranscriptMessage`, `TranscriptStyle`, `ToolCardDetail` |
| `elph/src/tui/transcript/card/builder.rs` | Bubble building + windowing                              |
| `elph/src/tui/transcript/markdown/`       | Streaming markdown buffer + parse workers                |
| `elph/src/tui/tool_params.rs`             | Tool display labels + MCP server names                   |
| `elph/src/tui/agent_bridge.rs`            | Event applier + streaming content caps                   |
| `elph/src/tui/shell.rs`                   | Event loop + archive trigger                             |
| `elph/src/tui/platform/paths.rs`          | `transcript_db_path()`                                   |

[`TranscriptMessage`]: https://github.com/riipandi/elph/blob/main/elph/src/tui/transcript/types.rs
[`TranscriptStyle`]: https://github.com/riipandi/elph/blob/main/elph/src/tui/transcript/types.rs
[`IncrementalLayoutCache`]: https://github.com/riipandi/elph/blob/main/elph/src/tui/transcript/layout.rs
[`parse_exposed_tool_name`]: https://github.com/riipandi/elph/blob/main/elph/src/tui/tool_params.rs
[`Paths::transcript_db_path()`]: https://github.com/riipandi/elph/blob/main/elph/src/platform/paths.rs
