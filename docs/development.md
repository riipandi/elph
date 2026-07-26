# Development

Design notes for building and working on the Elph workspace locally. Operational detail: root `Makefile`.

## Workspace binary

| Binary | Crate   | Role                   |
| ------ | ------- | ---------------------- |
| `elph` | `elph/` | Coding agent CLI + TUI |

Library crates (`elph-core`, `elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm`) are consumed by `elph` and published to crates.io.

### `elph-agent` feature flags

| Consumer        | Typical features                                |
| --------------- | ----------------------------------------------- |
| `elph` binary   | `mcp`, `extensions`, `builtin-tools`, `tracing` |
| Minimal embed   | `mcp` only (`--no-default-features`)            |
| Custom tool set | `tools-core`, `tools-explore`, … à la carte     |

See [`crates/elph-agent/docs/tools.md`](../crates/elph-agent/docs/tools.md) for the full feature matrix.

### Observability

The `tracing` Cargo feature enables [`fastrace`](https://crates.io/crates/fastrace) spans (not the `tracing` crate). Logging uses `log` + `logforth`. The `elph` binary enables tracing by default; library embeds opt in per crate.

| Output | Path                           | Control                                                |
| ------ | ------------------------------ | ------------------------------------------------------ |
| Logs   | `{logs_dir}/elph.jsonl`        | `ELPH_LOG_LEVEL`, `ELPH_LOG_FILE`, `ELPH_LOG_ROTATION` |
| Traces | `{logs_dir}/elph-traces.jsonl` | `ELPH_TRACE` (set `0` to disable)                      |

See [`crates/elph-agent/docs/observability.md`](../crates/elph-agent/docs/observability.md) for span names, HTTP `traceparent` propagation, and downstream integration.

## Make targets (build)

| Target            | Behavior                                              |
| ----------------- | ----------------------------------------------------- |
| `make build`      | Release-build `elph`; prints size, hash, elapsed time |
| `make build-elph` | Same as `make build`                                  |

Output directory: `target/release/`.

### Other common targets

| Target         | Behavior                                                           |
| -------------- | ------------------------------------------------------------------ |
| `make check`   | `cargo check --workspace`                                          |
| `make test`    | `cargo nextest run`                                                |
| `make lint`    | `cargo clippy --workspace -D warnings`                             |
| `make fmt`     | `cargo fmt` (edition 2024 style)                                   |
| `make run`     | `cargo run --bin elph`                                             |
| `make watch`   | `cargo watch` + `cargo run --bin elph`                             |
| `make install` | Copy `elph-dev` (debug) or `elph-next` (release) to `~/.local/bin` |
| `make help`    | List all targets                                                   |

## Extension development loop

1. Build guest WASM: see [extensions.md](./extensions.md) and `extensions/say-hello/README.md`.
2. Install: `elph plugin install extensions/say-hello --force`
3. Verify: `elph plugin list`
4. In TUI: `/say-hello World` or `/reload` after changes.

## Related

- [extensions.md](./extensions.md)
- [cli.md](./cli.md)
- [README.md](./README.md)
