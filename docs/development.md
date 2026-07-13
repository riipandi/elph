# Development

Design notes for building and working on the Elph workspace locally. Operational detail: root `Makefile`.

## Workspace binary

| Binary | Crate   | Role                   |
| ------ | ------- | ---------------------- |
| `elph` | `elph/` | Coding agent CLI + TUI |

Library crates (`elph-core`, `elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm`) are consumed by `elph` and published to crates.io.

## Make targets (build)

| Target            | Behavior                                              |
| ----------------- | ----------------------------------------------------- |
| `make build`      | Release-build `elph`; prints size, hash, elapsed time |
| `make build-elph` | Same as `make build`                                  |

Output directory: `target/release/`.

### Other common targets

| Target         | Behavior                               |
| -------------- | -------------------------------------- |
| `make check`   | `cargo check --workspace`              |
| `make test`    | `cargo nextest run`                    |
| `make lint`    | `cargo clippy --workspace -D warnings` |
| `make fmt`     | `cargo fmt` (edition 2024 style)       |
| `make run`     | `cargo run --bin elph`                 |
| `make watch`   | `cargo watch` + `cargo run --bin elph` |
| `make install` | Copy `elph-next` to `~/.local/bin`     |
| `make help`    | List all targets                       |

## Extension development loop

1. Build guest WASM: see [extensions.md](./extensions.md) and `extensions/say-hello/README.md`.
2. Install: `elph plugin install extensions/say-hello --force`
3. Verify: `elph plugin list`
4. In TUI: `/say-hello World` or `/reload` after changes.

## Related

- [extensions.md](./extensions.md)
- [cli.md](./cli.md)
- [README.md](./README.md)
