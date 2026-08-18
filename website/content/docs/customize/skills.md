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

Project skill dirs load only when the workspace is trusted. Extra paths and name filters:

```json
{
  "resources": {
    "skills": ["~/extra/skills", "!legacy-*"],
    "disabledSkills": ["create-skill"],
    "enableSkillCommands": true
  }
}
```

`enableSkillCommands: false` keeps skills in the model catalog (`list_skills`) but does not register `/name` commands.
