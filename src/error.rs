use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParaksesError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid HFS+ volume: {0}")]
    InvalidVolume(String),

    #[error("Not an HFS+ volume (signature: {0:#x})")]
    BadSignature(u16),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("{0}")]
    Other(String),
}
