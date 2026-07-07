# AgentHarness hooks design

Final design as implemented in `elph-agent`.

## Core model

Harness hooks are split into two categories:

1. **Observational** — `subscribe` sees events but does not participate in mutation semantics.
2. **Mutation** — typed handlers (`on_context`, `on_tool_call`, etc.) can return results that alter harness behavior.

Typed handlers are registered on `AgentHarness` and stored in `HookRegistry`. The harness calls `emit_*` methods at lifecycle boundaries.

## Registration API

```rust
// All events (agent loop + harness-specific)
harness.subscribe(|event, signal| async move {
    match event {
        AgentHarnessEvent::Agent(agent_event) => { /* ... */ }
        AgentHarnessEvent::Own(own_event) => { /* ... */ }
    }
}).await;

// Upstream-compatible generic hook registration (snake_case event names)
harness.on("before_provider_payload", |event| async move {
    let AgentHarnessOwnEvent::BeforeProviderPayload(event) = event else {
        return None;
    };
    Some(HarnessHookResult::BeforeProviderPayload(BeforeProviderPayloadResult {
        payload: event.payload, // transform as needed
    }))
}).await?;

// Typed mutation hooks (Rust-idiomatic aliases)
harness.on_before_agent_start(|event| async move {
    Ok(Some(BeforeAgentStartResult {
        messages: None,
        system_prompt: None,
    }))
}).await;

harness.on_context(|event| async move {
    Ok(None) // no transform
}).await;

harness.on_tool_call(|event| async move {
    Ok(Some(ToolCallHookResult {
        block: true,
        reason: Some("blocked".into()),
    }))
}).await;

harness.on_tool_result(|event| async move {
    Ok(Some(ToolResultPatch {
        terminate: Some(true),
        ..ToolResultPatch::default()
    }))
}).await;
```

Context hooks return `HarnessOpResult<Option<ContextResult>>`. A hook error propagates as `AgentHarnessError` and fails the turn.

## Event types

### Agent loop events (`AgentHarnessEvent::Agent`)

These mirror the low-level `AgentEvent` enum:

| Event                   | Description               |
| ----------------------- | ------------------------- |
| `agent_start`           | Agent begins processing   |
| `agent_end`             | Final event for the run   |
| `turn_start`            | New turn begins           |
| `turn_end`              | Turn completes            |
| `message_start`         | Message begins            |
| `message_update`        | Assistant streaming delta |
| `message_end`           | Message completes         |
| `tool_execution_start`  | Tool begins               |
| `tool_execution_update` | Tool streams progress     |
| `tool_execution_end`    | Tool completes            |

### Harness-specific events (`AgentHarnessEvent::Own`)

| Event                     | Description                              |
| ------------------------- | ---------------------------------------- |
| `context`                 | Context transform hook (internal)        |
| `before_agent_start`      | Inject messages / override system prompt |
| `before_provider_request` | Patch stream options before request      |
| `before_provider_payload` | Transform provider payload               |
| `after_provider_response` | Observe provider response                |
| `tool_call`               | Block or allow tool execution            |
| `tool_result`             | Patch tool result before emission        |
| `session_before_compact`  | Cancel or customize compaction           |
| `session_before_tree`     | Cancel or customize tree navigation      |
| `model_update`            | Model changed                            |
| `thinking_level_update`   | Thinking level changed                   |
| `tools_update`            | Tool registry changed                    |
| `resources_update`        | Resources changed                        |
| `queue_update`            | Steering/follow-up queue drained         |
| `save_point`              | Turn save point reached                  |
| `settled`                 | Operation settled                        |
| `session_compact`         | Compaction completed                     |
| `session_tree`            | Tree navigation completed                |

## Mutation semantics

### Context transform

Handlers run in order. Each sees the current messages from the previous handler.

```rust
harness.on_context(|event| async move {
    let pruned = prune_old_messages(event.messages.clone());
    if pruned.len() == event.messages.len() {
        Ok(None)
    } else {
        Ok(Some(ContextResult { messages: pruned }))
    }
}).await;
```

If any handler returns an error, the turn fails with `AgentHarnessError`.

### Before agent start

Collect injected messages, chain system prompt.

```rust
harness.on_before_agent_start(|event| async move {
    Ok(Some(BeforeAgentStartResult {
        messages: Some(vec![injected_user_message]),
        system_prompt: Some("Updated prompt".into()),
    }))
}).await;
```

### Tool call

Sequential, early exit on block.

```rust
harness.on_tool_call(|event| async move {
    if event.tool_name == "bash" {
        Ok(Some(ToolCallHookResult {
            block: true,
            reason: Some("bash is disabled".into()),
        }))
    } else {
        Ok(None)
    }
}).await;
```

### Tool result

Sequential patch accumulation. Each handler sees the current patched result.

```rust
harness.on_tool_result(|event| async move {
    Ok(Some(ToolResultPatch {
        details: Some(serde_json::json!({ "audited": true })),
        ..ToolResultPatch::default()
    }))
}).await;
```

### Provider request / payload

Provider hooks (`before_provider_request`, `before_provider_payload`, `after_provider_response`) are emitted internally during turn execution. Sequential transform semantics apply; stream option patching supports explicit field deletion.

### Session-before events

`session_before_compact` and `session_before_tree` hooks are handled internally by `HookRegistry`. Public registration for these is not yet exposed on `AgentHarness`; observe them via `subscribe` for now.

## Subscriber deadlock safety

`emit_subscriber` clones the subscriber list before invoking handlers. This prevents deadlocks when a subscriber calls back into the harness (for example, `steer()` or `follow_up()` from within a listener).

Typed handlers are similarly invoked from cloned handler lists.

## Harness usage

The harness only calls `emit_*` at lifecycle boundaries:

```rust
// Internal (simplified)
let result = self.shared.hooks.emit_context(&event, signal).await?;
let messages = result
    .and_then(|r| r.messages)
    .unwrap_or(event.messages);
```

The harness does not store handler policy beyond the `HookRegistry`.

## Comparison with Agent hooks

The low-level `Agent` class uses `before_tool_call` and `after_tool_call` in `AgentOptions` / `AgentLoopConfig`, not the harness hook registry.

| Concern                 | Agent               | AgentHarness                                                                    |
| ----------------------- | ------------------- | ------------------------------------------------------------------------------- |
| Tool preflight          | `before_tool_call`  | `on_tool_call`                                                                  |
| Tool postprocess        | `after_tool_call`   | `on_tool_result`                                                                |
| Context transform       | `transform_context` | `on_context`                                                                    |
| System prompt injection | —                   | `on_before_agent_start`                                                         |
| Provider hooks          | `stream_fn` wrapper | `before_provider_request`, `before_provider_payload`, `after_provider_response` |
| Session lifecycle       | —                   | `session_before_compact`, `session_before_tree`                                 |

Use `Agent` for lightweight, in-memory agents. Use `AgentHarness` when you need session persistence, compaction, tree navigation, skills, and extension hooks.

## Extension loading (planned)

Extension loading can construct a harness and register hooks:

```rust
// Future pattern
let harness = AgentHarness::new(options)?;
harness.on_context(extension_context_handler).await;
harness.on_tool_call(extension_tool_handler).await;
```

For reload, handlers would be cleared and re-registered while the harness is idle.

## Error policy

Hook failures after state commit do not roll back committed state. The public method returns `AgentHarnessError` with code `Hook`.

Context hook failures fail the turn before provider request.

Subscriber failures in `subscribe` handlers propagate as hook errors when the harness awaits settlement.

## Verdict

This design keeps the harness clean: registration lives in `HookRegistry`, mutation semantics are encoded per event type, and observational subscribers are separated from control-plane hooks.
