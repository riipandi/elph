# Development

Design notes for building and working on the Elph workspace locally. Operational detail: root `Makefile`.

## Workspace binary

| Binary | Crate                  | Role                   |
| ------ | ---------------------- | ---------------------- |
| `elph` | `crates/coding-agent/` | Coding agent CLI + TUI |

Library crates (`elph-ai`, `elph-agent`, `elph-tui`, `floppy`) are consumed by `elph` and published to crates.io. `elph-exec` has been merged into `elph-agent` as `crate::exec` (gated behind the `tools-shell-exec` feature).

### `elph-agent` feature flags

| Consumer        | Typical features                                |
| --------------- | ----------------------------------------------- |
| `elph` binary   | `mcp`, `extensions`, `builtin-tools`, `tracing` |
| Minimal embed   | `mcp` only (`--no-default-features`)            |
| Custom tool set | `tools-core`, `tools-explore`, … à la carte     |

See [`crates/elph-agent/docs/tools.md`](../crates/elph-agent/docs/tools.md) for the full feature matrix.

### Observability

The `tracing` Cargo feature enables [`fastrace`](https://crates.io/crates/fastrace) spans (not the `tracing` crate). Logging uses `log` + `logforth`. The `elph` binary enables tracing by default; library embeds opt in per crate.

| Output | Path                                | Control                                                |
| ------ | ----------------------------------- | ------------------------------------------------------ |
| Logs   | `{logs_dir}/elph.jsonl` (rolling)   | `ELPH_LOG_LEVEL`, `ELPH_LOG_FILE`, `ELPH_LOG_ROTATION` |
| Crash  | `{logs_dir}/crash.log-YYYYMMDD`     | Always on after path resolve                           |
| Traces | `{logs_dir}/elph-traces.jsonl`      | `ELPH_TRACE` (set `0` to disable)                      |
| MCP    | `{logs_dir}/mcp/<name>/…stderr.log` | Written when MCP stdio capture is enabled              |

See [`crates/elph-agent/docs/observability.md`](../crates/elph-agent/docs/observability.md) for span names, HTTP `traceparent` propagation, and downstream integration.

## Make targets (build)

| Target            | Behavior                                              |
| ----------------- | ----------------------------------------------------- |
| `make build`      | Release-build `elph`; prints size, hash, elapsed time |
| `make build-elph` | Same as `make build`                                  |

Output directory: `target/release/`.

### Other common targets

| Target         | Behavior                                                                       |
| -------------- | ------------------------------------------------------------------------------ |
| `make check`   | `cargo check --workspace`                                                      |
| `make test`    | `cargo nextest run`                                                            |
| `make lint`    | `cargo clippy --workspace -D warnings`                                         |
| `make fmt`     | `cargo fmt` (edition 2024 style)                                               |
| `make run`     | `cargo run --bin elph`                                                         |
| `make watch`   | `cargo watch` + `cargo run --bin elph`                                         |
| `make install` | Copy debug → `~/.local/bin/elph-debug` or release → `~/.local/bin/elph-canary` |
| `make help`    | List all targets                                                               |

### Installed binaries

| Binary path                | Channel             | Typical source                   |
| -------------------------- | ------------------- | -------------------------------- |
| `~/.local/bin/elph`        | production / stable | Release installers               |
| `~/.local/bin/elph-canary` | next (beta)         | `make install` (release profile) |
| `~/.local/bin/elph-debug`  | dev (unstable)      | `make install` (debug profile)   |

All share the same config/data layout (`CONFIG_DIR` / `APP_DATA`); override with `ELPH_HOME` / `ELPH_DATA_DIR` when testing channels side by side.

## Extension development loop

1. Build guest WASM: see [extensions.md](./extensions.md) and `crates/ext-hello/README.md`.
2. Install: `elph plugin install crates/ext-hello --force`
3. Verify: `elph plugin list`
4. In TUI: `/say-hello World` or `/reload` after changes.

## Related

- [extensions.md](./extensions.md)
- [cli.md](./cli.md)
- [README.md](./README.md)
