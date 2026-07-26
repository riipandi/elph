# Agent Instructions

<!-- OPENWIKI:START -->

## OpenWiki

This repository uses OpenWiki for recurring code documentation. Start with `openwiki/quickstart.md`, then follow its links to architecture, workflows, domain concepts, operations, integrations, testing guidance, and source maps.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->

---

## Import Conventions

Follow these rules for `use` statements in Rust.

### Split types and functions

Do not mix types (structs, enums, type aliases) and functions in one braced `use` group when the list would wrap across lines.

**Invalid:**

```rust
use crate::agent::{
    AgentUiEvent, CodingAgentSession, CreateSessionOptions, create_coding_session_with_events, load_resources,
    slash_commands_for_palette,
};
```

**Valid:**

```rust
use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions};
use crate::agent::{create_coding_session_with_events, load_resources, slash_commands_for_palette,};
```

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

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_example() {
        assert_eq!(example_fn(), expected);
    }
}
```

### Integration Tests

- Located in the root-level `tests/` directory.
- Each file is a separate test crate.
- Test only public APIs (no private/internal access).

```
tests/
  api_contract.rs
  user_flow.rs
```

### General Rules

- Keep tests small and focused.
- Cover edge cases and failure paths.
- Avoid duplication between unit and integration tests.
- Use clear, descriptive test names.
