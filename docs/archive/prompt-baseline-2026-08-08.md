# Prompt Token Baseline — 2026-08-08

Measured with the tokenx estimator (`elph_ai::utils::estimate::count_tokens_text`,
the same one the compaction module reuses) via
`crates/coding-agent/tests/prompt_baseline.rs`.

Fixture: 18 registered skills + 33 native/memory tools (no MCP), cwd
`/tmp/elph-project`, no AGENTS.md.

## Before (pre-Phase 1)

| Mode  | Full prompt tokens | `<available_skills>` tokens | `<available_tools>` tokens | Full prompt bytes |
|-------|-------------------|-----------------------------|----------------------------|-------------------|
| Build | 4083              | 1279                        | 379                        | 13538             |
| Plan  | 3876              | 1279                        | 379                        | 12800             |
| Brave | 4048              | 1279                        | 379                        | 13412             |

## After (Phase 1 + Phase 2)

| Mode  | Full prompt tokens | `<available_skills>` tokens | `<available_tools>` tokens | Full prompt bytes |
|-------|-------------------|-----------------------------|----------------------------|-------------------|
| Build | 3704              | 913                         | 379                        | 12389             |
| Plan  | 3497              | 913                         | 379                        | 11649             |
| Brave | 3669              | 913                         | 379                        | 12263             |

## Delta

| Mode  | Full prompt Δ | `<available_skills>` Δ |
|-------|--------------|------------------------|
| Build | –379         | –366                   |
| Plan  | –379         | –366                   |
| Brave | –379         | –366                   |

The project-scoped skills (`scope: project` in `metadata`) are filtered out
because the fixture cwd (`/tmp/elph-project`) is outside the elph repo.
`<available_skills>` tokens dropped by 28.6%.

Notes:
- `<available_skills>` is mode-independent; mode deltas come from the appendix.
- Re-run: `cargo test -p elph --test prompt_baseline -- --nocapture`.
