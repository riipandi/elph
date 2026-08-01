//! Embedded raw provider catalog JSON for CONFIG_DIR unpack (do not hand-edit;
//! regenerate when model catalogs change via generate-models / this scaffold).

/// `(provider_id kebab-case, raw JSON file body)`.
pub fn embedded_provider_json() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "amazon-bedrock",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/amazon_bedrock.json")),
        ),
        (
            "ant-ling",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ant_ling.json")),
        ),
        (
            "anthropic",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/anthropic.json")),
        ),
        (
            "azure-openai-responses",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/azure_openai_responses.json")),
        ),
        (
            "baseten",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/baseten.json")),
        ),
        (
            "cerebras",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/cerebras.json")),
        ),
        (
            "cloudflare-ai-gateway",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/cloudflare_ai_gateway.json")),
        ),
        (
            "cloudflare-workers-ai",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/cloudflare_workers_ai.json")),
        ),
        (
            "deepseek",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/deepseek.json")),
        ),
        (
            "fireworks",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/fireworks.json")),
        ),
        (
            "github-copilot",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/github_copilot.json")),
        ),
        (
            "google",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/google.json")),
        ),
        (
            "google-vertex",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/google_vertex.json")),
        ),
        ("groq", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/groq.json"))),
        (
            "huggingface",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/huggingface.json")),
        ),
        ("hyper", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/hyper.json"))),
        ("kilo", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/kilo.json"))),
        (
            "kimi-coding",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/kimi_coding.json")),
        ),
        (
            "minimax",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/minimax.json")),
        ),
        (
            "minimax-cn",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/minimax_cn.json")),
        ),
        (
            "mistral",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/mistral.json")),
        ),
        (
            "moonshotai",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/moonshotai.json")),
        ),
        (
            "moonshotai-cn",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/moonshotai_cn.json")),
        ),
        (
            "neuralwatt",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/neuralwatt.json")),
        ),
        (
            "nvidia",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/nvidia.json")),
        ),
        (
            "ollama-cloud",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ollama_cloud.json")),
        ),
        (
            "openai",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/openai.json")),
        ),
        (
            "openai-codex",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/openai_codex.json")),
        ),
        (
            "opencode",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/opencode.json")),
        ),
        (
            "opencode-go",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/opencode_go.json")),
        ),
        (
            "opengateway",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/opengateway.json")),
        ),
        (
            "openrouter",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/openrouter.json")),
        ),
        (
            "qwen-token-plan",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/qwen_token_plan.json")),
        ),
        (
            "qwen-token-plan-cn",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/qwen_token_plan_cn.json")),
        ),
        (
            "sumopod",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/sumopod.json")),
        ),
        (
            "together",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/together.json")),
        ),
        (
            "tokenrouter",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/tokenrouter.json")),
        ),
        (
            "vercel-ai-gateway",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/vercel_ai_gateway.json")),
        ),
        ("xai", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/xai.json"))),
        (
            "xiaomi",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/xiaomi.json")),
        ),
        (
            "xiaomi-token-plan-ams",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/xiaomi_token_plan_ams.json")),
        ),
        (
            "xiaomi-token-plan-cn",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/xiaomi_token_plan_cn.json")),
        ),
        (
            "xiaomi-token-plan-sgp",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/xiaomi_token_plan_sgp.json")),
        ),
        ("zai", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/zai.json"))),
        (
            "zai-coding-cn",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/zai_coding_cn.json")),
        ),
    ]
}
