# CLI Reference

Elph ships as the `elph` binary (`make build` → `build/release/elph`).

## Global usage

```sh
elph                      # Open interactive TUI (default)
elph --env-file .env      # Load env before any command
elph version              # Show build info
elph provider …           # Manage providers
elph doctor               # Stub — not implemented
```

## `elph version`

| Flag               | Output                                   |
|--------------------|------------------------------------------|
| (none)             | Full line: name, version, hash, platform |
| `-s`, `--short`    | `version (hash)`                         |
| `-S`, `--semantic` | Version only                             |

Version metadata is injected at build time via Makefile ldflags into `internal/config`.

## `elph provider`

### `connect [--force]`

Creates starter JSON under `~/.elph/providers` for:

- OpenAI
- Anthropic
- OpenCode Zen (`opencode`)
- OpenCode Go (`opencode-go`)
- DeepSeek
- Kimi

Without `--force`: existing files are preserved; missing `reasoning`, `thinkingLevelMap`, and `compat` fields are backfilled from templates.

With `--force`: overwrites existing provider files.

Set API keys via env vars referenced in the JSON (see [configuration.md](./configuration.md)).

### `update [--force]`

1. Same connect/backfill step as `connect`
2. Refreshes model metadata from [models.dev](https://models.dev) and live `/models` endpoints (OpenCode, DeepSeek, Kimi when keys are set)
3. Records `models.lastSync` in `~/.elph/settings.json`

The TUI auto-runs this sync on startup when `models.syncInterval` has elapsed (default 24h).

### `list`

Tabular summary of provider files: id, display name, status, API key configured/not set, enabled/total models.

### `enable` / `disable`

```sh
elph provider enable openai
elph provider disable anthropic
```

Toggles the top-level `enabled` field in the provider JSON.

### `model list|enable|disable`

```sh
elph provider model list openai
elph provider model enable openai gpt-4o
elph provider model disable openai gpt-4o-mini
```

## `elph doctor`

Currently prints that the command is not yet implemented (`cmd/coding-agent/doctor.go`).

## Development commands

From the repository root:

```sh
make prepare    # Install golangci-lint, gotestsum
make deps       # go mod download
make build      # build/release/elph
make run        # go run ./cmd/coding-agent
make test       # gotestsum
make install    # copy binary to ~/.local/bin
```

Requires **Go ≥ 1.26**. `make prepare` also expects `python3` (build timing only).

## Related docs

- [configuration.md](./configuration.md) — paths, settings, env vars
- [schemas/provider-schema.json](../schemas/provider-schema.json) — provider file format
