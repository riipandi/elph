# elph-core

Shared foundation for Elph applications. Provides filesystem helpers, default home-directory
scaffold files (`BundledManifest`, `TrustStore`, `VersionFile`), structured logging,
config/data path resolution utilities, and the `memz` agent memory module.

## Memory

Elph memory `memz` is a Turso-backed agent memory store: semantic retrieval, per-memory weight
scoring, and task-scoped lifecycle tracking. Memories persist across sessions so agents can
reuse lessons from past work.

```rust
use elph_core::memz::{MemzBuilder, FastEmbedOptions};

let store = MemzBuilder::new("/path/to/memory.db", "session-id")
    .fastembed(FastEmbedOptions::default())? // requires `fastembed`
    .build()?;

store.init().await?;
let result = store.start_task("implement auth middleware").await?;
// result.memories — top-k relevant memories for this task
```

**Feature:** `fastembed` — local embeddings via [fastembed](https://github.com/Anush008/fastembed-rs) (all-MiniLM-L6-v2, 384 dims).
Without it, supply your own [`EmbedFn`](https://docs.rs/elph_core/latest/elph_core/memz/type.EmbedFn.html).

**Configuration:** explicit via [`MemzBuilder`](src/memz/builder.rs) — memz does not read environment variables.
**Paths:** Elph stores project memory at `PROJECT_DIR/.elph/memory.db`. Standalone default: `MemzPaths::project_local()` → `./.memz/memory.db`.

Full documentation: [docs/memory.md](../../docs/memory.md).

## Third-party attribution

The `memz` module is ported from [memelord](https://github.com/glommer/memelord) (`packages/sdk`).
The original code is licensed under the MIT License. Copyright (c) 2026 Glauber Costa.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
