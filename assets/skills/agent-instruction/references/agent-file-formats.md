# Agent Instruction File Formats (cross-tool reference)

When generating or updating `AGENTS.md`, first scan the repository for existing agent instruction files from other tools.
Each tool reads a different file (or set of files). This catalogue documents the known formats, where they live, and how to reconcile them.

## Known instruction files

| Tool | File(s) | Location | Format | Notes |
|------|---------|----------|--------|-------|
| Elph / OpenAI Codex / generic | `AGENTS.md` | repo root or any subdir | Markdown | **Canonical target.** Codex reads every `AGENTS.md` whose scope covers a file it touches; nested files take precedence. |
| Claude Code (Anthropic) | `CLAUDE.md` | repo root, `~/.claude/CLAUDE.md`, or `.claude/CLAUDE.md` | Markdown | Project + user scope. Newer versions also honor `AGENTS.md` — verify the installed version. |
| GitHub Copilot (VS Code) | `.github/copilot-instructions.md`, `.github/instructions/*.instructions.md` | `.github/` | Markdown | VS Code recommends `AGENTS.md` for multi-tool workspaces and says "use only one — not both". |
| Cursor | `.cursorrules` (legacy), `.cursor/rules/*.mdc` | repo root / `.cursor/rules` | Markdown / MDC | Newer Cursor reads `AGENTS.md` too. |
| Gemini CLI (Google) | `GEMINI.md` | repo root | Markdown | Reads `AGENTS.md` as well in recent versions. |
| Codex (legacy) | `codex.md` / `CODEX.md` | repo root | Markdown | Older Codex filename; superseded by `AGENTS.md`. |
| Windsurf (Codeium) | `.windsurf/rules/*`, `.codeium/windsurf/memories` | repo root | Markdown / TOML | |
| Cline / Roo Code | `.clinerules`, `.claude/CLAUDE.md` | repo root | Markdown | Often reuses Claude Code's file. |
| Continue | `.continue/rules/*.md` | repo root | Markdown | |
| Sourcegraph Cody | `.cody/instructions/*.md` | repo root | Markdown | |
| Amazon Q Developer | `.amazonq/rules/*.md` | repo root | Markdown | |
| Aider | `.aider.conf.yml`, `.aiderignore` | repo root | YAML / text | Config, not prose; usually out of scope. |

## Detection

Scan (case-insensitive where the OS allows) for the globs above from the repo root.
Report which exist, which tool owns each, and whether `AGENTS.md` is already present.
Include nested `AGENTS.md` files in subdirectories (they matter for monorepos).

## Precedence model (from the Codex AGENTS.md spec)

- An `AGENTS.md` file's scope is the entire directory tree rooted at its folder.
- For every file the agent edits, it must obey any `AGENTS.md` whose scope includes it.
- Code-style / structure / naming rules apply only within that scope unless stated otherwise.
- More-deeply-nested `AGENTS.md` takes precedence on conflict.
- Direct user / developer / system instructions take precedence over `AGENTS.md`.
- For monorepos, the closest `AGENTS.md` in the tree wins; nest per-component files under their own directories.

## Conflict resolution

When other instruction files exist alongside (or instead of) `AGENTS.md`:

1. **Unify on `AGENTS.md` (recommended).** Make `AGENTS.md` the single source of truth. For tools that cannot yet read `AGENTS.md`, optionally write a thin redirect stub (one line pointing to `AGENTS.md`) so all tools converge — but only after the user approves editing those files.
2. **Coexist (no cross-edits).** Generate `AGENTS.md` for Elph/Codex and leave the others untouched. Add a short "Related agent instructions" note inside `AGENTS.md` for human readers; do not modify the other tools' files.
3. **Migrate & retire.** Fold the best content of the others into `AGENTS.md` and remove/archive them. **Destructive — requires explicit user approval per file.**
4. **Cancel.** Stop without writing anything.

### Reconciling contradictions

If two files disagree (e.g. tabs vs spaces, `npm test` vs `make test`, allowed vs forbidden actions, supported OS), surface each contradiction explicitly and let the user pick the canonical rule.
Do not silently pick one. Record the chosen rule in `AGENTS.md` with a one-line rationale where helpful.

## Source references

- Codex AGENTS.md spec (precedence, scope, nesting): `codex-rs/protocol/src/prompts/base_instructions/default.md` (OpenAI/codex).
- VS Code / Copilot agent-customization template & principles: `extensions/copilot/assets/prompts/skills/agent-customization/references/agent-instructions.md` (microsoft/vscode).
- OpenAI Codex AGENTS.md guide: <https://developers.openai.com/codex/guides/agents-md>
- VS Code custom instructions docs: <https://code.visualstudio.com/docs/agent-customization/custom-instructions>
