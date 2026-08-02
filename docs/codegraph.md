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

Elph ads a **structural knowledge graph for code reviews** (HCI subcommand `codegraph`).
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
store brand the gap between floppy memory and codegraph graph knowledge with minimal
duplication.

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

| Decision                                                                                    | Rationale                                                                                                                                                                                                               |
| ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Do **not** refactor existing flat `floppy` modules into `memory/` unless two features exist | Avoid cosmetic churn; `memory/` is justified _only_ because it becomes a peer module of `codegraph`                                                                                                                     |
| Keep codegraph schema/migrations **separate** from memory                                   | Different domain, different lifecycle; independent enabling                                                                                                                                                             |
| Shared DB file vs two files                                                                 | Prefer **separate files** (memory `store.db`, graph `.codegraph/graph.db`) for feature independence; if a shared Turso file is required, strict `cg_*` namespacing + a feature-combo-safe migration runner are mandates |
| Default model = MiniLM (Apache-2.0), no embedding-heavy default                             | Consumer hardware bound                                                                                                                                                                                                 |
| `jina-v3` / code-specific models only as opt-in                                             | License + memory + `embed_anything` limitations                                                                                                                                                                         |

---

## Recommendations

Each recommendation maps a **finding** to a concrete **position** to take when building.

| #   | Finding (temuan)                                                                                                      | Recommendation (rekomendasi)                                                                                                                                |
| --- | --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Floppy has **no FTS**; codemogger uses FTS5 + `vector8` + hybrid for 25–370× keyword search                           | Build FTS5 external-content + triggers + `vector8` as a **new layer** — do not treat it as optimizing the existing full-scan retrieval                      |
| 2   | FTS5 **is available** via `libsql-ffi` compile flags                                                                  | Use `turso` + raw FTS5 SQL (same core codemogger uses); no extra C dep needed                                                                               |
| 3   | `all-MiniLM-L6-v2` is Apache-2.0, 384-dim, ~90 MB, near-instant on CPU                                                | **Keep as default embedder** for both memory and codegraph                                                                                                  |
| 4   | `unixcoder-base`: encoder, not similarity-tuned, PyTorch-only, needs custom wrapper                                   | **Reject as default**; only consider for code-specific graph if converted + tuned, never as floppy default                                                  |
| 5   | `jina-embeddings-v3`: 570M, CC-BY-NC-4.0, ONNX ~1.15 GB, needs task-LoRA + Matryoshka handling `embed_anything` lacks | **Reject as default**; at most expose as a heavy opt-in behind a flag, and flag the non-commercial license                                                  |
| 6   | Bulk-embedding git history is too heavy for consumer hardware                                                         | **Do not bulk-embed history** — SHA-256 change detection + FTS5 on commit messages + selective embedding of work-relevant diffs                             |
| 7   | `ast-grep-core` + tree-sitter grammars provide the AST chunking codemogger uses                                       | Gate them under a **`codegraph` feature** so default builds stay lean and RAM-light                                                                         |
| 8   | Memory and graph are different domains with different lifecycle                                                       | Keep **separate schemas/migrations** (`mem_*` vs `cg_*`) and prefer **separate DB files**; share only the crate-internal `db.rs`/`embed.rs`/`paths.rs` core |

### Spin-out directives (in order of work)

1. **Don't change behavior** — the old floppy modules become `memory/` as an API move, no logic edits.
2. **Extract `db.rs`** (open/with-db retry + WAL cleanup) so both features share one connection path.
3. **Land the feature split first** (`default = ["memory"]`, `codegraph` off-by-default) so `cargo check` stays green at every step.
4. **Then** add chunking (index) + hybrid search (search) behind the flag.
5. **Git history** lands in `elph/` (host concern) via a new `Revwalk` helper, wired into floppy/codegraph data — never bulk-embedded.

---

## Open questions (to settle before build)

1. Default `memory` feature? (Recommended: yes, `default = ["memory"]`)
2. `codegraph` off-by-default feature? (Recommended: yes)
3. Shared single DB vs two separate DB files?
4. Which extension → grammar set for v1 chunking (`rust`, `python`, `go`, `javascript`,
   `typescript`, …)?
5. Flip default `accel-anything` embedding: keep MiniLM for memory AND graph, or allow a
   heavier opt-in model behind a flag?

---

## References

- codemogger: `glommer/codemogger` (GitHub)
- memelord → floppy: `crates/floppy` (this repository), schema V1–V3
- `libsql-ffi/build.rs` — SQLite compile flags (FTS3/FTS5/RTREE/JSON1)
- `docs.rs/ast-grep-core` 0.4x — Node API verified (children, dfs, field, find_all, range)
- HF model cards: `sentence-transformers/all-MiniLM-L6-v2`, `jinaai/jina-embeddings-v3`,
  `microsoft/unixcoder-base`
- `jina.ai` docs (Embeddings API, model sizes) and README (`license: cc-by-nc-4.0`)
- Elph source: `elph/src/cli/codegraph.rs`, `elph/src/codegraph/mod.rs`,
  `elph/src/utils/git.rs`, `elph/src/memory/capture.rs`
