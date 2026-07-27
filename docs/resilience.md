# Resilience — Circuit Breaker, Rate Limiter & Retry

Design for outbound HTTP resilience: rate limiting, circuit breaking, and retry with
exponential backoff for LLM provider API calls and external service requests.

## Problem

Elph makes outbound HTTP calls to 30+ LLM providers and external services (web search,
web fetch, MCP servers). Without resilience:

1. **Cascading failures** — A failing provider (e.g., 503 errors) causes requests to pile
   up, exhausting connections, memory, and threads.
2. **Rate limit violations** — Providers enforce RPM/TPM limits. Exceeding them triggers
   429 errors, which waste round-trips and delay the user.
3. **Transient failures** — Network blips, timeouts, and 5xx errors are common. Without
   retry, a single transient failure aborts the entire turn.

## Architecture

Three layers of resilience, applied from outermost to innermost:

```
┌──────────────────────────────────────────┐
│         ResilienceManager                │  Per-provider coordination
│  ┌───────────────┐ ┌──────────────────┐  │
│  │ governor      │ │ failsafe         │  │
│  │ Rate Limiter  │ │ Circuit Breaker  │  │
│  │ (token bucket)│ │ (state machine)  │  │
│  └───────────────┘ └──────────────────┘  │
└──────────────┬───────────────────────────┘
               │
       ┌───────▼───────┐
       │   backon      │  Retry + exponential backoff + jitter
       │   Retry       │
       └───────┬───────┘
               │
       ┌───────▼───────┐
       │ Provider API  │  HTTP call to Anthropic, OpenAI, etc.
       └───────────────┘
```

### 1. Rate Limiter (`governor`)

Token bucket per provider. Prevents exceeding provider RPM/TPM limits.

- **Algorithm**: Generic Cell Rate Algorithm (GCRA)
- **Granularity**: Per provider (e.g., separate limiters for `anthropic` and `openai`)
- **Behavior**: Non-blocking `check()` returns immediately; async `until_ready()` yields
  until a token is available
- **Defaults**: 10 requests/second, burst of 5

### 2. Circuit Breaker (`failsafe`)

Stops sending requests to a failing provider, giving it time to recover.

- **States**: Closed → Open → Half-Open → Closed
- **Trip condition**: N consecutive failures (default: 5)
- **Recovery**: Exponential backoff from 1s to recovery timeout (default: 30s)
- **Half-open**: Allows probe requests; success closes the circuit

### 3. Retry (`backon`)

Automatically retries transient failures with exponential backoff + jitter.

- **Strategy**: Exponential backoff with jitter
- **Max retries**: 3 (configurable)
- **Backoff range**: 500ms → 30s (configurable)
- **Retryable errors**: 429, 5xx, timeout, connection errors
- **Non-retryable**: 4xx (except 429), billing/quota errors

## Configuration

All settings are configurable via environment variables:

```bash
# Per-provider rate limiting
ELPH_RATE_LIMIT_<PROVIDER>_RPS=10       # requests per second
ELPH_RATE_LIMIT_<PROVIDER>_BURST=5      # burst size

# Per-provider circuit breaker
ELPH_CIRCUIT_BREAKER_<PROVIDER>_THRESHOLD=5   # failures before trip
ELPH_CIRCUIT_BREAKER_<PROVIDER>_TIMEOUT_MS=30000  # recovery timeout

# Global retry settings
ELPH_MAX_RETRIES=3
ELPH_MAX_RETRY_DELAY_MS=30000
```

Provider names are uppercased with hyphens replaced by underscores. Examples:

| Provider         | Environment prefix                   |
| ---------------- | ------------------------------------ |
| `anthropic`      | `ELPH_RATE_LIMIT_ANTHROPIC_RPS`      |
| `openai`         | `ELPH_RATE_LIMIT_OPENAI_RPS`         |
| `google`         | `ELPH_RATE_LIMIT_GOOGLE_RPS`         |
| `amazon-bedrock` | `ELPH_RATE_LIMIT_AMAZON_BEDROCK_RPS` |

### Programmatic configuration

```rust
use elph_ai::resilience::{ResilienceManager, ResilienceConfig};

let manager = ResilienceManager::new(
    ResilienceConfig::for_provider("anthropic")
        .with_rps(10)
        .with_burst(5)
        .with_failure_threshold(3)
        .with_recovery_timeout(Duration::from_secs(60))
        .with_max_retries(5)
        .with_backoff(Duration::from_millis(200), Duration::from_secs(10))
);
```

## Integration Points

### Provider API calls (`elph-ai`)

The primary integration point is `api/common.rs`. All provider API implementations
(Anthropic, OpenAI, Google, etc.) make HTTP calls through `send_with_abort()`.

Integration flow:

```
1. Check rate limiter  →  Wait if needed
2. Check circuit breaker →  Fail fast if open
3. Send HTTP request
4. On success →  Record success in circuit breaker
5. On error →  Check if retryable →  Retry with backoff
```

### Web tools (`elph-agent`)

`web_fetch` and `web_search` tools use `do_get()` / `do_post_json()` in
`tools/web/common.rs`. Rate limiting prevents hitting search engine API quotas.

### MCP connections (`elph-agent`)

MCP server connections can be rate-limited to prevent overwhelming remote servers.

## Error types

```rust
/// Errors from resilience checks
pub enum ResilienceError {
    RateLimited,   // Too many requests, caller should wait
    CircuitOpen,   // Provider is failing, call rejected
}

/// Errors from circuit breaker protected calls
pub enum CircuitBreakerError<E> {
    Open,     // Circuit is open — fail fast
    Inner(E), // Call was made but failed
}
```

## Defaults

| Setting             | Default | Rationale                             |
| ------------------- | ------- | ------------------------------------- |
| Requests per second | 10      | Conservative for most providers       |
| Burst size          | 5       | Allow short bursts                    |
| Failure threshold   | 5       | Avoid tripping on brief glitches      |
| Recovery timeout    | 30s     | Give providers time to recover        |
| Max retries         | 3       | Balance between persistence and speed |
| Initial backoff     | 500ms   | Start fast, back off on repeated fail |
| Max backoff         | 30s     | Cap delay to avoid long waits         |

## Library choices

| Library  | Version | Purpose         | Rationale                                       |
| -------- | ------- | --------------- | ----------------------------------------------- |
| governor | 0.10.4  | Rate limiter    | Most mature Rust rate limiter (46M+ downloads)  |
| failsafe | 1.3.0   | Circuit breaker | Only dedicated Rust CB library (15M+ downloads) |
| backon   | 1.6.0   | Retry + backoff | Best ergonomics, 64M+ downloads                 |

## Testing

Unit tests cover each layer independently:

- **Rate limiter**: Burst exhaustion, token sharing across clones
- **Circuit breaker**: Trip after threshold, success resets, open rejects calls
- **Retry**: Transient failure recovery, retry exhaustion
- **Manager**: Full lifecycle, provider independence, config loading

Integration tests verify end-to-end behavior with mock providers.

## Future work

- **Per-model rate limits**: Some providers have per-model quotas (e.g., GPT-4 vs GPT-3.5)
- **Distributed rate limiting**: For multi-process setups using Redis backend
- **Metrics export**: Prometheus/OTEL metrics for circuit breaker state changes
- **Adaptive thresholds**: Auto-tune failure threshold based on provider error rates
