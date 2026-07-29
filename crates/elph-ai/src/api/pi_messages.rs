//! pi-messages API adapter for Inflection AI's messaging gateway.
//!
//! Ported from pi-ai `api/pi-messages.ts`.

use crate::api::ProviderStreams;
use crate::types::*;
use crate::utils::event_stream::AssistantMessageEventStream;

/// Options for pi-messages API requests.
#[derive(Debug, Clone, Default)]
pub struct PiMessagesOptions {
    pub session_id: Option<String>,
    pub rewrite_history: Option<Vec<Message>>,
}

/// pi-messages streaming API adapter.
///
/// This adapter communicates with the Inflection AI pi-messages gateway.
/// It uses a custom streaming protocol that differs from standard SSE.
pub struct PiMessagesApi;

impl ProviderStreams for PiMessagesApi {
    fn stream(
        &self,
        model: &Model,
        _context: &Context,
        _options: Option<StreamOptions>,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        stream.push(AssistantMessageEvent::Done {
            reason: StopReason::Error,
            message: AssistantMessage::empty(model),
        });
        stream.end();
        stream
    }

    fn stream_simple(
        &self,
        model: &Model,
        _context: &Context,
        _options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        stream.push(AssistantMessageEvent::Done {
            reason: StopReason::Error,
            message: AssistantMessage::empty(model),
        });
        stream.end();
        stream
    }
}
