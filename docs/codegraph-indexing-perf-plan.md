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

## Explicitly out of scope for this plan

- Changing chunking strategy (`chunk.rs`) — already AST-based via `ast-grep`, not a bottleneck.
- Changing hash algorithm for change detection (`merkle.rs`) — already using `fast_hash`
  (non-crypto) correctly, not a bottleneck.
- Switching embedding backend away from `embed_anything`/Candle to `fastembed`/ONNX Runtime —
  worth doing later if Phase 1-3 still don't hit target on very large repos, but is a larger
  dependency change and should be its own follow-up plan, not bundled here.
