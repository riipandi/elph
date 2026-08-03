# Codegraph — Semantic Codebase Indexing (Research & Design)

Research log and proposed implementation for building **semantic code indexing** into Elph.
Ports the pattern from [glommer/codemogger](https://github.com/glommer/codemogger) (TypeScript)
into Rust, surfaces it as part of the **codegraph** product area, and reconciles it with
the existing **floppy** memory store.

> Status: **Design / research** — no implementation yet.
> Primary constraint: **Elph runs on consumer-class machines** — every decision below
> must hold under limited RAM, CPU, and disk.

---

## Executive Summary

- **Feasible in Rust.** codemogger architecture maps cleanly onto `ast-grep-core`
  (tree-sitter AST traversal) + tree-sitter grammar crates + `turso` (libSQL with FTS5
  and vector functions) + local embeddings.
- **Floppy has no FTS today** — the hybrid FTS + vector pattern is a **new layer**, not an
  optimization of the existing full-scan `vector_distance_cos` retrieval.
- **FTS5 is available** — `libsql-ffi` builds the bundled SQLite with
  `SQLITE_ENABLE_FTS3|FTS5|RTREE|JSON1`, so `turso` can drive FTS5 directly (same core
  codemogger uses).
- **Embedding model: keep all-MiniLM-L6-v2 (default, 384-dim)**. Rejected models:
  `microsoft/unixcoder-base` (not similarity-trained, PyTorch-only, needs custom wrapper) and
  `jina-embeddings-v3` (570M params, CC-BY-NC-4.0, heavy, not loadable via `embed_anything`/Candle).
- **Git history: do not bulk-embed.** Use SHA-256 change detection + FTS5 on commit
  messages + _selective_ embedding, instead of brute-force embedding of every commit/diff.
- **Proposed structure: floppy standalone with two feature-gated modules** — `memory`
  (default) and `codegraph` (optional). Code snippets live in a sibling module, sharing
  a crate-internal DB/embed/paths core, but with **separate, namespaced schema**.

---

## Background

Elph adds a **structural knowledge graph for code reviews** (CLI subcommand `codegraph`).
The CLI already has a full skeleton (`elph/src/cli/codegraph.rs`): `build`, `update`,
`watch`, `status`, `changes`, `eval`, `postprocess` (flows, communities, FTS),
`repos`/`register`/`unregister`, `visualize`, `serve`. The `elph/src/codegraph/mod.rs`
backend is empty/stub-only.

The reference implementation is **codemogger** — a Bun/TypeScript MCP server that indexes
a codebase into an embedded SQLite for instant semantic + keyword search. Its relevant
techniques:

- tree-sitter *_WASM_ grammar chunking (~13 languages), top-level defs, split >150 lines.
- Local embeddings via **all-MiniLM-L6-v2 q8** (Xenova/ONNX).
- **Single Turso embedded SQLite** with FTS + `vector8` (int8-quantized, ~395 B/chunk).
- **Incremental indexing** via SHA-256 file hashes (only changed files re-embedded).
- Hybrid retrieval: FTS5 BM25 keyword + vector distance → reported **25–370×** faster than
  raw `ripgrep` keyword search (codemogger claims; treat as indicative).

Elph also drives a **floppy** memory store (Turso vector search). This design lets one
store bridge the gap between floppy memory and codegraph graph knowledge with minimal
duplication. It also reconciles a third consumer — the per-project transcript — into the same
DB file, so memory, transcript, and graph share a single connection path.

### Prior art: `tirth8205/code-review-graph` (CRG)

Highly relevant reference spec for Elph's `codegraph` area — arguably closer than
codemogger, because CRG is literally a _code-review knowledge graph_ (not a snippet
index). Its CLI surface (`build`, `update`, `watch`, `status`, `changes`, `eval`,
`postprocess` [flows/communities/FTS], `repos`/`register`/`unregister`, `visualize`,
`serve`) matches Elph's stub (`elph/src/cli/codegraph.rs`) almost exactly, so CRG is
likely the conceptual source behind that skeleton.

Relevant signals that independently **validate this design's decisions**:

- Python 3 + Tree-sitter, SQLite at `.code-review-graph/graph.db`, MIT license.
- Incremental updates via SHA-256 file hashes (only changed files re-parsed).
- **FTS5 hybrid search** (keyword `BM25` + vector similarity).
- Default embedding `all-MiniLM-L6-v2` (validates keeping MiniLM as default).
- Embeddings computed from identifiers/signatures/structural context + a bounded
  docstring summary — **not full function bodies**.
- Postprocessing concepts to adopt for Elph's `postprocess` stub: execution flows,
  community detection (Leiden, auto-split), hub/bridge detection (betweenness),
  knowledge-gap analysis.
- Honest benchmarks to set expectations: keyword search ranked **MRR ~0.35**,
  flow-detection is ~recall on JS/Go, and "impact recall 1.0" is graph-derived
  (circular, not a 100% claim); median token savings ~65× vs whole corpus.

Boundaries for Elph: CRG is Python, so it is a **design/architecture reference, not
importable code**; it also ships far more surface than needed (30 MCP tools, daemon,
GitHub Action) — Elph should adopt the core graph + impact radius + FTS ideas, not chase
feature parity.

---

## Verified research findings

### FTS5 availability (Q2)

Floppy's retrieval (`crates/floppy/src/util.rs::retrieval_sql`) is a **pure brute-force
full scan** of `vector_distance_cos(vector32(embedding), vector32(?))`. There is no FTS in
floppy today — confirmed by reading `crates/floppy/src/migrations.rs` (schema V1/V2/V3) and
`crates/floppy/src/query/memories.rs`.

FTS5 **is available** in the `turso` crate: `libsql-ffi/build.rs` compiles the bundled
SQLite with `SQLITE_ENABLE_FTS3`, `SQLITE_ENABLE_FTS5`, `SQLITE_ENABLE_RTREE`,
`SQLITE_ENABLE_JSON1`. So the codemogger pattern (FTS external-content + triggers +
`vector8` int8 + hybrid) can be built as a new layer.

### Embedding model choices (Q3, Q4)

| Model                      | Params    | Dim  | Disk (ONNX)                | Context  | License          | Verdict                                                                                                                                                                                       |
| -------------------------- | --------- | ---- | -------------------------- | -------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `all-MiniLM-L6-v2`         | ~22.7M    | 384  | ~90 MB                     | 256      | **Apache-2.0**   | **Default** — fits consumer CPU, near-instant, used by floppy today                                                                                                                           |
| `microsoft/unixcoder-base` | 124M/125M | 768  | PyTorch-only               | 512      | MIT              | **Rejected** — encoder-based, not similarity-tuned, needs custom `unixcoder.py` wrapper + ONNX conversion; not loadable via `embed_anything`/Candle                                           |
| `jina-embeddings-v3`       | **570M**  | 1024 | fp16 ~1.15GB / fp32 ~2.3GB | 8192 tok | **CC-BY-NC-4.0** | **Rejected as default** — ~25× params / ~13× disk, requires task-LoRA adapters + Matryoshka truncation that `embed_anything`/Candle does not handle; non-commercial license is a hard blocker |

Notes:

- `AllMiniLML6V2Q` is only a catalog alias of `Xenova/all-MiniLM-L6-v2` (ONNX quantized,
  384/32-dim) — **the "Q" suffix is a no-op**; the comparison is really MiniLM vs the others.
- `jina-embeddings-v3` numbers verified from the HF model card (`license: cc-by-nc-4.0`,
  ONNX repo sized 2.29 GB fp32 / 1.15 GB fp16) and `docs.jina.ai` (570M params,
  1024-dim output via last-token pooling, task-LoRA).
- A 13–86% code-model advantage claim (source `bobmatnyc/mcp-vector-search`) could not be
  re-verified (404/410) — treat as **indicative only**.
- Even if a code-specific model were desired, `codegraph` could keep `all-MiniLM-L6-v2`
  as default and allow a heavy model *_opt-in_ behind a flag — never default.

### Embedding git history (Q5)

Brute-force embedding every historic commit/diff is **not viable on consumer machines**.
Design instead:

1. **Change detection without embedding**: SHA-256 file hash stored alongside each chunk;
   re-embed only on hash change (mirrors codemogger incremental indexing).
2. **Selective embedding**: embed only commit _message/subject_ (short text) + diffs of
   commits touching the currently-edited files — not full history.
3. **Temporal knowledge in floppy, not in big vectors**: reuse `MemoryCategory::Work` +
   `elph/src/memory/capture.rs` journaling (`MUTATION_TOOLS`, `paths_from_tool_input`),
   recording `{repo, branch, file_hash, last_commit}` on agent mutations so the agent can
   answer "when did this change" via lightweight FTS/retrieval.
4. **Staleness detection:** store `head_sha` per repo; mark chunks dirty when `HEAD` moves
   or worktree turns dirty (`elph/src/utils/git.rs::read_worktree_stats` already exposes this).

Live `git2` usage in Elph today is worktree-only (`utils/git.rs`: `is_worktree`,
`read_branch`, `read_worktree_stats`, `read_diff_stats`) — no history walk yet. A
git-history helper would add a `Revwalk`-based walk (`log_commits(cwd, limit)` →
`(subject, sha, diff_stat)`) to feed floppy/codegraph.

### Merkle trees (Q6)

The plan uses **flat SHA-256 per-file hashes + `head_sha`** — **not** a Merkle tree. Git
itself is a Merkle DAG, so repo-wide change detection already rides on `head_sha`/commit
hashes, and per-file hashes pinpoint exactly which file changed.

Cost/benefit of a Merkle tree here:

|                           | Full binary Merkle tree                   | Cumulative fingerprint         |
| ------------------------- | ----------------------------------------- | ------------------------------ |
| Build                     | O(n) hashing                              | O(n) hashing                   |
| Update one leaf           | O(log n) if siblings stored; else O(n)    | O(n) fold (fast at repo scale) |
| Membership proof          | Yes                                       | No (not needed here)           |
| Implementation cost       | Moderate (sorted leaves, sibling storage) | Very low                       |
| Value at small repo scale | Marginal                                  | Cheap, sufficient              |

Because Elph targets consumer machines and repo/index sizes are typically hundreds to low
thousands of files, recomputing a cumulative fold is cheap (`µs–ms`). A custom binary
Merkle tree adds storage + ordering complexity for negligible gain today.

**Recommendation:** if an index _fingerprint_ is ever needed — comparing snapshots, DB
hand-offs, non-git projects, or the multi-repo registry — use a **cumulative fingerprint**
`root = SHA256(sorted concat of per-file hashes)` stored in `cg_meta`, rather than a full
binary Merkle tree. This is optional and out of the core build.

---

## Components / libraries (rationale)

### Shared core (both features)

| Layer      | Crate               | Feature-gate | Why                                                            |
| ---------- | ------------------- | ------------ | -------------------------------------------------------------- |
| Store      | `turso` (workspace) | always       | libSQL driver; embedded FTS5 + `vector_distance_cos`           |
| Embeddings | `embed_anything`    | `embed`      | Candle/HF fast local models; default `AllMiniLML6V2` (384-dim) |
| Errors     | `anyhow`            | –            | Already present                                                |

### `memory` (existing — no new deps)

`chrono`, `memorable-ids`, `kalid`, `parking_lot`, `rand`, `serde`, `serde_json`, `tokio`.
No change to the working feature.

### `codegraph` (new, feature-gated)

| Library                                  | Feature-gate | Role                                                                                                                                                       |
| ---------------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`ast-grep-core`** (0.45, verified)     | `codegraph`  | AST traversal: `children()`, `dfs()`, `field()`, `find_all()`, `kind()`, `range()` (byte offsets), `get_inner_node()` — the core of top-level-def chunking |
| **tree-sitter grammar crates** (`^26.3`) | `codegraph`  | Native grammars wired to `ast-grep` via the `Language` trait (per-language: rust, python, go, js, ts, …)                                                   |
| **`sha2`**                               | `codegraph`  | SHA-256 file hash → incremental re-embedding                                                                                                               |
| **`ignore`**                             | `codegraph`  | File walking honoring `.gitignore` (skips `.git`, `target`, etc.)                                                                                          |
| **`rayon`** (optional)                   | `codegraph`  | Parallel chunking/embedding for large repos — not required by default                                                                                      |

---

## Proposed design: floppy standalone, two features

### 1. Final (feature gates in `crates/floppy/Cargo.toml`)

```toml
[features]
default   = ["memory"]             # out-of-box = memory store only
memory    = []
embed     = ["dep:embed_anything"] # vectorization foundation (can be Noop)
codegraph = ["memory", "embed", "dep:ast-grep-core", "dep:tree-sitter", "dep:sha2", "dep:ignore"]
```

- `codegraph` **not** default-on (consumer resource bound; heavy ast-grep/grammars must
  not load for plain memory users).
- `embed` stays separate from `memory` (embedding can be a Noop `embedder` — same as
  today's `noop_embedder`).

### 2. Module layout

```
crates/floppy/src/
  lib.rs          # pub mod memory; #[cfg(feature="codegraph")] pub mod codegraph;
  db.rs           # [NEW, crate-internal] Turso open/with-db (retry/backoff, WAL cleanup)
  util.rs         # shared (unchanged): vec_buf, VALID_EMBEDDING_BYTES, is_zero
  embed.rs        # shared (unchanged): create_embedder, resolve_embedding_model, DEFAULT_EMBED_MODEL
  paths.rs        # shared (unchanged): FloppyPaths, DEFAULT_DATA_DIR, DB_FILE_NAME
  memory/         # [MOVED] current modules, unchanged logic
    mod.rs builder.rs migrations.rs store.rs query/ scoring.rs report.rs types.rs
  codegraph/      # [NEW] #[cfg(feature="codegraph")]
    mod.rs builder.rs migrations.rs index.rs search.rs types.rs
```

### 3. Shared db core (`db.rs`)

Extract `MemoryStore::open_db()` / `with_db()` (retry + Jittered backoff, WAL sidecar
cleanup, `PRAGMA busy_timeout`) into a crate-internal helper reused by both features:

```rust
pub(crate) async fn open_local_db(db_path: &str) -> Result<Database>;
pub(crate) async fn with_db<T, F, Fut>(db_path: &str, f: F) -> Result<T>
  where F: FnOnce(Connection) -> Fut, Fut: Future<Output = Result<T>>;
```

This is the concrete benefit of the "floppy standalone with two features" decision:
`pub(crate)` is shareable because memory and graph are sibling modules in the **same
crate** (`crates/floppy`).

### 4. codegraph modules (concise)

- **`types.rs`**: `CodegraphConfig { db_path, embed: Option<EmbedFn>, apply_migrations }`;
  `Chunk { path, kind, start_line, end_line, content, vector: Option<Vec<f32>> }`;
  `ChunkKind` set; `ScanStats { files_walked, skipped, chunks_indexed, embedded, bytes }`.
- **`migrations.rs`** — separate, `eg`-namespaced schema:

    ```sql
    CREATE TABLE IF NOT EXISTS cg_chunks_v1 (
      id INTEGER PRIMARY KEY, path TEXT NOT NULL, kind TEXT NOT NULL,
      start_line INTEGER, end_line INTEGER, content TEXT NOT NULL, file_hash TEXT NOT NULL,
      vector BLOB);                         -- f32 LE (vec_buf); nullable when embed off
    CREATE VIRTUAL TABLE IF NOT EXISTS cg_fts USING fts5(
      content, content='cg_chunks_v1', content_rowid='id', tokenize='unicode61');
    -- external-content triggers: cg_chunks_v1_ai / _ad / _au keep cg_fts in sync
    CREATE INDEX IF NOT EXISTS cg_chunks_v1_idx_path ON cg_chunks_v1(path);
    ```

    (Optionally `cg_files(path PRIMARY KEY, file_hash)` and `cg_meta` version row.)

- **`index.rs`** — one pipeline shared by `build`/`update`:
  `walk (ignore::WalkBuilder) → skip binary/.gitignored → sha256 → if hash unchanged skip
→ ast-grep parse → top-level def chunks (split >150 lines) → embed each chunk →
upsert `cg_chunks_v1`+ FTS trigger → drop chunks for gone paths →`ScanStats`.
`language_for(path)`maps extension → concrete tree-sitter`Language` impl.
- **`search.rs`**: hybrid = FTS5 `BM25` over `cg_fts` (primary) + `vector_distance_cos`
  against the query embedding (augment), merged + ranked → `Vec<(Chunk, score)>`.

### 5. Wiring into `elph`

- `elph/src/cli/codegraph.rs` handlers call `floppy::codegraph::{…}` for
  `build`/`update`/`status`/`postprocess` etc.
- git introspection stays in `elph/` (`git2`, `utils/git.rs`) — host concern, not a
  store concern. A `git_history` helper adds `Revwalk`-based commit/diff feeding.

---

## Reasoned trade-offs / decisions to avoid

| Decision                                                                                    | Rationale                                                                                                                                                          |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Do **not** refactor existing flat `floppy` modules into `memory/` unless two features exist | Avoid cosmetic churn; `memory/` is justified _only_ because it becomes a peer module of `codegraph`                                                                |
| Keep codegraph schema/migrations **separate** from memory                                   | Different domain, different lifecycle; independent enabling                                                                                                        |
| Shared DB file vs two files                                                                 | **Chosen: single `store.db`** for both `mem_*` and `cg_*` to minimize open connections; mandates strict `cg_*` namespacing + a feature-combo-safe migration runner |
| Default model = MiniLM (Apache-2.0), no embedding-heavy default                             | Consumer hardware bound                                                                                                                                            |
| `jina-v3` / code-specific models only as opt-in                                             | License + memory + `embed_anything` limitations                                                                                                                    |

---

## Recommendations

Each recommendation maps a **finding** to a concrete **position** to take when building.

| #   | Finding (temuan)                                                                                                      | Recommendation (rekomendasi)                                                                                                                                                                      |
| --- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Floppy has **no FTS**; codemogger uses FTS5 + `vector8` + hybrid for 25–370× keyword search                           | Build FTS5 external-content + triggers + `vector8` as a **new layer** — do not treat it as optimizing the existing full-scan retrieval                                                            |
| 2   | FTS5 **is available** via `libsql-ffi` compile flags                                                                  | Use `turso` + raw FTS5 SQL (same core codemogger uses); no extra C dep needed                                                                                                                     |
| 3   | `all-MiniLM-L6-v2` is Apache-2.0, 384-dim, ~90 MB, near-instant on CPU                                                | **Keep as default embedder** for both memory and codegraph                                                                                                                                        |
| 4   | `unixcoder-base`: encoder, not similarity-tuned, PyTorch-only, needs custom wrapper                                   | **Reject as default**; only consider for code-specific graph if converted + tuned, never as floppy default                                                                                        |
| 5   | `jina-embeddings-v3`: 570M, CC-BY-NC-4.0, ONNX ~1.15 GB, needs task-LoRA + Matryoshka handling `embed_anything` lacks | **Reject as default**; at most expose as a heavy opt-in behind a flag, and flag the non-commercial license                                                                                        |
| 6   | Bulk-embedding git history is too heavy for consumer hardware                                                         | **Do not bulk-embed history** — SHA-256 change detection + FTS5 on commit messages + selective embedding of work-relevant diffs                                                                   |
| 7   | `ast-grep-core` + tree-sitter grammars provide the AST chunking codemogger uses                                       | Gate them under a **`codegraph` feature** so default builds stay lean and RAM-light                                                                                                               |
| 8   | Memory and graph are different domains with different lifecycle                                                       | Keep **separate schemas/migrations** (`mem_*` vs `cg_*`), but **co-locate both in a single `store.db`** to minimize open connections; share the crate-internal `db.rs`/`embed.rs`/`paths.rs` core |
| 9   | Merkle tree solves repo-wide integrity, but git already provides a Merkle DAG (`head_sha`)                            | Do **not** build a binary Merkle tree in core; add only an optional cumulative fingerprint (`SHA256` of sorted per-file hashes) in `cg_meta` if snapshot/DB comparison is ever needed             |

### Spin-out directives (in order of work)

1. **Don't change behavior** — the old floppy modules become `memory/` as an API move, no logic edits.
2. **Extract `db.rs`** (open/with-db retry + WAL cleanup) so both features share one connection path.
3. **Land the feature split first** (`default = ["memory"]`, `codegraph` off-by-default) so `cargo check` stays green at every step.
4. **Then** add chunking (index) + hybrid search (search) behind the flag.
5. **Git history** lands in `elph/` (host concern) via a new `Revwalk` helper, wired into floppy/codegraph data — never bulk-embedded.

---

## Decisions (confirmed before build)

1. **`memory` is the default feature** — `default = ["memory"]`. Floppy's identity is the
   memory store; it must work out-of-the-box without heavy deps.
2. **`codegraph` is off-by-default** — ast-grep-core, tree-sitter grammars, `sha2`, and
   `ignore` load only when `--features codegraph` is enabled (consumer-resource constraint).
3. **Single shared DB file `store.db`** — both memory (`mem_*`) and codegraph (`cg_*`)
   tables live in one libSQL file, minimizing open connections and simplifying backup.
   A feature-safe migration runner gates which schema applies (memory-only vs both), and
   one shared `db.rs` connection path keeps connection/file-handle churn minimal.
4. **v1 grammar set** — `rust`, `python`, `go`, `typescript` (`.ts` + `.tsx`),
   `javascript`. Covers Elph's own ecosystem plus common languages; additional grammars
   are additive via the same `language_for(path)` map.
5. **Embedding default stays `all-MiniLM-L6-v2` for both memory and codegraph** — heavy
   models (e.g. `jina-embeddings-v3`, code-specific) are exposed only as opt-in behind a
   flag/env var, with the CC-BY-NC-4.0 license caveat documented. No dimension mismatch:
   both stores run at 384-dim.
6. **Migration mechanism: no `app_migrations` ledger table.** For a non-critical dev store
   the ledger is ceremony. Use `PRAGMA user_version` (single built-in integer per file) +
   fully idempotent/additive DDL + an `ALTER TABLE ADD COLUMN` guard (`PRAGMA table_info`).
   One composed runner per DB file; module version bands prevent collisions within a file
   (memory 1–99, transcript 100–199, host 200–299, codegraph 500–599).
7. **Project DB consolidation: transcript merges into `store.db`.** The per-project
   transcript cache leaves `<project>/.elph/metadata.db` and lands in `store.db` beside the
   memory tables. Global `~/.local/share/elph/metadata.db` (sessions, goals, session tree)
   stays a separate, machine-global file with its own independent migration sequence.
8. **Floppy memory gains an FTS5 + hybrid retrieval layer.** Add an external-content FTS5
   table over `memories` + triggers (floppy migration V4), candidate-then-rerank vector
   search, and a BM25 + vector hybrid. The current vector-only path stays as fallback until
   the hybrid is proven on consumer-class hardware.

---

## Store DB architecture (single project file)

**Chosen layout after consolidation:**

| File                                       | Tables                                                                          | Migration owner                                                                        |
| ------------------------------------------ | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `~/.local/share/elph/metadata.db` (global) | sessions, goals, skill_cache, agent_spawn_edges, session tree                   | host `metadata_migrations()` (v1–8) + elph-agent (v100); **unchanged, machine-global** |
| `<project>/.elph/store.db`                 | floppy memory (`mem_*`) + transcript (`elph_transcript_*`) + codegraph (`cg_*`) | one composed, feature-gated runner                                                     |
| `<project>/.elph/metadata.db`              | (retired) transcript snapshot                                                   | merged into `store.db`                                                                 |

The per-project transcript cache no longer gets its own file; `store.db` becomes the sole
project DB (fewest open connections, one backup unit). Memory and codegraph keep separate
namespaced schemas (`mem_*` vs `cg_*`); the transcript tables use their own namespace so the
three consumers coexist without table collisions.

### Migration mechanism (no ledger table)

The `app_migrations` ledger table is **dropped**. For a non-critical dev store, the
`MAX(version)` ledger is ceremony. Replaced by:

- **`PRAGMA user_version`** — single built-in integer per DB file = highest applied schema.
  Fast-path: `current >= target` → skip.
- **Idempotent, additive DDL** — `CREATE ... IF NOT EXISTS` everywhere, so re-running is
  safe; a crash mid-run self-heals on the next open (no transaction needed).
- **`ALTER TABLE ADD COLUMN` guard** — not idempotent in SQLite; each is gated by a
  `PRAGMA table_info(<table>)` existence check before adding.
- **One composed runner per file** — the host concatenates the active modules' slices
  (memory-only vs memory+transcript → +codegraph) and applies them in version order.

### Version bands (module namespaces, per file)

`user_version` is one integer per file, so modules must not collide:

| Band    | Owner                          |
| ------- | ------------------------------ |
| 1–99    | floppy `memory` (V1..V3 today) |
| 100–199 | transcript (Elph)              |
| 200–299 | host / Elph-specific store     |
| 500–599 | floppy `codegraph` (`cg_*`)    |

`codegraph` sits high so it can be feature-disabled without touching the memory/transcript
schema. The global `metadata.db` runs its own independent sequence (v1–8 + 100) with the
same mechanism, in a separate file — no shared ledger across files.

---

## Floppy memory retrieval optimization (FTS5 + vector)

### Current state (verified in code)

- **No FTS.** Floppy retrieval (`crates/floppy/src/util.rs::retrieval_sql`) is a
  brute-force full scan: `vector_distance_cos(vector32(embedding), vector32(?))` is computed
  for every row, then `ORDER BY (1.0 - distance) * POWER(decay, …)` — distance computed
  twice, temp-b-tree sort over all rows, decay baked into the ORDER BY (unindexable).
- **Vector** stored as f32 blob (384 dims → 1536 B); `VectorType::Vector32` default.
- **FTS5 is available** — `libsql-ffi` compiles with
  `SQLITE_ENABLE_FTS3|FTS5|RTREE|JSON1`.
- **No side indexes / triggers** exist to keep an FTS or vector index consistent across
  insert/update/delete/merge/purge/flush.

### Optimization plan (phased)

**Phase 1 — FTS5 keyword layer** (largest win, guaranteed available). External-content FTS5
over `memories`:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    id UNINDEXED, category UNINDEXED, content,
    tokenize = "porter unicode61"
);
-- AFTER INSERT / UPDATE / DELETE triggers on memories keep memories_fts in sync
```

BM25 ranking (`bm25(memories_fts)`) gives lexical search; codemogger reports 25–370× faster
keyword retrieval than the existing scan.

**Phase 2 — Vector search (maximal).** Move decay out of the SQL `ORDER BY`; compute
similarity once per candidate and apply decay in the re-rank step (candidate-then-rerank:
fetch `candidate_multiplier × top_k`, re-rank). Optionally store `vector8` (int8 quantized,
384 B) for large corpora; default stays f32 for quality. Probe whether the bundled `turso`
engine supports native vector columns / `vec0`-style KNN; if so, feature-gate it and fall
back to brute-force otherwise.

**Phase 3 — Hybrid retrieval.** Merge FTS5 BM25 + vector distance (reciprocal-rank fusion
or weighted score) in `MemoryStore::search_memories` and the task-retrieval path. Keep the
legacy vector-only path as fallback.

**Phase 4 — Consistency & migration.** Ensure all write paths (write, consolidate, decay,
purge, flush, penalize) keep `memories_fts` in sync via triggers. New floppy migration
**V4** creates the FTS table + triggers + backfill, versioned under band 1–99 and coordinated
with the migration mechanism above. Unit tests assert FTS row count matches `memories`
after every mutation.

---

## References

- codemogger: `glommer/codemogger` (GitHub)
- code-review-graph (CRG): `tirth8205/code-review-graph` (session — Python/Tree-sitter;
  graphs, blast-radius, flows, Leiden communities, FTS5 hybrid)
- memelord → floppy: `crates/floppy` (this repository), schema V1–V3
- `libsql-ffi/build.rs` — SQLite compile flags (FTS3/FTS5/RTREE/JSON1)
- `docs.rs/ast-grep-core` 0.4x — Node API verified (children, dfs, field, find_all, range)
- HF model cards: `sentence-transformers/all-MiniLM-L6-v2`, `jinaai/jina-embeddings-v3`,
  `microsoft/unixcoder-base`
- `jina.ai` docs (Embeddings API, model sizes) and README (`license: cc-by-nc-4.0`)
- Elph source: `elph/src/cli/codegraph.rs`, `elph/src/codegraph/mod.rs`,
  `elph/src/utils/git.rs`, `elph/src/memory/capture.rs`
