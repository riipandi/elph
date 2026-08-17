---
title: Surfaces, approvals, and handover
description: Headless output formats, Plan vs Build, ACP, and importing Claude Code or Codex transcripts as inert context.
tags: [acp, run, import]
author: Elph
created: 2026-08-12T10:00:00
slug: surfaces-and-handover
---

The TUI is the daily path. Two other surfaces matter when you script or switch tools.

## Headless

```sh
elph run "write a test"
elph run --mode=plan "design the auth boundary"
elph run --output=json "summarize this diff"
```

Formats: `plain`, `pretty`, `json`, `stream-json`, and Anthropic-style `stream-message-json`. Use `-c` to continue the last session for the current project, or `-r <id>` to resume a specific one.

## Plan vs Build

Plan mode designs and asks. Approved plans land in `.elph/plans/` before the full tool surface returns. Risky shell and file actions stay visible; they are not a hidden black box.

## Editors

`elph acp --stdio` speaks ACP v1. Add `--experimental` for v2. Auth methods, session load, and tool updates follow the protocol; privileged methods require credentials.

## Handover

`elph import` reads Claude Code and Codex transcripts as **inert** context. Foreign tool calls are not executed. Use it when you are moving a conversation into Elph, not when you want another agent to keep running commands.

WASM extensions (`elph ext`) and MCP (`elph mcp`) stay behind those same approval and trust boundaries.
