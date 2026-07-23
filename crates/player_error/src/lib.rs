use std::fmt;
use std::io;
use std::path::PathBuf;

pub type PlayerResult<T> = Result<T, PlayerError>;

#[derive(Debug)]
pub enum PlayerError {
    Io { path: PathBuf, source: io::Error },
    Audio(String),
    Metadata(String),
    Store(String),
    Engine(String),
    InvalidInput(String),
}

impl PlayerError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn audio(message: impl Into<String>) -> Self {
        Self::Audio(message.into())
    }

    pub fn metadata(message: impl Into<String>) -> Self {
        Self::Metadata(message.into())
    }

    pub fn store(message: impl Into<String>) -> Self {
        Self::Store(message.into())
    }

    pub fn engine(message: impl Into<String>) -> Self {
        Self::Engine(message.into())
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "I/O error at {}: {source}", path.display())
            }
            Self::Audio(message) => write!(f, "audio error: {message}"),
            Self::Metadata(message) => write!(f, "metadata error: {message}"),
            Self::Store(message) => write!(f, "store error: {message}"),
            Self::Engine(message) => write!(f, "engine error: {message}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
        }
    }
}

impl std::error::Error for PlayerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Audio(_)
            | Self::Metadata(_)
            | Self::Store(_)
            | Self::Engine(_)
            | Self::InvalidInput(_) => None,
        }
    }
}
