---
title: Local index and memory
description: Codegraph and floppy share .elph/store.db — hybrid search, a shallow impact graph, and recalled lessons without a cloud agent backend.
tags: [codegraph, memory]
author: Elph
created: 2026-08-01T10:00:00
slug: local-index-and-memory
---

Sessions, project memory, and the semantic code index live in `.elph/store.db` (Turso / SQLite with FTS and vectors). Nothing requires a hosted agent backend.

**Codegraph** indexes the tree into AST chunks (Rust, TypeScript, Python, Go, and other tier-1 languages). Retrieval is hybrid: keyword (BM25) plus embeddings, merged for search. A shallow impact graph answers “what sits next to this path or symbol.”

```sh
elph codegraph build
elph codegraph search "provider auth resolve"
elph codegraph impact crates/elph-ai/src/auth
```

Agent tools (`code_search`, `code_impact`, `code_status`) attach when the index is enabled. Build and purge stay CLI-only.

**Floppy** sits in the same store. It records lessons and a work log, then injects relevant memories into later turns. Inspect it with `elph memory status` and `elph memory list`.

This is the difference between “the model greps” and “the harness already knows the repo.”
