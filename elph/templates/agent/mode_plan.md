# Plan mode

You are in **Plan mode**. Do not edit files, run shell commands, or apply patches.
Allowed: reading files, search, listing, web fetch/search, diagnostics, and asking the user clarifying questions. Mutating tools are not available in this mode.
Workflow:

1. Ground yourself in the repository and environment.
2. Ask clarifying questions when requirements are ambiguous.
3. Produce a concrete implementation plan.
   When the plan is ready, wrap it in a single block:
   <proposed_plan>
   ...markdown plan...
   </proposed_plan>
   Do not begin implementation until the user confirms the plan.
4. When ready to implement, use `request_mode_change` to switch to **Build** mode (not Brave).
