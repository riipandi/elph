# Skill & Tool XML Schema

Serialized XML schemas used by the agent harness to advertise skills and tools to the LLM. Both formats are designed for token efficiency while remaining fully parseable by modern models.

## Overview

| Block | Purpose | Source |
|-------|---------|--------|
| `<available_skills>` (system prompt) | Skills visible in the system prompt (compact form) | `format_skills_for_context` / `format_skills_for_system_prompt` |
| `<skill name="" path="">` | Compact single-skill advertisement — system prompt only | Same as above |
| `<available_skills>` (tool output) | Full skill catalog via `list_skills` tool (structured form) | `format_skill_catalog` |
| `<skill name="" path="">...` | Structured single-skill entry with child elements | Same as above |
| `<available_tools>` | Full tool catalog with parameter schemas | `create_list_available_tools` |
| `<tool name="" description="">` | Single tool entry with nested `<property>` children | Same as above |
| `<skill name="" path="">...content...</skill>` | Full skill invocation (slash command body) | `format_skill_invocation` |

**Key principle:** attributes carry metadata; element text carries descriptions. No redundant nesting.

---

## Skill Advertisement Format (System Prompt)

Used **only** in the system prompt. Compact single-line self-closing tags — `name` and `path` are attributes, no description or other fields included.

### Single-skill line

```xml
<skill name="rust-verify-harden" path="~/repo/.agents/skills/rust-verify-harden/SKILL.md"/>
```

### Full block

```xml
<available_skills note="On match: read SKILL.md fully before acting, resolve relative refs from its dir. Skip loosely related. No match -> proceed without one, don't browse to be thorough.">
  <skill name="rust-lean-refactor" path="~/repo/.agents/skills/rust-lean-refactor/SKILL.md"/>
  <skill name="update-models" path="~/repo/.agents/skills/update-models/SKILL.md"/>
</available_skills>
```

### Rules

| Field | Encoding | Example |
|-------|----------|---------|
| `name` | Attribute `name`, XML-escaped | `name="rust-lean-refactor"` |
| `path` | Attribute `path` (equals `file_path`), XML-escaped, home directory rendered as `~` | `path="~/repo/.agents/skills/rust-lean-refactor/SKILL.md"` |

**No description, no `trigger`, no child elements.** Paths under `$HOME` are rendered with `~` instead of the full absolute path to avoid exposing the username. Single-line compact form saves tokens.

### Relevance gating

Skills with `metadata.scope: project` are hidden when the session `cwd` is outside the skill's project root. Use `list_skills(relevance: "project")` to discover them at runtime.

---

## Skill Catalog Format (`list_skills` tool output)

Used when the model calls the `list_skills` tool. Returns the **full** structured form including all frontmatter fields — unlike the compact system-prompt form above.

### Single-skill entry

```xml
<skill name="pdf-processing" path="~/skills/pdf/SKILL.md">
  <description>Extract PDF text and tables.</description>
  <license>Apache-2.0</license>
  <compatibility>Requires poppler</compatibility>
  <allowed-tools>read shell_exec</allowed-tools>
  <metadata key="version" value="1.0"/>
  <metadata key="author" value="example-org"/>
</skill>
```

### Full block

```xml
<available_skills>
  <skill name="rust-verify-harden" path="~/repo/.agents/skills/rust-verify-harden/SKILL.md">
    <description>Run make check/lint/test and fix failures...</description>
    <license>MIT</license>
    <allowed-tools>shell_exec grep read_file write_file cargo_test</allowed-tools>
    <metadata key="scope" value="project"/>
  </skill>
  <skill name="animation-vocabulary" path="~/.agents/skills/animation-vocabulary/SKILL.md">
    <description>Reverse-lookup glossary terms for web animations.</description>
  </skill>
</available_skills>
```

### Child element rules

| Element | Condition |
|---------|-----------|
| `<description>` | Always present (required field) |
| `<license>` | `skill.license.is_some()` |
| `<compatibility>` | `skill.compatibility.is_some()` |
| `<allowed-tools>` | `skill.allowed_tools.is_some()` and non-empty — space-joined values |
| `<metadata key="..." value="..." />` | `skill.metadata.is_some()` — one per key-value pair |

Paths use the same `~/` tilde shorthand as the system prompt format.

---

## Tool Catalog Format

Produced by `list_available_tools`. Uses `quick-xml` serde serialization.

### Single-tool entry

```xml
<tool name="read_file" description="Read file contents. Prefer offset/limit (or ranges) after grep hits — do not load whole large files.">
  <property name="path" type="string" required="true">File path (absolute or relative to cwd). Use one of: read_file, write_file, edit_file, etc.</property>
  <property name="offset" type="number"/>
  <property name="limit" type="number"/>
</tool>
```

### Full catalog

```xml
<available_tools>
  <tool name="read_file" description="Read file contents.">
    <property name="path" type="string" required="true">File path</property>
    <property name="offset" type="number"/>
    <property name="limit" type="number"/>
  </tool>
  <tool name="grep" description="Search file contents with ripgrep.">
    <property name="pattern" type="string" required="true"/>
    <property name="glob" type="array of string" description="Restrict search to files matching these globs."/>
    <property name="limit" type="number"/>
  </tool>
  <tool name="spawn_agent" description="Spawn a subagent to handle a focused task in an isolated context.">
  </tool>
</available_tools>
```

### Property element rules

| Attribute | Type | Meaning |
|-----------|------|---------|
| `name` | string | Parameter name (required) |
| `type` | string | JSON-Schema type: `string`, `number`, `boolean`, `array`, `object`, `array of string`, `array of object`, `string\|number` (union), `any` |
| `required` | `true`/`false` | Present only when `true`; omitted when false |
| `enum` | string | Pipe-separated enum values: `"auto\|ddg\|exa"` |
| `description` | string | **Attribute** when property has nested children; **text node** for leaf properties |

### Description placement rule

```
Leaf property (no children):
  <property name="path" type="string" required="true">File path</property>

Object/array-of-object property (has children):
  <property name="ranges" type="array of object" description="Per-range settings">
    <property name="path" type="string" required="true"/>
    <property name="offset" type="number"/>
  </property>
```

This avoids duplication: description is either inline text OR a `description` attribute, never both.

### Empty-parameter tools

Tools with no `properties` (e.g. `spawn_agent` bare variant, `get_goal`) omit the `<property>` block entirely:

```xml
<tool name="get_goal" description="Get the current session goal status and remaining budgets."/>
```

---

## Skill Invocation Format

Used when a skill is dispatched via `/skill:name [args]` or slash palette. Includes full SKILL.md content plus metadata.

```xml
<skill name="rust-verify-harden" path="~/repo/.agents/skills/rust-verify-harden/SKILL.md">
<license>MIT</license>
<compatibility>Requires cargo, rustc, sqlite</compatibility>
<allowed-tools>shell_exec read_file write_file grep cargo_test</allowed-tools>
<meta key="scope" value="project" />
<meta key="version" value="2.1" />
References are relative to ~/repo/.agents/skills/rust-verify-harden.

# rust-verify-harden

Run make check/lint/test (cargo fmt/check/clippy/test) and fix failures...

## Memory safety audit
Check for leaks, deadlocks, data races...
</skill>
```

### Optional fields

| Tag | Condition |
|-----|-----------|
| `<license>` | `skill.license.is_some()` |
| `<compatibility>` | `skill.compatibility.is_some()` |
| `<allowed-tools>` | `skill.allowed_tools.is_some()` and non-empty |
| `<meta key="..." value="..." />` | `skill.metadata.is_some()` — one per key-value pair |

### Reference resolution

The line `References are relative to {skill_dir}.` tells the model where to resolve relative paths in the skill content. `skill_dir = dirname(file_path)`.

---

## Escaping

Both formats use XML escaping for all attribute and text values:

| Char | Escape |
|------|--------|
| `&` | `&amp;` |
| `<` | `&lt;` |
| `>` | `&gt;` |
| `"` | `&quot;` |
| `'` | `&apos;` |

Control characters outside XML 1.0 range (NUL, surrogate pairs) are stripped by `xml_clean()` before serialization.

---

## Implementation Reference

| Function | File | Output |
|----------|------|--------|
| `format_skills_for_system_prompt` | `crates/elph-agent/src/agent/harness/system_prompt.rs` | Skill ad block for system prompt |
| `format_skills_for_context` | same | Filtered + formatted for cwd |
| `format_skill_catalog` | `crates/elph-agent/src/tools/list_skills.rs` | `list_skills` tool output |
| `format_tool_catalog` | `crates/elph-agent/src/tools/list_available_tools.rs` | `list_available_tools` tool output |
| `format_skill_invocation` | `crates/elph-agent/src/skills/format.rs` | Full skill dispatch body |
