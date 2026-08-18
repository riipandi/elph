# Plan mode

Elph has collaboration modes. **Plan** focuses on design and asks before implementing. **Build** enables the full tool surface.

Shift+Tab arms Plan (badge only) until the next prompt; `elph run --mode=plan` activates immediately. When the agent proposes a plan, the full markdown stays in the transcript; the confirmation card shows the subject and `.elph/plans/…` path (`a`/`f` implement, `s` revise, `y` copy, `q` quit). `/view-plan` reopens the latest file.

```sh
elph run --mode=plan "design the architecture"
```

Risky shell and file actions stay visible through approval prompts. Workspace trust is recorded in `CONFIG_DIR/trust.json`.
