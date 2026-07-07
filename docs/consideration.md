# Consideration

Dependency candidates for elph-rs, mapped to crates and planned workspace deps (`fff-search`, `rmcp`, `jsonschema`, `agent-client-protocol`).

**Verdicts:** **Adopt** · **Keep** · **Defer** · **Ref** (study only) · **Skip**

**Near-term stack**

| Layer                | Crates                              |
| -------------------- | ----------------------------------- |
| `elph-ai`            | `genai`, `schemars`                 |
| `elph-agent`         | `fff-search`, `rmcp`, `jsonschema`  |
| `elph-tui`           | `syntect` (+ keep `pulldown-cmark`) |
| `elph-core` / `elph` | `figment`, `jsonc-parser`           |
| Shared               | `tracing`, `tokio`                  |

---

## LLM & provider (`elph-ai`)

| Verdict   | Item                                                                                                                                                                         | Rationale                                                                         |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| **Adopt** | [genai](https://crates.io/crates/genai)                                                                                                                                      | 25+ providers, streaming, tools, auth. Pin `0.6.5`; try `0.7.0-beta.9` on branch. |
| **Adopt** | [schemars](https://crates.io/crates/schemars)                                                                                                                                | Tool JSON Schema from Rust types; pairs with `schemas/elph-provider-schema.json`. |
| **Defer** | [adk-anthropic](https://crates.io/crates/adk-anthropic)                                                                                                                      | Anthropic-only fallback if genai lacks thinking/cache/batch.                      |
| **Defer** | [anthropic-auth](https://crates.io/crates/anthropic-auth)                                                                                                                    | Claude OAuth/PKCE for `oauth_selector`.                                           |
| **Defer** | [anthropic-async](https://crates.io/crates/anthropic-async)                                                                                                                  | Anthropic + prompt cache; niche vs genai.                                         |
| **Defer** | [async-openai](https://github.com/64bit/async-openai)                                                                                                                        | Deep OpenAI (⭐1956); only if genai OpenAI path falls short.                      |
| **Defer** | [llm-bridge-core](https://crates.io/crates/llm-bridge-core)                                                                                                                  | Protocol transform; gateway/proxy only.                                           |
| **Defer** | [iac-rs](https://crates.io/crates/iac-rs)                                                                                                                                    | Inter-agent protocol; `elph-swarm`, not MVP.                                      |
| **Defer** | [adk-rust](https://github.com/zavora-ai/adk-rust)                                                                                                                            | Full framework (39 crates); alternative to custom `elph-agent`.                   |
| **Skip**  | [anthropic-ai-sdk](https://crates.io/crates/anthropic-ai-sdk), [anthropic-rs](https://github.com/AbdelStark/anthropic-rs), [claude-sdk](https://crates.io/crates/claude-sdk) | Anthropic-only; superseded.                                                       |
| **Skip**  | [genai-rs](https://crates.io/crates/genai-rs)                                                                                                                                | Gemini-only; name collision with genai.                                           |
| **Skip**  | [openai-api-rs](https://crates.io/crates/openai-api-rs), [openai_dive](https://github.com/tjardoo/openai-client/tree/master/openai_dive)                                     | OpenAI-only.                                                                      |
| **Skip**  | [agent-sdk-rs](https://crates.io/crates/agent-sdk-rs), [ai.rs](https://github.com/prabirshrestha/ai.rs)                                                                      | Immature.                                                                         |

---

## Agent runtime (`elph-agent`)

| Verdict   | Item                                                                                                                                                          | Rationale                                                                 |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| **Adopt** | [fff-search](https://crates.io/crates/fff-search)                                                                                                             | Fast file finder; commented in workspace `Cargo.toml`. Pin version.       |
| **Ref**   | [yoagent](https://github.com/yologdev/yoagent)                                                                                                                | Pi-style loop blueprint (⭐171). Don't adopt as dep — overlaps `elph-ai`. |
| **Ref**   | [open-agent-sdk-rust](https://github.com/codeany-ai/open-agent-sdk-rust)                                                                                      | Tools/hooks/session patterns (⭐23).                                      |
| **Ref**   | [jcode](https://github.com/1jehuang/jcode), [rusty-gitclaw](https://github.com/open-gitagent/rusty-gitclaw), [codeany](https://github.com/codeany-ai/codeany) | Competitors / UX reference, not libs.                                     |
| **Skip**  | [zag](https://github.com/niclaslindstedt/zag)                                                                                                                 | Immature multi-provider CLI (⭐6).                                        |

---

## TUI, markdown & prompts

### Agent shell (`elph-tui` + iocraft)

| Verdict   | Item                                                                                               | Rationale                                                      |
| --------- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| **Keep**  | [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark)                                 | `render_markdown_lines` in production.                         |
| **Adopt** | [syntect](https://crates.io/crates/syntect)                                                        | Code-block highlighting (Pi parity).                           |
| **Defer** | [termimad](https://github.com/Canop/termimad)                                                      | Full markdown TUI (⭐1198); redundant with pulldown + syntect. |
| **Skip**  | [comrak](https://github.com/kivikakk/comrak), [markdown-rs](https://github.com/wooorm/markdown-rs) | Parser-only; still need custom layout.                         |

### CLI prompts (`elph` / `eclaw` subcommands only)

Blocking prompts for wizards and pickers — **not** inside the iocraft agent loop (raw-mode conflict).

| Verdict   | Item                                              | Rationale                                                                                                      |
| --------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Defer** | [inquire](https://github.com/mikaelmello/inquire) | Select/Confirm/Text wizards for CLI (`session` picker, `doctor`, bootstrap). Pin `0.9.x`, `crossterm` backend. |
| **Skip**  | [dialoguer](https://crates.io/crates/dialoguer)   | Superseded by inquire for new code.                                                                            |
| **Skip**  | inquire in `elph-tui`                             | `PromptInput`, `SelectList`, overlay selectors already cover agent UX.                                         |

**Overlap:** inquire `Text`/`Select` ≈ `PromptInput`/`SelectList`/`session_selector` — use iocraft components in `App`; use inquire only when subcommand runs and exits.

---

## Configuration (`elph-core` / `elph`)

Schema: `schemas/elph-config-schema.json`. Types: `elph/src/runtime/settings.rs`.

**Merge order (low → high):** defaults → `~/.elph/settings.*` → `<workDir>/.elph/settings.*` → `ELPH_*` env → CLI (later).

| Format            | figment native | Notes                               |
| ----------------- | -------------- | ----------------------------------- |
| JSON              | ✅             | Canonical on save (`settings.json`) |
| JSONC             | ❌             | Via `jsonc-parser` pre-pass         |
| TOML / YAML / ENV | ✅             | Optional user formats               |

| Verdict   | Item                                                                                                                 | Rationale                                                               |
| --------- | -------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| **Adopt** | [figment](https://crates.io/crates/figment)                                                                          | Layered merge + serde extract. Features: `json`, `toml`, `yaml`, `env`. |
| **Adopt** | [jsonc-parser](https://crates.io/crates/jsonc-parser)                                                                | Comments + trailing commas → `serde_json`.                              |
| **Defer** | [dotenvy](https://crates.io/crates/dotenvy)                                                                          | `.env` for local dev secrets.                                           |
| **Defer** | [confique](https://crates.io/crates/confique)                                                                        | Derive DX if `Settings` boilerplate grows.                              |
| **Defer** | [json5](https://crates.io/crates/json5), [json_comments](https://crates.io/crates/json_comments)                     | Alternative JSONC paths — pick one dialect.                             |
| **Defer** | [shellexpand](https://crates.io/crates/shellexpand)                                                                  | `${VAR}` / `~` in config strings.                                       |
| **Defer** | [config-rs](https://github.com/rust-cli/config-rs)                                                                   | Overlaps figment; weaker error provenance.                              |
| **Skip**  | [twelf](https://crates.io/crates/twelf), [envy](https://crates.io/crates/envy), [ron](https://github.com/ron-rs/ron) | Too narrow or extra format.                                             |

**Wire:** `Figment` merge chain → extract `Settings` → validate with `jsonschema` → write strict JSON to home on save.

---

## Infra

| Verdict   | Item                                                                                             | Rationale                                       |
| --------- | ------------------------------------------------------------------------------------------------ | ----------------------------------------------- |
| **Keep**  | [tracing](https://github.com/tokio-rs/tracing)                                                   | Active in `elph-core`.                          |
| **Defer** | [rapidhash](https://crates.io/crates/rapidhash)                                                  | Cache keys / content fingerprints.              |
| **Skip**  | [rustix](https://crates.io/crates/rustix), [async-stream](https://crates.io/crates/async-stream) | Already transitive; add only if needed in-tree. |

---

## Memory & integrations

| Verdict   | Item                                                            | Rationale                                                 |
| --------- | --------------------------------------------------------------- | --------------------------------------------------------- |
| **Done**  | [memelord](https://github.com/glommer/memelord) → `memz`        | Ported in-tree as `memz`; Turso agent memory with vector search and weight scoring. |
| **Defer** | [cortex-mem](https://github.com/sopaco/cortex-mem)              | Long-term memory layer (⭐288).                           |
| **Defer** | [liteparse](https://github.com/run-llama/liteparse)             | RAG ingestion (⭐11k).                                    |
| **Defer** | [obscura](https://docs.obscura.sh/guides/use-as-a-rust-library) | Embedded browser; high build cost.                        |
| **Defer** | [pest](https://github.com/pest-parser/pest)                     | Custom DSL only.                                          |
| **Skip**  | [teloxide](https://github.com/teloxide/teloxide)                | Telegram; out of scope.                                   |

---

## Dev & CI (not runtime deps)

| Verdict | Item                                             | Rationale                              |
| ------- | ------------------------------------------------ | -------------------------------------- |
| **Ref** | [crit](https://github.com/tomasz-tomczyk/crit)   | Human→agent review (⭐681, Go binary). |
| **Ref** | [ast-grep](https://github.com/ast-grep/ast-grep) | Structural lint/search CLI (⭐15k).    |
| **Ref** | [ffizer](https://crates.io/crates/ffizer)        | Scaffolding templates.                 |

---

## Decisions to avoid

| Rule                | Detail                                                                      |
| ------------------- | --------------------------------------------------------------------------- |
| One provider layer  | Not `genai` + `adk-anthropic` + `async-openai`.                             |
| One agent framework | Custom loop (yoagent ref) _or_ `adk-rust` — not both + SDK refs.            |
| One markdown stack  | `pulldown-cmark` + `syntect`; no second parser.                             |
| One config stack    | `figment` + one JSONC helper; not `config-rs` + `confique`.                 |
| One comment dialect | JSONC (VS Code) _or_ JSON5.                                                 |
| Two prompt layers   | iocraft for agent shell; inquire for CLI only — never nested in active TUI. |
