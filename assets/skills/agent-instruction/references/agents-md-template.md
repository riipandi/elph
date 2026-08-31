# AGENTS.md Template

Use this skeleton. Include only sections the project benefits from — every line should change agent behavior.
Ground every claim in the actual repo; link to existing docs instead of copying them (VS Code principle: "Link, don't embed").

```markdown
# AGENTS.md

Short one-line description of what this repo is and who the agent is helping.

## Project Overview
- What the project does and its primary language/stack.
- Key entry points and where to start reading.

## Tech Stack & Tooling
- Languages, frameworks, package managers.
- Required toolchain (versions matter): e.g. Rust 1.8x, Node 20, Python 3.12.
- How the project is built and run (reference the real commands).

## Build / Test / Lint
- Exact commands agents should run, e.g. `make build`, `make test`, `make lint`.
- Note when tests are slow or need network/sandbox and should be gated.

## Architecture
- Major components and service boundaries.
- The "why" behind structural decisions (not just the "what").
- For monorepos: which subdirectories own which concerns.

## Conventions
- Patterns that differ from common practice — include specific examples.
- Naming, error handling, module size limits, doc-comment expectations.
- Things enforced by linters — skip those (obvious-instructions anti-pattern).

## Common Tasks
- Frequent workflows (add a command, add a migration, cut a release).
- Where to find the relevant code/docs for each.

## Gotchas / Anti-patterns
- Known footguns, flaky tests, environment prerequisites.
- Actions the agent must NOT take (e.g. don't force-push, don't bypass review).

## Related Agent Instructions
- If other tools' instruction files coexist (CLAUDE.md, .cursorrules, .github/copilot-instructions.md, GEMINI.md, ...), note them here and how they relate to this file. Prefer a single canonical source.
```

## Writing rules (lean but clear)

Write for a busy agent, not a reader browsing docs. Be terse without losing meaning.

- **Lead with the rule.** State the instruction; add the reason only if it changes behavior.
- **One idea per line.** Short active sentences. No conjunctions chaining three clauses.
- **Straightforward, not clever.** Plain words beat jargon. Say "run `make test`", not "execute the test target".
- **Examples over prose.** One concrete snippet beats a paragraph of description.
- **No preamble or recap.** Skip "This document describes…" openings. Start at the rule.
- **Cut ruthlessly.** Delete any line that holds for every repo or that a linter already enforces.

## Section guidance

- **Minimal by default**: only what's relevant to *every* task.
- **Concise and actionable**: every line guides behavior.
- **Link, don't embed**: reference `docs/**`, `CONTRIBUTING.md`, `README.md`; inline only agent-critical gotchas not documented elsewhere.
- **Keep current**: refresh when practices change (this is the update-mode job).
- **Monorepo**: nest `AGENTS.md` per component; closest file wins.

## Anti-patterns to avoid

- Maintaining both `AGENTS.md` and `copilot-instructions.md` / `CLAUDE.md` with duplicated content.
- Kitchen-sink files: dumping everything instead of what matters.
- Copying the README instead of linking.
- Restating conventions already enforced by linters.
