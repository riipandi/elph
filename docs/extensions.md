# Extensions

Sandboxed Wasm plugins that customize Elph: slash commands, LLM tools, and agent-loop events. Runtime is **wasmi** (core Wasm). Guests are `wasm32-unknown-unknown` (no WASI, no Node/jiti).

Pi’s `registerCommand` / `registerTool` / `pi.on` / `ctx.ui.confirm` map to **host imports** with the same names. Pi `.ts` files are not source-compatible.

## Discovery

| Location | When loaded |
| --- | --- |
| `~/.elph/extensions/<name>/` | Always |
| `<project>/.elph/extensions/<name>/` | After project trust |
| Extra paths in settings `resources.extensions` | Always |

Each bundle:

```
<name>/
├── extension.toml
└── plugin.wasm
```

### `extension.toml`

Required: `name`, `wasm` (path relative to the bundle). Optional: `version`, `description`, `enabled` (default true).

## ABI

Linear memory: UTF-8 JSON. Return values are **u32 LE length + bytes**. Max blob 64 KiB. Fuel ~10M instructions per call. Memory grow is not WASI; keep guests small. Guest must **not** import WASI.

Guest exports: `memory`, `elph_alloc`, `elph_dealloc`, `elph_init`, `elph_on_event`, `elph_execute_command`, `elph_execute_tool`.

Host imports (`elph` module): `register_command`, `register_tool`, `subscribe`, `notify`, `confirm` (false when no TUI).

Events in this release: `session_start`, `before_agent_start`, `tool_call`, `tool_result`. Payloads are small JSON (tool name + args, not the full transcript).

`/reload` rediscovers bundles. Built-in slash commands win over extension commands.

## Authoring

Use `crates/elph-extension-pdk` (excluded from the host workspace). Example: `crates/ext-hello`.

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --manifest-path crates/ext-hello/Cargo.toml
elph extensions install crates/ext-hello --force
```

## CLI

`elph extensions list | install <dir> | remove <name> | enable | disable`

## Related

Harness hooks used by native Elph code stay in `AgentHarness`; Wasm guests attach through the same hook points after session restore.
