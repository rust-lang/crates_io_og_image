//! Error types for the crates_io_og_image crate.

use thiserror::Error;

/// Errors that can occur when generating OpenGraph images.
#[derive(Debug, Error)]
pub enum OgImageError {
    /// Environment variable error.
    #[error("Environment variable error: {0}")]
    EnvVarError(std::env::VarError),

    /// Failed to download avatar from URL.
    #[error("Failed to download avatar from URL '{url}': {source}")]
    AvatarDownloadError {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// JSON serialization error.
    #[error("JSON serialization error: {0}")]
    JsonSerializationError(#[source] serde_json::Error),

    /// Typst compilation failed.
    #[error("Typst compilation failed:\n{0}")]
    TypstCompilation(String),

    /// The Typst compilation task panicked.
    #[error("Typst compilation panicked: {0}")]
    TypstTaskPanic(String),
}
