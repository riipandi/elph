# Plan mode

Elph has collaboration modes. **Plan** focuses on design and asks before implementing. **Build** enables the full tool surface.

Shift+Tab arms Plan (badge only) until the next prompt; `elph run --mode=plan` activates immediately. When the agent proposes a plan, the TUI opens a review: scroll lines, `c` to comment, `s` to request changes, `y` to copy, `a`/`f` to implement, `q` to leave Plan. `/view-plan` reopens the latest file under `<project>/.elph/plans/plan-*.md`.

```sh
elph run --mode=plan "design the architecture"
```

Risky shell and file actions stay visible through approval prompts. Workspace trust is recorded in `CONFIG_DIR/trust.json`.
