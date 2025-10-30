// ABOUTME: Error types with structured exit codes for CLI
// ABOUTME: Maps domain errors to specific exit codes for shell scripting

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("API error {status} on {endpoint}: {message}")]
    Api {
        endpoint: String,
        status: u16,
        message: String,
    },

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("Summarization error: {0}")]
    Summarization(String),

    #[error("Indexing error: {0}")]
    Indexing(String),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Auth(_) => 2,
            Error::Network(_) => 3,
            Error::Api { .. } => 4,
            Error::Parse(_) => 5,
            Error::Filesystem(_) => 6,
            Error::Summarization(_) => 7,
            Error::Indexing(_) => 8,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_exit_codes() {
        assert_eq!(Error::Auth("test".into()).exit_code(), 2);
        assert_eq!(
            Error::Api {
                endpoint: "test".into(),
                status: 404,
                message: "not found".into()
            }
            .exit_code(),
            4
        );
        assert_eq!(Error::Summarization("test".into()).exit_code(), 7);
    }
}
