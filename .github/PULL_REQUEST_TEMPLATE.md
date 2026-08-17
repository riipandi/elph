## Summary

<!-- What changed and why. One short paragraph. Link the issue if there is one. -->

Fixes #

## Kind

- [ ] Bug fix
- [ ] Feature
- [ ] Docs only
- [ ] Refactor (no behavior change)
- [ ] Build / CI / make
- [ ] Port from pi (intent/behavior; implemented on Elph)

## Surface

<!-- Check what a reviewer must look at. -->

- [ ] `elph` CLI (`crates/coding-agent`)
- [ ] TUI (`elph-tui` / coding-agent TUI)
- [ ] Agent runtime (`elph-agent`)
- [ ] Providers / catalog (`elph-ai`)
- [ ] Memory (`floppy`) / codegraph / session store
- [ ] MCP, skills, or WASM extensions
- [ ] Public crate API or session schema
- [ ] `docs/` or crate docs (required for significant changes)

## Behavior

<!-- User-visible or caller-visible change. Delete the section if none. -->

**Before:**

**After:**

Breaking change? (CLI, crate API, schema, config/env) — yes / no.

If yes, what callers must do:

## Test plan

<!-- Prefer `make` targets. Do not run cargo directly. -->

- [ ] `make check` (or `make check-elph` / `make check-elph-tui`)
- [ ] `make lint` (or scoped `make lint-elph*`)
- [ ] `make test` / `make test-elph` / `make test-elph-tui` for the crates you touched
- [ ] Unit tests live next to the code; integration tests only hit public APIs
- [ ] Manual: <!-- command, TUI path, or "n/a" -->

## Docs

Significant change = new/removed public API, CLI, config/env, or a behavior a caller would notice. Then update `docs/` in this PR (not a follow-up). Do not edit generated OpenWiki pages.

- [ ] No docs change needed (internal / test-only / formatting)
- [ ] Updated existing `docs/` (path: )
- [ ] Added `docs/` page (path: )
- [ ] Crate README / `crates/*/docs/` updated

## Checklist

- [ ] Discussed in an issue, or this is a small obvious fix
- [ ] Follows [CONTRIBUTING.md](../CONTRIBUTING.md) and [AGENTS.md](../AGENTS.md)
- [ ] No leftover compat shims or dual paths unless the issue asked for them
- [ ] Rust `use` groups follow AGENTS.md (types vs functions, trailing commas)
- [ ] Secrets, API keys, and private workspace paths are not in the diff
