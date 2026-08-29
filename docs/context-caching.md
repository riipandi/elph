# Context caching

Elph uses provider-managed prompt-prefix caching to reduce repeated input
processing for multi-turn agent sessions. Elph does not store model KV state or
prompt bodies in a local cache. The provider remains responsible for cache
storage, expiry, invalidation, and cache-hit routing.

This document describes the implementation currently shipped in `elph-ai`,
`elph-agent`, and `coding-agent`. The design rationale and provider research are
recorded in the [context-caching PRD](./planned/context-caching.md).

## Scope

The implementation covers:

- one provider-neutral retention policy;
- stable session affinity for requests that can reuse a prefix;
- provider-specific request translation;
- suppression of unnecessary cache writes for one-shot operations;
- normalized cache read/write usage and cost accounting.

The implementation intentionally does not include:

- a local response or KV cache;
- a cross-provider cache service;
- persistent prompt-cache bodies;
- explicit Gemini `CachedContent` lifecycle management;
- a TUI settings screen or slash command.

## Retention policy

`elph_ai::CacheRetention` is the source type:

| Value | Behavior |
| --- | --- |
| `None` | Do not request cache creation or affinity where the provider exposes a control. This is best effort for providers with implicit-only caching. |
| `Short` | Use the provider's normal ephemeral prompt-cache behavior. |
| `Long` | Request the longest supported non-durable retention. Unsupported long retention degrades to `Short`. |

The policy is resolved for every request with this precedence:

1. `StreamOptions.cache_retention`;
2. the host/harness stream option;
3. `{PREFIX}_CACHE_RETENTION`;
4. `Short`.

The default identity uses the `ELPH` prefix. Hosts that configure a different
`ClientIdentity.env_prefix` use the corresponding prefix, for example
`MYAPP_CACHE_RETENTION`.

Accepted environment values are case-insensitive and whitespace-trimmed:

```text
none
short
long
```

An invalid value falls back to `short` and emits one warning per process, rather
than logging once for every request.

### Request-level configuration

Applications can override the environment policy for one request:

```rust
use elph_ai::{CacheRetention, StreamOptions};

let options = StreamOptions {
    cache_retention: Some(CacheRetention::Long),
    session_id: Some(session_id),
    ..Default::default()
};
```

Use `CacheRetention::None` for a request that must not opt into cache creation or
affinity:

```rust
let options = StreamOptions {
    cache_retention: Some(CacheRetention::None),
    ..Default::default()
};
```

The request-level value is also preserved when harness options are merged.
`Some(CacheRetention::None)` is distinct from an unspecified policy and cannot
be replaced by a harness or environment default.

## Workload policy

Elph chooses the policy according to expected prefix reuse:

| Workload | Policy | Affinity |
| --- | --- | --- |
| Main interactive agent turn | `Short` | Stable Elph session UUID |
| Tool-call continuation in the same turn | `Short` | Same session UUID |
| Worker/subagent multi-step loop | `Short` | Worker/subagent session UUID |
| Compaction summary | `None` | None |
| Branch summary | `None` | None |
| Session title generation | `None` | None |
| `/aside` one-shot answer | `None` | None |

Retries preserve the original retention policy and session key. A resumed
session reuses its durable session UUID, allowing the provider to route requests
to the same cache partition when supported.

Session IDs are opaque routing hints. They are not derived from prompt text,
repository paths, usernames, API keys, or other sensitive values. Elph does not
combine root and child sessions under one key.

## Provider behavior

### Anthropic Messages

Anthropic uses explicit `cache_control` markers:

- `None` omits cache markers;
- `Short` uses the provider's `ephemeral` marker;
- `Long` adds the one-hour TTL only when the model catalog says it is supported;
- otherwise `Long` falls back to the short marker.

The adapter marks only stable/cacheable locations:

1. the stable system prompt;
2. the final immediate tool definition when tool cache control is supported;
3. the last cacheable block in the final user message.

The final cacheable message block can be text, image, or `tool_result`. Deferred
tool definitions are not marked because the set of loaded deferred tools can
change during a session.

Anthropic cache creation metadata is normalized into `Usage.cache_write` and,
for the one-hour tier, `Usage.cache_write_1h`.

### OpenAI Responses

For models with catalog capability
`supports_explicit_prompt_cache_mode`:

- `None` sends `prompt_cache_options: {"mode": "explicit"}` without adding
  explicit breakpoints or a cache key;
- `Short` uses normal implicit caching and sends the stable `prompt_cache_key`;
- `Long` keeps the model's current behavior and does not send the legacy
  `prompt_cache_retention: "24h"` field.

For legacy supported targets:

- `Short` uses implicit provider caching and `prompt_cache_key`;
- `Long` sends `prompt_cache_retention: "24h"` only when catalog or direct
  provider compatibility permits it.

Unknown OpenAI-compatible endpoints do not receive cache-specific fields based
only on their model name or URL shape. They must opt in through model
compatibility data. This avoids sending OpenAI-only fields to gateways that
reject them.

### OpenAI Chat Completions

Direct OpenAI and explicitly compatible targets receive the stable prompt key
when retention is not `None`. Long-retention fields are sent only when the
resolved compatibility data permits them. Arbitrary OpenAI-compatible proxies
remain conservative and receive no inferred cache fields.

### OpenRouter and compatible gateways

Session affinity uses the catalog's `SessionAffinityFormat`, such as
`x-session-id`, `session_id`, or the provider-specific affinity headers.
Explicit cache-control payloads are also catalog-controlled; the adapter does
not infer Anthropic content-block support from a routed model name.

### Amazon Bedrock

Bedrock uses native `cachePoint` blocks:

- `None` omits cache points;
- `Short` adds the default cache point at supported positions;
- `Long` requests `ONE_HOUR` only for supported models, otherwise using the
  default short behavior.

### Mistral Conversations

When retention is not `None`, the adapter sends the stable session ID as
`prompt_cache_key`. `None` suppresses that field.

### Other providers

Providers with implicit-only caching may continue to cache internally even when
Elph cannot disable that behavior. `None` is therefore a request not to add
explicit cache controls or affinity, not a guarantee that the provider will
perform no internal caching.

## Session affinity

Affinity is applied by the shared adapter helper only when all of the following
are true:

- retention is not `None`;
- the request has a non-empty session ID;
- the model/provider compatibility data enables the relevant format.

Caller-provided headers remain authoritative. Elph inserts an affinity header
only when that header is absent. Provider-specific formats include:

| Format | Headers |
| --- | --- |
| OpenAI | `session_id`, `x-client-request-id`, `x-session-affinity` |
| OpenAI without session field | `x-client-request-id`, `x-session-affinity` |
| OpenRouter | `x-session-id` |

Affinity improves routing but does not replace prefix equality. Changes to the
model, system prompt, tools, mode, or compaction state legitimately invalidate a
provider prefix.

## Usage and cost accounting

Provider adapters normalize prompt-cache usage into `Usage`:

| Field | Meaning |
| --- | --- |
| `input` | Uncached input tokens |
| `cache_read` | Provider-reported cached input tokens |
| `cache_write` | Input tokens written to a cache |
| `cache_write_1h` | Anthropic input written with the one-hour TTL |
| `total_tokens` | Normalized total including input, output, cache reads, and cache writes |

For OpenAI Responses, `input_tokens_details.cached_tokens` is treated as
`cache_read`, and `input_tokens_details.cache_write_tokens` as `cache_write`.
The uncached input value is calculated as:

```text
input = input_tokens - cached_tokens - cache_write_tokens
```

All subtraction is saturating so malformed or provider-specific totals cannot
underflow. Cost calculation uses the model's cache rates when available,
including the Anthropic one-hour write tier.

The TUI turn stats card sums these values across every API call in a turn,
including tool-call iterations. See
[Usage Accounting & Token Displays](./design/usage-accounting.md) for the
distinction between per-call usage, session totals, and context-size estimates.

## Prefix stability and invalidation

Caching does not change Elph's source of truth. The current request state always
wins over a cache hit.

The following should remain deterministic for good cache reuse:

- system-prompt section ordering;
- tool-definition ordering;
- the stable prefix before changing conversation content;
- the session ID across turns and retries.

The following are expected cache invalidation boundaries:

- model changes;
- system-prompt changes;
- tool activation or removal;
- provider, MCP, skill, template, or hook reloads;
- compaction;
- branch changes.

Do not reorder tools or freeze dynamic state solely to improve cache hits.
Provider prompt caching and transport continuation/response IDs are separate
mechanisms and must not be reused interchangeably.

## Troubleshooting

### Cache writes are not visible

Check the provider's response usage fields rather than assuming that a missing
field means no caching occurred. Some providers cache implicitly but do not
report cache writes in the same format.

For deterministic inspection, attach an `on_payload` callback or use the
provider adapter tests to inspect the generated request. Do not log API keys,
full prompts, or sensitive session contents.

### `Long` does not produce a long-lived cache

`Long` is capability-aware. If the model catalog does not advertise long
retention, Elph intentionally falls back to `Short` instead of sending a field
that the provider may reject. For OpenAI Responses models using explicit prompt
cache mode, long retention currently remains the model's normal behavior.

### A one-shot request has no cache key

This is intentional. Compaction, summaries, session naming, and `/aside` are
configured with `None` because their prefixes are unlikely to be reused.

### Environment policy appears ignored

An explicit `StreamOptions.cache_retention` or harness policy takes precedence.
Verify the active identity prefix as well: a custom `ClientIdentity` may require
`{PREFIX}_CACHE_RETENTION` instead of `ELPH_CACHE_RETENTION`.

## Verification

The implementation is covered by deterministic tests for:

- policy precedence and environment parsing;
- request payloads for Anthropic and OpenAI Responses;
- cache-marker placement on text, image, and `tool_result` blocks;
- unknown OpenAI-compatible proxy safety;
- session-affinity suppression and format selection;
- Anthropic one-hour and OpenAI cache-write usage parsing;
- usage normalization and total-token fallback.

Provider live probes remain manual release checks because provider routing,
cache TTL, and cache-hit behavior are nondeterministic and require credentials.
