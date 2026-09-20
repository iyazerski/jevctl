use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("cannot read {path}: {source}")]
    InputIo {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TYPESAFE_API_KEY is not set")]
    MissingApiKey,
    #[error("authentication rejected")]
    Authentication,
    #[error("evaluation service unavailable after {attempts} attempts")]
    RetriesExhausted { attempts: usize },
    #[error("evaluation service unavailable")]
    ServiceUnavailable,
    #[error("evaluation request was rejected: {0}")]
    RequestRejected(String),
    #[error("invalid evaluation response: {0}")]
    InvalidResponse(String),
}

impl AppError {
    /// Return the stable process exit code for this error category.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) | Self::InputIo { .. } | Self::Json(_) => 2,
            Self::MissingApiKey | Self::Authentication => 3,
            Self::ServiceUnavailable | Self::RequestRejected(_) => 4,
            Self::InvalidResponse(_) => 5,
            Self::RetriesExhausted { .. } => 6,
        }
    }
}
