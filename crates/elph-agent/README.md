# elph-agent

Stateful agent runtime with tool execution, event streaming, and session-backed orchestration.
Built on [`elph-ai`](../elph-ai) and ported from [@earendil-works/pi-agent](https://github.com/earendil-works/pi/tree/main/packages/agent).

## Installation

Add both crates to your workspace:

```toml
[dependencies]
elph-agent = { path = "../elph-agent" }
elph-ai = { path = "../elph-ai" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick Start

```rust
use std::sync::Arc;

use elph_agent::{Agent, AgentOptions, PartialAgentState};
use elph_ai::{builtin_models, Message, UserContent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let models = builtin_models(None);
    let model = models
        .get_model("anthropic", "claude-sonnet-4-20250514")
        .expect("model");

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            ..Default::default()
        }),
        ..Default::default()
    });

    agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                if let elph_agent::AgentEvent::MessageUpdate { assistant_message_event, .. } = event {
                    if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = assistant_message_event {
                        print!("{delta}");
                    }
                }
            })
        }))
        .await;

    agent.prompt_text("Hello!", None).await?;
    Ok(())
}
```

## MCP tools

With the `mcp` feature (default), load remote MCP servers and expose them as agent tools:

```rust
use elph_agent::{McpConfig, McpServerConfig, McpToolRegistry};
use std::sync::Arc;

let mut config = McpConfig::default();
config.servers.insert(
    "fs".into(),
    McpServerConfig::stdio("npx", vec![
        "-y".into(),
        "@modelcontextprotocol/server-filesystem".into(),
        "/tmp".into(),
    ]),
);
let registry = Arc::new(McpToolRegistry::load(config).await?);
let mcp_tools = registry.create_agent_tools(); // names: mcp_fs__...
```

Supports **stdio** and **streamable HTTP**, connection pooling with reconnect, and fail-open discovery. Details: [docs/mcp.md](./docs/mcp.md).

For deterministic tests, use the `faux` provider from `elph-ai`:

```rust
use elph_ai::{faux_assistant_message, faux_provider, faux_text, Models, builtin_models};

let faux = faux_provider(Default::default());
faux.set_responses(vec![faux_assistant_message(faux_text("Hi!"), StopReason::Stop)]);

let mut models = builtin_models(None);
models.set_provider(faux.provider.clone());
let models: Arc<Models> = models.into_arc();
```

## Core Concepts

### AgentMessage vs LLM Message

The agent works with `AgentMessage`, a flexible enum that can include:

- Standard LLM messages (`user`, `assistant`, `toolResult`) via `AgentMessage::Llm`
- Built-in custom roles (`bashExecution`, `branchSummary`, `compactionSummary`, `custom`) via `AgentMessage::Custom`

LLMs only understand `user`, `assistant`, and `toolResult`. The `convert_to_llm` function bridges this gap by filtering and transforming messages before each LLM call.

### Message Flow

```
AgentMessage[] → transform_context() → AgentMessage[] → convert_to_llm() → Message[] → LLM
                    (optional)                           (required)
```

1. **transform_context**: Prune old messages, inject external context
2. **convert_to_llm**: Filter out UI-only messages, convert custom types to LLM format

The default converter is `default_convert_to_llm` / `default_convert_to_llm_fn`.

## Event Flow

The agent emits events for UI updates. Understanding the event sequence helps build responsive interfaces.

### prompt() Event Sequence

When you call `prompt_text("Hello")`:

```
prompt_text("Hello")
├─ agent_start
├─ turn_start
├─ message_start   { message: userMessage }      // Your prompt
├─ message_end     { message: userMessage }
├─ message_start   { message: assistantMessage } // LLM starts responding
├─ message_update  { message: partial... }       // Streaming chunks
├─ message_update  { message: partial... }
├─ message_end     { message: assistantMessage } // Complete response
├─ turn_end        { message, toolResults: [] }
└─ agent_end       { messages: [...] }
```

### With Tool Calls

If the assistant calls tools, the loop continues:

```
prompt_text("Read config.json")
├─ agent_start
├─ turn_start
├─ message_start/end  { userMessage }
├─ message_start      { assistantMessage with toolCall }
├─ message_update...
├─ message_end        { assistantMessage }
├─ tool_execution_start  { toolCallId, toolName, args }
├─ tool_execution_update { partialResult }           // If tool streams
├─ tool_execution_end    { toolCallId, result }
├─ message_start/end  { toolResultMessage }
├─ turn_end           { message, toolResults: [toolResult] }
│
├─ turn_start                                        // Next turn
├─ message_start      { assistantMessage }           // LLM responds to tool result
├─ message_update...
├─ message_end
├─ turn_end
└─ agent_end
```

Tool execution mode is configurable:

- `ToolExecutionMode::Parallel` (default): preflight tool calls sequentially, execute allowed tools concurrently, emit `tool_execution_end` as soon as each tool is finalized, then emit toolResult messages and `turn_end.tool_results` in assistant source order
- `ToolExecutionMode::Sequential`: execute tool calls one by one

In parallel mode, tool completion events follow tool completion order, but persisted toolResult messages still follow assistant source order.

The mode can be set globally via `tool_execution` in `AgentOptions`, or per-tool via `execution_mode` on `AgentTool`. If any tool call in a batch targets a tool with `execution_mode: Sequential`, the entire batch executes sequentially regardless of the global setting.

The `before_tool_call` hook runs after `tool_execution_start` and validated argument parsing. It can block execution. The `after_tool_call` hook runs after tool execution finishes and before TOON prompt encoding (when enabled), `tool_execution_end`, and final tool result message events are emitted.

Tools can also return `terminate: true` to hint that the automatic follow-up LLM call should be skipped. The loop only stops early when every finalized tool result in that batch sets `terminate: true`. Mixed batches continue normally.

Low-level loop callers can set `should_stop_after_turn` to stop gracefully after the current turn completes:

```rust
use elph_agent::{AgentLoopConfig, agent_loop};

let config = AgentLoopConfig {
    should_stop_after_turn: Some(Arc::new(|update| {
        Box::pin(async move {
            should_compact_before_next_turn(&update.context.messages)
        })
    })),
    ..config
};
```

`should_stop_after_turn` runs after `turn_end` is emitted and after the assistant response and any tool executions have completed normally. If it returns `true`, the loop emits `agent_end` and exits before polling steering or follow-up queues, and before starting another LLM call.

When you use the `Agent` class, assistant `message_end` processing is treated as a barrier before tool preflight begins. That means `before_tool_call` sees agent state that already includes the assistant message that requested the tool call.

### continue_run() Event Sequence

`continue_run()` resumes from existing context without adding a new message. Use it for retries after errors.

```rust
// After an error, retry from current state
agent.continue_run().await?;
```

The last message in context must be `user` or `toolResult` (not `assistant`).

### Event Types

| Event                   | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| `agent_start`           | Agent begins processing                                           |
| `agent_end`             | Final event for the run                                           |
| `turn_start`            | New turn begins (one LLM call + tool executions)                  |
| `turn_end`              | Turn completes with assistant message and tool results            |
| `message_start`         | Any message begins (user, assistant, toolResult)                  |
| `message_update`        | **Assistant only.** Includes `assistant_message_event` with delta |
| `message_end`           | Message completes                                                 |
| `tool_execution_start`  | Tool begins                                                       |
| `tool_execution_update` | Tool streams progress                                             |
| `tool_execution_end`    | Tool completes                                                    |

`Agent::subscribe()` listeners are awaited in registration order. `agent_end` means no more loop events will be emitted, but `wait_for_idle()` and `prompt_text(...)` only settle after awaited `agent_end` listeners finish.

## Agent Options

```rust
use elph_agent::{
    Agent, AgentOptions, PartialAgentState, QueueMode, ToolExecutionMode,
};

let agent = Agent::new(AgentOptions {
    // Initial state
    initial_state: Some(PartialAgentState {
        system_prompt: Some("...".into()),
        model: Some(model),
        thinking_level: Some(AgentThinkingLevel::Medium),
        tools: Some(vec![my_tool]),
        messages: Some(vec![]),
    }),

    // Convert AgentMessage[] to LLM Message[] (required for custom message types)
    convert_to_llm: Some(Arc::new(|messages| { /* filter */ messages })),

    // Transform context before convert_to_llm (for pruning, compaction)
    transform_context: Some(Arc::new(|messages, _signal| {
        Box::pin(async move { Ok(prune_old_messages(messages)) })
    })),

    // Steering mode: OneAtATime (default) or All
    steering_mode: QueueMode::OneAtATime,

    // Follow-up mode: OneAtATime (default) or All
    follow_up_mode: QueueMode::OneAtATime,

    // Custom stream function (for proxy backends)
    stream_fn: Some(stream_fn),

    // Session ID for provider caching
    session_id: Some("session-123".into()),

    // Dynamic API key resolution (for expiring OAuth tokens)
    get_api_key: Some(get_api_key_fn),

    // Tool execution mode
    tool_execution: ToolExecutionMode::Parallel,

    // Optional TOON encoding for tool results (default: Off, or ELPH_PROMPT_ENCODING)
    prompt_encoding: Some(PromptEncodingConfig {
        mode: PromptEncodingMode::Toon,
        min_bytes: 2048,
        ..PromptEncodingConfig::default()
    }),

    // Preflight each tool call after args are validated
    before_tool_call: Some(before_tool_call_fn),
    after_tool_call: Some(after_tool_call_fn),

    // Custom thinking budgets for token-based providers
    thinking_budgets: Some(thinking_budgets),

    ..Default::default()
});
```

## TOON prompt encoding

Optional [TOON](https://github.com/toon-format/toon) encoding compresses structured JSON in **model-visible** tool results (and can be used manually in user prompts via `encode_value`). Enabled through `AgentOptions.prompt_encoding` or `ELPH_PROMPT_ENCODING` (`off` | `toon` | `auto`).

```rust
use elph_agent::{PromptEncodingConfig, PromptEncodingMode};

let agent = Agent::new(AgentOptions {
    prompt_encoding: Some(PromptEncodingConfig {
        mode: PromptEncodingMode::Auto,
        ..PromptEncodingConfig::default()
    }),
    ..Default::default()
});
```

Encoding runs after `after_tool_call`, before tool results are sent to the LLM. MCP `structured_content` is supported. Details: [docs/prompt-encoding.md](./docs/prompt-encoding.md).

## Agent State

Access state via `agent.state().await`:

```rust
pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub tools: Vec<AgentTool>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub streaming_message: Option<AgentMessage>,
    pub pending_tool_calls: HashSet<String>,
    pub error_message: Option<String>,
}
```

During streaming, `streaming_message` contains the current partial assistant message.

`is_streaming` remains `true` until the run fully settles, including awaited `agent_end` subscribers.

## Methods

### Prompting

```rust
// Text prompt
agent.prompt_text("Hello", None).await?;

// With images
agent.prompt_text("What's in this image?", Some(vec![image])).await?;

// AgentMessage directly
agent.prompt_messages(vec![user_message]).await?;

// Continue from current context (last message must be user or toolResult)
agent.continue_run().await?;
```

### Control

```rust
agent.abort().await;            // Cancel current operation
agent.wait_for_idle().await;    // Wait for completion
agent.reset().await;            // Clear messages and queues
```

### Events

```rust
agent.subscribe(Arc::new(|event, signal| {
    Box::pin(async move {
        if matches!(event, AgentEvent::AgentEnd { .. }) {
            flush_session_state(&signal).await;
        }
    })
})).await;
```

## Steering and Follow-up

Steering messages let you interrupt the agent while tools are running. Follow-up messages let you queue work after the agent would otherwise stop.

```rust
use elph_agent::{llm_message_to_agent, QueueMode};
use elph_ai::{Message, UserContent};

agent.set_steering_mode(QueueMode::OneAtATime);
agent.set_follow_up_mode(QueueMode::OneAtATime);

// While agent is running tools
agent.steer(llm_message_to_agent(Message::User {
    content: UserContent::Text("Stop! Do this instead.".into()),
    timestamp: now_ms(),
}));

// After the agent finishes its current work
agent.follow_up(llm_message_to_agent(Message::User {
    content: UserContent::Text("Also summarize the result.".into()),
    timestamp: now_ms(),
}));

agent.clear_steering_queue();
agent.clear_follow_up_queue();
agent.clear_all_queues();
```

When steering messages are detected after a turn completes:

1. All tool calls from the current assistant message have already finished
2. Steering messages are injected
3. The LLM responds on the next turn

Follow-up messages are checked only when there are no more tool calls and no steering messages.

## Custom Message Types

In Rust, custom messages use the `CustomAgentMessage` enum instead of TypeScript declaration merging:

```rust
use elph_agent::{AgentMessage, CustomAgentMessage};

let msg = AgentMessage::Custom(CustomAgentMessage::Custom {
    kind: "notification".into(),
    content: serde_json::json!("Info"),
    display: true,
    details: None,
    timestamp: now_ms(),
});
```

Handle custom types in `convert_to_llm`:

```rust
let agent = Agent::new(AgentOptions {
    convert_to_llm: Some(Arc::new(|messages| {
        messages
            .into_iter()
            .filter(|m| m.role() != "custom" || /* keep some */ true)
            .collect()
    })),
    ..Default::default()
});
```

Built-in custom roles (`bashExecution`, `branchSummary`, `compactionSummary`) are converted by `default_convert_to_llm` into user messages with formatted summaries.

## Tools

Define tools using `AgentTool` or the `simple_tool` helper:

```rust
use elph_agent::{AgentTool, AgentToolResult, simple_tool};
use elph_ai::Tool;

let read_file_tool = simple_tool(
    Tool {
        name: "read_file".into(),
        description: "Read a file's contents".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    },
    "Read File",
    |_, args| {
        let path = args["path"].as_str().unwrap_or("").to_string();
        Box::pin(async move {
            let content = tokio::fs::read_to_string(&path).await?;
            Ok(AgentToolResult::text(content))
        })
    },
);
```

Built-in tools are grouped by capability:

| Helper                      | Tools                            |
| --------------------------- | -------------------------------- |
| `create_coding_tools`       | `read`, `bash`, `edit`, `write`  |
| `create_read_only_tools`    | `read`, `grep`, `find`, `ls`     |
| `create_all_tools`          | all seven filesystem tools above |
| `create_web_tools`          | `websearch`, `webfetch`          |
| `create_all_tools_with_web` | filesystem tools + web tools     |

```rust
use elph_agent::LocalExecutionEnv;
use elph_agent::{create_all_tools, create_web_tools, create_all_tools_with_web};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let coding = create_all_tools(env.clone());
let web = create_web_tools(); // no ExecutionEnv required
let all = create_all_tools_with_web(env);
```

`grep` and `find` use [`fff-search`](https://crates.io/crates/fff-search) for fast filesystem indexing and content search. `ls` uses [`walkdir`](https://crates.io/crates/walkdir) on a blocking thread pool. `read`, `write`, `edit`, and `bash` use `ExecutionEnv` directly.

`websearch` and `webfetch` query the public web via HTTP. They support multiple search providers with automatic ranking and fallback, and optionally use the [Obscura](https://docs.obscura.sh/guides/use-as-a-rust-library) headless browser for scraping when HTTP alone is insufficient. Web tools do not require an `ExecutionEnv`.

```rust
use elph_agent::create_web_tools;

let tools = create_web_tools();
// websearch: query the web (DuckDuckGo, Brave, Exa, FireCrawl, Jina, Perplexity, Tavily, SerpAPI)
// webfetch:    fetch a public URL as plain text
```

Set provider API keys via environment variables (`BRAVE_SEARCH_API_KEY`, `EXA_API_KEY`, `TAVILY_API_KEY`, etc.). DuckDuckGo, Jina, and FireCrawl work without keys. See [docs/tools.md](./docs/tools.md) for the full engine ranking table and parameters.

### Cargo features

| Feature   | Default | Description                                           |
| --------- | ------- | ----------------------------------------------------- |
| `obscura` | yes     | Embed Obscura for browser-based search/fetch fallback |

Disable Obscura for faster builds when browser fallback is not needed:

```bash
cargo build -p elph-agent --no-default-features
```

The first build with `obscura` compiles V8 from source and can take a long time.

See [docs/tools.md](./docs/tools.md) for parameters, output formats, truncation limits, and examples.

### Error Handling

**Return an error** when a tool fails. Do not return error messages as content.

```rust
Box::pin(async move {
    if !path.exists() {
        return Err(anyhow::anyhow!("File not found: {path}"));
    }
    Ok(AgentToolResult::text("..."))
})
```

Thrown errors are caught by the agent and reported to the LLM as tool errors with `is_error: true`.

Return `terminate: Some(true)` from `execute` or `after_tool_call` to hint that the agent should stop after the current tool batch.

## Proxy Usage

For apps that proxy through a backend:

```rust
use elph_agent::{Agent, AgentOptions, ProxyStreamOptions, stream_proxy};
use std::sync::Arc;

let stream_fn: StreamFn = Arc::new(|model, context, options| {
    stream_proxy(
        model,
        context,
        ProxyStreamOptions {
            base: options.unwrap_or_default(),
            auth_token: "...".into(),
            proxy_url: "https://your-server.com".into(),
        },
    )
});

let agent = Agent::new(AgentOptions {
    stream_fn: Some(stream_fn),
    ..Default::default()
});
```

## Low-Level API

For direct control without the `Agent` class:

```rust
use elph_agent::{agent_loop, agent_loop_continue, AgentContext, AgentLoopConfig, default_convert_to_llm_fn};

let context = AgentContext {
    system_prompt: "You are helpful.".into(),
    messages: vec![],
    tools: vec![],
};

let config = AgentLoopConfig {
    model,
    convert_to_llm: default_convert_to_llm_fn(),
    tool_execution: ToolExecutionMode::Parallel,
    ..Default::default()
};

let user_message = llm_message_to_agent(Message::User { /* ... */ });

let mut stream = agent_loop(vec![user_message], context, config, None, None);
while let Some(event) = stream.next_event().await {
    println!("{event:?}");
}

// Continue from existing context
let mut stream = agent_loop_continue(context, config, None, None);
```

These low-level streams are observational. They preserve event order, but they do not wait for your async event handling to settle before later producer phases continue. If you need message processing to act as a barrier before tool preflight, use the `Agent` class instead of raw `agent_loop()` or `agent_loop_continue()`.

## AgentHarness

`AgentHarness` is the session-backed orchestration layer above the low-level agent loop. It owns session persistence, runtime configuration, resource resolution, compaction, tree navigation, and extension hooks.

```rust
use elph_agent::{
    AgentHarness, AgentHarnessOptions, AgentHarnessResources, AgentThinkingLevel,
    InMemorySessionStorage, LocalExecutionEnv, QueueMode, Session, SystemPrompt, echo_tool,
};
use elph_ai::{Models, builtin_models};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let session = Session::new(InMemorySessionStorage::new(None)?)?;
let models: Arc<Models> = builtin_models(None).into_arc();

let harness = AgentHarness::new(AgentHarnessOptions {
    env,
    session,
    models,
    model,
    thinking_level: AgentThinkingLevel::Off,
    tools: vec![echo_tool()],
    resources: AgentHarnessResources::default(),
    system_prompt: SystemPrompt::Static("You are helpful.".into()),
    stream_options: Default::default(),
    active_tool_names: vec![],
    steering_mode: QueueMode::OneAtATime,
    follow_up_mode: QueueMode::OneAtATime,
})?;

harness.prompt("hello", None).await?;
harness.on_context(|event| async move { Ok(None) }).await;
harness.compact(None).await?;
harness.navigate_tree(target_entry_id, None).await?;
```

See [docs/agent-harness.md](./docs/agent-harness.md) for lifecycle, phases, save points, and pending writes. See [docs/hooks.md](./docs/hooks.md) for hook registration and mutation semantics.

## Elph Application Integration

Beyond the agent runtime, `elph-agent` provides shared application scaffolding used by Elph binaries (`elph`, `eclaw`):

### AgentBuilder

`AgentBuilder` resolves logging and initialization settings shared across apps:

```rust
use elph_agent::AgentBuilder;

let init = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
    .env_prefix("ELPH")
    .app_name("elph")
    .build();
```

### Datastore

`ensure_database` / `ensure_databases` initialize local Turso databases and run migrations:

```rust
use elph_agent::{DatabaseSpec, ensure_databases};

ensure_databases(&[DatabaseSpec {
    path: db_path.as_path(),
    migrations: MY_MIGRATIONS,
}]).await?;
```

### Session Backends

| Backend                  | Use case                                |
| ------------------------ | --------------------------------------- |
| `InMemorySessionStorage` | Tests, ephemeral sessions               |
| `JsonSessionStorage`     | File-backed append-only sessions        |
| `TursoSessionStorage`    | Durable local sessions with SQL queries |

### Prompt module (`prompt/`)

All prompt constants, filesystem slash-command templates, and built-in formatters live under `elph-agent/src/prompt/`:

| Submodule             | Purpose                                                                           |
| --------------------- | --------------------------------------------------------------------------------- |
| `prompt/builtin/`     | Static runtime prompts — plan mode, compaction summarization, auto session naming |
| `prompt/external/`    | Load `.md` slash-command templates from disk (`load_prompt_templates`)            |
| `prompt/invoke`       | Argument parsing and `$1` / `$ARGUMENTS` substitution for template invocation     |
| `prompt/session_name` | `generate_session_name()` — LLM-generated conversation titles                     |

```rust
use elph_agent::{generate_session_name, load_prompt_templates};

// Auto title from transcript (e.g. after first chat turn)
if let Some(title) = generate_session_name(&messages, &models, &model).await {
    println!("Session: {title}");
}

// Filesystem templates
let templates = load_prompt_templates(&env, &search_paths).await;
```

### Skills and Prompt Templates

Load workspace skills and slash-command templates from disk:

```rust
use elph_agent::{load_skills, load_prompt_templates};

let skills = load_skills(&env, &search_paths).await;
let templates = load_prompt_templates(&env, &search_paths).await;
```

#### Skills with Custom Options

```rust
use elph_agent::{
    load_skills_with_options, SkillLoadOptions, SkillValidationSettings,
    resolve_user_skills_dirs, resolve_project_skills_dirs,
};

// Resolve directories based on app name (elph-agent is agnostic)
let user_dirs = resolve_user_skills_dirs("elph");
let project_dirs = resolve_project_skills_dirs("/project", "elph");

// Load with strict validation
let options = SkillLoadOptions {
    validation: SkillValidationSettings { strict_mode: true },
};
let result = load_skills_with_options(&env, &dirs, Some(&options)).await;
```

#### Skills with All Spec Fields

SKILL.md supports all [agentskills.io](https://agentskills.io) fields:

```markdown
---
name: skill-name
description: A description of what this skill does.
license: MIT
compatibility: Requires git and rust-analyzer
metadata:
    author: your-org
    version: "1.0"
allowed-tools: read grep bash
---

# Skill Instructions

Your skill content here...
```

## Examples

| Example | Description |
| ------- | ----------- |
| `basic_agent` | OpenCode Zen `big-pickle` through `Agent` |
| `agent_skills` | Comprehensive skills demo with all spec fields |
| `agent_skill_math` | Math expert skill with real AI model call |
| `toon_no_tools` | TOON in user prompt (no tool calling) |
| `toon_tool_call` | TOON on custom tool JSON results |
| `toon_mcp_deepwiki` | TOON on DeepWiki MCP tool results |
| `default_no_tools` | Baseline (encoding off) — pair with `toon_no_tools` |
| `default_tool_call` | Baseline (encoding off) — pair with `toon_tool_call` |
| `default_mcp_deepwiki` | Baseline (encoding off) — pair with `toon_mcp_deepwiki` |

```bash
export OPENCODE_API_KEY="your-key"

cargo run -p elph-agent --example basic_agent
cargo run -p elph-agent --example toon_tool_call
cargo run -p elph-agent --example default_tool_call   # same prompt, compare tokens

cargo run -p elph-agent --features mcp --example toon_mcp_deepwiki
```

TOON comparison examples print a **Comparison summary** (tokens in/out, prompt bytes). Use identical flags on paired `toon_*` and `default_*` runs. See [docs/prompt-encoding.md](./docs/prompt-encoding.md).

For provider-level OpenCode streaming (without the agent loop), see `elph-ai` example `opencode_big_pickle`.

## Documentation

| Document                                              | Description                                    |
| ----------------------------------------------------- | ---------------------------------------------- |
| [tools.md](./docs/tools.md)                             | Built-in tools, web search/fetch, `fff-search` |
| [prompt-encoding.md](./docs/prompt-encoding.md)         | TOON encoding for tool results and prompts     |
| [skills.md](./docs/skills.md)                         | Skills loading, validation, formatting         |
| [mcp.md](./docs/mcp.md)                               | MCP client, tool naming, DeepWiki              |
| [agent-harness.md](./docs/agent-harness.md)           | Harness lifecycle, phases, save points         |
| [hooks.md](./docs/hooks.md)                           | Hook design and mutation semantics             |
| [models.md](./docs/models.md)                         | `elph_ai::Models` integration with harness     |
| [durable-harness.md](./docs/durable-harness.md)       | Semi-durable harness design (planned)          |
| [observability.md](./docs/observability.md)           | Observability design notes (planned)           |

Full `elph-ai` provider architecture is documented in the [`elph-ai`](../elph-ai) crate.

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
