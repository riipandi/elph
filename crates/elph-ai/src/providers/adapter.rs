use std::sync::Arc;

use crate::api::{AnthropicMessagesApi, AzureOpenAIResponsesApi, BedrockConverseStreamApi, FauxApi};
use crate::api::{GoogleGenerativeAIApi, GoogleVertexApi, MistralConversationsApi, OpenAICodexResponsesApi};
use crate::api::{OpenAICompletionsApi, OpenAIResponsesApi};
use crate::models::{ProviderApi, ProviderStreamsDyn};
use crate::types::ProviderStreams;

struct StreamsAdapter<T: ProviderStreams + Send + Sync + 'static>(T);

impl<T: ProviderStreams + Send + Sync + 'static> ProviderStreamsDyn for StreamsAdapter<T> {
    fn stream(
        &self,
        model: &crate::types::Model,
        context: &crate::types::Context,
        options: Option<crate::types::StreamOptions>,
    ) -> crate::utils::event_stream::AssistantMessageEventStream {
        self.0.stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: &crate::types::Model,
        context: &crate::types::Context,
        options: Option<crate::types::SimpleStreamOptions>,
    ) -> crate::utils::event_stream::AssistantMessageEventStream {
        self.0.stream_simple(model, context, options)
    }
}

fn arc_api<T: ProviderStreams + Send + Sync + 'static>(api: T) -> Arc<dyn ProviderStreamsDyn> {
    Arc::new(StreamsAdapter(api))
}

pub fn anthropic_messages_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(AnthropicMessagesApi)
}

pub fn openai_completions_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(OpenAICompletionsApi)
}

pub fn openai_responses_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(OpenAIResponsesApi)
}

pub fn openai_codex_responses_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(OpenAICodexResponsesApi)
}

pub fn azure_openai_responses_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(AzureOpenAIResponsesApi)
}

pub fn google_generative_ai_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(GoogleGenerativeAIApi)
}

pub fn google_vertex_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(GoogleVertexApi)
}

pub fn mistral_conversations_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(MistralConversationsApi)
}

pub fn bedrock_converse_stream_api() -> Arc<dyn ProviderStreamsDyn> {
    arc_api(BedrockConverseStreamApi)
}

pub fn faux_api(api: FauxApi) -> Arc<dyn ProviderStreamsDyn> {
    arc_api(api)
}

pub fn mixed_openai_apis() -> ProviderApi {
    let mut map = std::collections::HashMap::new();
    map.insert("anthropic-messages".to_string(), anthropic_messages_api());
    map.insert("openai-completions".to_string(), openai_completions_api());
    map.insert("openai-responses".to_string(), openai_responses_api());
    ProviderApi::Map(map)
}

/// Mixed API map for gateway providers with Google AI support.
pub fn mixed_gateway_apis() -> ProviderApi {
    let mut map = std::collections::HashMap::new();
    map.insert("anthropic-messages".to_string(), anthropic_messages_api());
    map.insert("google-generative-ai".to_string(), google_generative_ai_api());
    map.insert("openai-completions".to_string(), openai_completions_api());
    map.insert("openai-responses".to_string(), openai_responses_api());
    ProviderApi::Map(map)
}
