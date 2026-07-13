# Dependency Considerations

Design log for evaluating third-party libraries. Verdicts guide adoption — implementation detail lives in [openwiki](../openwiki/quickstart.md).

**Verdicts:** **Adopt** · **Keep** · **Done** · **Defer** · **Ref** (study only) · **Skip**

## Near-term stack (target)

| Layer           | Planned crates                                         |
| --------------- | ------------------------------------------------------ |
| LLM / providers | `genai`, `schemars`                                    |
| Agent runtime   | `fff-search` (done), `rmcp`, `jsonschema`              |
| TUI             | `tuie`, `syntect`, `anstyle-syntect`, `pulldown-cmark` |
| Config          | `figment`, `jsonc-parser`                              |
| Shared          | `tracing`, `tokio`, `chrono`, `memchr`                 |

---

## LLM & providers

| Verdict   | Item                                                          | Rationale                                            |
| --------- | ------------------------------------------------------------- | ---------------------------------------------------- |
| **Adopt** | [genai](https://crates.io/crates/genai)                       | 25+ providers, streaming, tools, auth                |
| **Keep**  | [schemars](https://crates.io/crates/schemars)                 | Tool JSON Schema from types                          |
| **Defer** | adk-anthropic, anthropic-auth, anthropic-async                | Fallbacks if genai gaps                              |
| **Defer** | async-openai, llm-bridge-core, iac-rs                         | Niche / gateway / swarm                              |
| **Defer** | adk-rust                                                      | Full framework alternative — pick one agent approach |
| **Skip**  | anthropic-only SDKs, openai-only clients, immature agent SDKs | Superseded or too narrow                             |

---

## Agent runtime

| Verdict  | Item                                              | Rationale                                                    |
| -------- | ------------------------------------------------- | ------------------------------------------------------------ |
| **Done** | [fff-search](https://crates.io/crates/fff-search) | Grep/find tools                                              |
| **Ref**  | yoagent, open-agent-sdk-rust                      | Loop and session patterns — study, do not adopt as framework |
| **Skip** | Immature multi-provider CLIs                      | Reference only                                               |

---

## TUI, markdown & prompts

Two layers: tuie agent shell + rich diff/overlay components.

| Verdict   | Item                                                                                                      | Rationale                                    |
| --------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| **Keep**  | [tuie](https://crates.io/crates/tuie)                                                                     | Agent shell for elph                         |
| **Keep**  | [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark)                                        | Markdown line rendering                      |
| **Adopt** | [syntect](https://crates.io/crates/syntect) + [anstyle-syntect](https://crates.io/crates/anstyle-syntect) | Code-block highlighting                      |
| **Defer** | anstyle-git, termimad                                                                                     | Git diff colors; full markdown TUI redundant |
| **Skip**  | comrak, markdown-rs, iocraft                                                                              | Parser-only or replaced stack                |

### Terminal styling

| Verdict   | Item                                        | Rationale                                |
| --------- | ------------------------------------------- | ---------------------------------------- |
| **Keep**  | anstream, anstyle, anstyle-parse (via clap) | CLI colors                               |
| **Defer** | anstyle-crossterm, full anstyle migration   | Only if unifying diff ANSI               |
| **Skip**  | human-panic, proc-exit                      | Incompatible with abort-on-panic release |

### CLI prompts (non-TUI)

Blocking wizards only outside the active TUI session (raw-mode conflict).

| Verdict   | Item                                              | Rationale                     |
| --------- | ------------------------------------------------- | ----------------------------- |
| **Keep**  | [dialoguer](https://crates.io/crates/dialoguer)   | Onboarding, ask-user flows    |
| **Defer** | [inquire](https://github.com/mikaelmello/inquire) | Future doctor/session pickers |
| **Skip**  | inquire inside TUI                                | Use shell overlays instead    |

---

## Configuration

Schema: [schemas/elph-schema.json](../schemas/elph-schema.json).

**Merge order:** defaults → home → project → env → CLI.

| Verdict   | Item                                                  | Rationale             |
| --------- | ----------------------------------------------------- | --------------------- |
| **Adopt** | [figment](https://crates.io/crates/figment)           | Layered merge         |
| **Adopt** | [jsonc-parser](https://crates.io/crates/jsonc-parser) | JSONC comments        |
| **Defer** | dotenvy, confique, json5, config-rs                   | Optional DX / overlap |
| **Skip**  | twelf, envy, ron                                      | Too narrow            |

---

## Infrastructure

| Verdict   | Item                           | Rationale                                        |
| --------- | ------------------------------ | ------------------------------------------------ |
| **Keep**  | tracing, chrono, memchr, rayon | Logging, time, parsing, parallel fuzzy filter    |
| **Defer** | rapidhash, obscura             | Fingerprints; embedded browser (high build cost) |
| **Skip**  | Duplicate transitive deps      | Add only when needed in-tree                     |

---

## Memory & integrations

| Verdict   | Item                        | Rationale                               |
| --------- | --------------------------- | --------------------------------------- |
| **Done**  | memelord → floppy           | In-tree agent memory with vector search |
| **Defer** | cortex-mem, liteparse, pest | Alternative memory / RAG / DSL          |
| **Skip**  | teloxide                    | Out of scope                            |

---

## Dev & CI (not runtime)

| Verdict | Item                   | Rationale                              |
| ------- | ---------------------- | -------------------------------------- |
| **Ref** | crit, ast-grep, ffizer | Review, structural search, scaffolding |

---

## Decisions to avoid

| Rule                | Detail                                                     |
| ------------------- | ---------------------------------------------------------- |
| One provider layer  | Not genai + multiple Anthropic/OpenAI SDKs                 |
| One agent framework | Custom loop or external framework — not both               |
| One markdown stack  | pulldown-cmark + syntect; no second parser                 |
| One styling path    | Manual ANSI or anstyle — planned cutover only              |
| One config stack    | figment + one JSONC helper                                 |
| Two prompt layers   | TUI overlays vs CLI dialoguer — never nested in active TUI |
| No panic UX layer   | Release uses abort-on-panic                                |
| One time library    | chrono for RFC 3339                                        |
