# Subagents

When collaboration tools are enabled, the harness can spawn subagents with their own
session branches, depth limits, and shared registry.

- Parent session tracks spawn graph in `APP_DATA/metadata.db`.
- Child sessions get artifact dirs under `APP_DATA/projects/<child-id>/`.
- Subagents inherit model/system prompt configuration from the parent spawn config.

Use when a task benefits from isolated parallel work (review, research, scoped edits).
