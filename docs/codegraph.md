# Codegraph — Semantic Codebase Indexing

Research log and implementation design for **semantic code indexing** in Elph.
Ports patterns from [glommer/codemogger](https://github.com/glommer/codemogger) (TypeScript)
into Rust, adopts selective ideas from [code-review-graph](https://github.com/tirth8205/code-review-graph),
and co-locates storage with **floppy** memory in project `store.db`.

> Status: **Implementation in progress** (validated design locked).
> Constraint: **consumer-class machines** — limited RAM, CPU, disk.

---

## Locked product decisions (validation)

| #   | Topic           | Decision                                                                                                                                                                |
| --- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Product         | **Codemogger base** + selective CRG concepts                                                                                                                            |
| 2   | Canonical unit  | **Chunk-first**; graph nodes/edges are **derived**                                                                                                                      |
| 3   | Merkle          | **File-only** leaves; for **reindex invalidation + snapshot fingerprint**; no remote cache/sync                                                                         |
| 4   | Index payload   | **Full chunk body** for embed **and** FTS                                                                                                                               |
| 5   | Storage         | All project data in **`<project>/.elph/store.db`** (memory + transcript + codegraph)                                                                                    |
| 6   | Concurrency     | **Logical partition + Turso MVCC**: domains may write in parallel (row-disjoint); **coordinated writer inside codegraph build**; Merkle root updated at **end of scan** |
| 7   | Graph v1        | **Minimal**: nodes from chunks, shallow edges (import heuristics), **impact BFS**; no multi-repo; no flows/communities/hub                                              |
| 8   | `ast-grep`      | **Chunking** via ast-grep; edges **heuristic/shallow**                                                                                                                  |
| 9   | Chunk rules     | Top-level defs; **split if > ~150 lines**                                                                                                                               |
| 10  | Retrieval       | **Parallel hybrid**: FTS5 BM25 + vector → RRF/weighted merge                                                                                                            |
| 11  | Freshness       | Explicit `build`/`update` + **dirty reindex on demand before search** (no fs watch in v1)                                                                               |
| 12  | Languages (AST) | **Tier-1:** Python, C, C++, Java, C#, JS, TS/TSX, Rust, Go, Elixir. **SQL:** text fallback (FTS/vector). **Tier-2** opt-in later. Unknown → text fallback               |
| 13  | Embed model     | **Single shared MiniLM** via **`models.embed`** (`model`, `quantized`) for memory + codegraph                                                                           |
| 14  | Surface         | **CLI always**; agent tools only when **`codegraph.enabled`** (default **false**). Tools: `code_search`, `code_impact`, `code_status`, `code_reindex`. Build/purge CLI-only |

### Agent loop (target)

```text
code_status → (empty? user runs: elph codegraph build)
code_search → dirty reindex internal → hybrid hits (path + range + snippet)
read_file(path, range)  // host tool, targeted
code_impact?            // optional
code_reindex?           // optional after large refactors
```

### CLI surface (v1)

| Command  | Role                                       |
| -------- | ------------------------------------------ |
| `build`  | Full index (CLI-only)                      |
| `update` | Dirty reindex                              |
| `status` | Stats + Merkle fingerprint                 |
| `search` | Hybrid search                              |
| `impact` | Shallow blast radius                       |
| `purge`  | Clear codegraph tables                     |

No multi-repo, watch, serve, eval, postprocess, or visualize subcommands in v1.

### Settings (`settings.json`)

```json
{
  "models": {
    "embed": {
      "model": "AllMiniLML6V2",
      "quantized": true
    }
  },
  "codegraph": {
    "enabled": false
  }
}
```

| Key | Default | Meaning |
| --- | ------- | ------- |
| `codegraph.enabled` | `false` | Register agent tools for the coding session |
| `models.embed.model` | `AllMiniLML6V2` | Local embedder (same as floppy memory) |
| `models.embed.quantized` | `true` | Prefer quantized ONNX weights |

Enable agent indexing tools:

```json
{ "codegraph": { "enabled": true } }
```

CLI (`elph codegraph build|update|status|search|impact|purge`) does **not** require `codegraph.enabled`.

### Startup onboarding (pre-TUI)

When launching the interactive TUI (`elph` with no subcommand):

1. Detect first Elph access to the project (`.elph/` missing before ensure).
2. If **`codegraph.enabled`** is true, the terminal is interactive, the index is empty, and the user has not declined before → show an interactive prompt (inquire Select: **Yes!** / **Skip**).
3. **Yes!** runs `build` with a `CliSpinner` progress line (files reindexed / path).
4. **Skip** writes `.elph/codegraph_index_declined` so the prompt does not repeat (delete the file to be asked again).

Skipped when: non-TTY, `ELPH_QUIET` / `CI`, `codegraph.enabled=false`, index already has files, or declined marker present.

---

## Executive summary (architecture)

- **Rust stack:** `ast-grep-core` + `ast-grep-language` (feature-gated grammars) + `turso` (FTS5 + `vector_distance_cos`) + shared floppy embedder.
- **Floppy feature:** `codegraph` (optional; pulls parsers + walk/hash deps). Enable from Elph as `features = ["embed", "codegraph"]`.
- **Schema namespace:** `cg_*` tables in the same `store.db` as memory.
- **Incremental:** SHA-256 **per file**; re-parse/re-embed only when file hash changes.
- **Merkle root:** `SHA256` over canonical sorted `(path, file_hash)` pairs — fingerprint + dirty detection vs worktree.

---

## Background / prior art

### codemogger

- tree-sitter chunking (top-level defs, split large bodies).
- Local MiniLM embeddings.
- SQLite FTS + vectors; incremental file hashes.
- Hybrid keyword + semantic search.

### code-review-graph (CRG)

Design reference only (Python). Adopted for Elph v1: shallow impact radius, FTS hybrid mindset.
**Not** adopted in v1: multi-repo registry, Leiden communities, flow postprocess, 30 MCP tools, daemon.

---

## Schema (v1)

```sql
-- File-level Merkle leaves
CREATE TABLE IF NOT EXISTS cg_files (
  path TEXT PRIMARY KEY,
  file_hash TEXT NOT NULL,
  lang TEXT,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS cg_chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL,
  kind TEXT NOT NULL,
  name TEXT,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  content TEXT NOT NULL,
  file_hash TEXT NOT NULL,
  embedding BLOB
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS cg_fts USING fts5(
  content, path, name, kind,
  content = 'cg_chunks',
  content_rowid = 'id',
  tokenize = 'unicode61'
);

CREATE TABLE IF NOT EXISTS cg_nodes (
  id TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  name TEXT,
  kind TEXT NOT NULL,
  start_line INTEGER,
  end_line INTEGER
) STRICT;

CREATE TABLE IF NOT EXISTS cg_edges (
  src TEXT NOT NULL,
  dst TEXT NOT NULL,
  kind TEXT NOT NULL,
  PRIMARY KEY (src, dst, kind)
) STRICT;

CREATE TABLE IF NOT EXISTS cg_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
```

External-content FTS triggers keep `cg_fts` in sync on chunk insert/update/delete.

Migration band for codegraph DDL: **500+** in the store’s migration ledger (alongside floppy memory 1–99), until a full `PRAGMA user_version` consolidation lands.

---

## Index pipeline

```text
walk (ignore::WalkBuilder, gitignore)
  → skip binary / vendor / minified / lockfiles / oversized (>512 KiB)
  → sha256(file)
  → if hash == cg_files.hash: skip
  → chunk:
       AST top-level defs, split >120 lines, max 48 chunks/file
       store body capped (~6k chars); skip bulk json/yaml/toml fallback
       OR sql/md text fallback (minified → short digest only)
  → import-edge heuristics (regex) → cg_edges (file-level)
  → embed compact text (path+kind+name+truncated body ≤~1.8k chars); FTS uses stored body
  → upsert cg_chunks + cg_nodes; update cg_files
  → drop paths gone from worktree
  → write Merkle root to cg_meta
```

---

## Hybrid search

1. Optionally dirty-reindex files whose hash changed.
2. FTS5 BM25 over `cg_fts` → top-k₁.
3. Vector cosine over `cg_chunks.embedding` → top-k₂.
4. Merge via **reciprocal rank fusion** (RRF).
5. Return `{ path, start_line, end_line, kind, name, score, snippet, source }`.

---

## Feature flags (`crates/floppy`)

Domain layout: `floppy::core` (always) · `floppy::memory` (default) · `floppy::codegraph` (optional).

```toml
[features]
default   = ["memory"]
memory    = []
embed     = ["dep:embed_anything"]
codegraph = [
  "dep:ast-grep-core",
  "dep:ast-grep-language",
  "dep:ignore",
  "dep:sha2",
  "dep:regex",
]
full      = ["memory", "embed", "codegraph"]
```

`codegraph` does **not** force `embed`; hosts pass a real or noop `EmbedFn`. Elph uses `features = ["full"]`.

---

## Concurrency (Turso)

- Prefer domain isolation: memory/transcript rows vs `cg_*` rows.
- Inside index build: single logical writer; commit per file (or small batches); **meta root once at end**.
- Optional later: `PRAGMA journal_mode='mvcc'` + `BEGIN CONCURRENT` + retry on row conflict (experimental).

---

## References

- codemogger: `glommer/codemogger`
- code-review-graph: `tirth8205/code-review-graph`
- floppy: `crates/floppy`
- Cursor-style indexing writeups (chunk + embedding + incremental) — design context only
- Turso concurrent writes / MVCC docs
- `ast-grep-core` / `ast-grep-language` 0.45
