# Plan mode

Elph has collaboration modes. **Plan** focuses on design and asks before implementing. **Build** enables the full tool surface.

Shift+Tab arms Plan (badge only) until the next prompt; `elph run --mode=plan` activates immediately. When the agent proposes a plan, the full markdown stays in the transcript; the confirmation card shows the subject, `.elph/plans/…` path, and a selectable action list (`↑↓` + Enter, or `a` / `f` / `s` / `q`). `/view-plan` reopens the latest file.

```sh
elph run --mode=plan "design the architecture"
```

In Plan, mutating workspace tools prompt **Allow once** or **Deny** only. That grant does not switch to Build. Repeated Allow once on write/edit/shell still changes files — deny calls that implement rather than investigate. Switching to implementation still waits for the plan confirmation card. Multi-agent and mutating MCP tools stay unavailable. Headless `elph run --mode=plan` denies mutating tools (no approval UI). Workspace trust is recorded in `CONFIG_DIR/trust.json`.
