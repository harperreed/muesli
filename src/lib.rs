// ABOUTME: Public library API for Muesli transcript sync
// ABOUTME: Re-exports core modules for external use

pub mod api;
pub mod auth;
pub mod cli;
pub mod convert;
pub mod error;
pub mod model;
pub mod refresh;
pub mod session_decrypt;
pub mod storage;
pub mod sync;
pub mod util;

#[cfg(feature = "index")]
pub mod index;

#[cfg(feature = "embeddings")]
pub mod embeddings;

#[cfg(feature = "summaries")]
pub mod summary;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "storage")]
pub mod db;

#[cfg(feature = "tui")]
pub mod tui;

pub use api::{ApiClient, ApiResponse};
pub use auth::resolve_token;
pub use convert::{prosemirror_to_markdown, to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{
    Attendee, CompanyInfo, DocumentMetadata, DocumentSummary, Employment, Frontmatter, LinkedIn,
    PersonDetails, PersonInfo, PersonName, ProseMirrorDoc, ProseMirrorMark, ProseMirrorNode,
    PublicNote, RawTranscript,
};
pub use storage::{read_frontmatter, write_atomic, Paths};
pub use sync::sync_all;
