use std::fmt;

pub type PlaybackResult<T> = Result<T, PlaybackError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackError {
    InvalidQueueIndex { index: usize, len: usize },
    EmptyQueue,
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQueueIndex { index, len } => {
                write!(f, "invalid queue index {index} for queue length {len}")
            }
            Self::EmptyQueue => write!(f, "queue is empty"),
        }
    }
}

impl std::error::Error for PlaybackError {}
