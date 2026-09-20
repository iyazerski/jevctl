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
}

impl AppError {
    /// Return the stable process exit code for this error category.
    pub fn exit_code(&self) -> i32 {
        2
    }
}
