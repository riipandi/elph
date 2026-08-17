# Sessions

Session trees live in the project store: `<project>/.elph/store.db`. Artifacts (tool outputs, MCP cache) live under `APP_DATA/sessions/<SESSION_ID>/`.

## Resume

```sh
elph -c                    # last session for this project
elph -r <session-id>       # specific session
elph session list
```

On open or resume, Elph repairs unanswered tool calls, closes interrupted harness operations, and restores model, thinking level, and active tools from the session tree.

## Goals

Session goals use the same store. In the TUI: `/goal`.
