---
name: agent-instruction
description: >-
    Generate or refresh the AGENTS.md agent-instructions file for a repository
    (the open cross-tool standard used by Elph, OpenAI Codex, VS Code/Copilot,
    Cursor, and Gemini CLI). Scans the repo for existing agent instructions from
    other tools (CLAUDE.md, .cursorrules, .github/copilot-instructions.md,
    GEMINI.md, codex.md, Windsurf, etc.), resolves conflicts, and updates the doc
    to match the current project/repo state. Always asks the user to confirm
    before writing. Use when the user wants to create, generate, write, update,
    refresh, sync, or repair AGENTS.md, agent instructions, repo guidance, or runs
    /agent-instruction.
metadata:
    scope: project
---

# Agent Instruction (AGENTS.md)

## Language

- **Generated file** (`AGENTS.md`) and any files this skill writes: **always English**, regardless of chat language. Keep paths, commands, symbols, and tool names literal.
- **In-chat responses / questions**: follow the language the user is currently using (Indonesian, English, …). Keep file paths, commands, and identifiers literal.

## Purpose

Produce a high-signal `AGENTS.md` that tells any AI coding agent how to work in this repository.
The file is the **canonical, open-format** instruction doc (Codex reads it; VS Code/Copilot, Cursor, and Gemini CLI also read it).
The skill also reconciles conflicts with other tools' instruction files and can **refresh an existing** `AGENTS.md` against the repo's current state.

## When to use

- "Generate an AGENTS.md for this repo."
- "Update / refresh our agent instructions with the latest setup."
- "We have CLAUDE.md and .cursorrules — consolidate them into AGENTS.md."
- `/agent-instruction`

## References

Read these before generating — they carry the detection rules, precedence model, and the section template:

- `references/agent-file-formats.md` — cross-tool file catalogue, detection globs, precedence, conflict-resolution options.
- `references/agents-md-template.md` — the section skeleton and writing principles.

## Workflow

### Phase 0 — Scan & conflict detection (always first)

1. From the repo root, detect existing agent instruction files using the globs in `references/agent-file-formats.md` (case-insensitive). Record each hit: path, owning tool, and whether `AGENTS.md` already exists. Include nested `AGENTS.md` files for monorepos.
2. If `AGENTS.md` exists, read it and parse its current sections.
3. Note any **contradictions** between detected files (indentation, test commands, forbidden actions, supported OS, …).

Do not skip this phase even when the user only asked to "create" a file — the whole point is to avoid clobbering or duplicating other agents' instructions.

### Phase 1 — Decide mode

- **Create mode**: no `AGENTS.md` yet → author from scratch using the template.
- **Update mode**: `AGENTS.md` already exists → refresh it. Re-scan the repo for drift (new deps, changed build/test commands, new CI, renamed modules, new conventions). Preserve still-valid project rules; update or remove stale ones. Ask the user whether to do a **full rewrite** or a **surgical update** of specific sections.

### Phase 2 — Resolve conflicts

If other instruction files exist, pick a strategy (see the references file):

- **Unify on AGENTS.md (recommended)** — make `AGENTS.md` the single source of truth.
- **Coexist** — write `AGENTS.md` only; leave the others untouched; add a "Related Agent Instructions" note for humans.
- **Migrate & retire** — fold the others into `AGENTS.md` and remove them. **Destructive**: requires explicit per-file user approval; never auto-delete.
- **Cancel** — stop.

For each contradiction found in Phase 0, present the conflicting rules and let the user choose the canonical one. Never silently pick a side.

### Phase 3 — Gather current project state

Explore the repo to ground the content (do not invent):

- `README.md`, `CONTRIBUTING.md`, `docs/**` — overview, conventions, testing.
- Manifest files: `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `pom.xml`, `Gemfile`, `*.csproj`, `requirements.txt`, `pnpm-workspace.yaml`, …
- Build/test/lint entrypoints: `Makefile`, `justfile`, CI workflows (`.github/workflows/**`, `.gitlab-ci.yml`), `rust-toolchain.toml`, `.nvmrc`.
- Directory layout & architecture (top-level dirs, `src/`, `crates/`, `apps/`).
- Lint/format configs: `clippy.toml`, `.editorconfig`, `biome.json`, `eslint.config.*`, `ruff.toml`, `rustfmt.toml`.
- Search existing docs for agent-critical gotchas; **link** them rather than copying.

### Phase 4 — Confirm before execution (mandatory)

Before writing **any** file, present a concise plan to the user and ask for confirmation via `ask_user_question` (always `allow_custom: true`):

- Mode (create / update-full / update-surgical).
- Target path (`AGENTS.md` at repo root, or nested for a monorepo component).
- Conflict-resolution choice and any files you intend to **modify or delete**.
- The sections you plan to include.

Offer at least: **Proceed**, **Modify plan**, **Cancel**. Only write after an affirmative answer. If the plan includes deleting/migrating other tools' files, make that explicit and require explicit approval for those specific files.

### Phase 5 — Generate / write

1. Build the `AGENTS.md` from `references/agents-md-template.md`. Include only sections the project benefits from. Every line must change agent behavior. Follow the **lean-but-clear** writing rules there: short active sentences, lead with the rule, one concrete example over a paragraph, no preamble or recap.
2. In update mode, merge: keep valid existing rules, apply Phase 3 findings, drop stale content, and note what changed if useful.
3. Respect the **precedence model**: scope rules to the directory tree; for monorepos, nest `AGENTS.md` per component (closest file wins).
4. Write the file with `write_file`. If migrating, only delete/archive other files after the confirmed approval from Phase 4.

### Phase 6 — Verify & report

1. Read back the written `AGENTS.md` to confirm it rendered correctly.
2. Report: mode used, path written, conflicts found and how each was resolved, and any files left untouched or removed.
3. Tell the user how agents consume it (Codex/Elph read `AGENTS.md` automatically; note any tool that needs a redirect stub).
4. If you only updated, summarize the delta from the previous version.

## Safety & constraints

- **Never write without the Phase 4 confirmation.**
- **Never delete or overwrite another tool's instruction file without explicit per-file approval.** Coexist is the safe default.
- **Ground everything in the repo.** Do not fabricate commands, versions, or conventions. If unsure, say so and ask.
- Keep the generated file **minimal and current** — avoid the anti-patterns in the references (kitchen-sink, doc duplication, linter-restating).
- When the repo already has `AGENTS.md`, prefer surgical update over full rewrite unless the user asks otherwise.

## Source references

- Codex AGENTS.md spec (precedence, scope, nesting): OpenAI/codex `codex-rs/protocol/src/prompts/base_instructions/default.md`.
- VS Code / Copilot agent-customization template & principles: microsoft/vscode `extensions/copilot/assets/prompts/skills/agent-customization/references/agent-instructions.md`.
- OpenAI Codex AGENTS.md guide: <https://developers.openai.com/codex/guides/agents-md>
