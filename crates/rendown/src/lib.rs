//! CommonMark/markdown renderer for terminals (ANSI).
//!
//! Parse once into a cacheable [`MarkdownDocument`], then write styled ANSI. Streaming
//! token deltas and mermaid diagrams are optional crate features.
//!
//! ```
//! use rendown::{ColorLevel, MarkdownTheme, Rendown};
//!
//! let md = Rendown::new()
//!     .width(80)
//!     .theme(MarkdownTheme::dark())
//!     .color_level(ColorLevel::TrueColor);
//! let ansi = md.render_string("# Hello\n\n**world**").unwrap();
//! assert!(ansi.contains("Hello"));
//! ```

mod ansi;
mod blocks;
mod builder;
mod colors;
mod highlight;
mod layout;
mod linkify;
mod mermaid;
mod model;
mod parse;
mod parser_config;
mod syntax;
mod table;
mod theme;
mod wrap;

#[cfg(feature = "stream")]
mod stream;

pub use blocks::{BLOCK_GAP_ROWS, CODE_BLOCK_INSET_H, CODE_BLOCK_INSET_V, CODE_VERTICAL_PADDING};
pub use blocks::{block_gap_after, code_content_width, segment_end, segment_gap_after};
pub use builder::Rendown;
pub use colors::{ColorLevel, detect_color_level, syntect_to_styled_span};
pub use layout::markdown_document_row_count;
pub use linkify::{path_to_file_url, spans_with_links};
pub use mermaid::mermaid_display_text;
pub use model::{FontWeight, MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, RgbColor, StyledSpan};
pub use parser_config::has_open_container_at;
pub use syntax::syntax_highlight_raw;
pub use theme::{MarkdownTheme, MarkdownThemeBuilder};
pub use wrap::wrap_with_hanging_ranges;

#[cfg(feature = "mermaid")]
pub use mermaid::render_mermaid_at_width;

#[cfg(feature = "stream")]
pub use stream::{StreamRenderer, terminal_width};
