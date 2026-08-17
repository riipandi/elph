# Subagents

When collaboration tools are enabled, the harness can spawn subagents with their own session branches, depth limits, and a shared registry.

- **Subagents** — delegated agents with their own context (different repos, isolated research).
- **Workers** — parallel instances of the same agent on one project, coordinating through the store (path claiming, messages).

Parent session tracks the spawn graph in `PROJECT/.elph/store.db`. Child sessions get artifact dirs under `APP_DATA/sessions/<child-id>/`. Subagents inherit model and system-prompt config from the parent spawn config.
