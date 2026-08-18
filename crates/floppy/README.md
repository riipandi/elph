# floppy

Agent **memory** for coding agents, powered by [Turso](https://turso.tech) (embedded SQLite + vectors + FTS5).

Designed as:

1. An **in-process library** (e.g. inside [Elph](https://github.com/riipandi/elph))
2. A future **standalone CLI** and **MCP server** (same crate domains)

## Domains

```text
floppy::core    # always — db open, embed adapters, paths, migration ledger
floppy::memory  # feature "memory" (default) — task-scoped memories
```

| Feature            | Enables                                                                            |
| ------------------ | ---------------------------------------------------------------------------------- |
| `memory` (default) | `MemoryStore`, scoring, task lifecycle                                             |
| `embed`            | Local MiniLM via `embed_anything` (Accelerate on macOS; Candle CPU elsewhere)      |
| `mkl`              | Intel MKL CPU backend for embeddings (x86_64; not compatible with the wild linker) |
| `full`             | `memory` + `embed`                                                                 |

## Example

```toml
floppy = { version = "0.0.1", features = ["memory", "embed"] }
```

```rust,ignore
use floppy::{create_embedder, create_memory_store, EmbedOptions, FloppyConfig};

let embed = create_embedder(EmbedOptions::default())?;
let store = create_memory_store(FloppyConfig::new(".floppy/store.db", "cli"), embed);
store.init().await?;
```

## Migration bands (shared `store.db`)

| Band | Owner    |
| ---- | -------- |
| 1–99 | `memory` |

Ledger: `app_migrations` via [`core::apply_set`].

## License

MIT
