use crate::types::{Model, OpenAICompletionsCompat, SessionAffinityFormat};

#[derive(Debug, Clone)]
pub struct ResolvedOpenAICompletionsCompat {
    pub supports_store: bool,
    pub supports_developer_role: bool,
    pub supports_reasoning_effort: bool,
    pub supports_usage_in_streaming: bool,
    pub max_tokens_field: String,
    pub requires_tool_result_name: bool,
    pub requires_assistant_after_tool_result: bool,
    pub requires_thinking_as_text: bool,
    pub requires_reasoning_content_on_assistant_messages: bool,
    pub thinking_format: String,
    pub zai_tool_stream: bool,
    pub supports_strict_mode: bool,
    pub cache_control_format: Option<String>,
    pub send_session_affinity_headers: bool,
    pub session_affinity_format: Option<SessionAffinityFormat>,
    pub supports_long_cache_retention: bool,
    pub supports_finish_reason: bool,
    pub supports_thinking_token_budget: bool,
}

pub fn detect_compat(model: &Model) -> ResolvedOpenAICompletionsCompat {
    let provider = model.provider.as_str();
    let base_url = model.base_url.as_str();
    let model_id = model.id.as_str();

    let is_zai = provider == "zai"
        || provider == "zai-coding-cn"
        || base_url.contains("api.z.ai")
        || base_url.contains("open.bigmodel.cn");
    let is_together =
        provider == "together" || base_url.contains("api.together.ai") || base_url.contains("api.together.xyz");
    let is_moonshot = provider == "moonshotai" || provider == "moonshotai-cn" || base_url.contains("api.moonshot.");
    let is_openrouter = provider == "openrouter" || base_url.contains("openrouter.ai");
    let is_cloudflare_workers_ai = provider == "cloudflare-workers-ai" || base_url.contains("api.cloudflare.com");
    let is_cloudflare_ai_gateway =
        provider == "cloudflare-ai-gateway" || base_url.contains("gateway.ai.cloudflare.com");
    let is_nvidia = provider == "nvidia" || base_url.contains("integrate.api.nvidia.com");
    let is_ant_ling = provider == "ant-ling" || base_url.contains("api.ant-ling.com");
    // Multi-vendor OpenAI-compatible gateways (Elph-local + shared). Treat as non-standard so we
    // never send OpenAI-only fields (`store`, `max_completion_tokens`, `developer`, `strict`).
    let is_gateway = matches!(
        provider,
        "tokenrouter"
            | "opengateway"
            | "baseten"
            | "kilo"
            | "sumopod"
            | "hyper"
            | "huggingface"
            | "vercel-ai-gateway"
            | "wafer"
            | "agnes"
            | "nara-router"
            | "databyte"
    ) || base_url.contains("tokenrouter.com")
        || base_url.contains("api.kilo.ai")
        || base_url.contains("opengateway")
        || base_url.contains("baseten.co")
        || base_url.contains("api.hyper.")
        || base_url.contains("huggingface.co")
        || base_url.contains("ai-gateway.vercel.sh")
        || base_url.contains("pass.wafer.ai")
        || base_url.contains("apihub.agnes-ai.com")
        || base_url.contains("router.bynara.id")
        || base_url.contains("ai.databyte.co.id");

    // Vendor routes on gateways (id like `moonshotai/kimi-k3-free`).
    let gateway_moonshot = is_gateway && model_id.starts_with("moonshotai/");
    let gateway_deepseek = is_gateway && (model_id.starts_with("deepseek/") || model_id.starts_with("deepseek-ai/"));
    let is_moonshot_route = is_moonshot || gateway_moonshot;

    let is_non_standard = is_nvidia
        || provider == "cerebras"
        || base_url.contains("cerebras.ai")
        || provider == "xai"
        || base_url.contains("api.x.ai")
        || is_together
        || base_url.contains("chutes.ai")
        || base_url.contains("deepseek.com")
        || is_zai
        || is_moonshot
        || provider == "opencode"
        || base_url.contains("opencode.ai")
        || is_cloudflare_workers_ai
        || is_cloudflare_ai_gateway
        || is_ant_ling
        || is_gateway;

    let use_max_tokens = base_url.contains("chutes.ai")
        || is_moonshot_route
        || is_cloudflare_ai_gateway
        || is_together
        || is_nvidia
        || is_ant_ling
        || is_gateway;

    let is_xai = provider == "xai" || base_url.contains("api.x.ai");
    let is_deepseek = provider == "deepseek" || base_url.contains("deepseek.com") || gateway_deepseek;
    let requires_tool_result_name = is_moonshot_route || provider == "kimi-coding" || base_url.contains("kimi.com");
    let is_openrouter_developer_role_model =
        is_openrouter && (model_id.starts_with("anthropic/") || model_id.starts_with("openai/"));
    // Cache wire formats are catalog capabilities. Do not infer them from a
    // model name because an OpenAI-compatible gateway may reject Anthropic
    // content blocks even when the routed model is Claude.
    let cache_control_format = None;

    let thinking_format = if is_deepseek {
        "deepseek"
    } else if is_zai {
        "zai"
    } else if is_together {
        "together"
    } else if is_ant_ling {
        "ant-ling"
    } else if is_openrouter {
        "openrouter"
    } else {
        "openai"
    };

    // Gateways: conservative defaults (no OpenAI-only fields). Moonshot routes may accept effort.
    let supports_reasoning_effort = if gateway_moonshot {
        true
    } else if is_gateway {
        false
    } else {
        !is_xai && !is_zai && !is_moonshot && !is_together && !is_cloudflare_ai_gateway && !is_nvidia && !is_ant_ling
    };

    ResolvedOpenAICompletionsCompat {
        supports_store: !is_non_standard,
        supports_developer_role: is_openrouter_developer_role_model || (!is_non_standard && !is_openrouter),
        supports_reasoning_effort,
        supports_usage_in_streaming: true,
        max_tokens_field: if use_max_tokens {
            "max_tokens".to_string()
        } else {
            "max_completion_tokens".to_string()
        },
        requires_tool_result_name,
        requires_assistant_after_tool_result: false,
        requires_thinking_as_text: false,
        requires_reasoning_content_on_assistant_messages: is_deepseek || gateway_moonshot,
        thinking_format: thinking_format.to_string(),
        zai_tool_stream: false,
        supports_strict_mode: !is_moonshot_route
            && !is_together
            && !is_cloudflare_ai_gateway
            && !is_nvidia
            && !is_gateway,
        cache_control_format,
        send_session_affinity_headers: false,
        // Unknown endpoints must opt in through catalog compat data before
        // receiving OpenAI's optional cache-retention field.
        supports_long_cache_retention: provider == "openai"
            || provider == "openrouter"
            || base_url.contains("api.openai.com")
            || base_url.contains("openrouter.ai"),
        session_affinity_format: None,
        // Most providers stream `finish_reason`; a few OpenAI-compat servers omit it.
        supports_finish_reason: true,
        // vLLM-style reasoning caps are opt-in via model compat overrides.
        supports_thinking_token_budget: false,
    }
}

pub fn get_compat(model: &Model) -> ResolvedOpenAICompletionsCompat {
    let detected = detect_compat(model);
    let Some(overrides) = model.openai_completions_compat.as_ref() else {
        return detected;
    };
    merge_compat(&detected, overrides)
}

fn merge_compat(
    detected: &ResolvedOpenAICompletionsCompat,
    overrides: &OpenAICompletionsCompat,
) -> ResolvedOpenAICompletionsCompat {
    ResolvedOpenAICompletionsCompat {
        supports_store: overrides.supports_store.unwrap_or(detected.supports_store),
        supports_developer_role: overrides
            .supports_developer_role
            .unwrap_or(detected.supports_developer_role),
        supports_reasoning_effort: overrides
            .supports_reasoning_effort
            .unwrap_or(detected.supports_reasoning_effort),
        supports_usage_in_streaming: overrides
            .supports_usage_in_streaming
            .unwrap_or(detected.supports_usage_in_streaming),
        max_tokens_field: overrides
            .max_tokens_field
            .clone()
            .unwrap_or_else(|| detected.max_tokens_field.clone()),
        requires_tool_result_name: overrides
            .requires_tool_result_name
            .unwrap_or(detected.requires_tool_result_name),
        requires_assistant_after_tool_result: overrides
            .requires_assistant_after_tool_result
            .unwrap_or(detected.requires_assistant_after_tool_result),
        requires_thinking_as_text: overrides
            .requires_thinking_as_text
            .unwrap_or(detected.requires_thinking_as_text),
        requires_reasoning_content_on_assistant_messages: overrides
            .requires_reasoning_content_on_assistant_messages
            .unwrap_or(detected.requires_reasoning_content_on_assistant_messages),
        thinking_format: overrides
            .thinking_format
            .clone()
            .unwrap_or_else(|| detected.thinking_format.clone()),
        zai_tool_stream: overrides.zai_tool_stream.unwrap_or(detected.zai_tool_stream),
        supports_strict_mode: overrides.supports_strict_mode.unwrap_or(detected.supports_strict_mode),
        cache_control_format: overrides
            .cache_control_format
            .clone()
            .or_else(|| detected.cache_control_format.clone()),
        send_session_affinity_headers: overrides
            .send_session_affinity_headers
            .unwrap_or(detected.send_session_affinity_headers),
        session_affinity_format: overrides
            .session_affinity_format
            .clone()
            .or_else(|| detected.session_affinity_format.clone()),
        supports_long_cache_retention: overrides
            .supports_long_cache_retention
            .unwrap_or(detected.supports_long_cache_retention),
        supports_finish_reason: overrides
            .supports_finish_reason
            .unwrap_or(detected.supports_finish_reason),
        supports_thinking_token_budget: overrides
            .supports_thinking_token_budget
            .unwrap_or(detected.supports_thinking_token_budget),
    }
}

pub fn has_tool_history(messages: &[crate::types::Message]) -> bool {
    messages.iter().any(|msg| match msg {
        crate::types::Message::ToolResult { .. } => true,
        crate::types::Message::Assistant(a) => a
            .content
            .iter()
            .any(|b| matches!(b, crate::types::AssistantContentBlock::ToolCall(_))),
        _ => false,
    })
}
