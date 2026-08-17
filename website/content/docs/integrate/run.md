# Headless (`elph run`)

Use the same harness without the TUI — scripts, CI, and editor glue that is not ACP.

```sh
elph run "write a test"
elph run --mode=plan "design the auth boundary"
elph run --output=json "summarize this diff"
```

Formats: `plain`, `pretty`, `json`, `stream-json`, `stream-message-json`.

Provider and model follow env / session defaults (`ELPH_PROVIDER`, `ELPH_MODEL`). Auth is the same as the TUI — see [Providers](/docs/start/providers).
