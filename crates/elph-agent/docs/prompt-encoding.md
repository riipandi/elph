# TOON prompt encoding

Optional [TOON](https://github.com/toon-format/toon) encoding for **model-visible** structured payloads in `elph-agent`. TOON is a compact text format for uniform tabular JSON; encoding happens in the agent runtime before tool results enter LLM context.

Follows prompting guidance from [Using TOON with LLMs](https://toonformat.dev/guide/llm-prompts): show the format in fenced blocks, use delimiter hints for tabular data, and validate model output with strict decode when needed.

Wire protocols (`elph-ai` request/response JSON) are unchanged — only prompt payloads sent to the model may be rewritten.

Module: `elph_agent::runtime::prompt_encoding` (re-exported at crate root).

## When encoding applies

Encoding runs in `finalize_executed_tool_call` **after** the `after_tool_call` hook and **before** `tool_execution_end` / toolResult messages are persisted.

| Surface | Field | Behavior |
| ------- | ----- | -------- |
| Tool result text | `AgentToolResult.content` text blocks that parse as JSON | Replace text with fenced TOON block |
| MCP structured payload | `AgentToolResult.details.structured_content` | Replace primary text block with TOON encoding of structured value |

Plain text, non-JSON tool output, payloads below `min_bytes`, and payloads where TOON is not smaller than JSON (per `min_savings_ratio`) are left unchanged.

Tool text parsing tolerates markdown ` ```json ` fences and embedded JSON objects.

## Configuration

```rust
use elph_agent::{
    Agent, AgentOptions, PromptEncodingConfig, PromptEncodingDelimiter, PromptEncodingMode,
    PromptEncodingTargets,
};

let agent = Agent::new(AgentOptions {
    prompt_encoding: Some(PromptEncodingConfig {
        mode: PromptEncodingMode::Toon,
        min_bytes: 512,
        min_savings_ratio: 1.05,
        delimiter: PromptEncodingDelimiter::Comma,
        tabular_delimiter: Some(PromptEncodingDelimiter::Tab),
        targets: PromptEncodingTargets::ALL,
        ..PromptEncodingConfig::default()
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
| `Auto` | Encode only uniform tabular JSON (root array or single-key wrapper like `{"items": [...]}`) |

### Defaults

| Field | Default |
| ----- | ------- |
| `mode` | `Off` |
| `min_bytes` | `2048` |
| `min_savings_ratio` | `1.0` (encode only when TOON is strictly smaller) |
| `delimiter` | `Comma` |
| `tabular_delimiter` | `Tab` |
| `targets` | `tool_result_text` + `structured_details` |
| `preamble` | `"Data is in TOON format (2-space indent, arrays show length and fields)."` |

### Environment

```bash
export ELPH_PROMPT_ENCODING=toon                      # off | toon | auto
export ELPH_PROMPT_ENCODING_MIN_BYTES=2048
export ELPH_PROMPT_ENCODING_DELIMITER=tab             # comma | tab | pipe
export ELPH_PROMPT_ENCODING_TABULAR_DELIMITER=tab     # comma | tab | pipe
```

When `AgentOptions.prompt_encoding` is `None`, `Agent::new` resolves via `PromptEncodingConfig::from_env()`.

## Output format

Encoded payloads are wrapped for the model:

````
Data is in TOON format (2-space indent, arrays show length and fields). Fields are tab-separated.

```toon
<toon body>
```
````

Tab delimiter adds the tab-separated hint automatically. Already-fenced TOON blocks are not double-encoded.

## Standalone helpers

Use outside the tool loop (e.g. embed TOON in a user message):

```rust
use elph_agent::{
    apply_to_tool_result, decode_toon_fence, encode_value, extract_json_value, parse_toon_fence,
    PromptEncodingConfig, PromptEncodingMode,
};
use serde_json::json;

let config = PromptEncodingConfig {
    mode: PromptEncodingMode::Toon,
    min_bytes: 1,
    min_savings_ratio: 1.05,
    ..PromptEncodingConfig::default()
};

let value = json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]);
if let Some(block) = encode_value(&value, &config) {
    let prompt = format!("Summarize this inventory:\n\n{block}");
    // agent.prompt_text(&prompt, None).await?;
}

// Validate model-generated TOON (strict decode)
let decoded = decode_toon_fence(&block)?;
```

| Function | Role |
| -------- | ---- |
| `encode_value` | Encode JSON when config/heuristics allow; returns fenced block |
| `apply_to_tool_result` | Same logic the agent loop uses on tool results |
| `extract_json_value` | Parse JSON from tool text (fences, embedded objects) |
| `parse_toon_fence` | Extract ```toon body from fenced text |
| `decode_toon_fence` | Strict decode of a fenced TOON block |

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

cargo run -p elph-agent --example toon_no_tools -- --rows 80 --tabular-delimiter tab
cargo run -p elph-agent --example toon_tool_call
cargo run -p elph-agent --features mcp --example toon_mcp_deepwiki
```

`toon_*` examples accept `--delimiter` and `--tabular-delimiter` for manual comparison.

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

Workspace crate: `toon-format` (encode/decode). Encoding uses `encode` with `EncodeOptions` (delimiter, indent). Round-trip tests use `decode_default` / `decode_strict`.