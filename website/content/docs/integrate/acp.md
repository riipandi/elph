# ACP

Elph speaks ACP JSON-RPC 2.0 over stdio. One process speaks one protocol version.

```sh
elph acp --stdio                  # ACP v1 (stable)
elph acp --stdio --experimental   # ACP v2 (draft)
elph acp --setup                  # interactive provider login for editors
```

Use v1 for Zed and editors that speak ACP 1. Use `--experimental` only when the client sends `protocolVersion: 2`.

## Shared rules

- Working directory must be an **absolute** path.
- Shell runs locally (`shell_exec` / `shell_use`). Elph does not call client `terminal/*`.
- Writes stay local. Elph does not call client `fs/write_text_file`.
- v1 advertises `session/delete` and `additionalDirectories`. Tool calls start `pending`, then `in_progress`.
- v2 prompt acks immediately; completion is a single idle `state_update` with `stopReason`. Usage reports context window size, not a copy of `used`.
- Privileged methods (`session/new`, `session/prompt`, and so on) need credentials. `initialize`, list, close, delete, and cancel do not.
- Existing env / `auth.json` keys allow privileged methods without a separate authenticate call.
- Logout is connection-scoped; it does not delete `auth.json`.

Tool approval uses `session/request_permission` (allow once / session / all / reject).
