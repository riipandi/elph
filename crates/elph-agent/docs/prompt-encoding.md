# TOON prompt encoding

Optional [TOON](https://github.com/toon-format/toon) encoding for **model-visible** structured payloads in `elph-agent`. TOON is a compact text format for uniform tabular JSON; encoding happens in the agent runtime before tool results enter LLM context.

Wire protocols (`elph-ai` request/response JSON) are unchanged — only prompt payloads sent to the model may be rewritten.

Module: `elph_agent::runtime::prompt_encoding` (re-exported at crate root).

## When encoding applies

Encoding runs in `finalize_executed_tool_call` **after** the `after_tool_call` hook and **before** `tool_execution_end` / toolResult messages are persisted.

| Surface | Field | Behavior |
| ------- | ----- | -------- |
| Tool result text | `AgentToolResult.content` text blocks that parse as JSON | Replace text with fenced TOON block |
| MCP structured payload | `AgentToolResult.details.structured_content` | Replace primary text block with TOON encoding of structured value |

Plain text, non-JSON tool output, and payloads below `min_bytes` are left unchanged.

## Configuration

```rust
use elph_agent::{
    Agent, AgentOptions, PromptEncodingConfig, PromptEncodingMode, PromptEncodingTargets,
};

let agent = Agent::new(AgentOptions {
    prompt_encoding: Some(PromptEncodingConfig {
        mode: PromptEncodingMode::Toon,
        min_bytes: 512,
        targets: PromptEncodingTargets::ALL,
        preamble: Some("Structured data below uses TOON format.".into()),
    }),
    ..Default::default()
});
```

`AgentHarness` reads `PromptEncodingConfig::from_env()` when building loop config (no separate harness option today).

### `PromptEncodingMode`

| Mode | Effect |
| ---- | ------ |
| `Off` (default) | No encoding |
| `Toon` | Encode all eligible JSON payloads ≥ `min_bytes` |
| `Auto` | Encode only uniform tabular JSON arrays (≥ 2 rows, identical object keys) |

### Defaults

| Field | Default |
| ----- | ------- |
| `mode` | `Off` |
| `min_bytes` | `2048` |
| `targets` | `tool_result_text` + `structured_details` |
| `preamble` | `"Structured data below uses TOON format."` |

### Environment

```bash
export ELPH_PROMPT_ENCODING=toon   # off | toon | auto
```

When `AgentOptions.prompt_encoding` is `None`, `Agent::new` resolves via `PromptEncodingConfig::from_env()`.

## Output format

Encoded payloads are wrapped for the model:

````
Structured data below uses TOON format.

```toon
<toon body>
```
````

Already-fenced TOON blocks are not double-encoded.

## Standalone helpers

Use outside the tool loop (e.g. embed TOON in a user message):

```rust
use elph_agent::{encode_value, apply_to_tool_result, PromptEncodingConfig, PromptEncodingMode};
use serde_json::json;

let config = PromptEncodingConfig {
    mode: PromptEncodingMode::Toon,
    min_bytes: 1,
    ..PromptEncodingConfig::default()
};

let value = json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]);
if let Some(block) = encode_value(&value, &config) {
    let prompt = format!("Summarize this inventory:\n\n{block}");
    // agent.prompt_text(&prompt, None).await?;
}
```

`apply_to_tool_result` mutates an `AgentToolResult` in place (same logic the loop uses).

## Examples

All examples use **OpenCode Zen `big-pickle`** (`opencode/big-pickle`). Set `OPENCODE_API_KEY` first.

### TOON enabled

| Example | Scenario |
| ------- | -------- |
| `toon_no_tools` | TOON embedded in user prompt (no tools) |
| `toon_tool_call` | Custom `list_inventory` tool → TOON on tool result |
| `toon_mcp_deepwiki` | DeepWiki MCP → TOON on `structured_content` |

```bash
export OPENCODE_API_KEY="your-key"

cargo run -p elph-agent --example toon_no_tools
cargo run -p elph-agent --example toon_tool_call
cargo run -p elph-agent --features mcp --example toon_mcp_deepwiki
```

### Default encoding (comparison baselines)

Same prompts and CLI flags as the TOON pair; encoding is `Off`. Run both and compare **Comparison summary** token counts at the end.

| Example | Pairs with |
| ------- | ---------- |
| `default_no_tools` | `toon_no_tools` |
| `default_tool_call` | `toon_tool_call` |
| `default_mcp_deepwiki` | `toon_mcp_deepwiki` |

```bash
cargo run -p elph-agent --example default_no_tools -- --rows 80
cargo run -p elph-agent --example toon_no_tools -- --rows 80
```

Shared prompts and helpers live in `examples/support/toon_common.rs`.

## MCP notes

MCP tools often return large `structured_content` in `details` while `content` holds a short preview. With `targets.structured_details: true`, TOON replaces the primary text block the model sees — useful for DeepWiki and similar servers.

See also [mcp.md](./mcp.md) and example `mcp_deepwiki` (raw MCP call without agent loop).

## Dependency

Workspace crate: `toon-format` (encode/decode). Encoding uses `encode_default`; round-trip tests use `decode_default`.