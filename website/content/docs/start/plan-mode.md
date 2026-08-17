# Plan mode

Elph has collaboration modes. **Plan** focuses on design and asks before implementing. **Build** enables the full tool surface.

Approved plans may be saved under `<project>/.elph/plans/plan-*.md`. Implementing a plan exits Plan mode and restores Build tools.

Toggle from the TUI (agent mode) or:

```sh
elph run --mode=plan "design the architecture"
```

Risky shell and file actions stay visible through approval prompts. Workspace trust is recorded in `CONFIG_DIR/trust.json`.
