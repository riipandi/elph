# Plan: Session History Restore, Schema Normalization, Retention, TODO + ReAct Structure

## Problem summary

Resume/continue fails to restore a reliable conversation view and lacks a clean, queryable record of turns, usage, and structured work. Root causes span three crates and a dual-write design. Separately, the append-only tree + UI snapshots have already produced multi-hundred-MB `store.db` files; retention and physical prune are required so restore stays solid as projects age.

### Critical bugs (history restore)

| Layer                | Current behavior                                                                                                                                            | Failure mode                                                                                                |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| **LLM context**      | `create_turn_state` → `session.build_context()` from branch (`session_entries` tree)                                                                        | Works **if** entries were flushed; durable for model context when tree is intact                            |
| **TUI transcript**   | Load order: `transcript_snapshot` cache → legacy tree `elph.transcript.snapshot` → reconstruct from LLM entries                                             | Snapshot saves via `tokio::spawn` (fire-and-forget) — process quit/crash loses last turn                    |
| **Dual write**       | On `RunCompleted`, still saves cache **and** appends full snapshot to session tree                                                                          | Tree bloated historically (7–8 MB/snapshot); prune deletes _all_ sessions’ tree snapshots on any cache open |
| **Unbounded growth** | Tree is append-only; compaction only transforms _context_, not disk; journal custom entries accumulate; terminals/mcp_cache grow under `APP_DATA/sessions/` | DB and disk balloon; open/resume slower; WAL large                                                          |
| **Metrics**          | Usage lives only inside assistant message JSON; `SessionStatistics.approximate_tokens` is always `0`                                                        | Cost/token not queryable; `elph stats` is stub                                                              |
| **Todos**            | Table exists (`todos` INTEGER PK, `completed` bool) but **no tools, no TUI, no prompt injection**                                                           | Agent has no structured plan channel; goals only cover session objective + budgets                          |

Primary code paths:

- Restore: `coding-agent/src/tui/startup.rs` (`load_chat_history`)
- Persist: `coding-agent/src/tui/shell/tick.rs` (cache + legacy tree snapshot)
- Schema: `coding-agent/src/platform/migrations.rs` (v101–107) + `elph-agent/src/session/migrations.rs` (v100)
- LLM rehydrate: `elph-agent/.../run_loop/turn_state.rs` → `session.build_context()`

### Assumptions (clean break)

- No data/schema migration from existing DBs; users may delete `.elph/store.db` or accept wipe on next open.
- No backward-compat shims for old `todos` INTEGER schema, legacy tree snapshots, or dual snapshot writers.
- Session **tree** (branching, compaction, leaf) stays as the structural backbone for LLM messages and config events; we **normalize** turn/usage/todo as first-class relational tables around it.
- Floppy **memory tasks** remain long-term episodic scoring (separate from session todos). Do not merge concerns.
- **Retention is first-class**, not a bolt-on: compaction must be able to _physically_ reclaim space; session GC is automatic with safe defaults and CLI override.
- **All user-tunable knobs live in `settings.json`** (home `CONFIG_DIR/settings.json` + optional project `.elph/settings.json`, same merge rules as today: project wins). Host maps settings into elph-agent options at session create — agent never reads settings paths.

---

## Settings (`settings.json`) — configuration surface

Follow existing domain groups (`camelCase`, serde defaults, home ← project merge). Add a **`session`** group (legacy flat `session` migration already exists and lifts into models — new group is intentionally for _storage/retention_, not live model state).

### Recommended shape

```json
{
    "preferredChatLanguage": "english",
    "maxRetries": 2,
    "defaultTimeout": "120s",
    "ui": { "...": "..." },
    "models": { "...": "..." },
    "memory": { "...": "..." },
    "codegraph": { "...": "..." },
    "mcp": {
        "cacheTtlSecs": 60,
        "cacheMaxEntries": 2048
    },
    "notifications": { "...": "..." },
    "compaction": {
        "thresholdPct": 80,
        "keepRecentTokens": 20000,
        "physicalPrune": true
    },
    "session": {
        "retention": {
            "enabled": true,
            "gcOnOpen": true,
            "maxSessionsPerCwd": 40,
            "maxSessionAgeDays": 30,
            "maxEntriesPerSession": 8000,
            "maxStoreDbBytes": 536870912,
            "protectLatestPerCwd": true,
            "maxEntryPayloadBytes": 262144,
            "journalKeepTurns": 20,
            "maxTerminalFilesPerSession": 50
        }
    }
}
```

### Structs (coding-agent `platform/settings.rs`)

```rust
// On Settings root:
pub session: SessionSettings,

#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    #[serde(default)]
    pub retention: SessionRetentionSettings,
}

#[serde(rename_all = "camelCase")]
pub struct SessionRetentionSettings {
    /// Master switch for automatic session GC + size enforcement.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Run GC when opening the project store (best-effort, never mid-turn).
    #[serde(default = "default_true")]
    pub gc_on_open: bool,
    /// Keep at most N non-pinned sessions per project cwd (newest `updated_at` first).
    #[serde(default = "default_max_sessions_per_cwd")] // 40
    pub max_sessions_per_cwd: u32,
    /// Drop non-pinned sessions older than this many days (`0` = no age limit).
    #[serde(default = "default_max_session_age_days")] // 30
    pub max_session_age_days: u32,
    /// Soft pressure: after this many tree entries, prefer compact+prune (or mark for GC).
    #[serde(default = "default_max_entries_per_session")] // 8000
    pub max_entries_per_session: u32,
    /// Soft budget for `.elph/store.db` file size in bytes (`0` = unlimited).
    #[serde(default = "default_max_store_db_bytes")] // 512 MiB
    pub max_store_db_bytes: u64,
    /// Never auto-GC the most recently updated session for a cwd.
    #[serde(default = "default_true")]
    pub protect_latest_per_cwd: bool,
    /// Truncate oversized entry payloads on write (bytes); large tool bodies stay on disk.
    #[serde(default = "default_max_entry_payload_bytes")] // 256 KiB
    pub max_entry_payload_bytes: u32,
    /// Keep harness journal custom entries covering approximately this many recent turns.
    #[serde(default = "default_journal_keep_turns")] // 20
    pub journal_keep_turns: u32,
    /// Cap terminal output files retained per session during long runs (`0` = unlimited until session GC).
    #[serde(default = "default_max_terminal_files")] // 50
    pub max_terminal_files_per_session: u32,
}

// Extend CompactionConfig:
pub physical_prune: bool, // default true — DELETE pre-boundary entries after compact
```

### Wiring rules

| Knob                                           | Consumer                                                                            |
| ---------------------------------------------- | ----------------------------------------------------------------------------------- |
| `session.retention.*`                          | `SessionManager` / GC on open; cascade delete; CLI `session prune` uses same policy |
| `session.retention.maxEntryPayloadBytes`       | Turso `append_entry` / harness message write path                                   |
| `session.retention.journalKeepTurns`           | Journal GC after idle / post-turn                                                   |
| `session.retention.maxTerminalFilesPerSession` | shell_exec terminal writer / periodic trim                                          |
| `compaction.physicalPrune`                     | Compaction completion hook in elph-agent (host passes bool in `CompactionSettings`) |
| `compaction.thresholdPct` / `keepRecentTokens` | Existing auto-compact (unchanged meaning)                                           |
| `mcp.cache*`                                   | Existing MCP cache (already retention-like)                                         |

- **`0` semantics:** for age/byte/count fields where documented, `0` means “unlimited / disabled for that dimension” (except booleans).
- **Project override:** heavy monorepos can set tighter `maxSessionsPerCwd` or larger `maxStoreDbBytes` in `.elph/settings.json`.
- **Pin is not a setting** — it is per-session DB state (`sessions.pinned`) via CLI/`elph session pin`.
- Document all keys in `docs/session-persistence.md` and bootstrap default `settings.json` via `Settings::defaults()`.
- No new env vars required for retention (settings.json is enough); optional `ELPH_*` only if we already have a pattern for overrides — prefer settings only.

---

## Target architecture

```text
                    ┌─────────────────────────────────────┐
                    │  ReAct loop (AgentHarness)           │
                    │  Observe → Plan → Act → Evaluate     │
                    └──────────────┬──────────────────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          ▼                        ▼                        ▼
   session_entries          session_turns              session_todos
   (tree / messages /       (lifecycle + usage/cost)   (structured work)
    config / journal)
          │                        │                        │
          └────────────────────────┼────────────────────────┘
                                   ▼
                          sessions (rollup + leaf + pin)
                                   │
              retention GC ──► prune old sessions / pre-compact entries
                                   │
                    transcript_view (derived from tree, not dual SoT)
```

### Source of truth (SoT)

| Concern                      | SoT                                                | Derived                                                               |
| ---------------------------- | -------------------------------------------------- | --------------------------------------------------------------------- |
| LLM conversation + branch    | `session_entries` tree (post-retention active set) | `build_session_context`                                               |
| Turn lifecycle + tokens/cost | `session_turns`                                    | Session rollup columns; `elph stats` (survives message prune if kept) |
| Agent work plan              | `session_todos`                                    | System-prompt injection; TUI Tasks panel                              |
| TUI cards                    | **Reconstruct from tree**                          | Drop unbounded snapshot-as-history                                    |
| Long-term knowledge          | floppy memory                                      | Unchanged                                                             |

**History restore rule:** on resume/continue, TUI loads by reconstructing from the session branch (same data the model uses). Full card snapshots are not the SoT.

**Retention rule:** disk holds only what is needed for resume + recent audit. Compaction is both a _context_ and a _storage_ event. Old sessions expire unless pinned.

---

## Phase 1 — Database redesign (best-fit for Elph)

Clean break: replace platform band **v101–107** and elph-agent session SQL with one **canonical platform band `200+`** (floppy memory 1–99 / codegraph 500+ unchanged). Shared SQL constants live in **elph-agent**; coding-agent migrations re-export them.

### Design principles

1. **Tree for conversation structure** — branching, compaction, leaf, config events stay parent-linked (Pi model that already works for resume).
2. **Relational for metrics & work** — turns, todos, rollups are queryable without parsing JSON.
3. **No UI blob tables** — transcript is reconstructed; never store multi-MB snapshots.
4. **GC-friendly columns** — every row that GC/prune touches has indexed foreign keys and size metadata.
5. **Cascade delete** — session removal is one transactional cascade + artifact dir wipe.

### 1.1 Recommended schema

```text
sessions                    1───* session_entries     (tree spine + payload)
   │                        1───* session_turns       (usage / lifecycle audit)
   │                        1───* session_todos
   │                        1───* goals
   │                        1───  session_sequences   (next_seq)
   └── parent_session_id    (subagent / fork lineage; soft ref)

agent_spawn_edges           (parent/child session graph; keep)
skill_cache                 (global, not per-session; keep)
```

#### `sessions`

| Column                                | Type                       | Purpose                                    |
| ------------------------------------- | -------------------------- | ------------------------------------------ |
| `id`                                  | TEXT PK                    | Session id                                 |
| `created_at`, `updated_at`            | TEXT                       | ISO; GC ordering uses `updated_at`         |
| `cwd`                                 | TEXT                       | Project key                                |
| `parent_session_id`                   | TEXT NULL                  | Fork / subagent parent                     |
| `provider_id`, `model_id`             | TEXT NULL                  | Last known (denorm; tree is authoritative) |
| `agent_mode`                          | TEXT                       | Default `build`                            |
| `name`                                | TEXT NULL                  | Display title                              |
| `system_prompt`                       | TEXT NULL                  | Optional snapshot                          |
| `metadata`                            | TEXT NULL                  | JSON bag for host                          |
| `active_leaf_id`                      | TEXT NULL                  | Tree leaf pointer                          |
| `pinned`                              | INTEGER NOT NULL DEFAULT 0 | Retention immune when 1                    |
| `turn_count`                          | INTEGER NOT NULL DEFAULT 0 | Rollup                                     |
| `total_input_tokens` … `total_tokens` | INTEGER                    | Rollups from turns                         |
| `total_cost`                          | REAL NOT NULL DEFAULT 0    |                                            |
| `last_turn_at`                        | TEXT NULL                  |                                            |
| `entry_count`                         | INTEGER NOT NULL DEFAULT 0 | Maintained on append/prune                 |
| `approx_bytes`                        | INTEGER NOT NULL DEFAULT 0 | Sum of payload sizes for soft budgets      |

Indexes: `cwd`, `updated_at`, `(cwd, updated_at)`, `pinned`.

#### `session_entries` (tree spine — redesigned columns)

| Column          | Type             | Purpose                                                                        |
| --------------- | ---------------- | ------------------------------------------------------------------------------ |
| `session_id`    | TEXT NOT NULL    | FK → sessions ON DELETE CASCADE                                                |
| `id`            | TEXT NOT NULL    | Entry id                                                                       |
| `entry_seq`     | INTEGER NOT NULL | Monotonic write order                                                          |
| `parent_id`     | TEXT NULL        | Tree parent                                                                    |
| `type`          | TEXT NOT NULL    | `message`, `compaction`, `model_change`, `custom`, …                           |
| `timestamp`     | TEXT NOT NULL    |                                                                                |
| `turn_id`       | TEXT NULL        | FK soft-link to `session_turns.id` when applicable                             |
| `role`          | TEXT NULL        | Denorm for messages: `user` / `assistant` / `tool` / … (NULL for non-messages) |
| `payload_bytes` | INTEGER NOT NULL | Length of payload for GC budgets                                               |
| `payload`       | TEXT NOT NULL    | Full `SessionTreeEntry` JSON                                                   |

PK `(session_id, id)`; unique `(session_id, entry_seq)`; indexes on `parent_id`, `type`, `turn_id`.

**Do not** store `elph.transcript.snapshot` custom types.

#### `session_sequences`

Unchanged intent: `(session_id PK, next_seq)`.

#### `session_turns` (new)

| Column                                        | Notes                                              |
| --------------------------------------------- | -------------------------------------------------- |
| `id` TEXT PK                                  | `turn_<kalid>`                                     |
| `session_id` TEXT NOT NULL                    | ON DELETE CASCADE                                  |
| `turn_index` INTEGER NOT NULL                 | Monotonic per session                              |
| `status` TEXT                                 | `started` / `completed` / `failed` / `interrupted` |
| `operation_id` TEXT NULL                      | Harness op link                                    |
| `started_at` / `finished_at`                  | ISO                                                |
| `wall_clock_ms` INTEGER                       |                                                    |
| `provider_id` / `model_id` / `thinking_level` |                                                    |
| token + cost columns                          | From `elph_ai::Usage`                              |
| `user_entry_id` / `assistant_entry_id`        | Optional entry links                               |
| `error_message` TEXT NULL                     |                                                    |

Indexes: `(session_id, turn_index)`, `(session_id, started_at)`.

**Retention:** turns are small fixed-width rows — **keep after in-session message prune** so stats survive compaction.

#### `session_todos` (replace INTEGER stub `todos`)

| Column                                       | Notes                                                 |
| -------------------------------------------- | ----------------------------------------------------- |
| `id` TEXT PK                                 | `todo_<kalid>`                                        |
| `session_id` TEXT NOT NULL                   | ON DELETE CASCADE                                     |
| `content` TEXT NOT NULL                      |                                                       |
| `status` TEXT                                | `pending` / `in_progress` / `completed` / `cancelled` |
| `position` INTEGER                           |                                                       |
| `created_at` / `updated_at` / `completed_at` |                                                       |

#### Keep unchanged in purpose

- `goals`, `agent_spawn_edges`, `skill_cache`

#### Drop permanently

- `todos` (INTEGER autoincrement stub)
- `transcript_snapshot`, `transcript_messages` (bloat SoT)

### 1.2 Why this shape (vs fully linear messages only)

| Approach                         | Pros                                | Cons for Elph                                  |
| -------------------------------- | ----------------------------------- | ---------------------------------------------- |
| Pure linear messages table       | Simple                              | Loses branch / compaction / navigate-tree      |
| Pure JSON tree only (status quo) | Flexible                            | No queryable cost/turns; hard GC; opaque stats |
| **Hybrid (chosen)**              | Branching + metrics + prune + todos | Slightly more write code                       |

### 1.3 Ownership by crate

| Crate            | Responsibility                                                                                                                                           |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **elph-agent**   | Canonical DDL; storage; TurnStore; TodoStore; turn accounting; physical prune after compact; journal GC; recovery                                        |
| **coding-agent** | Migrations re-export; **`SessionSettings` / retention in settings.json**; GC on open; TUI; CLI pin/prune; artifact cleanup; map settings → agent options |
| **floppy**       | Unrelated to chat schema; document memory `tasks` ≠ session todos                                                                                        |

---

## Phase 2 — Retention & size control (driven by settings.json)

Three layers; every numeric/boolean policy comes from **`session.retention`** + **`compaction.physicalPrune`** (see Settings section above). Host loads `Settings::load(paths)` and passes a plain `SessionRetentionPolicy` / flags into agent APIs (no settings path I/O inside elph-agent).

### 2.1 Write-path size control

| Mechanism             | Setting                                        | Behavior                                                                           |
| --------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------- |
| No UI snapshots in DB | (hard rule)                                    | Reconstruct TUI; never write snapshot tables                                       |
| Tool output offload   | —                                              | Large bodies under `APP_DATA/sessions/<id>/`; tree stores path + truncated preview |
| Payload soft cap      | `session.retention.maxEntryPayloadBytes`       | Truncate + `truncated` marker in details                                           |
| Journal hygiene       | `session.retention.journalKeepTurns`           | Drop applied/old harness journal customs outside window                            |
| Terminal file cap     | `session.retention.maxTerminalFilesPerSession` | Keep newest N; delete older files mid-session                                      |
| WAL checkpoint        | after GC/prune                                 | `PRAGMA wal_checkpoint(TRUNCATE)`                                                  |

### 2.2 In-session physical prune (after compaction)

Today compaction only changes _context_, not disk.

When `compaction.physicalPrune == true` (default) and a compaction entry commits:

1. DELETE `session_entries` rows for this session that are before the keep boundary / not on active path.
2. Rebuild index / leaf; update `entry_count` / `approx_bytes`.
3. Keep `session_turns` (stats audit).
4. Prune closed harness journal entries before the boundary.
5. Never prune mid-turn; never delete compaction entry or kept tail.

If `physicalPrune` is false: context-only compact (legacy behavior) — still allowed for debugging, not recommended.

### 2.3 Cross-session GC

When `session.retention.enabled` and (`gcOnOpen` or CLI prune):

Apply filters (all dimensions AND-able; `0` = skip that dimension):

1. Exclude `pinned = 1`
2. Exclude latest session per cwd if `protectLatestPerCwd`
3. Delete non-pinned with `updated_at` older than `maxSessionAgeDays`
4. Per cwd, keep only `maxSessionsPerCwd` newest
5. If file size of store.db > `maxStoreDbBytes`, delete oldest remaining non-pinned until under budget or only protected remain

Cascade: entries, sequences, turns, todos, goals, spawn edges, artifact dir.

CLI (uses same settings policy; flags only for scope/dry-run):

- `elph session pin|unpin <id>`
- `elph session prune [--dry-run] [--cwd|--all]`
- `elph session delete` (explicit)

### 2.4 Artifact retention

| Artifact                     | Policy                                                 |
| ---------------------------- | ------------------------------------------------------ |
| `terminals/*.txt`            | Session cascade + `maxTerminalFilesPerSession`         |
| `mcp_cache`                  | Session cascade + existing `mcp.cacheMaxEntries` / TTL |
| Orphan `APP_DATA/sessions/*` | GC pass removes dirs without DB session                |

### 2.5 Safety invariants

- Never GC the open/active session id of the current process
- Never GC pinned or (when configured) latest-per-cwd
- In-session prune never removes post-compaction tail needed for resume
- Stats remain available from `session_turns` after message prune

---

## Phase 3 — Durable turn + message write path (resilience)

### 3.1 Turn lifecycle in harness

Wire to existing journal concepts (`harness.turn_started` / `turn_finished`) **and** relational `session_turns`:

1. **`execute_turn` start** → insert `session_turns` row (`status=started`), journal as today
2. **Each `MessageEnd`** → append `session_entries` **synchronously** (already does); associate entry with current turn if needed via optional column or side index
3. **Turn end / AgentEnd** → update turn with usage from last assistant `Usage`, set status, update session rollups
4. **Interrupt/fail** → mark turn + repair tool results (`reconcile_session` already exists)

### 3.2 Flush reliability (fix empty resume)

- Remove fire-and-forget dependency for **authoritative** state: tree writes already sync in `handle_agent_event`; ensure turn row commit is in the same durability boundary as last message flush
- On process shutdown / TUI quit: await pending flushes (no `tokio::spawn` for critical path without join on exit)
- Keep `BEGIN IMMEDIATE` transactions in Turso backend for entry + leaf atomicity

### 3.3 Resume / continue contract

```text
SessionManager::create(Some(id))
  → open Turso tree + reconcile
  → AgentHarness::restore (model, tools, queues, journal)
  → create_turn_state.build_context()  // LLM history

TUI bootstrap
  → reconstruct_transcript_from_llm_entries(branch)
  → seed prompt history from user cards
  → load session_todos for Tasks panel

Datastore open (async, best-effort)
  → session retention GC (non-pinned, non-protected)
```

Success criteria: after kill -9 mid-idle (after last turn completed), `--continue` shows same user/assistant/tool cards **and** model receives prior messages.

---

## Phase 4 — TUI history: reconstruct-first (kill dual SoT)

1. **Delete** (or hard-deprecate) path that treats snapshot as primary SoT
2. Make `reconstruct_transcript_from_llm_entries` the **only** resume path for cards; harden it:
    - Thinking / text / tool_call / tool_result / durations (`_elph_ui`)
    - Skill/template prompt cards via `prompt_title` / `prompt_kind`
    - Compaction / branch_summary as system-style notices if needed
3. Remove dual save in `tick.rs` (`save_transcript_snapshot` tree + cache) for history — this alone stops the worst historical bloat
4. Optional later: compact UI prefs (`expanded` flags) keyed by entry id — out of scope unless needed for parity
5. Integration test: append multi-turn tree → open resume → non-empty transcript + context message count

---

## Phase 5 — TODO feature (structured ReAct work)

Inspired by:

- Kimi `TodoList`: statuses `pending` / `in_progress` / `done`; full replace or query; stale reminder after N turns
- Grok Build `todo`: `pending` / `in_progress` / `completed` / `cancelled`; **merge by id** (default) vs replace; reject duplicate ids

### 5.1 Agent tools (`elph-agent`)

Single tool **`todo_write`** (name aligned with grok; document alias in tools.md):

```json
{
    "merge": true,
    "todos": [
        {
            "id": "todo_…",
            "content": "…",
            "status": "pending|in_progress|completed|cancelled"
        }
    ]
}
```

- `merge: true` (default): upsert by id; omit content → keep previous
- `merge: false`: replace entire session list
- Empty `todos` + replace → clear
- Optional **`todo_read`** or allow empty write as read (kimi-style query) — prefer explicit `todo_read` for clarity
- Validation: at most one `in_progress`; reject duplicate ids in one call

`TodoStore` API: `list`, `replace`, `merge`, `clear` on shared DB handle (same pattern as `GoalStore`).

### 5.2 Host wiring (`coding-agent`)

- Register tools in `create_coding_session_with_events` alongside goals
- Active by default in build/brave (not plan/ask if tools are filtered)
- TUI **Tasks panel** above input (design already sketched in archived `docs/archive/tui.md`): show non-cancelled open items; hide when empty; completion notice when last active completes
- Subscribe to tool results to refresh panel without full redraw thrash

### 5.3 Prompt / ReAct structure (`prompts/` in coding-agent)

Update `coding_base.txt` (and mode snippets as needed):

1. **Observe** — use conversation, memory blocks, codegraph, todos
2. **Plan** — for multi-step (≥3) non-trivial work, call `todo_write` early; keep exactly one `in_progress`
3. **Act** — tools; do not re-do completed todos
4. **Evaluate** — mark completed/cancelled; only then move next item

Stale reminder (kimi pattern): if todos exist and no `todo_write` for N turns (e.g. 10), inject a non-user-facing system reminder with current list (hook in harness before provider call or via `prepare_next_turn` / context transform).

Do **not** spam todos for trivial single-step asks.

### 5.4 Goals vs todos

|              | Goals                        | Todos                           |
| ------------ | ---------------------------- | ------------------------------- |
| Scope        | Session objective + budgets  | Step checklist for current work |
| Tools        | create/get/update/set_budget | todo_write / todo_read          |
| Blocks turns | budget_limited / paused      | Never                           |

Keep both; document when to use each.

---

## Phase 6 — Stats, export, observability

- Implement `elph stats` from `session_turns` + session rollups (JSON + human)
- `compute_statistics`: fill tokens/cost from rollups
- Report store size + session/entry counts (retention pressure signals)
- Optional: log turn finish with cost fields in existing JSONL logs

---

## Phase 7 — Documentation (`docs/`)

Per `AGENTS.md`, document actual post-change behavior (not aspirational).

| Doc                                                                | Content                                                                                                                         |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| **`docs/session-persistence.md`** (new)                            | Schema ERD, SoT, resume contract, recovery, flush, **full `session.retention` + `compaction.physicalPrune` settings reference** |
| **`docs/agent-todo.md`** (new)                                     | Tool schema, merge semantics, ReAct usage, TUI panel, goals vs todos                                                            |
| **Update** configuration docs if present / settings module rustdoc | Nested `session` group examples for home + project layers                                                                       |
| **Update** `elph-agent/docs/durable-harness.md`                    | Turns, journal, physical prune                                                                                                  |
| **Update** `elph-agent/docs/tools.md`                              | todo tools                                                                                                                      |
| **Archive note**                                                   | Point old TodoList paths at new docs                                                                                            |

---

## Testing strategy

### Unit (same file as impl)

- `TodoStore` merge/replace/duplicate/status invariants
- Turn store start/finish/rollup math
- Transcript reconstruct fixtures (user, thinking, tool, skill card)
- Schema create on empty DB
- **Settings**: `SessionRetentionSettings` defaults, serde round-trip, project merge, `0` = unlimited semantics
- **Compaction physical prune**: on/off via flag; entries before keep-boundary deleted; context still valid
- **Session GC**: age/count/pin/protect-latest; cascade artifacts
- Payload truncate respects cap from policy

### Integration (`tests/`)

- `session_turso`: multi-message append → resume → entries + context non-empty
- New: resume transcript reconstruct parity
- New: turn usage recorded after harness turn with faux provider
- New: todo tools end-to-end with Turso
- New: long tree + compact + prune reduces row count without breaking resume
- New: exceeding `max_sessions_per_cwd` deletes oldest non-pinned

### Manual smoke

- Chat 2–3 turns with tools → quit cleanly → `elph --continue` → history + model continuity
- Kill after turn complete → continue
- Multi-step task → todos panel updates
- Long session with compaction → `store.db` size does not grow unboundedly after prune
- `elph session prune --dry-run` shows candidates

---

## Implementation order (PR-sized slices)

1. **Schema + stores** — hybrid DDL, TurnStore, TodoStore; drop transcript SoT tables
2. **Settings** — `SessionSettings` / `SessionRetentionSettings`, extend `CompactionConfig.physicalPrune`, defaults + load/save + unit tests; map into policy structs
3. **Harness turn accounting** — turns + rollups; faux provider tests
4. **TUI restore fix** — reconstruct-only; remove dual snapshot writes
5. **Retention enforcement** — payload caps, physical prune, GC on open, CLI pin/prune, artifact cleanup (all settings-driven)
6. **Todo tools + prompt + TUI panel**
7. **Stats CLI + docs** (include settings reference tables)

Each slice should compile and pass focused tests before the next.

---

## Out of scope

- Migrating existing user `store.db` rows (wipe or start fresh is OK)
- Provider stream mid-token resume
- Floppy memory schema changes
- Full durable queue journal beyond existing harness custom entries + bounded journal GC
- Cold archive to external object storage
- Graphite/PR stacking for this work

---

## Risk notes

| Risk                                    | Mitigation                                                                                        |
| --------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Reconstruct fidelity vs live cards      | Expand fixtures; prefer tree fields (`_elph_ui`, prompt meta) over UI-only state                  |
| Large sessions slow reconstruct         | Reconstruct once at bootstrap; physical prune keeps branch small; tree already loaded for harness |
| GC deletes a session user still wants   | `pinned`, `protect_latest_per_cwd`, dry-run prune CLI, conservative defaults                      |
| Physical prune breaks tree parent links | Only delete after compaction with explicit keep set; rebuild index; tests                         |
| Model ignores todos                     | Prompt rules + stale reminder; TUI visibility                                                     |
| Migration version collision             | Single platform band ≥200; share SQL with elph-agent constant                                     |

---

## Success criteria

1. `--continue` / `--resume <id>` restores TUI transcript **and** LLM context from the same tree SoT
2. Every completed turn has a `session_turns` row with tokens/cost when the provider returns usage
3. Session rollups and `elph stats` reflect real usage
4. Agent can maintain session todos; multi-step work stays structured without redoing completed steps
5. Docs under `docs/` describe the live system accurately
6. No dual SoT for history; no fire-and-forget authoritative writes
7. **Retention** is fully controllable via `settings.json` (`session.retention` + `compaction.physicalPrune`); defaults are safe; project overlay works
8. Physical prune + GC reclaim disk; no multi-MB snapshot tables; pinned / latest-per-cwd respected
