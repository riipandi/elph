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
| Retrieval  | Vector similarity (`vector32`; dims match the embed model)  |
| Embeddings | Local ONNX via `fastembed` (configurable model + cache)     |
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
`version > memz::migrations::LAST_VERSION` (currently `3`).

### Standalone library use

For non-Elph consumers, pass paths explicitly via [`MemzPaths`] or [`MemzBuilder`]:

| Constant           | Value       |
| ------------------ | ----------- |
| `DEFAULT_DATA_DIR` | `.memz`     |
| `DB_FILE_NAME`     | `memory.db` |

Default: `MemzPaths::project_local()` → `./.memz/memory.db`. The memz module does not read
environment variables; hosts supply directories and options as arguments.

### Model cache (Elph)

ONNX weights and tokenizers downloaded by `fastembed` are cached under the Elph data
directory (not inside the project):

```
~/.local/share/elph/          # or $ELPH_DATA_DIR / $XDG_DATA_HOME/elph
└── models/                   # Paths::models_dir()
```

The directory is created on first use (e.g. `elph memory search`). Downloads happen
automatically from Hugging Face when the chosen model is not yet cached.

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

| Setting              | Default                                        |
| -------------------- | ---------------------------------------------- |
| Embed model          | `AllMiniLML6V2` (quantized → `AllMiniLML6V2Q`) |
| Embedding dimensions | Model-dependent (384 for `AllMiniLML6V2`)      |
| Vector type          | `vector32`                                     |
| Top-k retrieval      | 5                                              |
| Learning rate (EMA)  | 0.1                                            |
| Decay rate           | 0.995                                          |
| Weight clamp         | [0.1, 5.0]                                     |

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

Prefer [`MemzBuilder`] — all configuration is explicit (no environment variables):

```rust
use elph_core::memz::{MemzBuilder, MemzPaths, FastEmbedOptions};

// Standalone path + local embeddings (feature fastembed)
let store = MemzPaths::project_local()
    .builder("session-id")
    .fastembed(
        FastEmbedOptions::default()
            .model("AllMiniLML6V2")
            .quantized(true)
            .cache_dir("/path/to/models"),
    )?
    .build()?;
store.init().await?;

// Read-only inspection without a model
let store = MemzBuilder::new("/path/to/memory.db", "session-id")
    .dimensions(384)
    .noop_embed()
    .build()?;
```

Lower-level: `create_memory_store(MemzConfig, EmbedFn)` remains available for custom wiring.

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

Local embeddings use [fastembed](https://github.com/Anush008/fastembed-rs) (ONNX). The
default model is **AllMiniLML6V2** — with `embedQuantized: true` (the default), the
quantized variant **AllMiniLML6V2Q** is selected automatically.

### Elph settings

Configure the embedder in `~/.elph/settings.json` (or `$ELPH_HOME/settings.json`):

```json
{
    "memory": {
        "embedModel": "AllMiniLML6V2",
        "embedQuantized": true
    }
}
```

| Field            | Default         | Description                                            |
| ---------------- | --------------- | ------------------------------------------------------ |
| `embedModel`     | `AllMiniLML6V2` | fastembed model name or Hugging Face alias (see below) |
| `embedQuantized` | `true`          | Prefer the `*Q` ONNX variant when one exists           |

`elph memory` loads these values via `Settings::load()` and passes them to
`create_fastembed` with `cache_dir` set to `Paths::models_dir()`. `MemzConfig::dimensions`
is set from the resolved model so vector queries match the embedder output.

### Model names and aliases

`resolve_embedding_model(name, quantized)` accepts any name understood by fastembed's
[`EmbeddingModel`](https://docs.rs/fastembed/latest/fastembed/enum.EmbeddingModel.html)
(via `FromStr`), plus common Hugging Face aliases:

| Alias                                     | Resolves to         |
| ----------------------------------------- | ------------------- |
| `sentence-transformers/all-MiniLM-L6-v2`  | `AllMiniLML6V2`     |
| `all-minilm-l6-v2`                        | `AllMiniLML6V2`     |
| `sentence-transformers/all-MiniLM-L12-v2` | `AllMiniLML12V2`    |
| `sentence-transformers/all-mpnet-base-v2` | `AllMpnetBaseV2`    |
| `BAAI/bge-small-en-v1.5`                  | `BGESmallENV15`     |
| `BAAI/bge-base-en-v1.5`                   | `BGEBaseENV15`      |
| `BAAI/bge-large-en-v1.5`                  | `BGELargeENV15`     |
| `nomic-ai/nomic-embed-text-v1`            | `NomicEmbedTextV1`  |
| `nomic-ai/nomic-embed-text-v1.5`          | `NomicEmbedTextV15` |

When `embedQuantized` / `FastEmbedOptions::quantized` is `true`, memz appends `Q` to the
canonical model name if a quantized variant exists (e.g. `AllMiniLML6V2` → `AllMiniLML6V2Q`).
Names that already end in `Q` are left unchanged.

Embedding dimensions are read from fastembed model metadata via `embedding_dims()` (384 for
`AllMiniLML6V2`; other models vary).

### Configuration

| Context      | Source                                                               |
| ------------ | -------------------------------------------------------------------- |
| memz library | [`MemzBuilder`] / [`FastEmbedOptions`] builder methods and arguments |
| Elph CLI     | `settings.json` mapped into `MemzBuilder` by `elph::memory::store`   |

memz itself does not read environment variables. Unset `FastEmbedOptions::model` falls back to
[`DEFAULT_EMBED_MODEL`] (`AllMiniLML6V2`).

### First-run download

Commands that need embeddings (`elph memory search`, and agent/runtime task APIs) download
the model on first use. Progress is shown by default. Subsequent runs reuse the cache under
`{data_dir}/models/`.

Read-only CLI commands (`status`, `list`, `tasks`, `log`, `purge`) use a no-op embedder and
do not touch the model cache.

### Changing models on an existing store

Embeddings are stored as fixed-size BLOBs for Turso `vector32` distance queries. If you
change `embedModel` to a model with different output dimensions, existing memories will not
match new queries until they are re-embedded — and dimension mismatches can cause retrieval
errors. Treat a model change on a populated database as requiring a fresh store or a manual
re-embed migration (not shipped today).

Memories inserted without embeddings are backfilled by `embed_pending()` (also called
automatically during `start_task`).

### Library feature flag

Enable in `elph-core`:

```toml
elph-core = { version = "...", features = ["fastembed"] }
```

Example with explicit options:

```rust
use elph_core::memz::{create_fastembed, FastEmbedOptions, resolve_embedding_model, embedding_dims};

let model = resolve_embedding_model("AllMiniLML6V2", true)?;
let dims = embedding_dims(&model);

let embed = create_fastembed(
    FastEmbedOptions::default()
        .model("AllMiniLML6V2")
        .quantized(true)
        .cache_dir("/path/to/models"),
)?;
```

---

## Migrations

Memz ships versioned migrations in `elph_core::memz::migrations`:

| Version | Name                            | Description                        |
| ------- | ------------------------------- | ---------------------------------- |
| 1       | `memz_create_schema`            | Core tables                        |
| 2       | `memz_fix_truncated_embeddings` | Null out truncated embedding blobs |
| 3       | `memz_query_indexes`            | Indexes for list/search/task joins |

Elph maps these into `memory_migrations()` and applies them during `ensure_datastore`.
`MemoryStore::init()` also calls `migrations::apply()` (idempotent).

To extend the schema in Elph, append migrations with `version > LAST_VERSION` in
`elph/src/runtime/migrations.rs`.

---

## Integration with Elph

| Layer                      | Role                                                                     |
| -------------------------- | ------------------------------------------------------------------------ |
| `elph::runtime::paths`     | `memory_db_path()` → `.elph/memory.db`; `models_dir()` → `{data}/models` |
| `elph::runtime::settings`  | `memory.embedModel` / `memory.embedQuantized` in `settings.json`         |
| `elph::runtime::datastore` | Runs metadata + memory migrations                                        |
| `elph::runtime::project`   | Creates `.elph/` and gitignore                                           |
| `elph::memory::store`      | Opens `MemoryStore` via `MemzBuilder` (`apply_migrations(false)`)        |
| `elph memory`              | CLI over `elph_core::memz::MemoryStore`                                  |

The agent runtime can open the same store path and call the task lifecycle API during
sessions. The CLI is for inspection and manual maintenance.

---

## Environment variables

| Variable           | Scope | Description                                                     |
| ------------------ | ----- | --------------------------------------------------------------- |
| `ELPH_HOME`        | Elph  | Config directory (default `~/.elph`; holds `settings.json`)     |
| `ELPH_DATA_DIR`    | Elph  | Data directory (default `~/.local/share/elph`; holds `models/`) |
| `ELPH_PROJECT_DIR` | Elph  | Project root (determines `.elph/memory.db`)                     |
| `XDG_DATA_HOME`    | Elph  | Base for data dir when `ELPH_DATA_DIR` is unset                 |

---

[`MemzBuilder`]: ../crates/elph-core/src/memz/builder.rs
[`MemzPaths`]: ../crates/elph-core/src/memz/paths.rs
[`DEFAULT_EMBED_MODEL`]: ../crates/elph-core/src/memz/embed.rs

---

## Further reading

- [memelord](https://github.com/glommer/memelord) — original SDK design
- `crates/elph-core/src/memz/` — implementation
- `elph/src/memory/` — CLI wiring and output formatting
