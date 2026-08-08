# Agent Memory

Design for **floppy** — project-local agent memory that persists lessons, corrections, and insights across sessions.

Inspired by [memelord](https://github.com/glommer/memelord) (MIT License, Copyright © 2026 Glauber Costa).

## Overview

| Concern    | Approach                                                                                     |
| ---------- | -------------------------------------------------------------------------------------------- |
| Storage    | Turso embedded SQLite (`store.db`)                                                           |
| Retrieval  | **Hybrid**: keyword (Turso-native FTS, Tantivy-backed) + vector (`vector32`), decay-weighted |
| Embeddings | Local ONNX (configurable model + cache)                                                      |
| Scoring    | Welford baseline + z-score task scoring, EMA weight updates                                  |
| IDs        | Kalid (time-sortable, 16 characters)                                                         |
| Migrations | Shared `app_migrations` ledger (`apply_set`); additive DDL; no `PRAGMA user_version`         |

### Lifecycle

1. **Start task** — embed description, retrieve top-k memories, record retrievals.
2. **Work** — agent uses context; reports corrections, user input, or insights.
3. **End task** — score vs baseline, update memory weights from credits.
4. **Maintenance** — decay unused weights, purge weak memories.

```
┌─────────────┐     start_task      ┌──────────────────┐
│   Agent     │ ──────────────────► │  store.db       │
│   session   │ ◄── top-k memories  │  (Turso + vec + FTS) │
└─────────────┘     end_task        └──────────────────┘
       │              report              │
       └──────── corrections ─────────────┘
```

## Storage layout

### Elph (default)

```
PROJECT_DIR/
└── .elph/
    ├── store.db          # gitignored; memory + codegraph
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

First semantic search downloads from Hugging Face; later runs reuse the cache. The initial download is bounded by a 5-minute timeout (`EMBEDDER_INIT_TIMEOUT`); on a slow or blocked network the command fails with a clear message instead of hanging.

## Schema

| Table               | Purpose                                               |
| ------------------- | ----------------------------------------------------- |
| `memories`          | Content, embedding, category, weight, retrieval stats |
| `tasks`             | Description, embedding, usage metrics, score          |
| `memory_retrievals` | Per (memory, task): similarity, self-report, credit   |
| `meta`              | Key-value (e.g. Welford baseline JSON)                |

Keyword search uses the Turso-native FTS index (`idx_memories_fts`, Tantivy-backed on
`memories.content`), applied by migration V4. The index is auto-maintained by Turso on
insert/update/delete. When `experimental_index_method` is not enabled, the FTS migration
is skipped and `fts_available = 0` is recorded in `meta` — keyword search falls back to
vector-only retrieval.

### Categories

| Category       | Typical source                                    |
| -------------- | ------------------------------------------------- |
| `correction`   | Agent mistake + lesson                            |
| `user`         | User denial, correction, explicit input           |
| `insight`      | Agent-discovered pattern                          |
| `discovery`    | Exploratory finding                               |
| `consolidated` | Merged or summarized memories                     |
| `work`         | Active task context (dedicated decay/purge rules) |

### Defaults

| Setting              | Default                                                |
| -------------------- | ------------------------------------------------------ |
| Embed model          | `AllMiniLML6V2` (quantized → `AllMiniLML6V2Q`)         |
| Embedding dimensions | Model-dependent (384 for AllMiniLML6V2)                |
| Vector type          | `vector32`                                             |
| Top-k retrieval      | 5 (floppy default; Elph host sets 8 via `memory.topK`) |
| Learning rate (EMA)  | 0.1                                                    |
| Decay rate           | 0.995                                                  |
| Weight clamp         | [0.1, 5.0]                                             |

## Scoring model

**Task baseline** — Welford online mean/variance over tokens, errors, and user corrections (persisted in `meta`).

**Task score** — vs baseline:

- Cold start (&lt; 10 tasks): normalized deltas + completion signal
- Steady state: z-scores (lower tokens/errors/corrections = better) + completion

**Credit** per retrieved memory:

```
credit = task_score × (self_report / 3) × (1 / num_retrieved)
```

**Weight update** — EMA toward credit with a weight-dependent learning rate, clamped [0.1, 5.0].

**Decay** — multiply weights by `decay_rate`, category-aware: `work` notes fade at most
`min(decay_rate, 0.98)` per pass, `correction`/`user` memories at most `max(decay_rate, 0.998)`
(kept longer), everything else at the base `decay_rate`. Purge deletes memories with
`weight < 0.15 AND retrieval_count > 5`; `work` notes are additionally purged when
`weight < 0.4`, older than 14 days, and retrieved fewer than 3 times. Maintenance also cleans
up orphaned `memory_retrievals` rows.

### Retrieval

Retrieval is hybrid: vector cosine similarity over `memories.embedding`, weighted by a decay
factor on the elapsed days since the memory was last retrieved, **plus** keyword search
through the Turso-native FTS index (Tantivy-backed, migration V4) when available. The FTS
pass surfaces exact keyword matches the vector search may have missed. Hits without embeddings
(e.g. pending/truncated embeddings at keyword-match time) get a rank-based synthetic score in
[0.35, 0.55]; hits with embeddings use their real cosine similarity:

```sql
SELECT id, content, category, weight, created_at, retrieval_count,
       vector_distance_cos(vector32(embedding), vector32(?)) AS distance
FROM memories
WHERE embedding IS NOT NULL
ORDER BY
  (1.0 - vector_distance_cos(vector32(embedding), vector32(?)))
  * POWER(?, (CAST(? AS REAL) - COALESCE(last_retrieved, created_at)) / 86400.0)
DESC
LIMIT ?
```

The bound parameters are `decay_rate`, current time in seconds, and `top_k`; memories that
have not been retrieved recently are boosted, keeping stale entries from crowding out fresh
context. The keyword path (hybrid) adds exact-match hits the vector search may have missed,
using the Turso-native FTS index on `memories.content` (migration V4).

## Agent integration API

### Task lifecycle

| Phase  | Action                                                        |
| ------ | ------------------------------------------------------------- |
| Start  | `start_task(description)` → task id + top-k memories          |
| During | `report_correction`, `report_user_input`, `insert_raw_memory` |
| End    | `end_task` with usage metrics + self-reports per memory       |

### Query & maintenance

| Operation               | Description                                                     |
| ----------------------- | --------------------------------------------------------------- |
| `get_status`            | Store statistics                                                |
| `list_memories`         | Optional category filter                                        |
| `list_recent_memories`  | Recent memories (`limit`)                                       |
| `list_tasks`            | Recent tasks with retrievals                                    |
| `get_timeline`          | Merged event timeline                                           |
| `search_memories`       | Hybrid semantic keyword + vector search without creating a task |
| `search`                | Full lifecycle search (creates task record)                     |
| `decay`                 | Apply decay + prune weak entries                                |
| `consolidate_similar`   | Merge similar memories (max 10 merges, weight cap 2.5)          |
| `purge`                 | Delete below weight threshold                                   |
| `flush`                 | Delete all memories                                             |
| `contradict_memory`     | Remove wrong memory, optionally store correction                |
| `insert_raw_memory`     | Insert a raw memory with explicit category                      |
| `embed_pending`         | Backfill missing embeddings                                     |
| `clear_zero_embeddings` | Drop zero-length embedding blobs                                |
| `penalize_memory`       | Scale a memory's weight down                                    |
| `get_top_by_weight`     | Highest-weight memories                                         |

## CLI

| Subcommand       | Description                                       |
| ---------------- | ------------------------------------------------- |
| `status`         | Overview                                          |
| `list`           | All memories; `--category` filter, `-n`           |
| `recent`         | Recent memories (`-n` limit)                      |
| `tasks`          | Recent tasks                                      |
| `log`            | Compact timeline                                  |
| `search <query>` | Semantic lookup, read-only (no task)              |
| `purge`          | Remove weak memories (`--threshold`, default 0.5) |
| `flush`          | Delete all memories (interactive confirm)         |
| `consolidate`    | Merge similar memories                            |

Read-only commands do not require a loaded embedding model. `search` downloads the model on first use (bounded by the 5-minute `EMBEDDER_INIT_TIMEOUT`; see [Model cache](#model-cache)).

## Settings

In `~/.elph/settings.json`:

| Field                       | Default         | Description                                              |
| --------------------------- | --------------- | -------------------------------------------------------- |
| `models.embed.model`        | `AllMiniLML6V2` | Local embedding model (shared with codegraph)            |
| `models.embed.quantized`    | `true`          | Prefer quantized catalog variant when available          |
| `memory.enabled`            | `true`          | Auto hooks / bootstrap injection                         |
| `memory.topK`               | `8`             | Semantic hits pulled into active per-turn recall         |
| `memory.contextBudgetChars` | `4000`          | Budget for injected memory XML                           |
| `memory.minQueryLength`     | `8`             | Min prompt length (task-like short prompts still recall) |
| `codegraph.enabled`         | `false`         | Agent codegraph tools (CLI always available)             |

Legacy `memory.embedModel` / `memory.embedQuantized` are migrated into `models.embed` on load.

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

## Migrations (implemented)

| Version | Description                                                                              |
| ------- | ---------------------------------------------------------------------------------------- |
| 1       | Core schema (`memories`, `tasks`, `memory_retrievals`, `meta`, all STRICT)               |
| 2       | Fix truncated embedding blobs (reset short embeddings)                                   |
| 3       | Query indexes (`idx_memories_*`, `idx_memory_retrievals_*`, partial pending-embed index) |
| 4       | Turso-native FTS index on `memories.content` (`CREATE INDEX ... USING fts`)              |

Migrations run through the shared `app_migrations` ledger (`apply_set` in
`floppy::core::migration`) — per-version membership, applied once, no `PRAGMA user_version`.
Memory uses band 1–99, codegraph 500–599 (see [`codegraph.md`](./codegraph.md#schema-v1)). The
and no version band.

## Related

- [configuration.md](./configuration.md) — paths and settings
- [cli.md](./cli.md) — `elph memory`
- [agent-runtime.md](./agent-runtime.md) — runtime integration
- [openwiki](../openwiki/quickstart.md) — implementation details
