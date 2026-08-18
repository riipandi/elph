---
title: Local project memory
description: Floppy memory lives in .elph/store.db — recalled lessons without a cloud agent backend.
tags: [memory]
author: Elph
created: 2026-08-01T10:00:00
slug: local-index-and-memory
---

Sessions and project memory live in `.elph/store.db` (Turso / SQLite with FTS and vectors). Nothing requires a hosted agent backend.

**Floppy** records lessons and a work log, then injects relevant memories into later turns. Inspect it with `elph memory status` and `elph memory list`.

The agent still finds code with `grep` / `read_file`. There is no separate codebase index in the harness.
