# Session persistence

How Elph stores sessions, restores history on `--continue` / `--resume`, and keeps the project store bounded.

**Source of truth**

| Concern | Where |
| --- | --- |
| LLM conversation + branch | `session_entries` tree (JSON payload per entry) |
| Turn lifecycle, tokens, cost | `session_turns` + rollup columns on `sessions` |
| Structured work checklist | `session_todos` |
| Session objective / budgets | `goals` |
| TUI transcript cards | **Reconstructed** from the tree on resume (not a separate snapshot SoT) |

Project store file: `<project>/.elph/store.db` (shared with floppy memory and codegraph).

Session artifacts (terminals, MCP cache): `APP_DATA/sessions/<SESSION_ID>/`.

## Schema (v201)

Clean band **201** (`elph_session_schema_v2_relational`). No upgrade path from experimental pre-v201 DBs — delete `store.db` if needed.

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
| `agent_spawn_edges` | `(parent, child)` | both → `sessions` CASCADE |

**Not FK-enforced (intentional soft links):** tree `parent_id` (append order / prune), turn `user_entry_id` / `assistant_entry_id` (may be pruned after compaction).

**Runtime:** every connection runs `PRAGMA foreign_keys = ON` (SQLite defaults to off).

Canonical SQL: `elph-agent` `CANONICAL_SESSION_SCHEMA_SQL` / platform migration v201.

## Resume / continue

1. Open session by id (`--resume`) or latest non-empty for cwd (`--continue`).
2. `reconcile_session` + `AgentHarness::restore` (model, tools, journal).
3. LLM context: `session.build_context()` from the active branch.
4. TUI: `reconstruct_transcript_from_llm_entries` on that branch.

Messages are flushed on each `MessageEnd` into `session_entries` (transactional with leaf). Do not rely on UI snapshot blobs.

## Export / import / tree (product surface)

| Command | Behavior |
| --- | --- |
| `/export [path]` | Writes the **full** session DAG as JSONL (`SessionTreeEntry` per line). Default path: `./elph-session-<shortid>.jsonl`. |
| `/import <path.jsonl>` | Creates a **new** Turso session for the current project, appends all lines, then switches the TUI to that session (`/resume` equivalent). CLI: `elph import <file>`. |
| `/tree` | **Interactive picker** (messages / branch summaries / compaction). ↑↓ move, type to filter, **Enter** jump leaf, **Ctrl+Enter** jump + branch summary, **Esc** cancel. |
| `/tree <entry_id> [--summary]` | Non-interactive navigate (scriptable / ACP). Reloads transcript for the new leaf. |
| `/tree --branch` | Interactive picker limited to the active branch path. |
| `/resume` | **Interactive session picker** for this project. ↑↓ / filter / Enter to switch. |
| `/resume <session_id>` | Switch directly without the picker. |
| `/trust` | Records the workspace under `CONFIG_DIR/trust.json` (`directories` map). Not project `.elph/trusted`. |

### Interactive slash commands

Some builtins open an **inline status-zone selector** (same pattern as `/model`, tool approval, `/rename`):

| Command | Interaction |
| --- | --- |
| `/model` | Provider + model tabs, filter, scoped models |
| `/resume` | Session list (`SelectItem` from `SessionManager::list`) |
| `/tree` | Navigable tree entries; confirm moves harness leaf via `navigate_tree` |
| `/rename` | Free-text title editor |
| Tool approval / plan confirm | Fixed option lists |

Implementation: `tui/item_selector.rs` + `item_selector_bar.rs` (`PendingItemSelector`, `SlashOutcome::OpenItemSelector`). Prefer interactive open when args are empty; keep path/id args for non-interactive use.

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
