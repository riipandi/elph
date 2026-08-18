# Memory

Project long-term memory (floppy) lives in `<project>/.elph/store.db` — Turso/SQLite plus embeddings and FTS.

## CLI

```sh
elph memory status
elph memory list
elph memory …
```

Memory is injected into the system prompt when enabled. Automatic hooks capture work memories and corrections during coding turns (best-effort).
