# Agent Client Protocol (ACP)

Elph speaks ACP over stdio. **One process = one protocol version.**

```sh
elph acp --stdio                  # ACP v1 (stable)
elph acp --stdio --experimental   # ACP v2 (draft)
elph acp                          # alias of `elph acp --stdio`
```

`--experimental` requires `--stdio`. There is no mixed-version stream.

## When to use which

| Command | Protocol | Typical clients |
|---|---|---|
| `--stdio` | v1 (stable) | Current Zed / editors that speak ACP 1 |
| `--stdio --experimental` | v2 (draft) | Clients that send `protocolVersion: 2` |

A v2 initialize against a v1 process is rejected, and the reverse.

## Shared behavior

- Working directory must be an absolute path.
- Resume `cwd` must match the stored session cwd.
- No ACP auth methods (credentials stay file/env).
- No audio prompts.
- No Client `fs/*` or `terminal/*` execution (Elph uses its own tools).
- Slash commands advertised after session setup are ACP-safe builtins **plus** the session's prompt templates and skills (same catalog as the TUI palette). Dispatch uses that catalog.
- `configOptions` advertise **model**, **thought_level** (reasoning), and **mode** (ask/plan/build/brave). On v1, Zed-style `modes` are thinking levels (same as [pi-acp](https://github.com/svkozak/pi-acp)); agent permission mode stays on `configOptions.mode`.
- Tool approval uses `session/request_permission` on both versions.

## v1 capabilities

`agentInfo`: `elph` / `Elph` / crate version.

- `loadSession`
- `promptCapabilities.embeddedContext`
- `sessionCapabilities.list` / `resume` / `close`

Methods: `session/new`, `session/load`, `session/resume`, `session/list`, `session/close`, `session/delete`, `session/prompt` (held until `stopReason`), `session/cancel`, `session/set_mode`.

`session/new` (and load/resume) include `modes` for thinking levels (`off`…`max`) and `configOptions` for model, thought_level, and agent mode.

`session/set_mode` sets thinking level (pi-acp / Zed). `session/set_config_option` sets `model`, `thought_level`, or `mode`.

## v2 capabilities

`info`: same implementation fields.

- `capabilities.session` (baseline methods)
- `session.prompt.embeddedContext`
- `session.delete`

Methods: `session/new`, `session/list`, `session/resume` (`replayFrom`), `session/close`, `session/delete`, `session/prompt` (ack `{}`, then `state_update`), `session/cancel`, `session/set_config_option`.

Not advertised until implemented: client `mcpServers` attach, `additionalDirectories` as a real root set, prompt images.

## Code

`crates/coding-agent/src/platform/acp/` — shared host (`session`, `commands`, `replay`) plus `v1/` (stable Agent builder) and root v2 (`Agent.v2()`).
