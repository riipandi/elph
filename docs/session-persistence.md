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

## Schema (v200)

Clean band **200** (`elph_session_schema_v2`). No upgrade path from experimental pre-v200 DBs — delete `store.db` if needed.

- **`sessions`** — metadata, leaf, pin, token/cost rollups, `entry_count` / `approx_bytes`
- **`session_entries`** — tree spine (`parent_id`, `type`, `role`, `payload_bytes`, `payload`)
- **`session_sequences`** — next `entry_seq`
- **`session_turns`** — per-turn usage and status
- **`session_todos`** — todo list (`todo_<kalid>` ids)
- **`goals`**, **`skill_cache`**, **`agent_spawn_edges`**

Canonical SQL: `elph-agent` `CANONICAL_SESSION_SCHEMA_SQL` / platform migration v200.

## Resume / continue

1. Open session by id (`--resume`) or latest non-empty for cwd (`--continue`).
2. `reconcile_session` + `AgentHarness::restore` (model, tools, journal).
3. LLM context: `session.build_context()` from the active branch.
4. TUI: `reconstruct_transcript_from_llm_entries` on that branch.

Messages are flushed on each `MessageEnd` into `session_entries` (transactional with leaf). Do not rely on UI snapshot blobs.

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
