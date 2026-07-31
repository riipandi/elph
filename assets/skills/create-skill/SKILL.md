---
name: create-skill
description: >
    Interactively create a new Elph skill (SKILL.md + optional scripts/references).
    Use when the user wants to create a skill, scaffold a skill, or runs /create-skill.
metadata:
    short-description: "Create a new Elph skill"
---

# Create Skill

Interactively gather requirements from the user and create a fully working Elph skill on disk.

## Resolve paths

Resolve the Elph config directory before creating a **user-scoped** skill:

- If `ELPH_HOME` is set, use it as `CONFIG_DIR`.
- Otherwise use `~/.config/elph` (or `$XDG_CONFIG_HOME/elph` when `XDG_CONFIG_HOME` is set).

Resolve to an absolute path. Use it wherever `<config-dir>` appears below.

Project skills live under the workspace:

- Preferred: `<repo-root>/.elph/skills/<name>/SKILL.md`
- Also valid: `<repo-root>/.agents/skills/<name>/SKILL.md`

## Step 1: Gather information

Ask the user the following questions **one at a time as regular conversation questions**
(do not use structured multi-select for free-text inputs):

1. **Skill name** — lowercase letters (a-z), digits (0-9), and hyphens only. Must start and
   end with a letter or digit. Length 2–64 (e.g. `deploy-k8s`). Validate before continuing.
2. **Scope** — present two options:
    - **Project** (Recommended when inside a git repo):
      `<repo-root>/.elph/skills/<name>/SKILL.md`
    - **User**: `<config-dir>/skills/<name>/SKILL.md` — available in all projects
    - Default to **Project** if inside a git repo, otherwise **User**.
3. **What it should do** — workflow description, example prompt, or task to automate.

## Step 2: Draft the description

Write a `description` frontmatter value that includes:

- What the skill does (1–2 sentences)
- Trigger phrases and keywords so Elph can auto-invoke it
- The slash command name (e.g. "Use when the user runs /deploy-k8s")

Show the draft and let the user approve or edit it.

## Step 3: Create the directory

```bash
mkdir -p <SKILL_DIR>
```

Where `<SKILL_DIR>` is:

- User: `<config-dir>/skills/<name>`
- Project: `<repo-root>/.elph/skills/<name>`

Optional: also create `<SKILL_DIR>/scripts/` and `<SKILL_DIR>/references/` if needed.

## Step 4: Write SKILL.md

Create `<SKILL_DIR>/SKILL.md` with this exact frontmatter shape:

```markdown
---
name: <skill-name>
description: <the description from Step 2>
---

<markdown body with instructions, steps, code blocks>
```

Write any supporting files the same way. Always use absolute paths.

## Step 5: Verify and confirm

1. Read back `SKILL.md` to confirm it was written correctly.
2. Tell the user how to use it:
    - Slash: `/<skill-name>`
    - Palette: type `/` and fuzzy-match the name
    - Automatic: when the description matches user intent
3. Note that skills reload when resources are refreshed (`/reload` or next session start).

## Guidelines

- Keep the body focused and actionable — it is a prompt for the agent, not end-user docs.
- The `description` field controls discovery and auto-invocation; be specific with triggers.
- Prefer existing CLI tools over custom scripts.
- Do not skip creating the directory.
- Never write into `CONFIG_DIR/bundled/skills/` (that tree is for built-ins only).
