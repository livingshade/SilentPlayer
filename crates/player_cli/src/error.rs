use std::error::Error;
use std::fmt;

pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 2,
        }
    }

    pub fn operation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::operation(error.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::operation(error.to_string())
    }
}

impl From<player_ffi::SilentAppClientError> for CliError {
    fn from(error: player_ffi::SilentAppClientError) -> Self {
        Self::operation(error.to_string())
    }
}

impl From<player_error::PlayerError> for CliError {
    fn from(error: player_error::PlayerError) -> Self {
        Self::operation(error.to_string())
    }
}
