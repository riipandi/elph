# Skills

Skills provide reusable instructions for specific tasks. They are loaded from `SKILL.md` files following the [agentskills.io](https://agentskills.io) specification.

## Directory Structure

A skill is a directory containing at minimum a `SKILL.md` file:

```
skill-name/
├── SKILL.md          # Required: metadata + instructions
├── scripts/          # Optional: executable code
├── references/       # Optional: documentation
├── assets/           # Optional: templates, resources
└── ...               # Any additional files or directories
```

## SKILL.md Format

The `SKILL.md` file contains YAML frontmatter followed by Markdown content:

```markdown
---
name: skill-name
description: A description of what this skill does and when to use it.
license: MIT
compatibility: Requires git and rust-analyzer
metadata:
  author: your-org
  version: "1.0"
allowed-tools: read grep bash
---

# Skill Instructions

Your skill content here...
```

### Frontmatter Fields

| Field           | Required | Constraints                                       |
| --------------- | -------- | ------------------------------------------------- |
| `name`          | Yes      | Max 64 chars. Lowercase, numbers, hyphens only.   |
| `description`   | Yes      | Max 1024 chars. Non-empty.                        |
| `license`       | No       | License name or reference to bundled file.        |
| `compatibility` | No       | Max 500 chars. Environment requirements.          |
| `metadata`      | No       | Arbitrary key-value mapping.                      |
| `allowed-tools` | No       | Space-separated list of pre-approved tools.       |

### Name Field Rules

- Must be 1-64 characters
- May only contain lowercase alphanumeric characters (`a-z`, `0-9`) and hyphens (`-`)
- Must not start or end with a hyphen
- Must not contain consecutive hyphens (`--`)
- Must match the parent directory name

## Loading Skills

### Basic Usage

```rust
use elph_agent::load_skills;

let result = load_skills(&env, &[".agents/skills"]).await;

for skill in &result.skills {
    println!("{}: {}", skill.name, skill.description);
}
```

### With Custom Options

```rust
use elph_agent::{
    load_skills_with_options, SkillLoadOptions, SkillValidationSettings,
};

let options = SkillLoadOptions {
    validation: SkillValidationSettings { strict_mode: true },
};

let result = load_skills_with_options(&env, &dirs, Some(&options)).await;
```

### Directory Resolution

Resolve directories based on app name (elph-agent is agnostic):

```rust
use elph_agent::{resolve_user_skills_dirs, resolve_project_skills_dirs};

// User-level: ~/.agents/skills, ~/.elph/skills, ~/.elph/bundled/skills
let user_dirs = resolve_user_skills_dirs("elph");

// Project-level: {project}/.agents/skills, {project}/.elph/skills
let project_dirs = resolve_project_skills_dirs("/project", "elph");
```

### Directory Priority

Directories are processed in order. **Last-wins**: later directories override earlier ones with the same skill name.

```
~/.agents/skills/*           # 1. Generic (lowest priority)
~/.{app_name}/skills/*       # 2. App-specific
~/.{app_name}/bundled/skills # 3. App bundled (highest priority)
```

## Conflict Resolution

When the same skill name exists in multiple directories, **last-wins**:

```rust
// .agents/skills/my-skill/SKILL.md  → description: "First"
// .elph/skills/my-skill/SKILL.md    → description: "Second (wins!)"

let result = load_skills(&env, &[".agents/skills", ".elph/skills"]).await;
// result.skills[0].description == "Second (wins!)"
```

## Validation

### Lenient Mode (Default)

No diagnostics for optional field violations. Skills load even with invalid compatibility length.

### Strict Mode

Emits diagnostics for optional field violations:

```rust
let options = SkillLoadOptions {
    validation: SkillValidationSettings { strict_mode: true },
};

let result = load_skills_with_options(&env, &dirs, Some(&options)).await;

for diag in &result.diagnostics {
    println!("{}: {}", diag.code, diag.message);
}
```

## Formatting

### System Prompt

Format skills for inclusion in system prompt:

```rust
use elph_agent::harness::format_skills_for_system_prompt;

let prompt = format_skills_for_system_prompt(&skills);
// <available_skills>
//   <skill>
//     <name>my-skill</name>
//     <description>...</description>
//     <location>/path/to/SKILL.md</location>
//   </skill>
// </available_skills>
```

### Skill Invocation

Format skill for invocation with optional instructions:

```rust
use elph_agent::format_skill_invocation;

let invocation = format_skill_invocation(&skill, Some("Focus on security."));
// <skill name="my-skill" location="/path/to/SKILL.md">
// <license>MIT</license>
// <allowed-tools>read grep</allowed-tools>
// References are relative to /path/to.
//
// Skill content...
// </skill>
//
// Focus on security.
```

## Examples

See [`examples/agent_skills.rs`](../examples/agent_skills.rs) for a comprehensive demonstration:

```bash
cargo run -p elph-agent --example agent_skills
```

This example demonstrates:
- Loading skills from multiple directories
- Parsing all agentskills.io spec fields
- Conflict resolution (last-wins)
- Strict vs lenient validation
- Directory resolution with app name
- Formatting skills for system prompt and invocation
