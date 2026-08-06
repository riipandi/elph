//! Buffered, part-based markdown rendering for assistant stream responses.

mod buffer;
mod layout;
mod partition;
pub(crate) mod render;
mod worker;

pub use buffer::AssistantMarkdownBuffer;
pub use layout::assistant_row_count;
pub use worker::{
    apply_markdown_parse_result, collect_markdown_parse_jobs, parse_markdown_on_worker, partition_assistant_markdown,
};
