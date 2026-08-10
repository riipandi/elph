# Session persistence

How Elph stores sessions, restores history on `--continue` / `--resume`, and keeps the project store bounded.

**Source of truth**

| Concern | Where |
| --- | --- |
| LLM conversation + branch | `session_entries` tree (JSON payload per entry) |
| Turn lifecycle, tokens, cost | `session_turns` + rollup columns on `sessions` — see [usage-accounting.md](./design/usage-accounting.md) for what each number means |
| Structured work checklist | `session_todos` |
| Session objective / budgets | `goals` |
| Cross-session compaction summary | `session_summaries` (one row per session, upserted on compaction) |
| TUI transcript cards | **Reconstructed** from the tree on resume (not a separate snapshot SoT) |

Project store file: `<project>/.elph/store.db` (shared with floppy memory and codegraph).

Session artifacts (terminals, MCP cache): `APP_DATA/sessions/<SESSION_ID>/`.

## Schema (v201–v203)

Clean band **201** (`elph_session_schema_v2_relational`). No upgrade path from experimental pre-v201 DBs — delete `store.db` if needed. Additive migrations **202** (workers) and **203** (session summaries) extend the schema.

### Entity relationship

```text
sessions
  ├── parent_session_id ──► sessions(id)     ON DELETE SET NULL
  ├── session_sequences     ON DELETE CASCADE
  ├── session_turns         ON DELETE CASCADE
  │     └── session_entries.turn_id ──► session_turns(id)  ON DELETE SET NULL
  ├── session_entries       ON DELETE CASCADE
  ├── session_todos         ON DELETE CASCADE
  ├── goals                 ON DELETE CASCADE
  ├── session_summaries     ON DELETE CASCADE
  └── agent_spawn_edges (parent/child) ON DELETE CASCADE
```

| Table | PK | FK |
| --- | --- | --- |
| `sessions` | `id` | `parent_session_id` → `sessions(id)` SET NULL |
| `session_sequences` | `session_id` | → `sessions(id)` CASCADE |
| `session_turns` | `id` | `session_id` → `sessions` CASCADE; UNIQUE `(session_id, turn_index)` |
| `session_entries` | `(session_id, id)` | `session_id` CASCADE; `turn_id` → `session_turns` SET NULL |
| `session_todos` | `id` | `session_id` CASCADE |
| `goals` | `id` | `session_id` CASCADE |
| `session_summaries` | `session_id` | → `sessions(id)` CASCADE |
| `agent_spawn_edges` | `(parent, child)` | both → `sessions` CASCADE |

**Not FK-enforced (intentional soft links):** tree `parent_id` (append order / prune), turn `user_entry_id` / `assistant_entry_id` (may be pruned after compaction).

**Runtime:** every connection runs `PRAGMA foreign_keys = ON` (SQLite defaults to off).

Canonical SQL: `elph-agent` `CANONICAL_SESSION_SCHEMA_SQL` / platform migration v201.

### Session summaries (v203)

Migration **203** (`elph_session_summaries_v1`) adds the `session_summaries` table:

```sql
CREATE TABLE IF NOT EXISTS session_summaries (
    session_id TEXT PRIMARY KEY NOT NULL,
    summary TEXT NOT NULL,
    tokens_before INTEGER NOT NULL DEFAULT 0,
    compaction_count INTEGER NOT NULL DEFAULT 0,
    first_kept_entry_id TEXT,
    details TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
```

One row per session. Upserted automatically when compaction completes (manual `/compact` or auto-compaction) via the `session_compact` harness hook. The `compaction_count` auto-increments on each upsert. Other sessions can read past context via the `get_session_summary` agent tool (`session_id` argument) without replaying full history.

## Resume / continue

1. Open session by id (`--resume`) or latest non-empty for cwd (`--continue`).
2. `reconcile_session` + `AgentHarness::restore` (model, tools, journal).
3. LLM context: `session.build_context()` from the active branch (full prior turns + tool results after compaction transform).
4. **Continuity brief:** each turn the dynamic system prompt appends `<session_state>` when the session has history, open todos, and/or an active goal — open checklist, goal status, last user/assistant anchors, and an explicit “do not restart finished work” rule (`session_continuity.rs`).
5. **TUI:** `reconstruct_transcript_from_llm_entries` on that branch; durable `session_todos` are re-emitted as `TodoUpdated` on open so the live panel matches the store.

Messages are flushed on each `MessageEnd` into `session_entries` (transactional with leaf). Do not rely on UI snapshot blobs.

## Empty session cleanup

A session that never produced a turn is discarded immediately rather than persisted:

- **TUI exit** — when the user quits without ever sending a prompt (no `session_turns` row), the session record and its artifact dir are deleted.
- **`/new` / `/resume <id>`** — when switching away from the current session, if it has zero turns, its record is deleted so the session list is not littered with blank entries.
- **Headless (`elph run`)** — when the run finishes (or errors) without a persisted turn, the session is deleted unless `--no-session` is set (which deletes unconditionally).

This is separate from retention GC and is best-effort: a session with `turn_count = 0` is considered empty and eligible for immediate removal.

## Export / import / tree (product surface)

| Command | Behavior |
| --- | --- |
| `/export [path]` | Writes the **full** session DAG as JSONL (`SessionTreeEntry` per line). Default path: `./elph-session-<shortid>.jsonl`. |
| `/import <path.jsonl>` | Creates a **new** Turso session for the current project, appends all lines, then switches the TUI to that session (`/resume` equivalent). CLI: `elph import <file>`. |
| `/tree` | **Interactive picker** (Pi TreeSelector modes). ↑↓, type-to-search, **Tab** / **Ctrl+O** cycle mode, **Enter** jump, **Ctrl+Enter** jump+summary, **Esc** cancel. |
| `/tree <entry_id> [--summary]` | Non-interactive navigate (scriptable / ACP). Reloads transcript for the new leaf. |
| `/tree --branch` | Interactive picker limited to the active branch path. |
| `/resume` | **Interactive session picker** for this project. ↑↓ / filter / Enter to switch. |
| `/resume <session_id>` | Switch directly without the picker. |
| `/trust` | Records the workspace under `CONFIG_DIR/trust.json` (`directories` map). Not project `.elph/trusted`. |

### `/tree` filter modes (Pi-aligned)

| Mode | Visibility |
| --- | --- |
| `default` | Hide settings/bookkeeping (model, thinking, session_info, custom, bare label entries) |
| `no-tools` | Default, also hide tool-result messages |
| `user-only` | Only user messages |
| `labeled-only` | Entries with a session label (`★`) or label rows |
| `all` | Every navigable entry |

Keyboard (while the tree picker is open):

| Chord | Action |
| --- | --- |
| `Tab` / `Ctrl+O` | Cycle mode forward |
| `Shift+Tab` / `Ctrl+Shift+O` | Cycle mode backward |
| `Ctrl+D` | Set `default` |
| `Ctrl+T` | Toggle `no-tools` |
| `Ctrl+U` | Toggle `user-only` |
| `Ctrl+L` | Toggle `labeled-only` |
| `Ctrl+A` | Toggle `all` |

### Interactive slash commands

Some builtins open an **inline status-zone selector** (same pattern as `/model`, tool approval, `/rename`):

| Command | Interaction |
| --- | --- |
| `/model` | Provider + model tabs, filter, scoped models |
| `/resume` | Session list (`SelectItem` from `SessionManager::list`) |
| `/tree` | Navigable tree entries + filter modes; confirm moves harness leaf via `navigate_tree` |
| `/rename` | Free-text title editor |
| Tool approval / plan confirm | Fixed option lists |

Implementation: `tui/item_selector.rs` + `item_selector_bar.rs` (`PendingItemSelector`, `SlashOutcome::OpenItemSelector`). Prefer interactive open when args are empty; keep path/id args for non-interactive use.

**TUI invariant:** never call `State::set` during the render/build path of the shell (including “sync selected index every frame”). That re-dirties the frame forever and freezes the UI. Sync selection only on open and in key handlers.

**Design note — tree UX:** Elph adopts **Pi’s conversation-tree model** (entry DAG + leaf jump + branch summary). Grok Build’s strengths (`/resume` picker polish, `/rewind` file snapshots, multi-session dashboard) are complementary session-management UX, not a substitute for entry-level branching. Codex is closer to linear resume/fork and does not map to Elph’s `session_entries` tree.

## Turns, modes, and foundation model

One **session** is a durable conversation container (cwd, name, leaf, rollups). Inside it:

| Concept | Cardinality | Persistence today | Gap |
| --- | --- | --- | --- |
| **Turn** | many per session | `session_turns` (+ rollups) | OK; wire `session_entries.turn_id` on write |
| **Messages / tools** | many per turn | `session_entries` tree (`type=message`) | OK — this **is** the transcript SoT |
| **Agent mode** (build/plan/ask/brave) | changes over session | mostly **process-local** `mode_state`; `sessions.agent_mode` only set at create | **Gap:** mode not durable per turn or as tree events |
| **Model / thinking / collab mode** | changes over session | tree entries (`model_change`, `thinking_level_change`, `collaboration_mode_change`) | OK on resume via `derive_session_context_state` |

**Recommended modest schema/code polish (not a full redesign):**

1. Add `agent_mode` (and optionally `collaboration_mode`) on `session_turns` — snapshot at turn start.  
2. On mode change: update `sessions.agent_mode` denorm **and** append a tree entry (e.g. `agent_mode_change`) so resume restores tools/prompt like model changes.  
3. Fill `session_entries.turn_id` during an active harness turn (column already exists, currently always `NULL`).  
4. Keep hybrid model: tree = conversation structure; relational turns = metrics/audit.

No need for a separate `transcript` table: reconstructing TUI cards from the message tree is intentional and keeps a single SoT.

## Removed / unused

- **`skill_cache`** — dropped from v200 schema. No readers/writers ever existed; skills load from the filesystem each run.

## Retention (`settings.json`)

All knobs are under **`session.retention`** and **`compaction.physicalPrune`**. Layers: home `CONFIG_DIR/settings.json` ← project `.elph/settings.json` (project wins).

```json
{
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

| Key | Default | Meaning |
| --- | --- | --- |
| `enabled` | `true` | Master switch for automatic GC |
| `gcOnOpen` | `true` | Run GC when opening the store |
| `maxSessionsPerCwd` | `40` | Keep newest non-pinned sessions per project |
| `maxSessionAgeDays` | `30` | Age limit (`0` = unlimited) |
| `maxEntriesPerSession` | `8000` | Soft pressure toward compact+prune |
| `maxStoreDbBytes` | `512MiB` | Soft file budget (`0` = unlimited) |
| `protectLatestPerCwd` | `true` | Never auto-GC latest session per cwd |
| `maxEntryPayloadBytes` | `256KiB` | Truncate oversized payloads on write |
| `journalKeepTurns` | `20` | Bound harness journal custom entries |
| `maxTerminalFilesPerSession` | `50` | Cap terminal log files mid-session |
| `compaction.physicalPrune` | `true` | DELETE pre-boundary entries after compact |

**Pin** is per-session DB state (`sessions.pinned`), not a settings key:

```bash
elph session pin <SESSION_ID>
elph session unpin <SESSION_ID>
elph session prune [--dry-run]
```

GC runs automatically when `session.retention.gcOnOpen` is true (on coding session open). CLI `session prune` uses the same policy.

After compaction, when `compaction.physicalPrune` is true, entries not on the active post-compaction branch are deleted from `session_entries` (turn rollups in `session_turns` are kept).

GC never removes the currently open session id (when known), pinned sessions, or (when configured) the latest session per cwd.

## Related

- [agent-todo.md](./agent-todo.md) — todo tools and ReAct checklist usage
- `elph-agent/docs/durable-harness.md` — journal / recovery
