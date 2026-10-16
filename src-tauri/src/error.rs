//! Shared error type for the Rust core and its IPC representation.

use std::path::PathBuf;

use serde::Serialize;
use specta::Type;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("archive error: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("not a folder or ZIP file: {}", .0.display())]
    UnsupportedSource(PathBuf),
    #[error("no Commander Keen games found")]
    NoGamesFound,
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("download failed: HTTP {0}")]
    HttpStatus(u16),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("could not extract archive: {0}")]
    Extract(String),
    #[error("no DOSBox Staging build available for this platform")]
    UnsupportedPlatform,
    #[error("DOSBox is not installed")]
    RuntimeMissing,
    #[error("not a usable DOSBox Staging executable: {0}")]
    InvalidRuntime(String),
    #[error("already in progress")]
    Busy,
    #[error("this game is already running")]
    AlreadyRunning,
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Database(_) | Error::Migration(_) => ErrorKind::Database,
            Error::Io(_) => ErrorKind::Io,
            Error::Tauri(_) => ErrorKind::Internal,
            Error::Archive(_) => ErrorKind::Archive,
            Error::UnsupportedSource(_) => ErrorKind::UnsupportedSource,
            Error::NoGamesFound => ErrorKind::NoGamesFound,
            Error::Network(_) | Error::HttpStatus(_) => ErrorKind::Network,
            Error::ChecksumMismatch { .. } => ErrorKind::ChecksumMismatch,
            Error::Extract(_) => ErrorKind::Archive,
            Error::UnsupportedPlatform => ErrorKind::UnsupportedPlatform,
            Error::RuntimeMissing => ErrorKind::RuntimeMissing,
            Error::InvalidRuntime(_) => ErrorKind::InvalidRuntime,
            Error::Busy => ErrorKind::Busy,
            Error::AlreadyRunning => ErrorKind::AlreadyRunning,
            Error::NotFound(_) => ErrorKind::NotFound,
        }
    }
}

/// Error category the frontend can branch on (e.g. to pick a translated message).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Database,
    Io,
    Internal,
    Archive,
    UnsupportedSource,
    NoGamesFound,
    Network,
    ChecksumMismatch,
    UnsupportedPlatform,
    RuntimeMissing,
    InvalidRuntime,
    Busy,
    AlreadyRunning,
    NotFound,
}

/// The error shape every command returns to the frontend.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: ErrorKind,
    /// Technical message in English, for logs and "details" views.
    pub message: String,
}

impl From<Error> for CommandError {
    fn from(err: Error) -> Self {
        tracing::error!("command failed: {err}");
        CommandError {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

pub type CommandResult<T> = std::result::Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_error_keeps_kind_and_message() {
        let err = Error::Io(std::io::Error::other("disk on fire"));
        let cmd: CommandError = err.into();
        assert_eq!(cmd.kind, ErrorKind::Io);
        assert_eq!(cmd.message, "I/O error: disk on fire");
    }

    #[test]
    fn command_error_serializes_camel_case() {
        let cmd = CommandError {
            kind: ErrorKind::Database,
            message: "x".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"kind":"database","message":"x"}"#);
    }
}
