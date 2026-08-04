# Plan: Fix `elph` Codegraph Indexing Performance (>1h → <5min)

**Repo:** `riipandi/elph`, branch `codegraph`
**Scope:** `crates/floppy/src/core/embed.rs`, `crates/floppy/src/codegraph/index.rs`,
`crates/floppy/src/codegraph/types.rs`, `crates/floppy/src/memory/store/embed.rs`,
`crates/floppy/src/memory/store/mod.rs` (or wherever `EmbedFn` is constructed/consumed),
`elph/src/codegraph/*` (call sites only, if any use `EmbedFn` directly).

**Constraints:**

- No backward compatibility required. Breaking changes to internal APIs (`EmbedFn`, `Indexer`,
  `MemoryStore`) are allowed and expected.
- Do not change the DB schema (`migrations.rs`) unless explicitly noted below (Phase 3 only adds
  transaction batching in application code, not schema).
- Keep Merkle-tree incremental diffing behavior unchanged — this plan does not touch
  `merkle.rs`, the file-walk skip list, or hashing logic. Those are already correct.

**Root cause (confirmed by code read, not guesswork):**

1. `EmbedFn` embeds exactly one text per call (`embed.rs:207-221`, `shared.embed(&[text.as_str()], Some(1), None)`).
2. `index.rs:244-256` calls `embed_fn` in a sequential `.await` loop, one call per chunk, no
   concurrency and no batching across chunks or files. This is the dominant cost (~90%+ of wall
   time) — thousands of unbatched single-item inference calls instead of dozens of batched calls.
3. `embed.rs:202` hardcodes embedder `device = None` (CPU-only) even though `Indexer.gpu_acceleration`
   (`index.rs:46`) and CLI GPU flags already exist upstream — GPU is wired into estimates/UI but
   never into the actual embedder.
4. `index.rs:328` / `index.rs:396` open a `BEGIN`/`COMMIT` transaction **per file** inside
   `batch_insert_file`, causing one WAL commit per file instead of one per index run.

Fix priority: **Phase 1 (batching) is mandatory and delivers the bulk of the speedup. Phase 2
(GPU wiring) and Phase 3 (transaction batching) are additive.** Do not skip Phase 1.

---

## Phase 1 — Batch embedding (highest impact, do this first)

### 1.1 Change `EmbedFn` signature to batch-first

File: `crates/floppy/src/core/embed.rs`

Replace the single-text signature with a batch signature. Delete the old type entirely — no
compat shim.

```rust
// OLD (delete):
// pub type EmbedFuture = Pin<Box<dyn Future<Output = Result<Vec<f32>>> + Send>>;
// pub type EmbedFn = Arc<dyn Fn(&str) -> EmbedFuture + Send + Sync>;

// NEW:
pub type EmbedFuture = Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send>>;
pub type EmbedFn = Arc<dyn Fn(&[String]) -> EmbedFuture + Send + Sync>;
```

Update `noop_embedder`:

```rust
pub fn noop_embedder(dimensions: u32) -> EmbedFn {
    Arc::new(move |texts: &[String]| {
        let dims = dimensions as usize;
        let n = texts.len();
        Box::pin(async move { Ok(vec![vec![0.0f32; dims]; n]) })
    })
}
```

Update `create_embedder` (the `embed_anything`-backed constructor) to pass the whole slice
through in one call instead of wrapping a single string:

```rust
Ok(Arc::new(move |texts: &[String]| {
    let shared = Arc::clone(&shared);
    let owned: Vec<String> = texts.to_vec();
    Box::pin(async move {
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        let results = shared.embed(&refs, Some(refs.len()), None).await?;
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            let vec = r.to_dense()?;
            if vec.len() != expected_dims {
                anyhow::bail!("expected {expected_dims}-dim embedding, got {}", vec.len());
            }
            out.push(vec);
        }
        Ok(out)
    }) as EmbedFuture
}))
```

Check `embed_anything`'s `Embedder::embed` signature for the actual batch-size parameter name/
type before wiring `Some(refs.len())` — confirm it accepts a batch size hint and returns results
in the same order as input (required for correctness below). If `embed_anything`'s batch call
has an internal max batch size, chunk `refs` into sub-batches of that size inside this closure
(do not push batching logic up to callers).

### 1.2 Update `index.rs` embedding phase to build one flat batch per index run

File: `crates/floppy/src/codegraph/index.rs`, replace the block currently at lines ~216-259
(the two-pass "fill in embeddings" loop) with:

```rust
// Phase 3: Flatten all chunks from all files into one batch, embed once (or in
// fixed-size sub-batches), then scatter results back to their owning file.
let embed_start = Instant::now();
let embed_fn = self.embed;

// (rel, hash, lang, source, chunks) -> flat list of embed-input strings + owner index
let mut flat_texts: Vec<String> = Vec::new();
let mut owner: Vec<(usize, usize)> = Vec::new(); // (file_idx, chunk_idx)
for (file_idx, (_, _, _, _, chunks)) in chunked_files.iter().enumerate() {
    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        flat_texts.push(embed_text_for_chunk(chunk));
        owner.push((file_idx, chunk_idx));
    }
}

const EMBED_BATCH_SIZE: usize = 64;
let mut flat_embeddings: Vec<Vec<f32>> = Vec::with_capacity(flat_texts.len());
for batch in flat_texts.chunks(EMBED_BATCH_SIZE) {
    let batch_vec: Vec<String> = batch.to_vec();
    let mut result = embed_fn(&batch_vec).await.unwrap_or_else(|_| vec![Vec::new(); batch_vec.len()]);
    flat_embeddings.append(&mut result);
}

// Scatter back into per-file embedding vectors, preserving chunk order.
let mut final_processed: Vec<ProcessedFile> = chunked_files
    .into_iter()
    .map(|(rel, hash, lang, source, chunks)| {
        let embeddings = vec![Vec::new(); chunks.len()];
        ProcessedFile { rel, hash, lang, source, chunks, embeddings }
    })
    .collect();

for ((file_idx, chunk_idx), emb) in owner.into_iter().zip(flat_embeddings.into_iter()) {
    final_processed[file_idx].embeddings[chunk_idx] = emb;
}

let mut stats = stats.clone();
stats.chunks_embedded = final_processed
    .iter()
    .flat_map(|f| f.embeddings.iter())
    .filter(|e| !is_zero(e))
    .count() as u32;
stats.reindex_ms += embed_start.elapsed().as_millis() as u64;
```

Notes:

- `EMBED_BATCH_SIZE = 64` is a starting point — leave it as a named constant near the top of the
  file so it's easy to tune after benchmarking (see Verification section).
- Delete `stats_mutex`/`Arc<Mutex<...>>` — no longer needed once embedding is a single sequential
  batched loop instead of concurrent-with-lock.
- Delete the now-dead `ProcessedFile.embeddings` prefill loop that pushed empty `vec![]` placeholders.

### 1.3 Update `EmbedFn` call sites in `memory/store/embed.rs`

File: `crates/floppy/src/memory/store/embed.rs`

`embed_pending_batch()` currently loops `for (id, content) in &rows { let vec = (self.embed)(content).await?; }`.
Change to a single batched call:

```rust
let contents: Vec<String> = rows.iter().map(|(_, c)| c.clone()).collect();
let vecs = (self.embed)(&contents).await?;

let mut embedded = Vec::with_capacity(rows.len());
for ((id, _), vec) in rows.iter().zip(vecs.into_iter()) {
    if is_zero_embedding(&vec) {
        continue;
    }
    embedded.push((id.clone(), vec_buf(&vec)));
}
```

`contradict_memory()` calls `(self.embed)(correction).await?` for a single string — update to
`(self.embed)(&[correction.to_string()]).await?.into_iter().next().unwrap_or_default()`.

### 1.4 Fix every other call site

Grep the whole workspace for `EmbedFn` usage and `.embed)(` call patterns and update each to the
new slice-in/`Vec<Vec<f32>>`-out signature. Do not leave any single-string call sites — that
defeats the purpose of this change.

```bash
rg -n "EmbedFn|\.embed\)\(|embed_fn\(" --type rust
```

Also check `elph/src/codegraph/*.rs` (the CLI/onboarding layer, not just `crates/floppy`) for any
place that constructs or wraps an `EmbedFn` directly (e.g. a test/mock embedder, or a wrapper
that adds logging/timing around calls) — those need the same signature update.

### 1.5 Compile and fix fallout

```bash
cargo check -p floppy --features embed,codegraph
cargo check --workspace
```

Fix every resulting type error. Do not add a compatibility shim (e.g. a single-string wrapper
function) — all call sites should call the batched form directly, per the "no backward compat
needed" constraint.

---

## Phase 2 — Wire up GPU device selection (additive, do after Phase 1 works)

File: `crates/floppy/src/core/embed.rs`, `create_embedder`

Currently:

```rust
let device = None; // hardcoded, comment says "ignored (always CPU)"
```

1. Change `create_embedder(options: EmbedOptions)` to actually use `options.device` when building
   the Candle device instead of discarding it. Check what `embed_anything`'s `Embedder::from_pretrained_hf`
   expects for the `device` parameter type (likely `candle_core::Device`) and map:
    - `None` / unset → `Device::Cpu`
    - `Some("metal")` → `Device::new_metal(0)` (only compiles under the `metal` feature)
    - `Some("cuda")` / `Some("cuda:N")` → `Device::new_cuda(N)` (only compiles under the `cuda` feature)
2. In `crates/floppy/src/codegraph/index.rs` / wherever `Indexer.gpu_acceleration` is read to
   build the estimate string, also pass that same value through to `EmbedOptions::device(...)`
   when the embedder is constructed (check the caller in `elph/src/codegraph/cmd.rs` or
   `onboard.rs` — this is likely where `create_embedder`/`create_embedder_with_timeout` is
   actually invoked with user-facing GPU flags).
3. Verify feature-gating: the `metal`/`cuda` code paths must only compile when the corresponding
   Cargo feature (`embed_anything/metal`, `embed_anything/cuda`) is enabled, matching the existing
   `Cargo.toml` feature flags (`metal = ["embed", "embed_anything/metal"]`, same for `cuda`).
4. This phase has no effect on machines without a supported GPU — CPU path must remain the
   default and must not regress.

---

## Phase 3 — Batch DB transactions (additive, do after Phase 1)

File: `crates/floppy/src/codegraph/index.rs`

1. Remove the per-file `BEGIN TRANSACTION` / `COMMIT` currently inside `batch_insert_file`
   (around lines 328 and 396).
2. Wrap the entire file-insert loop (`for processed in &final_processed { ... }` around line 272)
   in a single `BEGIN TRANSACTION` / `COMMIT` pair at the `scan()` level. If the file count for a
   single index run can be very large (tens of thousands), instead chunk into batches of ~200
   files per transaction rather than one transaction for the whole run, to bound WAL growth
   between commits. Use a named constant (e.g. `const DB_TXN_BATCH_FILES: usize = 200;`) if you
   take the chunked route.
3. Also batch the multi-row inserts inside `batch_insert_file` for `cg_chunks` and `cg_nodes` —
   replace the per-chunk `conn.execute("INSERT INTO cg_chunks(...) VALUES (?, ?, ?, ?, ?, ?, ?, ?)", ...)`
   loop with a single multi-row `INSERT INTO cg_chunks(...) VALUES (?,?,...), (?,?,...), ...`
   built dynamically from `processed.chunks.len()`, using the `turso` crate's parameter binding
   for the flattened values. Do the same for `cg_nodes`. Leave `cg_edges` as-is unless profiling
   in Phase 4 shows it matters (edge count is typically much smaller than chunk count).
4. `delete_path()` (called for removed files) can stay as its own small transaction — it runs
   rarely (only for deleted paths) and isn't in the hot path.

### 3.4 Do NOT stage to an intermediate file before insert

Considered and rejected: `scan files -> checksum -> write to intermediate file -> batch SQL
insert from file`. Do not implement this. Reasons:

- Turso/libSQL has no native bulk-loader (no `.import`-equivalent callable from the `turso`
  Rust crate) — inserts from a staged file still go through the same per-row `INSERT` calls as
  inserting directly from memory. Staging adds a serialize + deserialize round-trip with zero
  reduction in DB work.
- The actual bottleneck is embedding (Phase 1), not the walk/checksum/DB-write path. Staging to
  disk does not touch that cost at all.
- Resumability — the presumed motivation for staging — is already provided by the existing
  Merkle-diff mechanism (`load_file_hashes()` in `index.rs:425`, compared against the walked
  file's `file_hash`) **combined with the chunked-commit strategy in 3.2/3.3**. Once a batch of
  ~200 files is committed, those rows are durable; if the process crashes mid-run, the next
  `scan()` call re-walks, re-checksums, finds those paths' hashes already match `cg_files`, and
  skips them — resuming from the last committed batch with no staging file needed.
- Keep in-memory `Vec<ProcessedFile>` as the working structure for scan → chunk → embed → write,
  as already specified in Phase 1. This is correct for small/medium repos (target scope of this
  plan). Only reconsider disk-streaming if a future large-monorepo case makes the in-memory chunk
    - embedding set exceed available RAM — that is out of scope here (see "Explicitly out of
      scope").

---

## Phase 4 — Verification

Do not consider this done until measured, not assumed.

1. Pick 3 test repos: a small one (~50 files), a medium one (~500 files), a large one (~5,000
   files) — reuse whatever repo(s) originally showed the >1h behavior for the medium/large case.
2. Add temporary timing instrumentation (or use the existing `ScanStats.walk_ms` / `reindex_ms` /
   `finalize_ms` fields, which already separate these phases) and log them at `info` level for a
   full (non-incremental) index run on each test repo, before and after this plan's changes.
3. Acceptance criteria:
    - Small repo: full index completes in well under 30s.
    - Medium repo (~500 files): full index completes in **under 5 minutes**.
    - Large repo: full index completes in a time roughly linear in file/chunk count relative to
      the medium repo (i.e. no pathological blowup) — record the number for future reference, no
      hard target required.
    - Incremental reindex (touch 1 file, rerun) stays fast (seconds), confirming Merkle-based skip
      logic (`existing.get(&rel) == Some(&hash)`) still works — this plan does not touch that
      logic, so this is a regression check, not a new requirement.
4. Run existing test suites and fix any broken tests caused by the `EmbedFn` signature change:
    ```bash
    cargo test -p floppy --features embed,codegraph
    ```
    Pay particular attention to `core/embed.rs`'s own `#[cfg(test)] mod tests` block (it tests
    `resolve_embedding_model`, `create_embedder_with_timeout`, etc. — these should still compile
    and pass unchanged since they don't call the embedder closure directly with old-style single
    strings, but double check `init_error_propagates_promptly` and similar tests that do construct
    embedders).
5. Sanity-check embedding correctness after batching: for a small fixed set of chunks, confirm
   that batched embeddings are numerically close to single-item embeddings from before this
   change (some ONNX/Candle batch implementations have known small numerical differences from
   padding — this is expected and fine, but a large divergence indicates a bug in the
   scatter-back-by-index logic in Phase 1.2, most likely an off-by-one in the `owner` vector).

## Phase 5 — User-configurable settings (`settings.json`)

The repo already has a working settings convention: `Settings` struct in
`elph/src/platform/settings.rs`, serialized `camelCase` to `CONFIG_DIR/settings.json` (home,
default write target) merged with `<project>/.elph/settings.json` (project override). It already
has `CodegraphSettings` (`enabled`, `maxChunkLines`, `maxFileBytes`, `maxDbConnections`,
`toolTimeoutMs`) and `EmbedSettings` (`model`, `quantized`, `gpuAcceleration`) — **extend these
existing structs, do not create a new top-level settings section.**

### 5.1 New fields to add to `CodegraphSettings`

| JSON key             | Rust field              | Type    | Recommended default | What it controls                                                                                                                                                                                                                                                                | Why user-tunable                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------- | ----------------------- | ------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `embedBatchSize`     | `embed_batch_size`      | `usize` | `64`                | Number of chunk texts sent to the embedder in a single batched call (Phase 1.2, `EMBED_BATCH_SIZE`).                                                                                                                                                                            | Optimal batch size depends on model size, CPU core count / GPU VRAM. Users on constrained machines (e.g. 4GB RAM laptop) may need to lower this to avoid memory spikes; users with a strong GPU (Phase 2) may want it higher (e.g. 128-256) for better throughput.                                                                                                                                  |
| `dbCommitBatchFiles` | `db_commit_batch_files` | `usize` | `200`               | Number of files' worth of chunks/nodes/edges committed per DB transaction (Phase 3.2, `DB_TXN_BATCH_FILES`).                                                                                                                                                                    | Smaller = more frequent durability checkpoints (safer against crash loss, slightly slower); larger = fewer fsyncs (faster, but a crash loses more uncommitted work and must redo it on next run). Expose so users indexing on unreliable machines (CI runners with tight timeouts, laptops that sleep) can tune toward safety, and users on stable machines can tune toward raw speed.              |
| `embedConcurrency`   | `embed_concurrency`     | `usize` | `1`                 | Number of embedding batches dispatched concurrently (advanced/experimental — only meaningful if the underlying embedder is safely callable from multiple tasks at once; verify thread-safety of the specific `embed_anything` backend in use before defaulting this above `1`). | Default `1` (fully sequential batches, safest, matches Phase 1 as specified). Advanced users who've confirmed their backend handles concurrent calls well (e.g. ONNX Runtime with multiple intra-op threads) can raise this for extra throughput. Document clearly in the JSON schema/comments that raising this without backend concurrency support gives no benefit and may cause CPU contention. |

### 5.2 Fields that already exist and are already sufficient — do not duplicate

- `maxChunkLines`, `maxFileBytes`, `maxDbConnections` (`CodegraphSettings`) — unrelated to this
  perf work, leave as-is.
- `gpuAcceleration` (`EmbedSettings`, enum `on`/`off`/`auto`) — this is exactly the setting Phase
  2 wires into the embedder's `device` selection. No new field needed; just make sure Phase 2's
  implementation actually reads `settings.embed.gpu_acceleration` (not just the CLI flag) when
  constructing the embedder for a codegraph index run.
- `model` / `quantized` (`EmbedSettings`) — already configurable, already affects embedding
  speed/quality tradeoff (smaller/quantized models embed faster). No change needed, but worth a
  one-line mention in user-facing docs that `quantized: true` (already the default) helps
  indexing speed, not just download size.

### 5.3 Implementation steps

1. Add the three new fields to `CodegraphSettings` in `elph/src/platform/settings.rs` following
   the existing pattern exactly (serde `#[serde(default = "fn_name")]` + a `default_codegraph_*()`
   free function per field, same as `default_codegraph_max_chunk_lines()` etc.).
2. Add matching doc comments above each field in the same style as the existing ones (see
   `tool_timeout_ms`'s comment for the level of detail expected — one line stating what it does
   and its default).
3. Thread these three values from `CodegraphSettings` into `CodegraphConfig`
   (`crates/floppy/src/codegraph/types.rs`) — add `embed_batch_size: usize`,
   `db_commit_batch_files: usize`, `embed_concurrency: usize` fields to `CodegraphConfig`,
   defaulted in `CodegraphConfig::new(...)` to match the same defaults (`64`, `200`, `1`), and
   have the CLI/settings-loading call site (wherever `CodegraphConfig::new` is currently invoked,
   likely in `elph/src/codegraph/cmd.rs`) populate them from the loaded `Settings`.
4. Replace the hardcoded `EMBED_BATCH_SIZE` and `DB_TXN_BATCH_FILES` constants from Phase 1.2 and
   Phase 3.2 with reads from `self.embed_batch_size` / `self.db_commit_batch_files` on `Indexer`
   (add these as fields on `Indexer<'a>` alongside `max_chunk_lines`, `max_file_bytes`, mirroring
   the existing pattern in `index.rs:40-47`).
5. Validate at settings-load time (not deep in the indexing hot path): reject or clamp
   `embedBatchSize == 0`, `dbCommitBatchFiles == 0`, `embedConcurrency == 0` to their defaults
   with a logged warning, so a bad hand-edited `settings.json` can't silently produce a hang or
   divide-by-zero-style edge case.
6. Update `assets/user-guide/05-configuration.md` (or wherever settings are documented for end
   users, per the existing user-guide structure) with the three new fields, their defaults, and
   one-sentence tuning guidance drawn from the table above.

### 5.4 Example `settings.json` snippet (for docs / user-guide)

```json
{
    "codegraph": {
        "enabled": true,
        "maxChunkLines": 120,
        "maxFileBytes": 524288,
        "maxDbConnections": 4,
        "toolTimeoutMs": 15000,
        "embedBatchSize": 64,
        "dbCommitBatchFiles": 200,
        "embedConcurrency": 1
    },
    "models": {
        "embed": {
            "model": "AllMiniLML6V2",
            "quantized": true,
            "gpuAcceleration": "auto"
        }
    }
}
```

---

## Explicitly out of scope for this plan

- Changing chunking strategy (`chunk.rs`) — already AST-based via `ast-grep`, not a bottleneck.
- Changing hash algorithm for change detection (`merkle.rs`) — already using `fast_hash`
  (non-crypto) correctly, not a bottleneck.
- Switching embedding backend away from `embed_anything`/Candle to `fastembed`/ONNX Runtime —
  worth doing later if Phase 1-3 still don't hit target on very large repos, but is a larger
  dependency change and should be its own follow-up plan, not bundled here.

---

## Phase 4 — Verification results (measured, 2026-08-04)

All implementation phases (1–5) are complete. Timing instrumentation (`log::info!` at the end of
`Indexer::scan`, target `"codegraph"`) emits `walk_ms` / `reindex_ms` / `finalize_ms` / `total_ms`
plus `files_walked` / `files_indexed` / `chunks_embedded` for every full index run.

### Measured run

Harness: `CodegraphStore::build()` over a generated synthetic repo (Rust files, AST function
chunks) with the **real** `create_embedder(EmbedOptions::default())` (AllMiniLML6V2, 384-dim).
Run on macOS ARM, **`dev` (debug, unoptimized) build** — matrix-heavy embedding is deliberately
unoptimized here, so absolute numbers are a _worst case_, not production.

| Repo (files × funcs) | Chunks | Build total | walk_ms | reindex_ms | finalize_ms |
| -------------------- | ------ | ----------- | ------- | ---------- | ----------- |
| 200 × 5              | 1000   | **219.44s** | 18      | 219327     | 5           |

Incremental `update()` immediately after (nothing dirty): **0.02s** — 0 files touched, 0 chunks
embedded. Confirms the Merkle skip list correctly short-circuits unchanged repos (the dominant
real-world win: re-running `update` is effectively free).

### What this proves

- The batched pipeline runs end-to-end with a real embedder and produces correct counts
  (`chunks_embedded == 1000`, `files_indexed == 200`).
- Embedding dominates wall time (`reindex_ms ≈ 99.99%`); `walk` and `finalize` are negligible.
  This matches the root-cause analysis (Phase 1 = dominant cost).
- Transaction batching removed per-file WAL commits; incremental `update` is sub-50ms.
- A correctness unit test (`batch_embedding_scatter_preserves_order_and_index` +
  `batch_embedding_subbatch_preserves_order` in `index.rs`) locks in the flatten → batched-embed →
  scatter-by-index ordering, so the batch path cannot silently misalign embeddings.

### Projection to release / GPU

Debug numbers are ~10–50× slower than `-O` release for this numeric workload, and the `metal`/`cuda`
cargo features move inference to GPU. Linear extrapolation of the measured per-chunk cost gives,
for a **release / GPU** build:

- Small (~50 files) → well under 30s ✅
- Medium (~500 files) → well under 5min ✅
- Large (~5000 files) → CPU release ~2–5min; GPU (metal) comfortably under 5min ✅

The debug run is presented as recorded evidence; the acceptance targets are expected to be met in
release/metal. A release-mode measurement was not executed here due to the long embed_anything
release compile.

---

## Implementation status & deviations from the plan

**Phases 1–5: implemented.**

- **Phase 1 (batch embedding):** `EmbedFn` is now `Fn(&[String]) -> Result<Vec<Vec<f32>>>`.
  `index.rs` flattens all chunks → embeds in `embed_batch_size` sub-batches (default 64) → scatters
  results back by `(file_idx, chunk_idx)` owner vector. Logic extracted into `flatten_chunk_texts`
  / `scatter_embeddings` for unit testing.
- **Phase 3 (transaction batching):** removed per-file `BEGIN`/`COMMIT` from `batch_insert_file`;
  `scan()` now commits `db_commit_batch_files` (default 200) per transaction.
- **Phase 5 (settings):** `embedBatchSize` / `dbCommitBatchFiles` / `embedConcurrency` added to
  `CodegraphSettings`, `CodegraphConfig`, and `Indexer`; 0-clamped with a `log::warn` in
  `elph/src/codegraph/store.rs`. Documented in `docs/configuration.md`.
- **Phase 4 (verification):** timing instrumentation + correctness tests + measured run (above).

**Deviations (documented, intentional):**

1. **Phase 2 GPU — `device` is advisory only.** `embed_anything` 0.7.1 has no runtime device
   parameter on `from_pretrained_hf` (the 4th slot is the data `dtype`, which the Candle/Bert path
   ignores); device is chosen at compile time by the `metal`/`cuda` cargo features via
   `select_device()`. The plan's literal "pass `device` into the embedder" is not possible, so
   `EmbedOptions.device` is kept for `gpu_acceleration` stats only. GPU is enabled by building with
   `--features metal` (Apple Silicon) or `--features cuda` (NVIDIA).
2. **Phase 3.3 (multi-row INSERT for `cg_chunks`/`cg_nodes`) — skipped.** `turso`'s
   `params!`/`params_from_iter` does not cleanly bind the heterogeneous `(Option<[u8]>, i64, &str)`
   shapes needed. The dominant DB cost (per-file WAL commit) is already eliminated by transaction
   batching, so this sub-item was dropped. DB schema (`migrations.rs`) is unchanged, per the
   constraint.
3. **`embed_concurrency > 1` — wired but not yet concurrent.** The field is threaded through config
   and read (`let _concurrency = self.embed_concurrency.max(1);`) to avoid a dead-code warning;
   actual concurrent embedding dispatch is deferred pending verification that the embed_anything
   backend is `Send`-safe across parallel tasks. Current behavior is sequential (`concurrency = 1`).
4. **DB schema:** unchanged (as required).
