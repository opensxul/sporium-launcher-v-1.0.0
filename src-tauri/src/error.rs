use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("provider rate limit reached")]
    RateLimited,
    #[error("content is incompatible with this instance")]
    ContentIncompatible,
    #[error("content destination is not supported")]
    ContentUnsupported,
    #[error("existing content must be preserved")]
    ContentConflict,
    #[error("required dependencies cannot be resolved safely")]
    DependencyConflict,
    #[error("storage unavailable")]
    Io(#[from] std::io::Error),
    #[error("database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("stored data cannot be read")]
    CorruptData(#[from] serde_json::Error),
    #[error("schema is newer than this application")]
    SchemaTooNew,
    #[error("settings are invalid")]
    InvalidSettings,
    #[error("settings were changed by another operation")]
    SettingsConflict,
    #[error("background worker unavailable")]
    Worker,
    #[error("invalid instance or collection metadata")]
    InvalidInput,
    #[error("record no longer exists")]
    NotFound,
    #[error("record changed in another window")]
    RecordConflict,
    #[error("filesystem path contains a link or does not belong to this library")]
    UnsafePath,
    #[error("another library operation is in progress")]
    LibraryBusy,
    #[error("source files changed during copying")]
    SourceChanged,
    #[error("system file manager could not be opened")]
    OpenFolderFailed,
    #[error("network request failed")]
    Network,
    #[error("download integrity check failed")]
    Integrity,
    #[error("archive contains unsafe entries")]
    UnsafeArchive,
    #[error("operation was cancelled")]
    Cancelled,
    #[error("version metadata is not supported")]
    UnsupportedVersion,
    #[error("a compatible Java runtime could not be found")]
    JavaUnavailable,
    #[error("instance is installing or running")]
    InstanceBusy,
    #[error("game process could not be started")]
    LaunchFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    RateLimited,
    ContentIncompatible,
    ContentUnsupported,
    ContentConflict,
    DependencyConflict,
    StorageUnavailable,
    DataCorrupt,
    SchemaTooNew,
    InvalidSettings,
    SettingsConflict,
    WorkerUnavailable,
    InvalidInput,
    NotFound,
    RecordConflict,
    UnsafePath,
    LibraryBusy,
    SourceChanged,
    OpenFolderFailed,
    Network,
    Integrity,
    UnsafeArchive,
    Cancelled,
    UnsupportedVersion,
    JavaUnavailable,
    InstanceBusy,
    LaunchFailed,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: ErrorCode,
    pub retryable: bool,
}

impl From<CoreError> for CommandError {
    fn from(error: CoreError) -> Self {
        if let CoreError::Io(cause) = &error {
            tracing::warn!(kind = ?cause.kind(), os_code = cause.raw_os_error(), "filesystem_operation_failed");
        }
        let code = match error {
            CoreError::RateLimited => ErrorCode::RateLimited,
            CoreError::ContentIncompatible => ErrorCode::ContentIncompatible,
            CoreError::ContentUnsupported => ErrorCode::ContentUnsupported,
            CoreError::ContentConflict => ErrorCode::ContentConflict,
            CoreError::DependencyConflict => ErrorCode::DependencyConflict,
            CoreError::Io(_) => ErrorCode::StorageUnavailable,
            CoreError::Database(rusqlite::Error::SqliteFailure(ref cause, _))
                if matches!(
                    cause.code,
                    rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                ) =>
            {
                ErrorCode::DataCorrupt
            }
            CoreError::Database(_) => ErrorCode::StorageUnavailable,
            CoreError::CorruptData(_) => ErrorCode::DataCorrupt,
            CoreError::SchemaTooNew => ErrorCode::SchemaTooNew,
            CoreError::InvalidSettings => ErrorCode::InvalidSettings,
            CoreError::SettingsConflict => ErrorCode::SettingsConflict,
            CoreError::Worker => ErrorCode::WorkerUnavailable,
            CoreError::InvalidInput => ErrorCode::InvalidInput,
            CoreError::NotFound => ErrorCode::NotFound,
            CoreError::RecordConflict => ErrorCode::RecordConflict,
            CoreError::UnsafePath => ErrorCode::UnsafePath,
            CoreError::LibraryBusy => ErrorCode::LibraryBusy,
            CoreError::SourceChanged => ErrorCode::SourceChanged,
            CoreError::OpenFolderFailed => ErrorCode::OpenFolderFailed,
            CoreError::Network => ErrorCode::Network,
            CoreError::Integrity => ErrorCode::Integrity,
            CoreError::UnsafeArchive => ErrorCode::UnsafeArchive,
            CoreError::Cancelled => ErrorCode::Cancelled,
            CoreError::UnsupportedVersion => ErrorCode::UnsupportedVersion,
            CoreError::JavaUnavailable => ErrorCode::JavaUnavailable,
            CoreError::InstanceBusy => ErrorCode::InstanceBusy,
            CoreError::LaunchFailed => ErrorCode::LaunchFailed,
        };
        // Do not log Display/Debug of foreign errors: they can contain private paths or data.
        tracing::warn!(code = ?code, "command_failed");
        Self {
            code,
            retryable: matches!(
                code,
                ErrorCode::StorageUnavailable
                    | ErrorCode::RateLimited
                    | ErrorCode::SettingsConflict
                    | ErrorCode::WorkerUnavailable
                    | ErrorCode::RecordConflict
                    | ErrorCode::LibraryBusy
                    | ErrorCode::SourceChanged
                    | ErrorCode::Network
                    | ErrorCode::Integrity
                    | ErrorCode::Cancelled
                    | ErrorCode::InstanceBusy
                    | ErrorCode::JavaUnavailable
                    | ErrorCode::LaunchFailed
            ),
        }
    }
}
