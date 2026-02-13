use std::io;

use thiserror::Error;

pub type Result<T, E = PulseError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum PulseError {
    #[error("home directory not found")]
    HomeDirNotFound,
    #[error("Pulse is not initialized. Run `pulse init` first.")]
    ConfigMissing,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

impl PulseError {
    pub fn message<T: Into<String>>(msg: T) -> Self {
        Self::Message(msg.into())
    }
}
