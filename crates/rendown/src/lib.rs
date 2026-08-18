//! CommonMark/markdown renderer for terminals (ANSI).
//!
//! Parse once into a cacheable [`MarkdownDocument`], then write styled ANSI. Streaming
//! token deltas, mermaid diagrams, and syntect highlighting are optional crate features.
//!
//! Headless ANSI wrap and TUI (iocraft) wrap are **not** guaranteed to match row-for-row.
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
mod linkify;
mod mermaid;
mod model;
mod parse;
mod parser_config;
mod table;
mod theme;
mod wrap;

pub mod layout;
pub mod link;

#[cfg(feature = "highlight")]
pub mod syntax;

#[cfg(feature = "stream")]
mod stream;

pub use builder::Rendown;
pub use colors::{ColorLevel, detect_color_level};
pub use mermaid::{mermaid_display_shared, mermaid_display_text};
pub use model::{FontWeight, MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, RgbColor, StyledSpan};
pub use parser_config::has_open_container_at;
pub use theme::{MarkdownTheme, MarkdownThemeBuilder};

#[cfg(feature = "mermaid")]
pub use mermaid::render_mermaid_at_width;

#[cfg(feature = "stream")]
pub use stream::{StreamRenderer, terminal_width};
