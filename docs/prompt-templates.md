# Prompt Templates

Prompt templates are Markdown snippets that expand into full prompts. Type `/name` in the TUI input
to invoke a template, where `name` is the filename without `.md`.

When submitted, the expanded prompt is sent to the agent as a normal user turn. The original slash
input (for example `/identify auth`) is shown in the chat history.

## Locations

Elph loads prompt templates from:

| Scope   | Path                           | Notes                                         |
|---------|--------------------------------|-----------------------------------------------|
| Global  | `~/.elph/prompts/*.md`         | Available in every session                    |
| Project | `<workDir>/.elph/prompts/*.md` | Overrides global templates with the same name |

Set `ELPH_PROMPTS_DIR` to replace the global directory (similar to `ELPH_PROVIDERS_DIR`).

Templates are loaded when the TUI starts. Restart Elph after adding or editing template files.

## Format

```markdown
---
description: Identify the codebase architecture
argument-hint: "<focus-area>"
---
Analyze this codebase and identify its architecture.
Focus on: $1
Additional context: $@
```

| Field           | Required | Description                                                                                                                       |
|-----------------|----------|-----------------------------------------------------------------------------------------------------------------------------------|
| Filename        | Yes      | Becomes the command name. `identify.md` becomes `/identify`.                                                                      |
| `description`   | No       | Shown in autocomplete and `/help`. Falls back to the first non-empty body line (truncated to 60 characters).                      |
| `argument-hint` | No       | Shown in autocomplete before the description. Use `<angle brackets>` for required args and `[square brackets]` for optional ones. |
| Body            | Yes      | The prompt content. Supports argument placeholders (see below).                                                                   |

## Usage

Type `/` followed by the template name in the input. Autocomplete shows available templates
alongside built-in slash commands.

```
/identify                         # Expands identify.md
/review                           # Expands review.md
/component Button                 # Expands with one argument
/component Button "click handler" # Multiple arguments
```

Built-in slash commands take precedence over prompt templates. For example, `/help` always runs
the built-in help command even if `help.md` exists in the prompts directory.

## Arguments

Templates support positional arguments, defaults, and simple slicing:

| Placeholder        | Meaning                                                    |
|--------------------|------------------------------------------------------------|
| `$1`, `$2`, ...    | Positional arguments                                       |
| `$@`, `$ARGUMENTS` | All arguments joined with spaces                           |
| `${1:-default}`    | Argument 1 when present and non-empty, otherwise `default` |
| `${@:N}`           | Arguments from position N onward (1-indexed)               |
| `${@:N:L}`         | L arguments starting at position N                         |

Quoted strings are parsed bash-style (`"click handler"` is one argument).

Example template:

```markdown
---
description: Create a component
---
Create a component named $1 with features: $@
```

Usage: `/component Button "onClick handler" "disabled support"`

- `$1` → `Button`
- `$@` → `Button onClick handler disabled support`

Default values are useful for optional arguments:

```markdown
Summarize the current state in ${1:-7} bullet points.
```

Usage: `/summarize` uses `7`; `/summarize 5` uses `5`.

## Loading Rules

- Discovery is **non-recursive** — only `*.md` files directly inside the prompts directory are loaded.
- Subdirectories are ignored.
- Project templates override global templates when both define the same command name.
- Built-in commands (`/help`, `/model`, `/exit`, and so on) always win over prompt templates.

## Autocomplete

When the input starts with `/`, the command palette lists matching slash commands and prompt
templates. Each template entry shows:

```
/identify <focus-area>    Identify the codebase architecture
```

Use `Tab`, `Up`, or `Down` to select a suggestion, then continue typing arguments before pressing `Enter`.
