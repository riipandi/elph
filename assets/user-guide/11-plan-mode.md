# Plan Mode

Elph supports collaboration modes (Plan vs Build). In Plan mode the agent focuses on
design and asks for confirmation before implementing; Build mode enables the full tool
surface.

- Approved plans may be saved under `<project>/.elph/plans/plan-*.md`.
- Implementing a plan exits Plan mode and restores the Build tool surface.

Toggle / enter plan flows via the TUI and agent collaboration tools when multi-agent
features are enabled.
