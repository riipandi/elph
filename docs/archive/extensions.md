# Extensions

> Historical design document. The WASM extension runtime was removed. See the
> [current native lifecycle hook documentation](../hooks.md).

Design for Pi-compatible WASM extensions — sandboxed plugins that contribute slash commands, tools, and lifecycle hooks without running arbitrary native code in the host process.

Inspired by [Pi extensions](https://pi.dev/docs/latest/extensions); this document records the former **wasmi + core Wasm** (JSON ABI, host imports) design instead of TypeScript/jiti.

## Goals

- **Safe by default** — extensions run in a WASM sandbox; host APIs are explicit imports.
- **Hot reload** — `/reload` rediscovers extensions without restarting the TUI.
- **Pi-compatible layout** — global + project-local discovery, built-in commands win over extension commands.
- **Incremental surface** — slash commands, tools, and selected harness events; custom TUI later.

## Discovery layout

| Location                             | Scope         | When loaded              |
| ------------------------------------ | ------------- | ------------------------ |
| `~/.elph/extensions/<name>/`         | Global        | Always (after bootstrap) |
| `<project>/.elph/extensions/<name>/` | Project-local | After project trust      |
| Extra paths in `extensions.json`     | Configured    | Per settings             |

Each extension bundle:

```
<name>/
├── extension.toml    # manifest (name, version, wasm path, enabled)
└── plugin.wasm       # core Wasm guest
```

### `extension.toml` (manifest)

| Field         | Required | Description                                |
| ------------- | -------- | ------------------------------------------ |
| `name`        | yes      | Stable id; install directory name          |
| `version`     | no       | Semver string for display                  |
| `description` | no       | Human-readable summary                     |
| `wasm`        | yes      | Path to `.wasm`, relative to bundle root   |
| `enabled`     | no       | Default `true`                             |
| `trusted`     | no       | Skip trust prompt when installing (future) |

### `extensions.json` (host settings)

Path: `~/.elph/extensions.json`

```json
{
    "disabled": ["legacy-ext"],
    "extraPaths": ["/path/to/more/extensions"]
}
```

Merge order for slash dispatch: **built-in commands → extension commands → prompt templates**.

## WASM guest contract

The former ABI (`elph_init`, host imports, JSON blobs) is no longer implemented.

Tools and lifecycle events (`session_start`, `tool_call`, …) belonged to the former host ABI. Custom TUI widgets were never implemented.

### Build toolchain

- Guest: Rust + `elph-extension-pdk`, `wasm32-unknown-unknown`.
- Host: `wasmi` interpreter (no WASI).
- Example bundle: `crates/ext-hello/`.

## CLI: `elph extensions`

| Subcommand           | Design behavior                                                 |
| -------------------- | --------------------------------------------------------------- |
| `list`               | Installed extensions, enabled state, contributed `/commands`    |
| `install <path>`     | Copy bundle to `~/.elph/extensions/<name>/`; `--force` replaces |
| `remove <name>`      | Delete global bundle directory                                  |
| `enable` / `disable` | Toggle via `extensions.json` without uninstalling               |

`update` and git/npm package installs are **planned** (Pi `packages.md` parity).

## TUI integration

| Behavior        | Design                                                           |
| --------------- | ---------------------------------------------------------------- |
| `/reload`       | Rediscover extensions + resources; refresh slash palette         |
| Extension slash | Result shown as system transcript line                           |
| Banner          | Extension count in chrome (planned; currently stub `0`)          |
| Plan mode       | Extension tools blocked when mutating tools are blocked (future) |

## Security & trust

1. Extensions run with WASM sandbox — no direct filesystem or network unless host imports are added later.
2. Project-local `.elph/extensions/` loads only after project trust (same policy as `.pi/extensions/`).
3. Install from paths the user controls; no auto-install from network without explicit `plugin install`.

## Relationship to MCP

|         | Extensions                                          | MCP                          |
| ------- | --------------------------------------------------- | ---------------------------- |
| Runtime | WASM component in-process                           | External server (stdio/HTTP) |
| Tools   | Guest-exported (future)                             | Server-advertised            |
| Config  | `extension.toml` + dirs                             | `mcp.json`                   |
| Naming  | `mcp_{server}__{tool}` vs extension tool prefix TBD |                              |

Both may coexist; harness merges tool catalogs with exposure policy.

## Phased roadmap

| Phase | Surface                                                | Status                                |
| ----- | ------------------------------------------------------ | ------------------------------------- |
| 1     | Slash commands via WASM                                | **Designed / initial implementation** |
| 2     | `registerTool` equivalent + harness wiring             | Planned                               |
| 3     | Lifecycle hooks (`before_agent_start`, `tool_call`, …) | Planned                               |
| 4     | Custom TUI (`ctx.ui.custom`)                           | Planned                               |
| 5     | npm/git extension packages                             | Planned                               |

## Related

- [slash-commands.md](./slash-commands.md) — built-in vs extension commands
- [configuration.md](./configuration.md) — paths
- [cli.md](./cli.md) — `plugin` subcommand
- [agent-runtime.md](./agent-runtime.md) — harness hooks
