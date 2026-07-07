use crate::types::{Context, Model, SimpleStreamOptions, StreamOptions, ThinkingBudgets, ThinkingLevel};
use crate::utils::estimate::estimate_context_tokens;

const CONTEXT_SAFETY_TOKENS: u32 = 4096;
const MIN_MAX_TOKENS: u32 = 1;

pub fn clamp_max_tokens_to_context(model: &Model, context: &Context, max_tokens: u32) -> u32 {
    if model.context_window == 0 {
        return max_tokens.max(MIN_MAX_TOKENS);
    }
    let available = model
        .context_window
        .saturating_sub(estimate_context_tokens(context).tokens)
        .saturating_sub(CONTEXT_SAFETY_TOKENS);
    max_tokens.min(available.max(MIN_MAX_TOKENS))
}

pub fn build_base_options(
    model: &Model,
    context: &Context,
    options: Option<&SimpleStreamOptions>,
    api_key: Option<String>,
) -> StreamOptions {
    let opts = options.map(|o| &o.base);
    StreamOptions {
        temperature: opts.and_then(|o| o.temperature),
        max_tokens: Some(clamp_max_tokens_to_context(
            model,
            context,
            opts.and_then(|o| o.max_tokens).unwrap_or(model.max_tokens),
        )),
        api_key: api_key.or_else(|| opts.and_then(|o| o.api_key.clone())),
        transport: opts.and_then(|o| o.transport),
        cache_retention: opts.and_then(|o| o.cache_retention),
        session_id: opts.and_then(|o| o.session_id.clone()),
        headers: opts.and_then(|o| o.headers.clone()),
        timeout_ms: opts.and_then(|o| o.timeout_ms),
        websocket_connect_timeout_ms: opts.and_then(|o| o.websocket_connect_timeout_ms),
        max_retries: opts.and_then(|o| o.max_retries),
        max_retry_delay_ms: opts.and_then(|o| o.max_retry_delay_ms),
        metadata: opts.and_then(|o| o.metadata.clone()),
        env: opts.and_then(|o| o.env.clone()),
        on_payload: opts.and_then(|o| o.on_payload.clone()),
        on_response: opts.and_then(|o| o.on_response.clone()),
        signal: opts.and_then(|o| o.signal.clone()),
    }
}

pub fn clamp_reasoning(effort: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    match effort {
        Some(ThinkingLevel::Xhigh) => Some(ThinkingLevel::High),
        other => other,
    }
}

pub fn adjust_max_tokens_for_thinking(
    base_max_tokens: Option<u32>,
    model_max_tokens: u32,
    reasoning_level: ThinkingLevel,
    custom_budgets: Option<&ThinkingBudgets>,
) -> (u32, u32) {
    let default_budgets = ThinkingBudgets {
        minimal: Some(1024),
        low: Some(2048),
        medium: Some(8192),
        high: Some(16384),
    };
    let level = clamp_reasoning(Some(reasoning_level)).unwrap();
    let thinking_budget = custom_budgets
        .and_then(|b| match level {
            ThinkingLevel::Minimal => b.minimal,
            ThinkingLevel::Low => b.low,
            ThinkingLevel::Medium => b.medium,
            ThinkingLevel::High | ThinkingLevel::Xhigh => b.high,
        })
        .or(match level {
            ThinkingLevel::Minimal => default_budgets.minimal,
            ThinkingLevel::Low => default_budgets.low,
            ThinkingLevel::Medium => default_budgets.medium,
            ThinkingLevel::High | ThinkingLevel::Xhigh => default_budgets.high,
        })
        .unwrap_or(1024);

    let max_tokens = match base_max_tokens {
        None => model_max_tokens,
        Some(base) => (base + thinking_budget).min(model_max_tokens),
    };

    let min_output_tokens = 1024u32;
    let thinking_budget = if max_tokens <= thinking_budget {
        max_tokens.saturating_sub(min_output_tokens)
    } else {
        thinking_budget
    };

    (max_tokens, thinking_budget)
}
