# Agent Instructions

This repository uses documentation in the `/openwiki` directory as the primary source of truth.

## Language

Write **documentation**, **skills** (`.agents/skills`, OpenWiki, `docs/`), commit/PR prose when drafting, and agent reports in **English** (e.g. _behavior_, _serialize_, _catalog_).
Keep code identifiers, paths, and upstream names literal. Match the user only if they explicitly request another language for the reply.

## Readable docs and reports

Prefer short prose, headings, and tagged bullets over wide markdown tables—especially for status, audits, gap lists, and porting notes under `docs/porting/`.
Use a table only when a compact multi-axis comparison is clearly clearer than bullets.

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

<!-- OWLY:START -->
<!-- openwiki-context -->

## OpenWiki Documentation

When searching for repository context, read `openwiki/quickstart.md` first and follow links to the relevant section pages under `openwiki/`.
Prefer those docs over re-exploring the entire codebase when they already answer the question.

Entry point: `openwiki/quickstart.md`.

<!-- OWLY:END -->
