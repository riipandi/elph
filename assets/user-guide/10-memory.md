# Memory

Project long-term memory is stored in `<project>/.elph/store.db` (floppy / Turso +
embeddings).

## CLI

```sh
elph memory status
elph memory list
elph memory …
```

Memory is injected into the system prompt when enabled. Automatic hooks capture work
memories and corrections during coding turns (best-effort).

See repo `docs/memory.md` for architecture and scoring details.
