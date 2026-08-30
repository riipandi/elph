//! Shared toggle context for clickable process-phase headers.

use iocraft::prelude::*;
use std::sync::{Arc, RwLock};

use super::super::types::TranscriptMessage;

/// Live message list + revision counter so header clicks can expand/collapse and invalidate layout.
#[derive(Clone, Copy)]
pub struct CollapsibleToggleCtx {
    pub messages: State<Arc<RwLock<Vec<TranscriptMessage>>>>,
    pub messages_revision: State<u64>,
}
