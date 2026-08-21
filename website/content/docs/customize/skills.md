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

`resources.skills` entries with a `!` or `-` prefix exclude a skill by name or path (leading `~` expands to the home dir; a bare directory path excludes everything under it; a relative path like `.agents/skills` matches any project at that relative location); `+` force-includes. `resources.disabledSkills` drops skills by name glob after discovery.

`enableSkillCommands: false` keeps skills in the model catalog (`list_skills`) but does not register `/name` commands.
