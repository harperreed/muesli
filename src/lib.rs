// ABOUTME: Public library API for Muesli transcript sync
// ABOUTME: Re-exports core modules for external use

pub mod auth;
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::{read_frontmatter, write_atomic, Paths};
