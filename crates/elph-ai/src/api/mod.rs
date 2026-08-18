//! Provider API implementations. Stable names are re-exported below.

#[doc(hidden)]
pub mod anthropic_messages;
#[doc(hidden)]
pub mod azure_base_url;
#[doc(hidden)]
pub mod azure_openai_responses;
#[cfg(feature = "bedrock")]
#[doc(hidden)]
pub mod bedrock_converse_stream;
#[cfg(feature = "bedrock")]
#[doc(hidden)]
pub mod bedrock_shared;
#[doc(hidden)]
pub mod cloudflare;
#[doc(hidden)]
pub mod codex_transport;
#[doc(hidden)]
pub mod common;
#[doc(hidden)]
pub mod faux;
#[doc(hidden)]
pub mod github_copilot_headers;
#[doc(hidden)]
pub mod google_generative_ai;
#[doc(hidden)]
pub mod google_shared;
#[doc(hidden)]
pub mod google_vertex;
#[doc(hidden)]
pub mod http_proxy;
#[doc(hidden)]
pub mod mistral_conversations;
#[doc(hidden)]
pub mod openai_codex_responses;
#[doc(hidden)]
pub mod openai_compat;
#[doc(hidden)]
pub mod openai_completions;
#[doc(hidden)]
pub mod openai_prompt_cache;
#[doc(hidden)]
pub mod openai_responses;
#[doc(hidden)]
pub mod openai_responses_shared;
#[doc(hidden)]
pub mod openrouter_images;
#[doc(hidden)]
pub mod pi_messages;
#[doc(hidden)]
pub mod simple_options;
#[doc(hidden)]
pub mod sse;
#[doc(hidden)]
pub mod transform_messages;
#[doc(hidden)]
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
