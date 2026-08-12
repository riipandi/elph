# Subagents

When collaboration tools are enabled, the harness can spawn subagents with their own
session branches, depth limits, and shared registry.

## Subagents vs Workers

- **Subagents**: Separate delegated AI agents with their own context window for independent tasks (different projects, different domains, or tasks needing isolated context). Each subagent is a completely independent AI agent session.
- **Workers**: Parallel instances of the same agent working on the same project simultaneously. Workers coordinate through the shared project store for path claiming and messaging.

## When to use Subagents

Use subagents for completely independent tasks like:
- Different projects/repos
- Different domains (e.g., frontend + backend in separate repos)
- Tasks requiring their own isolated context or different agent profiles
- Research tasks that don't need coordination with the main task

## When to use Workers

Use workers for parallelizing work on the same project:
- Simultaneous work on different files/areas of the same project
- Non-overlapping file ownership with automatic path claiming
- Coordinating via worker messages for shared understanding

## Implementation

- Parent session tracks spawn graph in `PROJECT/.elph/store.db`.
- Child sessions get artifact dirs under `APP_DATA/sessions/<child-id>/`.
- Subagents inherit model/system prompt configuration from the parent spawn config.

Use when a task benefits from isolated parallel work (review, research, scoped edits).
