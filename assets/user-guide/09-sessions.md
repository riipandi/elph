# Sessions

Product sessions live in the project-local Turso store `PROJECT/.elph/store.db` (`session_entries` tree). Artifacts
(tool outputs, MCP cache, terminals) live under `APP_DATA/sessions/<SESSION_ID>/`.

## Resume

```sh
elph session list
elph --resume <session-id>   # when supported by CLI flags
```

## Semi-durable recovery

On open/resume Elph:

1. Repairs unanswered tool calls with synthetic error results
2. Closes open harness operations as interrupted
3. Rehydrates queues (steer / follow-up / next-turn) and pending session writes
4. Restores model, thinking level, and active tools from the session tree

Journal entries use custom types prefixed `harness.*`. See `docs/agent-runtime.md`.

## Goals

Session goals use the same project-local store (`goals` table). Slash: `/goal`.
