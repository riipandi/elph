# Floppy

AI memory with vector search powered by Turso.

Turso-backed agent memory: semantic retrieval, per-memory weight scoring, and
task-scoped lifecycle tracking. Memories persist across sessions so agents can
reuse lessons from past work.

```rust
use floppy::{FloppyBuilder, EmbedOptions};

let store = FloppyBuilder::new("/path/to/store.db", "session-id")
    .embed(EmbedOptions::default())? // requires `embed`
    .build()?;

store.init().await?;
let result = store.start_task("implement auth middleware").await?;
// result.memories — top-k relevant memories for this task
```

**Feature:** `embed` — local embeddings via [embed_anything](https://github.com/StarlightSearch/EmbedAnything)
(Candle / Hugging Face; default AllMiniLML6V2, 384 dims).
Without it, supply your own [`EmbedFn`](src/store/mod.rs).

**Configuration:** explicit via [`FloppyBuilder`](src/builder.rs) — floppy does not read environment variables.
**Paths:** Elph stores project memory at `PROJECT_DIR/.elph/store.db`. Standalone default:
`FloppyPaths::project_local()` → `./.floppy/store.db`.

## Third-party attribution

Ported from [memelord](https://github.com/glommer/memelord) (`packages/sdk`).
The original code is licensed under the MIT License. Copyright (c) 2026 Glauber Costa.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
