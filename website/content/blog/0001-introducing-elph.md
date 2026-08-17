---
title: Introducing Elph
description: A local, opinionated coding agent harness — TUI, headless run, and ACP — with the repo on disk and the model you already pay for.
tags: [release, harness]
author: Elph
created: 2026-07-15T10:00:00
slug: introducing-elph
---

Elph is an opinionated AI coding agent harness. It is not a browser chat product. You open a repository, describe the outcome, and the agent works through a native tool loop: read, edit, search, shell, and verify.

Three surfaces ship in one binary:

- `elph` — interactive TUI (keyboard-first, mouse support, streaming transcript)
- `elph run` — headless prompts for scripts and CI
- `elph acp --stdio` — Agent Client Protocol for editors

Install:

```sh
curl -fsSL https://elph.space/install.sh | bash
```

Or `cargo install --locked elph` (Rust ≥ 1.97). Linux and macOS, x86_64 and arm64.

The application is Apache-2.0. The libraries (`elph-ai`, `elph-agent`, `elph-tui`, `floppy`) are MIT. The project is actively developed — read the release notes before you upgrade.
