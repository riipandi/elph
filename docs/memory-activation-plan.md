# Plan: Active Floppy Memory Integration

> **Status:** plan only — implement phase-by-phase; this document is the source of truth for the work.  
> **Scope (code):** `crates/floppy`, `elph/src/memory`, `elph/src/agent`, `elph/templates/agent`, memory-related settings/CLI wiring in `elph`.  
> **Goal:** memory aktif by default — auto-recall, auto-record work/changes, reduce repeated filesystem scans, avoid redoing completed or failed work.  
> **Self-contained:** do not rely on other documents under `docs/` for design details; verify behavior against the source files listed here.

---

## Table of contents

1. [Problem statement](#1-problem-statement)
2. [Design principles](#2-design-principles)
3. [Current code architecture](#3-current-code-architecture)
4. [Target architecture](#4-target-architecture)
5. [Locked decisions](#5-locked-decisions)
6. [Success metrics](#6-success-metrics)
7. [Phase map & execution order](#7-phase-map--execution-order)
8. [Phase 0 — Instrumentation](#phase-0--instrumentation)
9. [Phase 1 — Shared runtime & lifecycle](#phase-1--shared-runtime--lifecycle)
10. [Phase 2 — Active write path](#phase-2--active-write-path)
11. [Phase 3 — Prompt & policy](#phase-3--prompt--policy)
12. [Phase 4 — Smarter recall & packing](#phase-4--smarter-recall--packing)
13. [Phase 5 — Project map & scan reduction](#phase-5--project-map--scan-reduction)
14. [Phase 6 — Ops, settings, quality](#phase-6--ops-settings-quality)
15. [Cross-cutting implementation rules](#15-cross-cutting-implementation-rules)
16. [PR ticket index](#16-pr-ticket-index)
17. [Program definition of done](#17-program-definition-of-done)

---

## 1. Problem statement

Floppy is wired into Elph but under-used for day-to-day coding efficiency.

| Area            | Current behavior (code)                                                                       | Gap                                                   |
| --------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Session start   | `build_memories_context` → top-5 by weight into Dynamic system prompt                         | Weight ≠ turn relevance; previews ~160 chars          |
| Per-turn recall | `register_automatic_memory_hooks` → `start_task` + adaptive threshold                         | No work log / recent changes in injection             |
| Agent tools     | `memory_start_task`, `memory_end_task`, `memory_report`, `memory_contradict`, `memory_status` | Model rarely calls them; no search/recent tools       |
| Auto-write      | User-correction keywords + tool **errors** only                                               | Successful edits / “already done” not stored          |
| Task lifecycle  | Auto start/end on turn                                                                        | `end_task` weak metrics; task id in `thread_local!`   |
| Store instances | Tools (`OnceCell`) vs hooks (`RECALL_STORE`)                                                  | Separate `MemoryStore` → `current_task_id` not shared |
| Prompts         | Generic “do not re-fetch known info” in `coding_base.md`                                      | No memory-first / anti-rescan policy                  |
| Categories      | `correction`, `user`, `insight`, `discovery`, `consolidated`                                  | No first-class `work`; discovery underused            |

**Desired outcomes:**

1. Auto-recall lessons **and** recent work every substantive turn.
2. Durable record of agent actions (paths, outcomes, decisions).
3. Fewer redundant `list_dir` / `read_file` / broad greps for known layout.
4. No replaying the same failed approach or finished subtask.

---

## 2. Design principles

1. **Memory-first, tools-second** — hooks inject context; filesystem is for unknown or stale facts.
2. **Write on signal** — mutations, corrections, failed tools, turn outcomes — not every token.
3. **One shared store per process/session** — tools + hooks + startup share task state.
4. **Structured memory text** — `[work]`, `[change]`, `[discovery]` templates, English content.
5. **Blend rank** — similarity + weight + recency (not weight alone).
6. **Best-effort** — lock/timeout/embed failure never blocks the user turn hard.
7. **No compat shims** — change APIs/templates directly when a phase needs it.
8. **Shippable phases** — each phase alone improves product; later phases build on earlier ones.

---

## 3. Current code architecture

### 3.1 Runtime flow today

```
create_coding_session (elph/src/agent/runtime.rs)
  ├─ create_memory_tools(paths)          // lazy OnceCell store in tools.rs
  ├─ build_memories_context(paths)       // open_store(needs_embed=false); top by weight
  ├─ SystemPrompt::Dynamic appends memory section
  └─ register_automatic_memory_hooks     // separate RECALL_STORE with embedder

Turn:
  before_agent_start
    ├─ is_user_correction? → report_user_input
    ├─ prompt length < 15 chars? → skip
    ├─ start_task(prompt) → memories; ACTIVE_TASK_ID (thread_local)
    ├─ adaptive threshold filter; take 5
    └─ inject <memory_context> into system_prompt
  on_tool_result
    └─ is_error → report_correction
  TurnEnd
    └─ end_task(tokens, tool_calls, errors≈0|all, no self_report)

Session end:
  session_end_maintenance → embed_pending + decay
```

### 3.2 Key files (authoritative)

| Concern                             | Path                                                                                                          |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Floppy store / task / report        | `crates/floppy/src/store/{mod,tasks,write,read,embed}.rs`                                                     |
| Floppy query                        | `crates/floppy/src/query/{memories,tasks,search,status,timeline}.rs`                                          |
| Floppy types / categories / scoring | `crates/floppy/src/types/*`, `scoring.rs`, `util.rs`                                                          |
| Schema                              | `crates/floppy/src/migrations.rs` (V1–V3, `LAST_VERSION = 3`)                                                 |
| Host open store                     | `elph/src/memory/store.rs` (`session_id` currently `"elph-cli"`)                                              |
| Host hooks                          | `elph/src/memory/hooks.rs`                                                                                    |
| Host tools                          | `elph/src/memory/tools.rs`                                                                                    |
| Slash / CLI entry                   | `elph/src/memory/mod.rs`, `elph/src/memory/cmd.rs`, `elph/src/cli/memory.rs`                                  |
| Session factory                     | `elph/src/agent/runtime.rs`                                                                                   |
| Prompt templates                    | `elph/templates/agent/coding_base.md`, `mode_*.md`                                                            |
| Prompt assembly                     | `elph/src/agent/prompt/builder.rs`                                                                            |
| Harness hooks API                   | `crates/elph-agent/src/agent/harness/hooks.rs`                                                                |
| Hook event shapes                   | `BeforeAgentStartEvent { prompt, system_prompt, ... }`, `ToolResultEvent { tool_name, input, is_error, ... }` |

### 3.3 Floppy APIs already available (reuse first)

| API                                                                                           | Notes                                                          |
| --------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `start_task(description)`                                                                     | Creates task + vector top-k + records retrievals               |
| `end_task(id, TaskEndInput)`                                                                  | Scores task; updates weights from self_report                  |
| `search_memories(query)`                                                                      | Read-only semantic search, **no** task                         |
| `search(query)`                                                                               | Lifecycle search (creates task) — prefer not for passive tools |
| `report` / `report_correction` / `report_user_input`                                          | Writes linked via `current_task_id`                            |
| `insert_raw_memory(content, category, weight)`                                                | Generic insert                                                 |
| `get_top_by_weight`, `list_memories`, `list_tasks`, `get_timeline`, `get_stats`, `get_status` | Inspection                                                     |
| `contradict_memory`, `decay`, `purge`, `embed_pending`                                        | Maintenance                                                    |

### 3.4 Known bugs / footguns to fix early

1. **Dual stores** — tools and hooks each call `open_store`; in-memory `current_task_id` is not shared.
2. **`thread_local! ACTIVE_TASK_ID`** — TurnEnd may run on another worker thread → silent skip of `end_task`.
3. **Hardcoded `session_id: "elph-cli"`** in `open_store`.
4. **Auto end_task** sets `self_report: None` and coarse `errors`.
5. **Injection** truncates aggressively; no memory ids in startup block (harder to contradict).

---

## 4. Target architecture

```
┌──────────────────────────────────────────────────────────────┐
│ MemoryRuntime (session-scoped Arc)                           │
│  - one MemoryStore (embed-capable, lazy init)                │
│  - active_task_id: Arc<Mutex<Option<String>>>                │
│  - turn_scratch: paths_touched, user_correction_count, …     │
│  - last_injected_ids (dedupe bootstrap vs turn)              │
└──────────────────────────────────────────────────────────────┘
        ▲                    ▲                    ▲
   hooks (start/turn/end)   capture (mutations)  agent tools
        │                    │                    │
        └────────────┬───────┴────────────────────┘
                     ▼
          inject into system prompt:
          <memory_context>  <recent_work>  <project_map>
                     │
                     ▼
          templates: memory-first policy (coding_base + modes)
```

**Memory kinds (content contracts):**

| Kind             | Category (locked)                                       | Write trigger                                  |
| ---------------- | ------------------------------------------------------- | ---------------------------------------------- |
| Lesson           | `correction` / `user` / `insight`                       | existing + improved                            |
| Work log         | **`work`** (new)                                        | turn summary after mutations / explicit record |
| Change footprint | **`work`** with `[change]` prefix or same category body | coalesced successful mutations                 |
| Structural map   | `discovery`                                             | exploration heuristic / insight                |
| Merged           | `consolidated`                                          | Phase 6 job                                    |

---

## 5. Locked decisions

Resolve ambiguity up front so implementers do not re-debate:

| ID  | Decision                                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------- |
| D1  | Add **`MemoryCategory::Work`** (SQL category already free `TEXT`; update Rust enum + scoring + filters).        |
| D2  | **One floppy task per user turn** (substantive prompts only).                                                   |
| D3  | Auto self-report: **conservative defaults** on TurnEnd (see Phase 1); no model-rating tool required.            |
| D4  | Mutations: **coalesce per turn** into one change blob + one work summary (not one memory per edit).             |
| D5  | Project map: **structured discovery memories only** — no `path_notes` table in MVP.                             |
| D6  | Keep `memory_start_task` tool; description says auto-recall already ran.                                        |
| D7  | Stored memory **content language: English**.                                                                    |
| D8  | Secret paths (`.env`, `credentials`, `*.pem`, etc.) — **never** store content; redact path basenames if needed. |

---

## 6. Success metrics

| Metric                                                      | Target                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------ |
| Substantive turns with injection when store has ≥5 memories | ≥ 70%                                                  |
| Multi-step coding turn leaves ≥1 `work` entry               | yes                                                    |
| Tools + hooks share same active task for `source_task`      | always when task started                               |
| Recall path wall time                                       | respect existing timeouts (~2s best-effort); fail open |
| Character budget for all memory XML                         | hard cap (default 3000; Phase 4/6)                     |
| Dual-store / thread_local issues                            | eliminated after Phase 1                               |

---

## 7. Phase map & execution order

| Phase | Name                       | Outcome                     | Effort | Depends            |
| ----- | -------------------------- | --------------------------- | ------ | ------------------ |
| **0** | Instrumentation            | Debuggable baseline         | S      | —                  |
| **1** | Shared runtime & lifecycle | Correct task loop           | M      | 0                  |
| **2** | Active write path          | Auto work/change journal    | M–L    | 1                  |
| **3** | Prompt & policy            | Memory-first agent behavior | S–M    | 1 (better after 2) |
| **4** | Smarter recall & packing   | Relevant denser context     | M      | 1–2                |
| **5** | Project map                | Less FS thrash              | M      | 2–4                |
| **6** | Ops / settings / quality   | Production hygiene          | M      | 2–5                |

**Order:** `0 → 1 → 3a (prompt-only) → 2 → 4 → 5 → 6`  
Phase 3a may ship in parallel with late Phase 1 if it only touches templates/tool strings.

```
Phase 0 ──► Phase 1 ──┬──► Phase 2 ──► Phase 4 ──► Phase 5
                      │         │                    │
                      └──► Phase 3 ──────────────────┘
                                      │
                                      ▼
                                   Phase 6
```

**Per-phase gate:** `make check` / `make lint` / `make test` (or project equivalents) green before merge.

---

# Phase 0 — Instrumentation

## Goal

Observe recall/write/task lifecycle without changing user-visible behavior.

## Tasks

### Task 0.1 — Structured log points

**Files:** `elph/src/memory/hooks.rs`, optionally `tools.rs`

Add `log::debug!` / `log::info!` (no new user UI) for:

| Event                    | Fields                                                           |
| ------------------------ | ---------------------------------------------------------------- |
| `memory.recall.start`    | prompt_len, skipped_reason?                                      |
| `memory.recall.hits`     | raw_count, after_threshold, threshold, task_id?                  |
| `memory.recall.injected` | injected_count, total_chars                                      |
| `memory.task.start`      | task_id, description_len                                         |
| `memory.task.end`        | task_id, completed, tokens, tool_calls, errors, user_corrections |
| `memory.write`           | kind (`correction`/`user`/`insight`/later `work`), id?, ok/err   |
| `memory.store.init`      | needs_embed, elapsed_ms, err?                                    |

**Rules:** never log full user prompts if they may contain secrets; log lengths and ids.

### Task 0.2 — Manual baseline checklist

Run once and record results in the PR description (not a separate design doc):

- [ ] Empty `.elph/store.db` session — no injection, no panic
- [ ] Store with ≥3 corrections — injection non-empty on long prompt
- [ ] Short prompt (`hi`) — skip recall
- [ ] Second process locking DB — timeout fail-open
- [ ] Whether model spontaneously calls any `memory_*` tool

### Task 0.3 — Verify existing tests still pass

- CLI memory tests under `elph/tests/`
- Floppy unit tests under `crates/floppy`

## Acceptance criteria

- [ ] Logs visible at debug without changing transcript content
- [ ] No new public APIs
- [ ] Tests green

## Out of scope

New tools, schema, prompt policy, shared runtime.

---

# Phase 1 — Shared runtime & lifecycle

## Goal

Single `MemoryRuntime` so tools, hooks, and maintenance share store + active task; fix async-safe task id; improve end_task + context formatting.

## Tasks

### Task 1.1 — Introduce `MemoryRuntime`

**New file:** `elph/src/memory/runtime.rs`  
**Wire:** `elph/src/memory/mod.rs`, `store.rs`, `tools.rs`, `hooks.rs`, `elph/src/agent/runtime.rs`

**Suggested type shape:**

```rust
pub struct MemoryRuntime {
    paths: Paths,
    session_id: String,
    store: tokio::sync::Mutex<Option<MemoryStore>>, // lazy
    active_task_id: std::sync::Mutex<Option<String>>,
    // turn-local counters (reset each before_agent_start / after TurnEnd)
    turn: std::sync::Mutex<TurnScratch>,
}

pub struct TurnScratch {
    user_corrections: u32,
    injected_memory_ids: Vec<String>,
    // later phases: paths_touched, exploration roots, …
}
```

**Methods (minimal Phase 1):**

| Method                                  | Behavior                                                                                                           |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `MemoryRuntime::new(paths, session_id)` | No DB open yet                                                                                                     |
| `ensure_store(needs_embed: bool)`       | Open once; if first open was noop and later needs embed, reopen or always open with embed when auto hooks on       |
| `start_task_for_prompt(prompt)`         | `start_task`, set `active_task_id`                                                                                 |
| `end_active_task(input)`                | take id, `end_task`, clear                                                                                         |
| `report_*`                              | delegate; rely on store `current_task_id` — **must** keep store’s `current_task_id` in sync with runtime active id |
| `build_bootstrap_context()`             | replace `build_memories_context`                                                                                   |
| `session_end_maintenance()`             | existing logic                                                                                                     |

**Embed strategy (recommended):** always init with embedder when session enables memory hooks (accept first-turn download cost once), so tools and hooks never diverge.

**session_id:** pass real coding session id from `create_coding_session` into `FloppyBuilder` / `open_store` (change `store.rs` signature).

### Task 1.2 — Remove dual store + thread_local

1. Delete `RECALL_STORE` static and `ACTIVE_TASK_ID` thread_local from `hooks.rs`.
2. Change `create_memory_tools` to take `Arc<MemoryRuntime>` instead of only `Paths`.
3. Change `register_automatic_memory_hooks(harness, runtime: Arc<MemoryRuntime>)`.
4. `create_coding_session`: construct one `Arc<MemoryRuntime>`, pass to tools + hooks + bootstrap context.

### Task 1.3 — Task lifecycle rules (implement exactly)

| Condition                                                                          | Action                                                   |
| ---------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `prompt.trim().chars().count() < MIN_QUERY_LENGTH` (keep 15 unless settings later) | No `start_task`; no injection change                     |
| Substantive prompt                                                                 | `start_task(prompt)`; store memories; set active id      |
| `start_task` timeout/err                                                           | Fallback `search_memories`; **no** active task; skip end |
| TurnEnd + active id present                                                        | `end_task` then clear                                    |
| TurnEnd + no active id                                                             | no-op                                                    |
| User correction keywords this turn                                                 | `turn.user_corrections += 1` (+ existing report)         |

**One task per turn:** always `end` previous active task before starting a new one if somehow still set (defensive).

### Task 1.4 — Richer `TaskEndInput`

On TurnEnd:

| Field              | Source                                                                                                                                                                                    |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tokens_used`      | assistant usage (existing)                                                                                                                                                                |
| `tool_calls`       | `tool_results.len()`                                                                                                                                                                      |
| `errors`           | count tool result messages that are errors **if** distinguishable; else assistant error flag (improve over “all or nothing”)                                                              |
| `user_corrections` | `TurnScratch.user_corrections`                                                                                                                                                            |
| `completed`        | `!has_assistant_error`                                                                                                                                                                    |
| `self_report`      | For each `injected_memory_ids`: score **2** if completed && user_corrections==0; score **1** if completed && user_corrections>0; score **0** if !completed. Cap list to injected set only |

### Task 1.5 — Context formatting v1

Unify formatters used by bootstrap and turn injection:

**Rules:**

- Max **500** chars per memory body (char-safe truncation).
- Total block soft cap **2000** chars Phase 1 (Phase 4 raises/configures).
- Include `id=` for contradict/ratings.
- Headings:
    - Bootstrap: `Persistent high-weight lessons:`
    - Turn: `Retrieved for this turn (similarity):`

Example line:

```text
1. [correction | id=… | score=0.72 | w=2.10 | used=4x] <content>
```

### Task 1.6 — Floppy small API (only if needed)

If host cannot keep `MemoryStore.current_task_id` aligned:

- Add `MemoryStore::set_current_task_id(Option<String>)` + `current_task_id()` in `crates/floppy`.
- Unit test: set → report links `source_task`.

Prefer setting via existing `start_task` / `end_task` path when using shared store only.

### Task 1.7 — Tests

| Test                                                                  | Location                                                 |
| --------------------------------------------------------------------- | -------------------------------------------------------- |
| Runtime lazy init once                                                | `elph/src/memory/runtime.rs` cfg(test) or dedicated test |
| active_task_id set/cleared without thread_local                       | unit                                                     |
| format_memory_context includes id + respects cap                      | unit                                                     |
| Floppy: source_task linked when report after start_task on same store | floppy store_tests                                       |

## Acceptance criteria

- [ ] No `thread_local` task id
- [ ] Single store path for tools + hooks in a session
- [ ] `memory_report` during turn sets `source_task` = auto task id
- [ ] Short prompts skip task creation
- [ ] Tests green

## Risks & mitigations

| Risk                             | Mitigation                                                                                   |
| -------------------------------- | -------------------------------------------------------------------------------------------- |
| Embed init blocks session create | Keep bootstrap context on noop/read path OR async lazy on first turn; do not double-download |
| Mutex deadlock                   | Never hold runtime mutex across long embed if avoidable; match existing `with_db` patterns   |

---

# Phase 2 — Active write path

## Goal

Automatically persist **what the agent did** (work + file footprint) so later turns/sessions do not re-discover or redo work.

## Tasks

### Task 2.1 — Add `MemoryCategory::Work`

**Files:** `crates/floppy/src/types/config.rs` (or category enum location), `util.rs` (`category_str` / `category_from_str`), `scoring.rs` (`initial_weight`), format filters in `elph/src/memory/format.rs`, slash help strings.

**Scoring default:** `initial_weight(Work) = 1.0` (ephemeral operational; Phase 6 may decay faster).

**SQL:** no migration required for category TEXT; optional V4 only if adding indexes/columns later.

**CLI/slash:** `/memory list work` must accept the new filter.

### Task 2.2 — Content templates module

**New file:** `elph/src/memory/templates.rs` (or `capture.rs` helpers)

```text
[work] <one-line outcome>
Paths: <comma-separated relative paths>
Outcome: success|partial|failed
Note: <≤200 chars optional>

[change] tools=<edit_file|write_file|…> count=N
Paths:
- path/a.rs (edit_file)
- path/b.rs (write_file)
```

Helpers:

- `format_work_entry(...)`
- `format_change_entry(...)`
- `is_sensitive_path(path) -> bool`
- `normalize_project_path(cwd, path) -> relative string`

### Task 2.3 — Turn scratch for mutations

Extend `TurnScratch`:

```rust
paths_touched: Vec<(String /*path*/, String /*tool*/)>, // capped
mutation_successes: u32,
```

### Task 2.4 — Capture on `on_tool_result` (success path)

**File:** `hooks.rs` (same registration site as error handler)

When `!is_error` and `tool_name` ∈:

- `edit_file`, `write_file`, `delete_path`, `move_path`, `copy_path` (confirm exact builtin names from `BuiltinToolsBuilder` / catalog at implement time)

Extract path(s) from `event.input` JSON (`path`, `from`, `to` as applicable).

- Skip sensitive paths
- Push to `paths_touched` (cap e.g. 20 entries)
- **Do not** insert DB row per tool call

Keep existing error → `report_correction` path; optionally improve lesson string with tool name + path only (no body).

### Task 2.5 — Flush work/change on TurnEnd

After metrics collected, **before or after** `end_task` (prefer **before** end so `source_task` still set):

1. If `paths_touched` non-empty:
    - Insert one `Work` memory via `insert_raw_memory` or dedicated `report_work` helper using `[change]` template.
2. If `paths_touched` non-empty **or** substantive completed work:
    - Insert one `[work]` summary: first line of user prompt (truncated) + outcome + path list.
3. Rate limits:
    - Max **2** auto inserts per turn (change + work)
    - Skip if both would be empty/near-duplicate of last insert (optional hash set on runtime for session)

### Task 2.6 — Agent tools: search + recent

**File:** `tools.rs`

| Tool            | Behavior                                                                                                                                                 |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `memory_search` | `search_memories(query)`; return ranked list; **no** task                                                                                                |
| `memory_recent` | List recent memories: prefer new API `list_recent_memories(limit, category?)`; fallback `list_memories` sorted by `created_at` if adding query in floppy |

**Floppy add (Task 2.6a):**

```rust
// crates/floppy — query/memories.rs
pub async fn list_recent_memories(
    &self,
    limit: u32,
    category: Option<MemoryCategory>,
) -> Result<Vec<MemoryRecord>>;
// SQL: ORDER BY created_at DESC LIMIT ?
```

Update tool descriptions (also Phase 3 polish): historical questions → these tools before FS scan.

Optional: `memory_record_work` — only if auto-flush proves insufficient; skip in MVP.

### Task 2.7 — Tests

| Case                            | Layer                                     |
| ------------------------------- | ----------------------------------------- |
| category Work round-trip        | floppy unit                               |
| list_recent_memories order      | floppy unit                               |
| sensitive path skipped          | elph unit                                 |
| coalesced change memory content | elph unit with mock/store                 |
| tools schemas registered        | compile + simple tool list test if exists |

## Acceptance criteria

- [ ] After multi-file edit turn, store contains `work` category rows
- [ ] `/memory list work` or `elph memory list --category work` shows them
- [ ] No full file bodies stored
- [ ] Auto inserts ≤ 2 per turn
- [ ] Tests green

## Out of scope

Project-map exploration heuristics (Phase 5), multi-source rank (Phase 4).

---

# Phase 3 — Prompt & policy

## Goal

Make the model **prefer memory** over redundant scanning and **trust/repair** recalled entries.

## Tasks

### Task 3.1 — `coding_base.md` section

**File:** `elph/templates/agent/coding_base.md`

Insert `<memory_and_context>` after `<context_and_rules>` (or before `<execution>`).

**Required bullets (keep short):**

1. Treat injected `<memory_context>`, `<recent_work>`, `<project_map>` as starting truth.
2. Do not re-run broad `list_dir` / exploratory sweeps for areas already covered by those blocks unless user/build signals staleness.
3. Prefer `memory_search` / `memory_recent` for “what did we do / what was decided” questions.
4. Do not re-implement completed `[work]` items; continue from remaining gaps.
5. If a recalled memory is wrong → `memory_contradict` (with correction).
6. Durable preferences/lessons → `memory_report` when auto-capture would miss intent (user style, architectural decision).

**Avoid:** long examples, JSON dumps, repeating full tool schemas.

### Task 3.2 — Mode templates

**Files:** `mode_build.md`, `mode_brave.md`, `mode_plan.md`, `mode_ask.md`

| Mode          | Extra line(s)                                                                                  |
| ------------- | ---------------------------------------------------------------------------------------------- |
| build / brave | Use recent work aggressively; minimize redundant reads after injection                         |
| plan / ask    | Recall heavily; do not invent work logs for pure Q&A; discoveries OK when mapping architecture |

> **Note (2026-08-01):** the mode-specific memory lines above were consolidated into the
> always-rendered `<memory_and_context>` section of `coding_base.md` and removed from the
> `mode_*.md` templates to keep the static prompt lean. Mode templates now carry only
> mode-protocol guidance (Plan keeps the "no work-log entries during planning" guard).

### Task 3.3 — Tool description rewrites

**File:** `elph/src/memory/tools.rs`

| Tool                | Description intent                                                                        |
| ------------------- | ----------------------------------------------------------------------------------------- |
| `memory_start_task` | Auto-recall already ran for the user message; call only for a **different** subtask pivot |
| `memory_end_task`   | Usually automatic; advanced manual close                                                  |
| `memory_report`     | Durable lesson / preference / insight not covered by auto work capture                    |
| `memory_search`     | Cheap historical lookup; prefer before re-reading many files                              |
| `memory_recent`     | Latest work/changes without semantic query                                                |
| `memory_contradict` | Remove wrong memory; optional correction                                                  |
| `memory_status`     | Diagnostics                                                                               |

### Task 3.4 — Prompt assembly test

**File:** `elph/src/agent/prompt/builder.rs` tests (or adjacent)

- Render coding prompt in Build mode with memory tool names in `active_tool_names` if required by template conditionals.
- Assert substring `memory_and_context` or distinctive policy phrase present.

Note: memory XML injection is runtime-appended, not necessarily in `build_coding_system_prompt` — test template policy separately from injection.

## Acceptance criteria

- [ ] Template contains memory policy section
- [ ] Mode files updated
- [ ] Tool descriptions updated
- [ ] Snapshot/unit assertion on policy presence
- [ ] Tests green

## Out of scope

Changing builtin FS tool implementations.

---

# Phase 4 — Smarter recall & packing

## Goal

Inject **relevant, actual** context from multiple sources under a hard budget; stop double-counting bootstrap + turn.

## Tasks

### Task 4.1 — Rank/merge pure module

**New file:** `elph/src/memory/rank.rs`

```rust
pub struct RankedMemory {
    pub memory: Memory, // or host DTO
    pub rank: f64,
    pub source: RankSource, // Semantic | Recent | Sticky
}

pub fn merge_and_rank(
    semantic: Vec<Memory>,
    recent: Vec<MemoryRecord>, // map to common shape
    sticky: Vec<Memory>,
    now: i64,
    opts: RankOptions,
) -> Vec<RankedMemory>;
```

**Default formula:**

```
rank = 0.50 * similarity_or_0
     + 0.25 * normalize_weight(weight, 0.1..5.0)
     + 0.25 * recency_boost(created_at, now)  // e.g. exp decay half-life 7d
```

**Category boosts (multipliers on rank):**

- `user`, `correction`: ×1.15
- `work`: ×1.20 if prompt looks like continuation (`continue`, `next`, `fix remaining`, `lanjut`, …)
- `discovery`: ×1.10 if prompt has structure cues (`where`, `structure`, `layout`, `arsitektur`, …)

**Dedupe:** by memory id; keep highest rank.

### Task 4.2 — Multi-source fetch in turn hook

Per substantive turn:

| Source      | Call                                                       | Cap before merge |
| ----------- | ---------------------------------------------------------- | ---------------- |
| Semantic    | `start_task` / search results                              | top_k (5)        |
| Recent work | `list_recent_memories(5, Some(Work))`                      | 5                |
| Sticky      | `get_top_by_weight(3)` filtered to correction/user/insight | 3                |

Then `merge_and_rank` → take until budget.

### Task 4.3 — Context packer

**File:** `elph/src/memory/pack.rs` (or same as rank)

Emit sections:

```xml
<memory_context>
… lessons / corrections / insights …
</memory_context>

<recent_work>
… work category …
</recent_work>
```

**Budget algorithm:**

1. `budget = 3000` chars default (const; settings in Phase 6)
2. Sort by rank desc
3. Append full entry if fits; else skip (do not mid-truncate below 80 chars of useful body — prefer skip)
4. Priority reserve: try to keep ≥1 sticky user/correction with weight > 3.0

### Task 4.4 — Bootstrap vs turn dedupe

- Bootstrap (`SystemPrompt::Dynamic` fixed section): sticky only **or** remove bootstrap entirely and always inject via `before_agent_start` (cleaner).
- **Recommended:** Phase 4 migrates to **turn-only injection** for dynamic recall; bootstrap keeps only a one-liner “Memory auto-recall is active” **or** empty.  
  If bootstrap kept: pass `last_injected_ids` and exclude them from turn pack.

### Task 4.5 — Adaptive threshold refinements

Keep existing threshold tiers; add:

- Continuation prompt → threshold − 0.05
- (Optional) If semantic top score < 0.35 and store large → still inject recent work section only

### Task 4.6 — Tests

- merge dedupes ids
- budget never exceeded
- continuation boost prefers work entries
- sticky high-weight not dropped first

## Acceptance criteria

- [ ] Continuation prompts surface prior turn work without FS tools
- [ ] New-topic prompts still surface semantic lessons
- [ ] Injection char length ≤ budget in unit tests
- [ ] Tests green

---

# Phase 5 — Project map & scan reduction

## Goal

Capture compact **structural** facts so the agent stops re-walking the tree for known areas.

## Tasks

### Task 5.1 — Exploration scratch

Extend `TurnScratch`:

```rust
list_dir_roots: HashMap<String, u32>, // root path → call count
read_prefixes: HashMap<String, u32>,
```

On successful `list_dir` / `find_path` / (optional) `grep` with path scope: bump counters.

### Task 5.2 — Discovery flush heuristic

On TurnEnd (rate-limited):

**If** any root has `list_dir` count ≥ **2** in one turn **or** ≥ **3** reads under same top-level prefix:

- Build short `[discovery]` text:

```text
[discovery] Area: elph/src/memory/
Observed tools: list_dir×2, read_file×4
Notes: hooks.rs (auto recall), tools.rs (agent tools), store.rs (open_store)
```

Notes line: derive from tool args basenames only (no file contents). Cap 500 chars.

- Category: `Discovery`
- Skip if recent identical discovery exists (same area prefix within last N memories — string match on `Area:`)

### Task 5.3 — Injection section

Phase 4 packer gains optional third section:

```xml
<project_map>
… discovery entries selected by rank …
</project_map>
```

Include discoveries in sticky/recent merge with structure-cue boost (already in 4.1).

### Task 5.4 — Prompt cross-check

Ensure Phase 3 policy mentions `<project_map>` and staleness exceptions (user said layout changed, compile error path missing, etc.).

### Task 5.5 — Tests

- Heuristic triggers only above thresholds
- Sensitive roots skipped
- Discovery content has no file bodies
- Injection section present when discoveries ranked in

## Acceptance criteria

- [ ] After exploratory turn, a discovery memory exists for the area
- [ ] Later “where is X?” turn can inject that discovery
- [ ] No path_notes schema
- [ ] Tests green

## Risks

Stale maps — mitigate with timestamps in text (`Observed at unix=…`) and `memory_contradict` policy.

---

# Phase 6 — Ops, settings, quality

## Goal

Configurable, maintainable memory at scale; finish productization.

## Tasks

### Task 6.1 — Settings knobs

Locate current `MemorySettings` (embed model fields already exist under platform settings). Extend with:

| Field (serde camelCase)  | Default | Effect                                                                   |
| ------------------------ | ------- | ------------------------------------------------------------------------ |
| `enabled`                | true    | If false: no auto hooks, no bootstrap inject; tools may still open store |
| `autoRecall`             | true    | Per-turn injection                                                       |
| `autoCaptureWork`        | true    | Phase 2 flush                                                            |
| `autoCaptureExploration` | true    | Phase 5 flush                                                            |
| `topK`                   | 5       | Pass into FloppyConfig / start_task                                      |
| `contextBudgetChars`     | 3000    | Packer                                                                   |
| `minQueryLength`         | 15      | Skip trivial prompts                                                     |

Wire settings into `MemoryRuntime::new` / session factory. Defaults must match today’s “always on” feel when fields missing (serde defaults).

### Task 6.2 — Category-aware decay

**File:** `crates/floppy/src/store/write.rs` `decay`

Option A (simple): multi-statement updates:

- `work`: multiply by `min(decay_rate, 0.98)` or extra `* 0.99`
- `correction`/`user`: `* max(decay_rate, 0.998)`
- others: existing `decay_rate`

Option B: single SQL with `CASE category`.

Purge rules unchanged unless easy: low-weight work with high age.

### Task 6.3 — Consolidation MVP

**Session-end or explicit API** `consolidate_similar(threshold: f64)`:

1. Load memories with embeddings (limit batch)
2. Pairwise cosine (or reuse vector distance) within same category
3. If distance < ε and both weight < W: write one `consolidated` summary (concat truncated), delete or penalize sources

Keep conservative: max 10 merges per session end; log counts.

If too heavy for MVP: document in code comments that purge + decay is the v1 hygiene and ship settings only — but prefer a minimal pairwise consolidator for `work` duplicates.

### Task 6.4 — Goals bridge (optional, small)

On goal status → completed/cancelled (if hook/tool path easy): write one `[work]` line with goal title. Skip if wiring is invasive.

### Task 6.5 — In-code observability polish

Ensure Phase 0 logs still accurate after runtime refactor; add counters for auto-capture skips (disabled by settings, sensitive path, rate limit).

### Task 6.6 — Tests matrix

| Area              | Cases                                       |
| ----------------- | ------------------------------------------- |
| Settings defaults | missing fields → enabled true               |
| enabled=false     | hooks no-op / no injection                  |
| decay by category | work weight drops faster than user          |
| consolidate       | two near-dup work → one consolidated        |
| Integration smoke | CLI `memory status` after simulated inserts |

### Task 6.7 — User-facing strings only where code owns them

Update:

- Slash `/memory help` text for new categories/tools behavior
- CLI `--help` for memory subcommands if flags added
- Tool schemas already updated in earlier phases

Do **not** spend effort refreshing unrelated design documents.

## Acceptance criteria

- [ ] Master `memory.enabled` kill switch works
- [ ] Decay differentiates work vs user/correction
- [ ] Session-end maintenance still best-effort
- [ ] Full check/lint/test green
- [ ] Manual: Day-1 work visible Day-2 via injection or `memory_recent`

---

## 15. Cross-cutting implementation rules

1. **Verify against source**, not other markdown under `docs/`.
2. **Timeouts:** keep ~2s for mid-turn DB; ~8s startup; fail open with `log::warn!`.
3. **UTF-8 safe truncation** — char iterators only (existing hooks pattern).
4. **No secrets in DB** — path allow/deny list; never store tool result bodies for read_file.
5. **Import style** — follow project Rust import conventions (split types/functions, trailing commas).
6. **Tests** — unit next to impl (`#[cfg(test)]`); integration only for public CLI/API.
7. **Commits** — one phase or one PR ticket per change set when possible.
8. **Do not** expand scope into transcript compaction, cloud sync, or full AST indexing.

---

## 16. PR ticket index

Use these as agent/PR titles. Each ticket should be mergeable alone if dependencies are met.

| ID      | Phase | Title                                                             | Depends    | Primary paths                                            |
| ------- | ----- | ----------------------------------------------------------------- | ---------- | -------------------------------------------------------- |
| **M0**  | 0     | memory: structured debug logging for recall/task/write            | —          | `hooks.rs`, `tools.rs`                                   |
| **M1a** | 1     | memory: introduce MemoryRuntime; remove dual store + thread_local | M0         | `runtime.rs`, `tools.rs`, `hooks.rs`, `agent/runtime.rs` |
| **M1b** | 1     | memory: end_task metrics + self_report defaults + format v1       | M1a        | `hooks.rs`, format helpers                               |
| **M3a** | 3     | prompts: memory-first policy in coding_base + modes               | M1a (soft) | `templates/agent/*`, `tools.rs` descriptions             |
| **M2a** | 2     | floppy: MemoryCategory::Work + scoring + list filters             | M1a        | `crates/floppy`, `format.rs`                             |
| **M2b** | 2     | memory: coalesce mutation capture + turn work flush               | M2a, M1b   | `hooks.rs`, `templates.rs`/`capture.rs`                  |
| **M2c** | 2     | memory tools: memory_search + memory_recent (+ list_recent API)   | M2a        | `tools.rs`, `query/memories.rs`                          |
| **M4a** | 4     | memory: rank/merge + pack budget                                  | M2c        | `rank.rs`, `pack.rs`, `hooks.rs`                         |
| **M4b** | 4     | memory: turn-only or deduped bootstrap injection                  | M4a        | `agent/runtime.rs`, `hooks.rs`                           |
| **M5**  | 5     | memory: exploration → discovery map + project_map section         | M4a, M2b   | `hooks.rs`, packer                                       |
| **M6a** | 6     | memory: settings knobs + kill switch                              | M1a        | platform settings, `MemoryRuntime`                       |
| **M6b** | 6     | floppy/memory: category decay + consolidate MVP + tests           | M6a, M2a   | `write.rs`, maintenance, tests                           |

### Suggested parallel tracks

```
Track A (core):     M0 → M1a → M1b → M2a → M2b → M2c → M4a → M4b → M5 → M6b
Track B (prompt):   M3a (after M1a; refresh tool text again after M2c)
Track C (config):   M6a (after M1a; wire flags used by later tickets)
```

### Per-ticket checklist (copy into PR)

```markdown
- [ ] Implements only this ticket’s scope
- [ ] Verified against listed source files
- [ ] Unit tests for new pure logic
- [ ] make check / lint / test (or equivalent)
- [ ] No unrelated refactors
- [ ] Settings/defaults documented in code comments if user-visible
```

---

## 17. Program definition of done

All of the following must hold:

1. Returning session injects relevant lessons **and** recent work without requiring a tool call first.
2. Multi-step coding turns leave durable `work` memories (CLI/slash listable).
3. Shared runtime: one store, async-safe active task, reports linked to auto tasks.
4. Prompt policy steers agent away from redundant scans when memory already holds layout/work.
5. Settings can disable auto-recall/capture without removing the store.
6. Fail-open under lock/timeout; no user-visible crash from memory subsystem.
7. Workspace quality gates green.

---

## Appendix A — Mutation tool name verification

At implementation time, resolve exact tool names from the builtin catalog (do not assume):

```text
grep / search BuiltinToolsBuilder, tool name strings under crates/elph-agent and elph/src/agent
```

Map only confirmed names into the capture allow-list in Task 2.4.

## Appendix B — Sensitive path heuristics (minimum)

Treat as sensitive if path (case-insensitive) contains or ends with:

- `.env`, `.env.*`
- `credentials`, `secret`, `secrets`
- `id_rsa`, `id_ed25519`, `*.pem`, `*.p12`, `*.key`
- `auth.json`, `token`, `*.keystore`

Do not store contents; skip change lines or store only `(redacted-sensitive-path)`.

## Appendix C — Constant defaults (single place)

When implementing `MemoryRuntime` / settings, centralize:

| Constant                         | Default |
| -------------------------------- | ------- |
| `MIN_QUERY_LENGTH`               | 15      |
| `RECALL_DB_TIMEOUT`              | 2s      |
| `MEMORY_STARTUP_LOCK_TIMEOUT`    | 8s      |
| `TOP_K`                          | 5       |
| `PER_MEMORY_CHARS`               | 500     |
| `CONTEXT_BUDGET_CHARS`           | 3000    |
| `MAX_PATHS_PER_TURN`             | 20      |
| `MAX_AUTO_WRITES_PER_TURN`       | 2       |
| `EXPLORATION_LIST_DIR_THRESHOLD` | 2       |

---

_End of plan. Implement by ticket order; do not require later-phase features in early PRs._
