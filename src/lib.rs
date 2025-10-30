// ABOUTME: Public library API for Muesli transcript sync
// ABOUTME: Re-exports core modules for external use

pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::{read_frontmatter, write_atomic, Paths};
