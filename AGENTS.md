# Agent Instructions

<!-- OPENWIKI:START -->

## OpenWiki

This repository uses OpenWiki for recurring code documentation. Start with `openwiki/quickstart.md`, then follow its links to architecture, workflows, domain concepts, operations, integrations, testing guidance, and source maps.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->

---

## Documentation Updates

After completing a task that introduces a **significant change** — new feature, bug fix that alters behavior/contract, breaking change, new config/env var, new public API, or architectural change — update the relevant docs in `docs/` **before** considering the task done.

### What counts as "significant"

- New or removed public API, endpoint, CLI command, or config option.
- Behavior change a consumer/caller would notice (not pure internal refactor).
- Bug fix that changes documented behavior, edge-case handling, or error contract.
- New external dependency, service, vendored patch, or integration.
- New module/subsystem, or a change to how existing ones interact.

Not significant (skip docs update): internal refactor with no behavior change, formatting/lint fixes, test-only changes, typo fixes, dependency version bumps with no API impact.

### What to update

1. Find existing doc(s) in `docs/` covering the affected area (search by feature/module name first — do not assume a doc doesn't exist).
2. If found: update in place — keep structure/tone consistent with the rest of the file, only touch sections affected by the change.
3. If not found and the change warrants one: create a new doc under `docs/` following the existing naming/structure convention in that folder.
4. Do **not** edit generated OpenWiki pages (see OpenWiki section above) — if the change should surface there, it will regenerate on the next scheduled run. Update source docs/comments instead if OpenWiki pulls from them.

### Rules

- Docs update is part of the task, not a follow-up — don't mark the task complete without it when the change qualifies as significant.
- Docs must represent the **current, actual state of the implementation** — describe what the code does now, not what it's planned to do, used to do, or should ideally do. Verify against the changed code itself before writing, don't rely on memory of the old behavior.
- If existing doc content is now inaccurate or outdated because of this change, correct it — don't leave stale statements alongside the new ones.
- Remove or update any docs referencing removed/renamed APIs, params, or behavior as part of the same task.
- If ambiguous whether a change is "significant enough," ask via `ask_user_question` rather than guessing (skip this if the tool isn't available in this agent's toolset — state the assumption instead and proceed).
- Keep doc changes scoped to what the code change actually affects — no unrelated rewrites.
- Match existing `docs/` file format (Markdown conventions, heading levels, code block style) already used in that folder.

---

## Implementation Principles

- Choose the simplest implementation that fully meets the current requirements. No speculative abstraction, no extra config/flags/hooks for hypothetical future needs — solve what's asked, not what might be asked later.
- Do not preserve backward compatibility. Change/remove old APIs, signatures, schemas, or behavior directly when the task calls for it — no compat shims, deprecated-but-kept branches, or dual code paths, unless explicitly requested.

---

## Import Conventions

Follow these rules for `use` statements in Rust.

### Split types and functions

Do not mix types (structs, enums, type aliases) and functions in one braced `use` group when the list would wrap across lines.

### General rules

- Prefer **separate `use` lines** over one long braced import that wraps awkwardly.
- Group by **kind**: types in one `use`, functions in another, traits in another when needed.
- End multi-item braced imports with a **trailing comma** on the last item.
- Keep each braced group on **one line** when it fits within `max_width` (120); split into multiple `use` statements instead of wrapping mid-list.
- `cargo fmt` is authoritative for final layout; write imports so they match the style above before formatting.

---

## Testing Conventions

Follow these rules strictly.

### Unit Tests

- Located **in the same file** as the implementation.
- Use `#[cfg(test)]` modules.
- Test internal logic directly.

### Integration Tests

- Located in the root-level `tests/` directory.
- Each file is a separate test crate.
- Test only public APIs (no private/internal access).

### General Rules

- Keep tests small and focused.
- Cover edge cases and failure paths.
- Avoid duplication between unit and integration tests.
- Use clear, descriptive test names.
