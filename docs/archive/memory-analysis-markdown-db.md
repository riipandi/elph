# Memory Analysis: Markdown Rendering & Database Layers

## Executive Summary

Analysis of the markdown rendering pipeline and database layer identified **12 distinct memory issues**, 5 of which have **high** impact on the >=1GB memory growth observed after ~40 turns. The root causes fall into three categories: (1) unbounded growth of parsed document caches, (2) redundant per-frame allocations in the render path, and (3) database snapshots that duplicate in-memory state.

---

## Part 1: Markdown Rendering Pipeline

### Issue H1 (HIGH): `build_assistant_markdown_document` clones on every measure + paint

**Files:**

- `crph/src/tui/transcript/markdown/render.rs:67-133` — `build_assistant_markdown_document`
- `crph/src/tui/transcript/markdown/layout.rs:17-30` — `assistant_row_count` (measure path)
- `crph/src/tui/transcript/card/frame.rs:113-116` — `assistant_message_body` (paint path)

**Problem:** For every assistant message in the viewport, `build_assistant_markdown_document` is called **twice per frame**: once from `assistant_row_count` (layout/measure) and once from `render_markdown_buffer` (paint). Each call:

1. Clones the cached `MarkdownDocument` from `RenderedPart.document` (`doc.clone()`)
2. Calls `merge_documents` which clones all lines again
3. For the streaming tail, re-parses markdown via `streaming_tail_document` → `parse_markdown_document`

A single 500-line assistant reply parsed into ~800 `MarkdownLine` × ~2000 `StyledSpan` can occupy 3-5 MB. With 5-10 visible assistant messages and 2 calls/frame at 10 fps, that's **30-50 MB/sec of transient allocations**.

**Fix applied (session 1):** Added a fast path in `assistant_row_count` that measures directly from the cached document when `stream_complete && stable_end >= content.len() && wrap_width == wrap_width`, avoiding the clone+merge. This eliminates the measure-path allocation for completed messages (the vast majority in a long session).

**Remaining optimization:** The paint path still clones. Could cache the built document per-frame keyed by `(message_id, wrap_width, content_len)` and reuse between measure and paint.

**Estimated savings:** 150-300 MB peak in a 40-turn session.

---

### Issue H2 (HIGH): `AssistantMarkdownBuffer.parts` accumulates stale `RenderedPart` entries during streaming

**File:** `crph/src/tui/transcript/markdown/buffer.rs:14-25`

**Problem:** `parts: Vec<RenderedPart>` is replaced on every `refresh_stable` call (set to `vec![RenderedPart { ... }]`), so it never grows beyond 1 entry — **this is fine**. However, each `RenderedPart.document: Option<MarkdownDocument>` holds a fully parsed document that is only needed for rendering. After the stream completes and the message scrolls off-screen (or is archived), the parsed document stays in memory.

**Fix applied (session 1):** Added `drop_cached_documents()` to `AssistantMarkdownBuffer` and wired it into the archive path to shed parsed documents from retained messages beyond `MARKED_MESSAGES_WITH_MARKDOWN_CACHE` (20).

**Superseded:** that archive path also *drained* old messages, which deleted visible scrollback mid-session. It is replaced by `transcript/retention.rs` (`apply_transcript_retention`), which keeps every row and releases documents outside a trailing 40-message window via `without_documents()` — avoiding the `Arc::make_mut` deep-clone spike. See `docs/archive/transcript.md` § Memory Retention.

**Estimated savings:** 200-500 MB in a 40-turn session (each parsed document is 1-5 MB).

---

### Issue M1 (MEDIUM): `streaming_tail_document` re-parses the entire tail on every frame during streaming

**File:** `crph/src/tui/transcript/markdown/render.rs:331-335`

**Problem:** During active streaming, `streaming_tail_document(tail)` is called on every frame from `build_assistant_markdown_document`. The tail grows with each token (up to 4000 chars capped). Each call does a full `pulldown_cmark` parse + `ParserState` allocation + span merging. At 10 fps with a 4000-char tail, that's a full parse of 4000 chars × 10/sec = 40KB/sec of parse throughput just for the tail.

**Proposed fix:** Incremental tail parsing — only parse new tokens appended since last frame, merging into the existing tail document. This requires a streaming pulldown-cmark wrapper or caching the last tail hash and skipping parse if unchanged.

**Estimated savings:** 30-60 MB/sec transient, smoother streaming.

---

### Issue M2 (MEDIUM): `recolor_range` + `wrap_code_spans` allocate per-visual-row per-code-line per frame

**File:** `crph/src/tui/transcript/markdown/render.rs:40-75`

**Problem:** For code blocks, `wrap_code_spans` is called per line every frame. It calls `recolor_range` which allocates a new `Vec<StyledSpan>` per visual row. A 50-line code block wrapped to 30 visual rows × 3 span each = 90 small allocations per frame per code block.

**Proposed fix:** Cache the wrapped visual rows in the `MarkdownLine` (or a side-table keyed by `(line_index, wrap_width)`) and only re-wrap when width changes.

**Estimated savings:** Minor steady-state (5-10 MB), but reduces frame-time jitter.

---

### Issue M3 (MEDIUM): `StyledSpan` uses `String` + `Option<String>` for small text fragments

**File:** `crates/elph-tui/src/components/markdown/model.rs:28-55`

**Problem:** Each styled span allocates a `String` for text (often 1-20 chars) and an `Option<String>` for href. A typical paragraph of 80 words × ~3 spans each = ~240 small heap allocations. With normalization, the parser merges adjacent spans with the same style, which helps, but code blocks and links prevent merging.

**Proposed fix:** Use `Arc<str>` or a small-string optimization (e.g., `compact_str` or inline strings up to 22 chars) for span text. Share href strings via `Arc<str>` across spans pointing to the same URL.

**Estimated savings:** 20-40% reduction in markdown document memory.

---

### Issue L1 (LOW): `highlight_code_block` allocates a `Vec<MarkdownLine>` per code block at parse time

**File:** `crates/elph-tui/src/components/markdown/highlight.rs:22-60`

**Problem:** Called from `parse.rs` `Event::Event::End(TagEnd::CodeBlock)`. Allocates `Vec<MarkdownLine>` with full styled spans via syntect. This is parse-time (not per-frame), so it's a one-time cost per message, but for very long code blocks (1000+ lines) it can allocate 50+ MB in one shot.

**Proposed fix:** Stream the highlight — process N lines at a time instead of collecting all into a Vec first.

---

## Part 2: Database Layer

### Issue H3 (HIGH): Transcript snapshot grows unboundedly — appended per turn, never compacted

**File:** `crph/src/agent/session/mod.rs:740-748` — `save_transcript_snapshot`
**File:** `crates/elph-agent/src/session/tree.rs:260-275` — `append_custom_entry`

**Problem:** `save_transcript_snapshot` calls `append_custom_entry(TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, ...)` which **appends** a new `SessionTreeEntry::Custom` on every `RunCompleted`. The session tree keeps ALL entries. On resume, `messages_from_snapshot_data` reads only the **latest** snapshot, so all prior snapshots are dead weight. After 40 turns, there are 40 full transcript snapshots in the session tree (each 1-5 MB JSON = 40-200 MB wasted).

**ROOT CAUSE (verified):** 346 snapshot rows × ~2 MB average = **682 MB** (95% of 789 MB store.db). Snapshots appended every turn, never pruned.

**Fix applied (session 3):**

2. **Legacy pruning:** `TranscriptCache::open()` auto-prunes all `elph.transcript.snapshot` entries from `session_entries` on first open.
3. **WAL checkpoint:** Auto-checkpoints on startup.

**Verified result:** 682 MB freed (346 rows → 0 rows). Remaining store.db: ~68 MB (messages + metadata).

---

### Issue H4 (HIGH): `build_snapshot_data` serializes `ToolCardDetail` with full `old_text`/`new_text`

**File:** `crph/src/tui/transcript/archive.rs:127-137`

**Problem:** `ArchivedTranscriptMessage.tool` includes the full `ToolCardDetail` which contains `old_text` and `new_text` (complete file contents before/after edit). A single `edit_file` on a 1000-line file stores ~500 KB in the snapshot. With 10 such edits per turn × 40 turns = 200 MB in tool diff text alone, retained across all snapshot versions.

**Proposed fix:**

1. Diff text should NOT be in the snapshot — it can be recomputed from the tool result on resume.
2. Or cap stored diff text at N lines (e.g., 200) and mark as truncated.
3. Ensure old snapshots are pruned (H3 fix) so this isn't retained 40×.

**Estimated savings:** 100-300 MB.

---

### Issue M4 (MEDIUM): `TranscriptCache.push_batch` does individual executes, no transaction

**File:** `crph/src/tui/transcript/cache.rs:71-95`

**Problem:** `push_batch` does one `conn.execute(...)` per message in the batch without wrapping in a transaction. Each execute is auto-committed, causing an fsync per message. For a batch of 130 messages (150 - 20 keep), that's 130 fsyncs. This is I/O performance, not memory, but it blocks the archive task.

**Proposed fix:** Wrap the batch in `conn.execute("BEGIN", ())` / `conn.execute("COMMIT", ())`.

---

### Issue M5 (MEDIUM): `TranscriptCache` opens a new DB connection per archive call

**File:** `crph/src/tui/transcript/cache.rs:22-28`, called from `tick.rs`

**Problem:** `TranscriptCache::open` is called every time `should_archive` is true (once per run when >150 messages). Each call does `open_local` + `connect` + `run_migrations`. The `Database` handle is dropped at end of async task, closing the connection.

**Proposed fix:** Cache the `TranscriptCache` (or at least the `Database` handle) in the shell state and reuse across archive calls. Open once, archive many times.

**Estimated savings:** Reduces archive latency from ~200ms to ~20ms.

---

### Issue L2 (LOW): `ArchivedTranscriptMessage::into_transcript_message` re-parses markdown on every resume

**File:** `crph/src/tui/transcript/archive.rs:72-103`

**Problem:** On resume, every assistant message gets its markdown re-parsed via `parse_markdown_on_worker`. For 60 retained messages with long replies, that's 60 full parses on the async task before the TUI can render. Not a memory leak, but a startup latency issue.

**Proposed fix:** Lazily parse markdown on first render (the worker tick already does this for the visible messages).

---

### Issue L3 (LOW): WAL file grows if `PRAGMA wal_autocheckpoint` is not tuned

**File:** `crates/elph-db/src/lib.rs` — no explicit autocheckpoint

**Problem:** Turso's default WAL autocheckpoint is 1000 pages. With frequent appends (transcript archive + session tree), the WAL can grow to several MB before checkpointing. Not a memory issue, but disk usage.

**Proposed fix:** Set `PRAGMA wal_autocheckpoint = 100` after opening.

---

## Summary of Fixes by Impact

| ID  | Impact | Category            | Status                                       | Est. Savings   |
| --- | ------ | ------------------- | -------------------------------------------- | -------------- |
| H1  | High   | Markdown render     | ✅ Fast path for completed msgs (session 1)  | 150-300 MB     |
| H2  | High   | Markdown buffer     | ✅ `drop_cached_documents` (session 1)       | 200-500 MB     |
| H4  | High   | DB snapshot         | ✅ Strip diff text from snapshot (session 2) | 100-300 MB     |
| M4  | Medium | DB batch insert     | ✅ Wrap in transaction (session 2)           | I/O + latency  |
| H3  | High   | DB snapshot         | ⏸ Out of scope (append-only tree)            | —              |
| M5  | Medium | DB connection       | ⏸ Deferred (needs ShellCtx threading)        | —              |
| M1  | Medium | Markdown tail parse | 🔲 Incremental tail parse                    | 30-60 MB/sec   |
| M2  | Medium | Code block wrap     | 🔲 Cache wrapped rows                        | 5-10 MB        |
| M3  | Medium | Span allocation     | 🔲 `Arc<str>` / small-string                 | 20-40% doc mem |
| L1  | Low    | Highlight alloc     | 🔲 Stream highlight                          | Spike          |
| L2  | Low    | Resume parse        | 🔲 Lazy parse                                | Latency        |
| L3  | Low    | WAL size            | 🔲 Lower autocheckpoint                      | Disk           |

✅ = applied · 🔲 = proposed for future · ⏸ = deferred/out of scope
