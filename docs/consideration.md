# Consideration

Dependency candidates for elph-rs. Evaluated against current architecture: `elph-ai` (unified LLM, stubs), `elph-agent` (runtime + Turso), `elph-tui` (iocraft + pulldown-cmark), workspace plans (`fff-search`, `rmcp`, `jsonschema`, `agent-client-protocol`).

**Recommended near-term stack:** `genai` + `schemars` → `elph-ai`; `fff-search` + `rmcp` + `jsonschema` → `elph-agent`; `syntect` → `elph-tui`; keep `tracing`, `pulldown-cmark`, `tokio`.

Verdicts: **Adopt** wire soon · **Keep** in use · **Defer** valid, later · **Ref** study only · **Skip** redundant or poor fit

---

## LLM & provider (`elph-ai`)

| Verdict   | Item                                                                            | Rationale                                                                                                                                 |
| --------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| **Adopt** | [genai](https://crates.io/crates/genai)                                         | Best match for README: 25+ providers, streaming, tools, auth resolver, token usage. Pin `0.6.5` for prod; try `0.7.0-beta.9` on a branch. |
| **Adopt** | [schemars](https://crates.io/crates/schemars)                                   | Generate tool JSON Schema from Rust types; complements `schemas/elph-provider-schema.json`.                                               |
| **Defer** | [adk-anthropic](https://crates.io/crates/adk-anthropic)                         | Mature Anthropic-only client (thinking, cache, batch). Use if genai lacks a specific Anthropic feature.                                   |
| **Defer** | [anthropic-auth](https://crates.io/crates/anthropic-auth)                       | Claude OAuth/PKCE; pairs with TUI `oauth_selector` when provider login ships.                                                             |
| **Defer** | [anthropic-async](https://crates.io/crates/anthropic-async)                     | Anthropic-only + prompt caching; niche vs genai.                                                                                          |
| **Defer** | [async-openai](https://github.com/64bit/async-openai)                           | Deep OpenAI coverage (⭐1956); only if genai OpenAI path is insufficient.                                                                 |
| **Defer** | [llm-bridge-core](https://crates.io/crates/llm-bridge-core)                     | Anthropic↔OpenAI protocol transform; useful only for a unified gateway/proxy.                                                             |
| **Defer** | [iac-rs](https://crates.io/crates/iac-rs)                                       | Inter/intra-agent protocol; relevant for `elph-swarm`, not MVP.                                                                           |
| **Defer** | [adk-rust](https://github.com/zavora-ai/adk-rust)                               | Full agent framework (39 crates, MCP, RAG, browser). Alternative to building `elph-agent` — heavy adopt.                                  |
| **Skip**  | [anthropic-ai-sdk](https://crates.io/crates/anthropic-ai-sdk)                   | Anthropic-only; superseded by genai or adk-anthropic.                                                                                     |
| **Skip**  | [anthropic-rs](https://github.com/AbdelStark/anthropic-rs)                      | Smaller Anthropic SDK (⭐81).                                                                                                             |
| **Skip**  | [claude-sdk](https://crates.io/crates/claude-sdk)                               | Anthropic-only (1.5k dl).                                                                                                                 |
| **Skip**  | [genai-rs](https://crates.io/crates/genai-rs)                                   | Gemini-only; name collides with genai.                                                                                                    |
| **Skip**  | [openai-api-rs](https://crates.io/crates/openai-api-rs)                         | OpenAI-only despite high downloads.                                                                                                       |
| **Skip**  | [openai_dive](https://github.com/tjardoo/openai-client/tree/master/openai_dive) | OpenAI-only, low traction (⭐82).                                                                                                         |
| **Skip**  | [agent-sdk-rs](https://crates.io/crates/agent-sdk-rs)                           | Agent loop crate; 90 downloads, too immature to depend on.                                                                                |
| **Skip**  | [ai.rs](https://github.com/prabirshrestha/ai.rs)                                | Immature (⭐12).                                                                                                                          |

---

## Agent runtime (`elph-agent`)

| Verdict   | Item                                                                     | Rationale                                                                                                                         |
| --------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| **Adopt** | [fff-search](https://crates.io/crates/fff-search)                        | Fast file finder for agent tools; already commented in workspace `Cargo.toml`. Pin version explicitly (nightly tags move).        |
| **Ref**   | [yoagent](https://github.com/yologdev/yoagent)                           | Pi-inspired loop, events, MCP, skills (⭐171, 7.9k dl). Blueprint for custom `elph-agent` — adopt as dep overlaps with `elph-ai`. |
| **Ref**   | [open-agent-sdk-rust](https://github.com/codeany-ai/open-agent-sdk-rust) | 25+ tools, hooks, session (⭐23). Architecture reference, not production dep.                                                     |
| **Ref**   | [jcode](https://github.com/1jehuang/jcode)                               | Coding-agent harness (⭐8170); UX/loop patterns, not a Rust library.                                                              |
| **Ref**   | [rusty-gitclaw](https://github.com/open-gitagent/rusty-gitclaw)          | Git-native agent (⭐12); similar problem space.                                                                                   |
| **Ref**   | [codeany](https://github.com/codeany-ai/codeany)                         | Go terminal agent (⭐184); competitor/reference.                                                                                  |
| **Skip**  | [zag](https://github.com/niclaslindstedt/zag)                            | Multi-provider CLI (⭐6), not mature.                                                                                             |

---

## TUI & markdown (`elph-tui`)

| Verdict   | Item                                                               | Rationale                                                                              |
| --------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| **Keep**  | [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) | In use via `render_markdown_lines`; fast CommonMark parser.                            |
| **Adopt** | [syntect](https://crates.io/crates/syntect)                        | Syntax-highlight fenced code blocks (Pi parity); cheaper than swapping markdown stack. |
| **Defer** | [termimad](https://github.com/Canop/termimad)                      | TUI-rich markdown renderer (⭐1198); redundant if pulldown + syntect suffice.          |
| **Skip**  | [comrak](https://github.com/kivikakk/comrak)                       | GFM parser; heavier, no TUI renderer — would still need custom layout.                 |
| **Skip**  | [markdown-rs](https://github.com/wooorm/markdown-rs)               | Parser only; same issue as comrak.                                                     |

---

## Infra & utilities

| Verdict   | Item                                                  | Rationale                                                                                                  |
| --------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| **Keep**  | [tracing](https://github.com/tokio-rs/tracing)        | Active in `elph-core`; extend spans in agent loop.                                                         |
| **Defer** | [config-rs](https://github.com/rust-cli/config-rs)    | Layered config (⭐3186); `elph` already uses `settings.json` + `Paths` — adopt when merge/env layers grow. |
| **Defer** | [rapidhash](https://crates.io/crates/rapidhash)       | Fast hashing for cache keys and content fingerprints.                                                      |
| **Skip**  | [rustix](https://crates.io/crates/rustix)             | Already transitive via crossterm/turso; no direct dep needed.                                              |
| **Skip**  | [async-stream](https://crates.io/crates/async-stream) | Transitively available; add only if macro streams are written in-tree.                                     |

---

## Memory, parsing, integrations

| Verdict   | Item                                                            | Rationale                                                                                                                    |
| --------- | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| **Defer** | [memelord](https://github.com/glommer/memelord)                 | Per-project agent memory on Turso (⭐187); aligns with elph DB story, but separate product/MCP — evaluate when memory ships. |
| **Defer** | [cortex-mem](https://github.com/sopaco/cortex-mem)              | Cognitive memory foundation (⭐288); future long-term memory layer.                                                          |
| **Defer** | [liteparse](https://github.com/run-llama/liteparse)             | Document parser for RAG (⭐11k); not needed until ingestion tools exist.                                                     |
| **Defer** | [obscura](https://docs.obscura.sh/guides/use-as-a-rust-library) | Embedded browser (V8 from source, git dep); only for web-browse tools — high build cost.                                     |
| **Defer** | [pest](https://github.com/pest-parser/pest)                     | Custom grammars; only if elph adds a DSL beyond JSON/markdown.                                                               |
| **Skip**  | [teloxide](https://github.com/teloxide/teloxide)                | Telegram bots (⭐4175); out of scope unless a Telegram channel is planned.                                                   |

---

## Dev & CI tooling (not runtime deps)

| Verdict | Item                                             | Rationale                                                                                                                 |
| ------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| **Ref** | [crit](https://github.com/tomasz-tomczyk/crit)   | Human→agent review loop for plans/diffs/UI (⭐681, Go binary). Dev workflow integration (Pi-compatible), not a crate dep. |
| **Ref** | [ast-grep](https://github.com/ast-grep/ast-grep) | Structural search/lint CLI (⭐15k); CI and agent `grep` tooling, not embedded.                                            |
| **Ref** | [ffizer](https://crates.io/crates/ffizer)        | Project scaffolding from templates; contributor onboarding only.                                                          |

---

## Decisions to avoid

- **One provider layer:** do not combine `genai` + `adk-anthropic` + `async-openai` in `elph-ai`.
- **One agent framework:** pick custom loop (Ref yoagent) _or_ adopt `adk-rust` / `yoagent` — not both plus `open-agent-sdk-rust`.
- **Markdown:** keep pulldown-cmark; add syntect for highlighting, not a second parser.
