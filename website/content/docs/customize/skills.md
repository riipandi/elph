# Skills

Skills are directories containing a `SKILL.md` file — reusable instructions the agent loads for a specific workflow.

## Locations (priority, last wins)

| Location                     | Scope                           |
| ---------------------------- | ------------------------------- |
| `CONFIG_DIR/bundled/skills/` | Built-in (shipped with binary)  |
| `~/.agents/skills/`          | User (shared agent conventions) |
| `CONFIG_DIR/skills/`         | User (Elph-specific)            |
| `<project>/.agents/skills/`  | Project                         |
| `<project>/.elph/skills/`    | Project (highest)               |

## SKILL.md format

```markdown
---
name: my-skill
description: What it does and when to use it. Triggers: /my-skill, "deploy", …
---

# My skill

Step-by-step instructions for the agent…
```

After bootstrap, `CONFIG_DIR/bundled/skills/create-skill/` is available. Invoke `/create-skill` (or ask Elph to create a skill) to scaffold a new package.

Project skill dirs always load. Extra paths that resolve to the same folder as a built-in location are ignored (no false conflict). Extra paths and name filters:

```json
{
  "resources": {
    "skills": ["~/extra/skills", "!~/.agents/skills/*", "!legacy-*"],
    "disabledSkills": ["create-skill"],
    "enableSkillCommands": true
  }
}
```

`resources.skills` entries are evaluated in order. Positive entries add extra skill directories and re-include matching discovered skills; `!` excludes by name or path, `-` uses exact matching for names (path entries use path matching), and `+` force-includes a matching skill. Leading `~` expands to the home directory, and relative paths are resolved from the project directory during workspace discovery. A bare directory path applies to everything below that directory.

For example, to keep project skills while selecting only two shared user skills:

```json
{
  "resources": {
    "skills": [
      ".agents/skills",
      "!~/.agents/skills/*",
      "~/.agents/skills/commit-only",
      "~/.agents/skills/identify"
    ]
  }
}
```

`resources.disabledSkills` is applied after discovery and removes matching names even if a path entry includes them.

`enableSkillCommands: false` keeps skills in the model catalog (`list_skills`) but does not register `/name` commands.
