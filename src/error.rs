use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdaterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("HTTP request failed: {0}")]
    Http(#[from] ureq::Error),
    
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Version comparison failed: {0}")]
    VersionCompare(String),
}

pub type Result<T> = std::result::Result<T, UpdaterError>;