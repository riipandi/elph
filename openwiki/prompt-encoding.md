---
title: "TOON Prompt Encoding"
last_updated: 2026-07-12T10:00:00Z
category: operations
tags:
    - elph-agent
    - prompt-encoding
    - toon
    - tokens
    - mcp
status: published
---

# TOON Prompt Encoding

Optional [TOON](https://github.com/toon-format/toon) compression for **model-visible** structured data in `elph-agent`. Large JSON tool results (and MCP `structured_content`) can be rewritten as fenced TOON blocks before the next LLM call, reducing input tokens on tabular payloads.

Wire protocols (`elph-ai` JSON) are unchanged — only prompt context sent to the model may differ.

**Implementation reference:** [`crates/elph-agent/docs/prompt-encoding.md`](../crates/elph-agent/docs/prompt-encoding.md)

---

## Where it runs

```
tool executes
    → after_tool_call hook (optional mutation)
    → TOON encoding (if enabled)
    → tool_execution_end / toolResult message → LLM
```

Module: `elph_agent::runtime::prompt_encoding` (re-exported at crate root).

Surfaces rewritten when eligible:

| Surface                | Source                                                            |
| ---------------------- | ----------------------------------------------------------------- |
| Tool result text       | `AgentToolResult.content` blocks that parse as JSON               |
| MCP structured payload | `AgentToolResult.details.structured_content` → primary text block |

Plain text, non-JSON output, and payloads below `min_bytes` are left as-is.

---

## Configuration

### Environment variable

| Variable               | Values                | Default |
| ---------------------- | --------------------- | ------- |
| `ELPH_PROMPT_ENCODING` | `off`, `toon`, `auto` | `off`   |
| `ELPH_PROMPT_ENCODING_MIN_BYTES` | usize | `2048` |
| `ELPH_PROMPT_ENCODING_DELIMITER` | `comma`, `tab`, `pipe` | `comma` |
| `ELPH_PROMPT_ENCODING_TABULAR_DELIMITER` | `comma`, `tab`, `pipe` | `tab` |

```sh
export ELPH_PROMPT_ENCODING=toon   # encode eligible JSON tool results
export ELPH_PROMPT_ENCODING=auto   # tabular JSON arrays only
export ELPH_PROMPT_ENCODING_TABULAR_DELIMITER=tab
```

`Agent::new` and `AgentHarness` resolve encoding via `PromptEncodingConfig::from_env()` when not set explicitly on `AgentOptions`.

Owly does not expose a CLI flag today — set `ELPH_PROMPT_ENCODING` in the shell or `~/.owly/.env` to enable for documentation runs.

### Modes

| Mode   | Behavior                                                                      |
| ------ | ----------------------------------------------------------------------------- |
| `off`  | Default — no transformation                                                   |
| `toon` | Encode all eligible JSON ≥ `min_bytes` (default 2048)                         |
| `auto` | Encode only uniform tabular JSON arrays (≥ 2 rows, identical keys per object) |

### Programmatic (`AgentOptions`)

```rust
use elph_agent::{Agent, AgentOptions, PromptEncodingConfig, PromptEncodingMode};

let agent = Agent::new(AgentOptions {
    prompt_encoding: Some(PromptEncodingConfig {
        mode: PromptEncodingMode::Auto,
        min_bytes: 512,
        ..PromptEncodingConfig::default()
    }),
    ..Default::default()
});
```

---

## Output format

Encoded blocks include an optional preamble and a TOON fence:

````
Data is in TOON format (2-space indent, arrays show length and fields).

```toon
<toon body>
```
````

Already-fenced TOON is not double-encoded.

---

## Examples (comparison)

Runnable examples use **OpenCode Zen `big-pickle`** (`OPENCODE_API_KEY` required). Pair `toon_*` with `default_*` using **identical flags** and compare the **Comparison summary** (tokens in/out, prompt bytes).

| TOON enabled        | Baseline (`off`)       | Scenario                                      |
| ------------------- | ---------------------- | --------------------------------------------- |
| `toon_no_tools`     | `default_no_tools`     | TOON embedded in user prompt (no tools)       |
| `toon_tool_call`    | `default_tool_call`    | Custom `list_inventory` tool → TOON on result |
| `toon_mcp_deepwiki` | `default_mcp_deepwiki` | DeepWiki MCP → TOON on `structured_content`   |

```sh
export OPENCODE_API_KEY="your-key"

# Tool calling comparison
cargo run -p elph-agent --example default_tool_call -- --rows 80
cargo run -p elph-agent --example toon_tool_call -- --rows 80

# MCP DeepWiki
cargo run -p elph-agent --features mcp --example toon_mcp_deepwiki
cargo run -p elph-agent --features mcp --example default_mcp_deepwiki
```

Shared prompts live in `crates/elph-agent/examples/support/toon_common.rs`.

---

## MCP integration

MCP tools often return a short text preview in `content` and the full body in `details.structured_content`. With `targets.structured_details` enabled (default), TOON replaces the primary text the model sees — useful for [DeepWiki](https://mcp.deepwiki.com/mcp) and similar servers.

See also: [Elph MCP design](../docs/mcp.md), example `mcp_deepwiki` (raw MCP call without agent loop).

---

## API helpers

| Function                                                            | Role                                                           |
| ------------------------------------------------------------------- | -------------------------------------------------------------- |
| `encode_value(&Value, &PromptEncodingConfig)`                       | Encode JSON when config/heuristics allow; returns fenced block |
| `apply_to_tool_result(&mut AgentToolResult, &PromptEncodingConfig)` | Same logic the agent loop uses on tool results                 |
| `decode_toon_fence(&str)`                                           | Strict decode of a ```toon fenced block                        |
| `extract_json_value(&str)`                                          | Parse JSON from tool text (fences, embedded objects)           |

Use `encode_value` to embed TOON directly in user prompts without tool calling. See [Using TOON with LLMs](https://toonformat.dev/guide/llm-prompts).

---

## Related docs

- [`elph-agent` README](../crates/elph-agent/README.md) — crate overview and examples table
- [`docs/agent-runtime.md`](../docs/agent-runtime.md) — product design (tool loop step)
- [`docs/configuration.md`](../docs/configuration.md) — `ELPH_PROMPT_ENCODING` in env table
