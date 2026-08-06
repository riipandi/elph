# Prompt Templates

Design for Markdown snippets that expand into full prompts. Type `/name` in the TUI where `name` is the filename without `.md`.

## Locations

| Scope   | Path                                                     | Notes                        |
| ------- | -------------------------------------------------------- | ---------------------------- |
| Global  | `~/.elph/prompts/*.md`                                   | Available in every session   |
| Project | `<workDir>/.elph/prompts/*.md` or `.agents/prompts/*.md` | Overrides global by filename |

`ELPH_PROMPTS_DIR` replaces the global directory.

Templates load at TUI startup. Restart after adding or editing files.

## Format

Example template file:

```markdown
---
description: Identify the codebase architecture
argument-hint: "<focus-area>"
---

Analyze this codebase and identify its architecture.
Focus on: $1
Additional context: $@
```

| Field           | Required | Description                                                   |
| --------------- | -------- | ------------------------------------------------------------- |
| Filename        | Yes      | `identify.md` → `/identify`                                   |
| `description`   | No       | Autocomplete and `/help`; fallback first body line (60 chars) |
| `argument-hint` | No       | Palette hint; `<required>` `[optional]`                       |
| Body            | Yes      | Prompt content with placeholders                              |

## Usage examples

```
/identify
/review
/component Button
/component Button "click handler"
```

Built-in slash commands win over templates (e.g. `/help` even if `help.md` exists).

## Arguments

| Placeholder        | Meaning                               |
| ------------------ | ------------------------------------- |
| `$1`, `$2`, …      | Positional arguments                  |
| `$@`, `$ARGUMENTS` | All arguments joined with spaces      |
| `${1:-default}`    | Default when arg missing or empty     |
| `${@:N}`           | Arguments from position N (1-indexed) |
| `${@:N:L}`         | L arguments starting at position N    |

Quoted strings count as one argument (bash-style).

## Loading rules

- Non-recursive — only `*.md` directly in the directory
- Project overrides global for the same command name
- Built-in slash commands always win

## Autocomplete

Palette entry format:

```
/identify <focus-area>    Identify the codebase architecture
```

- No args: Enter runs immediately
- With `argument-hint`: Enter completes to `/name `; second Enter submits with args

## Display

1. **User message** — slash input, e.g. `/identify auth`
2. **Detail block** — collapsed, label `Prompt`, one-line preview

Expand/collapse with `Ctrl+O` or header click. Thinking blocks follow `autoExpandThinking`.

## Related

- [slash-commands.md](./slash-commands.md)
- [configuration.md](./configuration.md)
- [tui.md](./tui.md)
