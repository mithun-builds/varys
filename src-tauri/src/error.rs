use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("settings: {0}")]
    Settings(#[from] rusqlite::Error),

    #[error("audio: {0}")]
    Audio(String),

    #[error("sidecar: {0}")]
    Sidecar(String),

    #[error("detection: {0}")]
    Detection(String),

    #[error("recording is already in progress")]
    AlreadyRecording,

    #[error("recording is not in progress")]
    NotRecording,

    #[error("permission required: {0}")]
    PermissionRequired(&'static str),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
