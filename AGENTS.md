# Agent Instructions

This repository uses documentation in the `/openwiki` directory as the primary source of truth.

## Getting Started

- Read: [OpenWiki quickstart](openwiki/quickstart.md)
- Follow links from the quickstart to relevant sections (architecture, workflows, domain, operations, testing).
- Prefer OpenWiki over re-exploring the codebase when documentation already answers the question.

---

## Testing Conventions (Rust)

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


<!-- openwiki-context -->
## OpenWiki Documentation

When searching for repository context, read `openwiki/quickstart.md` first and follow links to the relevant section pages under `openwiki/`. Prefer those docs over re-exploring the entire codebase when they already answer the question.

Entry point: `openwiki/quickstart.md`.
