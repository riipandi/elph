# Agent Memory

Design for **floppy** — project-local agent memory that persists lessons, corrections, and insights across sessions.

Inspired by [memelord](https://github.com/glommer/memelord) (MIT License, Copyright © 2026 Glauber Costa).

## Overview

| Concern    | Approach                                                    |
| ---------- | ----------------------------------------------------------- |
| Storage    | Turso embedded SQLite (`store.db`)                          |
| Retrieval  | Vector similarity (`vector32`; dims match embed model)      |
| Embeddings | Local ONNX (configurable model + cache)                     |
| Scoring    | Welford baseline + z-score task scoring, EMA weight updates |
| IDs        | TSID (time-sortable, 13 characters)                         |
| Migrations | Versioned SQL via shared `app_migrations` ledger            |

### Lifecycle

1. **Start task** — embed description, retrieve top-k memories, record retrievals.
2. **Work** — agent uses context; reports corrections, user input, or insights.
3. **End task** — score vs baseline, update memory weights from credits.
4. **Maintenance** — decay unused weights, purge weak memories.

```
┌─────────────┐     start_task      ┌──────────────────┐
│   Agent     │ ──────────────────► │  store.db       │
│   session   │ ◄── top-k memories  │  (Turso + vec)   │
└─────────────┘     end_task        └──────────────────┘
       │              report              │
       └──────── corrections ─────────────┘
```

## Storage layout

### Elph (default)

```
PROJECT_DIR/
└── .elph/
    ├── store.db          # gitignored
    └── .gitignore
```

### Standalone / library hosts

| Constant         | Value      |
| ---------------- | ---------- |
| Default data dir | `.floppy`  |
| Database file    | `store.db` |

Hosts supply paths explicitly; the memory layer does not read environment variables directly.

### Model cache

Embedding weights live in the user data directory (not in the project):

```
~/.local/share/elph/     # or ELPH_DATA_DIR / XDG_DATA_HOME/elph
└── models/
```

First semantic search downloads from Hugging Face; later runs reuse the cache.

## Schema

| Table               | Purpose                                               |
| ------------------- | ----------------------------------------------------- |
| `memories`          | Content, embedding, category, weight, retrieval stats |
| `tasks`             | Description, embedding, usage metrics, score          |
| `memory_retrievals` | Per (memory, task): similarity, self-report, credit   |
| `meta`              | Key-value (e.g. Welford baseline JSON)                |
| `app_migrations`    | Migration ledger                                      |

### Categories

| Category       | Typical source                          |
| -------------- | --------------------------------------- |
| `correction`   | Agent mistake + lesson                  |
| `user`         | User denial, correction, explicit input |
| `insight`      | Agent-discovered pattern                |
| `discovery`    | Exploratory finding                     |
| `consolidated` | Merged or summarized memories           |

### Defaults

| Setting              | Default                                        |
| -------------------- | ---------------------------------------------- |
| Embed model          | `AllMiniLML6V2` (quantized → `AllMiniLML6V2Q`) |
| Embedding dimensions | Model-dependent (384 for AllMiniLML6V2)        |
| Vector type          | `vector32`                                     |
| Top-k retrieval      | 5                                              |
| Learning rate (EMA)  | 0.1                                            |
| Decay rate           | 0.995                                          |
| Weight clamp         | [0.1, 5.0]                                     |

## Scoring model

**Task baseline** — Welford online mean/variance over tokens, errors, and user corrections (persisted in `meta`).

**Task score** — vs baseline:

- Cold start (&lt; 10 tasks): normalized deltas + completion signal
- Steady state: z-scores (lower tokens/errors/corrections = better) + completion

**Credit** per retrieved memory:

```
credit = task_score × (self_report / 3) × (1 / num_retrieved)
```

**Weight update** — EMA toward credit, clamped [0.1, 5.0].

**Decay** — multiply weights by `decay_rate`; delete below 0.15 when `retrieval_count > 5`.

## Agent integration API (design)

### Task lifecycle

| Phase  | Action                                                  |
| ------ | ------------------------------------------------------- |
| Start  | `start_task(description)` → task id + top-k memories    |
| During | `report_correction`, `report` (insight / user / …)      |
| End    | `end_task` with usage metrics + self-reports per memory |

### Query & maintenance

| Operation         | Description                                      |
| ----------------- | ------------------------------------------------ |
| `get_status`      | Store statistics                                 |
| `list_memories`   | Optional category filter                         |
| `list_tasks`      | Recent tasks with retrievals                     |
| `get_timeline`    | Merged event timeline                            |
| `search_memories` | Semantic search without creating a task          |
| `search`          | Full lifecycle search (creates task record)      |
| `decay`           | Apply decay + prune weak entries                 |
| `purge`           | Delete below weight threshold                    |
| `contradict`      | Remove wrong memory, optionally store correction |
| `embed_pending`   | Backfill missing embeddings                      |

## CLI

| Subcommand       | Description                       |
| ---------------- | --------------------------------- |
| `status`         | Overview                          |
| `list`           | All memories; `--category` filter |
| `tasks`          | Recent tasks                      |
| `log`            | Compact timeline                  |
| `search <query>` | Semantic lookup (needs embedder)  |
| `purge`          | Remove weak memories              |

Read-only commands do not require a loaded embedding model. `search` downloads the model on first use.

## Settings

In `~/.elph/settings.json`:

| Field                   | Default         | Description                             |
| ----------------------- | --------------- | --------------------------------------- |
| `memory.embedModel`     | `AllMiniLML6V2` | Model name or Hugging Face alias        |
| `memory.embedQuantized` | `true`          | Prefer `*Q` ONNX variant when available |

### Model aliases (examples)

| Alias                                    | Resolves to       |
| ---------------------------------------- | ----------------- |
| `sentence-transformers/all-MiniLM-L6-v2` | AllMiniLML6V2     |
| `all-minilm-l6-v2`                       | AllMiniLML6V2     |
| `BAAI/bge-small-en-v1.5`                 | BGESmallENV15     |
| `nomic-ai/nomic-embed-text-v1.5`         | NomicEmbedTextV15 |

### Changing models

Embeddings are fixed-size blobs for `vector32` queries. Changing to a model with different dimensions requires re-embedding or a fresh store — dimension mismatches break retrieval.

## Environment (Elph host)

| Variable           | Effect                          |
| ------------------ | ------------------------------- |
| `ELPH_HOME`        | Config dir (`settings.json`)    |
| `ELPH_DATA_DIR`    | Data dir (`models/` cache)      |
| `ELPH_PROJECT_DIR` | Project root (`.elph/store.db`) |
| `XDG_DATA_HOME`    | Base for data dir when unset    |

## Migrations (design)

| Version | Description                   |
| ------- | ----------------------------- |
| 1       | Core schema                   |
| 2       | Fix truncated embedding blobs |
| 3       | Query indexes                 |

Host-specific migrations use version numbers above the floppy baseline.

## Related

- [configuration.md](./configuration.md) — paths and settings
- [cli.md](./cli.md) — `elph memory`
- [agent-runtime.md](./agent-runtime.md) — runtime integration
- [openwiki](../openwiki/quickstart.md) — implementation details
