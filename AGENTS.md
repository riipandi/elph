# Elph — agent instructions

Go 1.26 coding agent CLI (`elph`, module `github.com/riipandi/elph`). TUI in `internal/renderer`; turn loop in `pkg/core/agent`.

## Commands

```bash
make build   # ./build/release/elph
make test    # gotestsum, ./...
make test PKG=./pkg/ai/...
go test ./... -count=1
make lint fmt vet
```

Run focused tests before wide `go test ./...`. Check `git status` — provider refactor may be uncommitted.

## Layout

| Path                | Role                                                 |
|---------------------|------------------------------------------------------|
| `cmd/coding-agent/` | CLI (Cobra)                                          |
| `internal/`         | App-private: renderer, runtime, prompt, settings     |
| `pkg/core/agent/`   | Turn loop, history limits, tool loop                 |
| `pkg/tool/`         | Built-in tools + provider schemas                    |
| `pkg/ai/`           | Provider facade (`LoadProviders`, `ResolveProvider`) |
| `docs/`             | Architecture, runtime, tools, config                 |

Diagnostic slash helpers live in `internal/tools` — not model-callable. Agent tools live in `pkg/tool`.

## AI provider stack (Pi / fantasy-style)

Three layers — **do not collapse them**:

```
pkg/ai/protocol/     Turn contract (TurnRequest, Provider, Compat, errors)
                     No catalog imports. Adapters import this only.

pkg/ai/provider/     Catalog (~/.elph/providers), NewProvider routing,
                     thinking resolve, models.dev. Re-exports protocol via aliases.go.

pkg/ai/providers/      SDK adapters: openai, openaicompat, openrouter, anthropic, google
pkg/ai/providertests/  Shared httptest suites across adapters
```

### `NewProvider` routing (`catalog.go`)

| `model.API`          | Condition                               | Adapter                                        |
|----------------------|-----------------------------------------|------------------------------------------------|
| `openai-completions` | `compat.thinkingFormat == "openrouter"` | `openrouter`                                   |
| `openai-completions` | else                                    | `openaicompat` (wraps `openai` + compat hooks) |
| `anthropic-messages` | —                                       | `anthropic`                                    |
| other                | —                                       | error                                          |

**Google** (`providers/google`) is a stub — not wired in `NewProvider` yet.

### SDK deps (`go.mod` replace)

- `github.com/openai/openai-go/v3` → `github.com/charmbracelet/openai-go/v3`
- `github.com/anthropics/anthropic-sdk-go` → `github.com/charmbracelet/anthropic-sdk-go`

Extend Charm forks; do not add raw HTTP in `pkg/ai/provider/`.

## How to change providers

1. **Shared types / errors** → `pkg/ai/protocol/`
2. **Catalog, config, thinking templates** → `pkg/ai/provider/`
3. **HTTP/SDK behavior** → `pkg/ai/providers/<name>/` (new subpackage if needed)
4. **Cross-provider behavior tests** → `pkg/ai/providertests/` with `writeJSONResponse` / SSE helpers
5. Wire new API kind in `NewProvider` + provider schema if needed

### Adapter package shape

Typical files per provider: `*_provider.go` or `<name>.go`, `language_model.go`, `tools.go`, `error.go`, `provider_options.go`, optional `language_model_hooks.go`. Shared HTTP header logic: `providers/internal/httpheaders/`.

### Pitfalls

- **Import cycle**: adapters must not import `pkg/ai/provider`. Use `pkg/ai/protocol` (often aliased `provider`).
- **Anthropic base URL**: strip trailing `/v1` before SDK — SDK appends `v1/messages`.
- **Anthropic system**: SDK sends `system` as `[]TextBlockParam`, not a plain string — match in tests.
- **Compat helpers**: use exported methods (e.g. `ReasoningEffortSupported()`), not unexported aliases.
- **Provider errors**: guard nil `*http.Request` in `DumpRequest` paths.

### Adding Google / new API

Implement `Complete` in `providers/google`, add `APIGoogleGenerative` (or equivalent) to catalog types, branch in `NewProvider`, add `providertests` coverage.

## Agent runtime (brief)

1. User input → `internal/runtime.Session` → `pkg/core/agent.RunTurn`
2. System prompt from `internal/prompt` (includes nearest `AGENTS.md` walking up from work dir)
3. Native tool calling when provider exposes tools (`docs/tools.md`)
4. Stream deltas → Bubble Tea TUI events

Memory-conscious: defer heavy work, cap history/tool output (`pkg/core/agent/limits.go`, `internal/runtime/execute.go`). See `docs/architecture.md` § Performance.

## Conventions

- Focused diffs only — no drive-by refactors or unsolicited markdown.
- Match existing naming, imports, and test style in the touched package.
- Provider JSON schema: `schemas/provider-schema.json`
- Deeper detail: `docs/architecture.md`, `docs/agent-runtime.md`, `docs/tools.md`
