# Agent Instructions

<!-- OPENWIKI:START -->

## OpenWiki

This repository uses OpenWiki for recurring code documentation. Start with `openwiki/quickstart.md`, then follow its links to architecture, workflows, domain concepts, operations, integrations, testing guidance, and source maps.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->

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
  user_flow.rs
  api_contract.rs
```

### General Rules

- Keep tests small and focused.
- Cover edge cases and failure paths.
- Avoid duplication between unit and integration tests.
- Use clear, descriptive test names.
