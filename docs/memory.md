# Elph Memory

Elph Memory (`memz`) is a project-local agent memory store. It keeps lessons, corrections,
and insights across sessions, retrieves them by semantic similarity at task start, and
adjusts per-memory weights from task outcomes.

The implementation lives in `elph-core` as the `memz` module. It is ported from the
[memelord](https://github.com/glommer/memelord) SDK (`packages/sdk`). The original code is
licensed under the MIT License. Copyright (c) 2026 Glauber Costa.

---

## Overview

| Concern    | Approach                                                    |
| ---------- | ----------------------------------------------------------- |
| Storage    | Turso embedded SQLite (`memory.db`)                         |
| Retrieval  | Vector similarity (`vector32`, 384-dim embeddings)          |
| Scoring    | Welford baseline + z-score task scoring, EMA weight updates |
| IDs        | UUID v7                                                     |
| Migrations | Versioned SQL via shared `app_migrations` ledger            |

At a high level:

1. **Start task** — embed the task description, retrieve top-k memories, record retrievals.
2. **Work** — agent uses retrieved context; reports corrections, user input, or insights.
3. **End task** — score the task vs a running baseline, update memory weights from credits.
4. **Maintenance** — decay unused weights, purge low-weight memories.

```
┌─────────────┐     start_task      ┌──────────────────┐
│   Agent     │ ──────────────────► │  memory.db       │
│   session   │ ◄── top-k memories  │  (Turso + vec)   │
└─────────────┘     end_task        └──────────────────┘
       │              report              │
       └──────── corrections ─────────────┘
```

---

## Storage layout

### Elph (default)

Project memory is stored next to the repo:

```
PROJECT_DIR/
└── .elph/
    ├── memory.db          # memz store (gitignored)
    └── .gitignore
```

Path resolution: `Paths::memory_db_path()` → `PROJECT_DIR/.elph/memory.db`.

Migrations run through Elph's datastore bootstrap (`elph::runtime::migrations::memory_migrations`),
composed from `elph_core::memz::migrations::MIGRATIONS`. Host-specific migrations use
`version > memz::migrations::LAST_VERSION` (currently `2`).

### Standalone library use

For non-Elph consumers, `MemzPaths` resolves:

| Constant / env     | Value       |
| ------------------ | ----------- |
| `DEFAULT_DATA_DIR` | `.memz`     |
| `ENV_DATA_DIR`     | `MEMZ_DIR`  |
| `DB_FILE_NAME`     | `memory.db` |

Default path: `./.memz/memory.db`.

---

## Schema

| Table               | Purpose                                                    |
| ------------------- | ---------------------------------------------------------- |
| `memories`          | Content, embedding blob, category, weight, retrieval stats |
| `tasks`             | Task description, embedding, usage metrics, score          |
| `memory_retrievals` | Per (memory, task): similarity, self-report, credit        |
| `meta`              | Key-value store (e.g. Welford baseline JSON)               |
| `app_migrations`    | Migration ledger (shared with Elph datastore)              |

### Memory categories

| Category       | Typical source                             |
| -------------- | ------------------------------------------ |
| `correction`   | Agent mistake + lesson learned             |
| `user`         | User denial, correction, or explicit input |
| `insight`      | Agent-discovered pattern                   |
| `discovery`    | Exploratory finding during a task          |
| `consolidated` | Merged or summarized memories              |

### Default configuration

| Setting              | Default                  |
| -------------------- | ------------------------ |
| Embedding dimensions | 384 (`all-MiniLM-L6-v2`) |
| Vector type          | `vector32`               |
| Top-k retrieval      | 5                        |
| Learning rate (EMA)  | 0.1                      |
| Decay rate           | 0.995                    |
| Weight clamp         | [0.1, 5.0]               |

---

## Scoring model

**Task baseline** — Welford online mean/variance over tokens, errors, and user corrections.
Persisted in `meta` as JSON.

**Task score** — Compared to baseline:

- Cold start (&lt; 10 tasks): normalized deltas + completion signal.
- Steady state: z-scores (lower tokens/errors/corrections = better) + completion signal.

**Credit** — Per retrieved memory:

```
credit = task_score × (self_report / 3) × (1 / num_retrieved)
```

**Weight update** — EMA toward credit, clamped to [0.1, 5.0].

**Initial weight** — Category-dependent; corrections scale with `tokens_wasted`.

**Decay** — Multiply all weights by `decay_rate`; delete memories below 0.15 with
`retrieval_count > 5` during decay runs.

---

## CLI

```bash
elph memory <subcommand>
```

| Subcommand       | Description                                                         |
| ---------------- | ------------------------------------------------------------------- |
| `status`         | Counts, categories, top memories, task stats                        |
| `list`           | All memories; optional `--category <name>`                          |
| `tasks`          | Recent tasks with retrievals and outcomes (`--limit`, default 10)   |
| `log`            | Compact timeline of tasks and memory events (`--limit`, default 20) |
| `search <query>` | Semantic search (creates a task record; needs embedder)             |
| `purge`          | Delete memories below weight threshold (default 0.5)                |

### Examples

```bash
# Overview of the project store
elph memory status

# Corrections only
elph memory list --category correction

# Semantic lookup (downloads embedding model on first run)
elph memory search "how does session auth work"

# Remove weak memories
elph memory purge --threshold 0.3
```

Read-only commands (`status`, `list`, `tasks`, `log`, `purge`) use a no-op embedder.
`search` requires the `fastembed` feature (enabled in the `elph` binary).

---

## Library API

### Opening a store

```rust
use std::sync::Arc;
use elph_core::memz::{
    MemzConfig, MemoryStore, create_memory_store,
    create_fastembed, FastEmbedOptions, EmbedFn,
};

// With local embeddings (feature fastembed)
let embed = create_fastembed(FastEmbedOptions::default())?;

// Or a custom embedder
let embed: EmbedFn = Arc::new(|text| {
    Box::pin(async move {
        Ok(my_embed(text).await?)
    })
});

let store = create_memory_store(
    MemzConfig::new("/path/to/memory.db", "session-id"),
    embed,
);
store.init().await?;
```

### Task lifecycle

```rust
use elph_core::memz::{ReportCorrectionInput, TaskEndInput, SelfReportEntry};

// 1. Start — retrieve relevant memories
let start = store.start_task("fix flaky integration tests").await?;

// 2. Report during work
store.report_correction(ReportCorrectionInput {
    lesson: "Always await async fixture teardown".into(),
    what_failed: "tests hung on CI".into(),
    what_worked: String::new(),
    tokens_wasted: Some(12_000),
    tools_wasted: Some(8),
}).await?;

// 3. End — update weights from outcome
store.end_task(&start.task_id, TaskEndInput {
    tokens_used: 4000,
    tool_calls: 12,
    errors: 0,
    user_corrections: 0,
    completed: true,
    self_reports: vec![SelfReportEntry {
        memory_id: start.memories[0].id.clone(),
        score: 3.0,
    }],
}).await?;
```

### Unified report API

```rust
use elph_core::memz::{MemoryReportInput, MemoryReportType, UserInputSource};

store.report(MemoryReportInput {
    report_type: MemoryReportType::Insight,
    lesson: "Auth middleware runs before route handlers".into(),
    what_failed: None,
    what_worked: None,
    tokens_wasted: None,
    tools_wasted: None,
    source: None,
}).await?;
```

### Query API (read-only)

| Method                    | Description                               |
| ------------------------- | ----------------------------------------- |
| `get_status()`            | Extended store statistics                 |
| `list_memories(category)` | All memories, optional category filter    |
| `list_tasks(limit)`       | Recent tasks with retrievals              |
| `get_timeline(limit)`     | Merged event timeline                     |
| `search_memories(query)`  | Semantic search without creating a task   |
| `search(query)`           | Full task lifecycle search (creates task) |

### Maintenance

| Method                       | Description                                      |
| ---------------------------- | ------------------------------------------------ |
| `decay()`                    | Apply decay rate, prune very weak memories       |
| `purge(threshold)`           | Delete memories below threshold                  |
| `contradict(id, correction)` | Remove wrong memory, optionally store correction |
| `embed_pending()`            | Backfill NULL embeddings                         |

---

## Embeddings

Default model: **all-MiniLM-L6-v2** (384 dimensions) via `fastembed`.

| Env var            | Purpose                                                                         |
| ------------------ | ------------------------------------------------------------------------------- |
| `MEMZ_EMBED_MODEL` | Override model name (`AllMiniLML6V2`, `sentence-transformers/all-MiniLM-L6-v2`) |

Enable in `elph-core`:

```toml
elph-core = { version = "...", features = ["fastembed"] }
```

Embeddings are stored as BLOBs for Turso `vector32` distance queries. Memories inserted
without embeddings are backfilled by `embed_pending()` (also called automatically during
`start_task`).

---

## Migrations

Memz ships versioned migrations in `elph_core::memz::migrations`:

| Version | Name                            | Description                        |
| ------- | ------------------------------- | ---------------------------------- |
| 1       | `memz_create_schema`            | Core tables                        |
| 2       | `memz_fix_truncated_embeddings` | Null out truncated embedding blobs |

Elph maps these into `memory_migrations()` and applies them during `ensure_datastore`.
`MemoryStore::init()` also calls `migrations::apply()` (idempotent).

To extend the schema in Elph, append migrations with `version > LAST_VERSION` in
`elph/src/runtime/migrations.rs`.

---

## Integration with Elph

| Layer                      | Role                                    |
| -------------------------- | --------------------------------------- |
| `elph::runtime::paths`     | `memory_db_path()` → `.elph/memory.db`  |
| `elph::runtime::datastore` | Runs metadata + memory migrations       |
| `elph::runtime::project`   | Creates `.elph/` and gitignore          |
| `elph memory`              | CLI over `elph_core::memz::MemoryStore` |

The agent runtime can open the same store path and call the task lifecycle API during
sessions. The CLI is for inspection and manual maintenance.

---

## Environment variables

| Variable           | Scope           | Description                                 |
| ------------------ | --------------- | ------------------------------------------- |
| `ELPH_PROJECT_DIR` | Elph            | Project root (determines `.elph/memory.db`) |
| `MEMZ_DIR`         | memz standalone | Data directory (default `.memz`)            |
| `MEMZ_EMBED_MODEL` | Embeddings      | Override fastembed model                    |

---

## Further reading

- [memelord](https://github.com/glommer/memelord) — original SDK design
- `crates/elph-core/src/memz/` — implementation
- `elph/src/memory/` — CLI wiring and output formatting
