use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::auth::oauth::{anthropic_oauth, github_copilot_oauth, openai_codex_oauth};
use crate::auth::{AuthModel, AuthResolveInput, AuthResult, ModelAuth, ProviderAuth, env_api_key_auth};
use crate::models::catalog::*;
use crate::models::{
    CreateModelsOptions, CreateProviderOptions, MutableModels, Provider, ProviderApi, create_models, create_provider,
};
use crate::providers::adapter::{
    anthropic_messages_api, azure_openai_responses_api, bedrock_converse_stream_api, google_generative_ai_api,
    google_vertex_api, mistral_conversations_api, mixed_openai_apis, openai_codex_responses_api,
    openai_completions_api, openai_responses_api,
};
use crate::providers::cloudflare_auth::{cloudflare_ai_gateway_auth, cloudflare_workers_ai_auth};

macro_rules! simple_provider {
    ($id:expr, $name:expr, $models:expr, $api:expr, $env:expr) => {
        create_provider(CreateProviderOptions {
            id: $id.to_string(),
            name: Some($name.to_string()),
            base_url: None,
            headers: None,
            auth: ProviderAuth {
                api_key: Some(env_api_key_auth($env.1, $env.0.to_vec())),
                oauth: None,
            },
            models: $models.to_vec(),
            refresh_models: None,
            api: ProviderApi::Single($api()),
        })
    };
}

pub fn amazon_bedrock_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "amazon-bedrock".to_string(),
        name: Some("Amazon Bedrock".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(bedrock_auth()),
            oauth: None,
        },
        models: AMAZON_BEDROCK_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(bedrock_converse_stream_api()),
    })
}

fn bedrock_auth() -> crate::auth::ApiKeyAuth {
    crate::auth::ApiKeyAuth {
        name: "AWS credentials".to_string(),
        resolve: Arc::new(|input: AuthResolveInput| {
            Box::pin(async move {
                if let Some(key) = input.credential.and_then(|c| c.key) {
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(key),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("stored credential".to_string()),
                    });
                }
                let checks = [
                    "AWS_BEARER_TOKEN_BEDROCK",
                    "AWS_PROFILE",
                    "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
                    "AWS_CONTAINER_CREDENTIALS_FULL_URI",
                    "AWS_WEB_IDENTITY_TOKEN_FILE",
                ];
                for var in checks {
                    if input.ctx.env(var).await.is_some() {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: None,
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some(var.to_string()),
                        });
                    }
                }
                let has_key = input.ctx.env("AWS_ACCESS_KEY_ID").await.is_some()
                    && input.ctx.env("AWS_SECRET_ACCESS_KEY").await.is_some();
                if has_key {
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: None,
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("AWS access keys".to_string()),
                    });
                }
                None
            }) as Pin<Box<dyn Future<Output = Option<AuthResult>> + Send>>
        }),
        login: None,
    }
}

pub fn google_vertex_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "google-vertex".to_string(),
        name: Some("Google Vertex AI".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(vertex_auth()),
            oauth: None,
        },
        models: GOOGLE_VERTEX_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(google_vertex_api()),
    })
}

fn vertex_auth() -> crate::auth::ApiKeyAuth {
    const ADC_PATH: &str = "~/.config/gcloud/application_default_credentials.json";
    crate::auth::ApiKeyAuth {
        name: "Google Cloud credentials".to_string(),
        resolve: Arc::new(|input: AuthResolveInput| {
            Box::pin(async move {
                let had_credential = input.credential.is_some();
                let key = input
                    .credential
                    .and_then(|c| c.key)
                    .or(input.ctx.env("GOOGLE_CLOUD_API_KEY").await);
                if let Some(key) = key {
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(key),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some(if had_credential {
                            "stored credential".to_string()
                        } else {
                            "GOOGLE_CLOUD_API_KEY".to_string()
                        }),
                    });
                }
                let adc = input
                    .ctx
                    .env("GOOGLE_APPLICATION_CREDENTIALS")
                    .await
                    .unwrap_or_else(|| ADC_PATH.to_string());
                let has_credentials = input.ctx.file_exists(&adc).await;
                let has_project = input.ctx.env("GOOGLE_CLOUD_PROJECT").await.is_some()
                    || input.ctx.env("GCLOUD_PROJECT").await.is_some();
                let has_location = input.ctx.env("GOOGLE_CLOUD_LOCATION").await.is_some();
                if has_credentials && has_project && has_location {
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: None,
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("gcloud application default credentials".to_string()),
                    });
                }
                None
            }) as Pin<Box<dyn Future<Output = Option<AuthResult>> + Send>>
        }),
        login: None,
    }
}

pub fn anthropic_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "anthropic".to_string(),
        name: Some("Anthropic".to_string()),
        base_url: Some("https://api.anthropic.com".to_string()),
        headers: None,
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth(
                "Anthropic API key",
                vec!["ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"],
            )),
            oauth: Some(anthropic_oauth()),
        },
        models: ANTHROPIC_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(anthropic_messages_api()),
    })
}

pub fn openai_codex_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "openai-codex".to_string(),
        name: Some("OpenAI Codex".to_string()),
        base_url: Some("https://chatgpt.com/backend-api".to_string()),
        headers: None,
        auth: ProviderAuth {
            api_key: None,
            oauth: Some(openai_codex_oauth()),
        },
        models: OPENAI_CODEX_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(openai_codex_responses_api()),
    })
}

pub fn openai_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "openai".to_string(),
        name: Some("OpenAI".to_string()),
        base_url: Some("https://api.openai.com/v1".to_string()),
        headers: None,
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth("OpenAI API key", vec!["OPENAI_API_KEY"])),
            oauth: None,
        },
        models: OPENAI_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(openai_responses_api()),
    })
}

pub fn github_copilot_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "github-copilot".to_string(),
        name: Some("GitHub Copilot".to_string()),
        base_url: Some("https://api.individual.githubcopilot.com".to_string()),
        headers: None,
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth("GitHub Copilot token", vec!["COPILOT_GITHUB_TOKEN"])),
            oauth: Some(github_copilot_oauth()),
        },
        models: GITHUB_COPILOT_MODELS.to_vec(),
        refresh_models: None,
        api: mixed_openai_apis(),
    })
}

pub fn cloudflare_ai_gateway_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "cloudflare-ai-gateway".to_string(),
        name: Some("Cloudflare AI Gateway".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(cloudflare_ai_gateway_auth()),
            oauth: None,
        },
        models: CLOUDFLARE_AI_GATEWAY_MODELS.to_vec(),
        refresh_models: None,
        api: mixed_openai_apis(),
    })
}

pub fn cloudflare_workers_ai_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "cloudflare-workers-ai".to_string(),
        name: Some("Cloudflare Workers AI".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(cloudflare_workers_ai_auth()),
            oauth: None,
        },
        models: CLOUDFLARE_WORKERS_AI_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(openai_completions_api()),
    })
}

pub fn fireworks_provider() -> Provider {
    let mut map = HashMap::new();
    map.insert("anthropic-messages".to_string(), anthropic_messages_api());
    map.insert("openai-completions".to_string(), openai_completions_api());
    create_provider(CreateProviderOptions {
        id: "fireworks".to_string(),
        name: Some("Fireworks".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth("Fireworks API key", vec!["FIREWORKS_API_KEY"])),
            oauth: None,
        },
        models: FIREWORKS_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Map(map),
    })
}

pub fn kimi_coding_provider() -> Provider {
    create_provider(CreateProviderOptions {
        id: "kimi-coding".to_string(),
        name: Some("Kimi For Coding".to_string()),
        base_url: None,
        headers: None,
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth("Moonshot API key", vec!["MOONSHOT_API_KEY"])),
            oauth: None,
        },
        models: KIMI_CODING_MODELS.to_vec(),
        refresh_models: None,
        api: ProviderApi::Single(anthropic_messages_api()),
    })
}

pub fn builtin_providers() -> Vec<Provider> {
    vec![
        amazon_bedrock_provider(),
        simple_provider!(
            "ant-ling",
            "Ant Ling",
            ANT_LING_MODELS,
            openai_completions_api,
            (vec!["ANT_LING_API_KEY"], "Ant Ling API key")
        ),
        anthropic_provider(),
        simple_provider!(
            "azure-openai-responses",
            "Azure OpenAI",
            AZURE_OPENAI_RESPONSES_MODELS,
            azure_openai_responses_api,
            (vec!["AZURE_OPENAI_API_KEY"], "Azure OpenAI API key")
        ),
        simple_provider!(
            "cerebras",
            "Cerebras",
            CEREBRAS_MODELS,
            openai_completions_api,
            (vec!["CEREBRAS_API_KEY"], "Cerebras API key")
        ),
        cloudflare_ai_gateway_provider(),
        cloudflare_workers_ai_provider(),
        simple_provider!(
            "deepseek",
            "DeepSeek",
            DEEPSEEK_MODELS,
            openai_completions_api,
            (vec!["DEEPSEEK_API_KEY"], "DeepSeek API key")
        ),
        fireworks_provider(),
        github_copilot_provider(),
        simple_provider!(
            "google",
            "Google",
            GOOGLE_MODELS,
            google_generative_ai_api,
            (vec!["GEMINI_API_KEY"], "Gemini API key")
        ),
        google_vertex_provider(),
        simple_provider!(
            "groq",
            "Groq",
            GROQ_MODELS,
            openai_completions_api,
            (vec!["GROQ_API_KEY"], "Groq API key")
        ),
        simple_provider!(
            "huggingface",
            "Hugging Face",
            HUGGINGFACE_MODELS,
            openai_completions_api,
            (vec!["HF_TOKEN"], "Hugging Face token")
        ),
        kimi_coding_provider(),
        simple_provider!(
            "minimax",
            "MiniMax",
            MINIMAX_MODELS,
            openai_completions_api,
            (vec!["MINIMAX_API_KEY"], "MiniMax API key")
        ),
        simple_provider!(
            "minimax-cn",
            "MiniMax (China)",
            MINIMAX_CN_MODELS,
            openai_completions_api,
            (vec!["MINIMAX_API_KEY"], "MiniMax API key")
        ),
        simple_provider!(
            "mistral",
            "Mistral",
            MISTRAL_MODELS,
            mistral_conversations_api,
            (vec!["MISTRAL_API_KEY"], "Mistral API key")
        ),
        simple_provider!(
            "moonshotai",
            "Moonshot AI",
            MOONSHOTAI_MODELS,
            openai_completions_api,
            (vec!["MOONSHOT_API_KEY"], "Moonshot API key")
        ),
        simple_provider!(
            "moonshotai-cn",
            "Moonshot AI (China)",
            MOONSHOTAI_CN_MODELS,
            openai_completions_api,
            (vec!["MOONSHOT_API_KEY"], "Moonshot API key")
        ),
        simple_provider!(
            "nvidia",
            "NVIDIA NIM",
            NVIDIA_MODELS,
            openai_completions_api,
            (vec!["NVIDIA_API_KEY"], "NVIDIA API key")
        ),
        openai_provider(),
        openai_codex_provider(),
        simple_provider!(
            "opencode",
            "OpenCode Zen",
            OPENCODE_MODELS,
            openai_completions_api,
            (vec!["OPENCODE_API_KEY"], "OpenCode API key")
        ),
        simple_provider!(
            "opencode-go",
            "OpenCode Go",
            OPENCODE_GO_MODELS,
            openai_completions_api,
            (vec!["OPENCODE_API_KEY"], "OpenCode API key")
        ),
        simple_provider!(
            "openrouter",
            "OpenRouter",
            OPENROUTER_MODELS,
            openai_completions_api,
            (vec!["OPENROUTER_API_KEY"], "OpenRouter API key")
        ),
        simple_provider!(
            "together",
            "Together AI",
            TOGETHER_MODELS,
            openai_completions_api,
            (vec!["TOGETHER_API_KEY"], "Together API key")
        ),
        simple_provider!(
            "vercel-ai-gateway",
            "Vercel AI Gateway",
            VERCEL_AI_GATEWAY_MODELS,
            openai_completions_api,
            (vec!["VERCEL_AI_GATEWAY_API_KEY"], "Vercel AI Gateway API key")
        ),
        simple_provider!(
            "xai",
            "xAI",
            XAI_MODELS,
            openai_completions_api,
            (vec!["XAI_API_KEY"], "xAI API key")
        ),
        simple_provider!(
            "xiaomi",
            "Xiaomi MiMo",
            XIAOMI_MODELS,
            openai_completions_api,
            (vec!["XIAOMI_API_KEY"], "Xiaomi API key")
        ),
        simple_provider!(
            "xiaomi-token-plan-ams",
            "Xiaomi Token Plan (AMS)",
            XIAOMI_TOKEN_PLAN_AMS_MODELS,
            anthropic_messages_api,
            (vec!["XIAOMI_API_KEY"], "Xiaomi API key")
        ),
        simple_provider!(
            "xiaomi-token-plan-cn",
            "Xiaomi Token Plan (CN)",
            XIAOMI_TOKEN_PLAN_CN_MODELS,
            anthropic_messages_api,
            (vec!["XIAOMI_API_KEY"], "Xiaomi API key")
        ),
        simple_provider!(
            "xiaomi-token-plan-sgp",
            "Xiaomi Token Plan (SGP)",
            XIAOMI_TOKEN_PLAN_SGP_MODELS,
            anthropic_messages_api,
            (vec!["XIAOMI_API_KEY"], "Xiaomi API key")
        ),
        simple_provider!(
            "zai",
            "ZAI Coding Plan",
            ZAI_MODELS,
            openai_completions_api,
            (vec!["ZAI_API_KEY"], "ZAI API key")
        ),
        simple_provider!(
            "zai-coding-cn",
            "ZAI Coding Plan (China)",
            ZAI_CODING_CN_MODELS,
            openai_completions_api,
            (vec!["ZAI_API_KEY"], "ZAI API key")
        ),
    ]
}

pub fn builtin_models(options: Option<CreateModelsOptions>) -> MutableModels {
    let mut models = create_models(options);
    for provider in builtin_providers() {
        models.set_provider(provider);
    }
    models
}

pub use crate::models::catalog::{get_builtin_model, get_builtin_models, get_builtin_providers};
