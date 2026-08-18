//! Provider API implementations. Stable names are re-exported below; other
//! submodule paths are for adapters and in-tree tests.

pub mod anthropic_messages;
pub mod azure_base_url;
pub mod azure_openai_responses;
#[cfg(feature = "bedrock")]
pub mod bedrock_converse_stream;
#[cfg(feature = "bedrock")]
pub mod bedrock_shared;
pub mod cloudflare;
pub mod codex_transport;
pub mod common;
pub mod faux;
pub mod github_copilot_headers;
pub mod google_generative_ai;
pub mod google_shared;
pub mod google_vertex;
pub mod http_proxy;
pub mod mistral_conversations;
pub mod openai_codex_responses;
pub mod openai_compat;
pub mod openai_completions;
pub mod openai_prompt_cache;
pub mod openai_responses;
pub mod openai_responses_shared;
pub mod openrouter_images;
pub mod pi_messages;
pub mod simple_options;
pub mod sse;
pub mod transform_messages;
pub mod websocket_connect;

pub use anthropic_messages::{AnthropicMessagesApi, AnthropicOptions};
pub use azure_openai_responses::{AzureOpenAIResponsesApi, AzureOpenAIResponsesOptions};
#[cfg(feature = "bedrock")]
pub use bedrock_converse_stream::{BedrockConverseStreamApi, BedrockOptions};
pub use common::{wrap_on_payload, wrap_on_response};
pub use faux::FauxApi;
pub use google_generative_ai::{GoogleGenerativeAIApi, GoogleOptions};
pub use google_vertex::GoogleVertexApi;
pub use mistral_conversations::{MistralConversationsApi, MistralOptions};
pub use openai_codex_responses::{OpenAICodexResponsesApi, OpenAICodexResponsesOptions};
pub use openai_completions::{OpenAICompletionsApi, OpenAICompletionsOptions};
pub use openai_responses::{OpenAIResponsesApi, OpenAIResponsesOptions};
pub use openrouter_images::OpenRouterImagesApi;
pub use pi_messages::PiMessagesApi;

use crate::types::ProviderStreams;

/// Registry of built-in API implementations matching [`ProviderStreams`].
pub fn builtin_apis() -> Vec<(&'static str, Box<dyn ProviderStreams>)> {
    #[allow(unused_mut)]
    let mut apis: Vec<(&'static str, Box<dyn ProviderStreams>)> = vec![
        ("anthropic-messages", Box::new(AnthropicMessagesApi)),
        ("openai-completions", Box::new(OpenAICompletionsApi)),
        ("openai-responses", Box::new(OpenAIResponsesApi)),
        ("openai-codex-responses", Box::new(OpenAICodexResponsesApi)),
        ("azure-openai-responses", Box::new(AzureOpenAIResponsesApi)),
        ("google-generative-ai", Box::new(GoogleGenerativeAIApi)),
        ("google-vertex", Box::new(GoogleVertexApi)),
        ("mistral-conversations", Box::new(MistralConversationsApi)),
        ("pi-messages", Box::new(PiMessagesApi)),
    ];
    #[cfg(feature = "bedrock")]
    apis.push(("bedrock-converse-stream", Box::new(BedrockConverseStreamApi)));
    apis
}

pub fn api_for(name: &str) -> Option<Box<dyn ProviderStreams>> {
    builtin_apis().into_iter().find(|(n, _)| *n == name).map(|(_, api)| api)
}
