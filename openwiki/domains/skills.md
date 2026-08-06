---
type: Concept
title: Skills — SKILL.md Discovery and Resolution
description: Elph's skill system — SKILL.md file format, YAML frontmatter parsing, fail-open diagnostics, and MiniJinja template invocation
tags: [skills, skill-file, frontmatter, MiniJinja, templates]
---

# Skills

The skill system lives in `crates/elph-agent/src/skills/`. It discovers `SKILL.md` files in the repository and loads them as structured skill definitions for the agent. Skills are invoked during the [Agent Loop](../workflows/agent-loop.md) turn cycle, either programmatically via `AgentHarness::skill()` or through the TUI slash command system (see [Operations](../operations.md)).

[Prompt templates](../quickstart.md#backlog) (MiniJinja-based) live in `crates/elph-agent/src/prompt/` and `crates/coding-agent/templates/agent/`, separate from the skill system.

## Module Structure

```
crates/elph-agent/src/skills/
├── mod.rs     — re-exports
├── args.rs    — argument validation for skill invocations
├── format.rs  — format_skill_invocation() — renders skill as XML block
└── load/
    ├── mod.rs       — load_skills(), load_skills_with_options(), frontmatter parsing
    ├── types.rs     — LoadSkillsResult, SourcedSkill, SkillDiagnostic, SkillDiagnosticCode
    ├── ignore.rs    — IgnoreMatcher (respects .gitignore, .elphignore)
    └── parse.rs     — frontmatter parsing, name/description/compatibility validation
```

## SKILL.md Format

Skills are Markdown files with YAML frontmatter, typically named `SKILL.md`:

```markdown
---
name: "my-skill"
description: "Does X, Y, Z"
compatibility: "elph >= 0.0.28"
license: "MIT"
disable-model-invocation: false
allowed-tools: "read_file grep"
argument-hint: "<query>"
metadata:
    author: "user"
    priority: "high"
---

# Skill content

This content is injected into the system prompt when the skill is invoked.
References are relative to the skill directory.
```

## Frontmatter Fields

Parsed by `SkillFrontmatter` struct (from `load/mod.rs`):

| Field                      | Type                             | Required | Description                                 |
| -------------------------- | -------------------------------- | -------- | ------------------------------------------- |
| `name`                     | `Option<String>`                 | No       | Skill name; defaults to filename            |
| `description`              | `Option<String>`                 | No       | Purpose description                         |
| `disable-model-invocation` | `Option<bool>`                   | No       | If true, prevent model-initiated invocation |
| `license`                  | `Option<String>`                 | No       | License identifier                          |
| `compatibility`            | `Option<String>`                 | No       | Version constraint (e.g. `elph >= 0.0.28`)  |
| `allowed-tools`            | `Option<String>`                 | No       | Space-separated tool allow list             |
| `argument-hint`            | `Option<String>`                 | No       | Hint for argument format                    |
| `metadata`                 | `Option<HashMap<String, Value>>` | No       | Arbitrary key-value metadata                |

## Skill Loading

`load_skills()` (from `load/mod.rs`):

```rust
pub async fn load_skills(
    fs: &impl FileSystem,
    options: SkillLoadOptions,
) -> Result<LoadSkillsResult>
```

- Scans for `SKILL.md` files from the configured paths.
- Parses YAML frontmatter.
- Validates `name`, `description`, and `compatibility`.
- Returns `LoadSkillsResult` with valid skills + diagnostics for failures.
- Fail-open: invalid skills are reported as diagnostics, not errors.

## Skill Invocation Format

`format_skill_invocation()` (from `format.rs`):

```rust
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String
```

Renders the skill as an XML block:

```xml
<skill name="my-skill" location="/path/to/SKILL.md">
<license>MIT</license>
<compatibility>elph >= 0.0.28</compatibility>
<allowed-tools>read_file grep</allowed-tools>
<meta key="author" value="user" />

Skill content here. References are relative to /path/to/.

</skill>
```

## Invocation

Skills are invoked via:

- `AgentHarness::skill(name, additional_instructions)` — programmatic invocation
- `/skill:<name> [args]` — slash command in the TUI
- The prompt title is set to `/skill:{name} [args]` for transcript/history tracking.

## Source References

- `crates/elph-agent/src/skills/load/mod.rs` — `load_skills()`, `SkillFrontmatter`, `diagnostic()`
- `crates/elph-agent/src/skills/load/types.rs` — `LoadSkillsResult`, `SourcedSkill`, `SkillDiagnostic`, `SkillDiagnosticCode`
- `crates/elph-agent/src/skills/load/parse.rs` — `parse_frontmatter()`, `validate_name()`, `validate_compatibility()`
- `crates/elph-agent/src/skills/format.rs` — `format_skill_invocation()`
- `crates/elph-agent/src/skills/args.rs` — argument validation utilities
