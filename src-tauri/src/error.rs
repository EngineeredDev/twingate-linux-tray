use thiserror::Error;

#[derive(Error, Debug)]
pub enum TwingateError {
    #[error("Command execution failed: {0}")]
    CommandError(#[from] tauri_plugin_shell::Error),

    #[allow(dead_code)]
    #[error("Authentication required for resource: {0}")]
    AuthRequired(String),

    #[error("Service not running")]
    ServiceNotRunning,

    #[error("Network data parsing failed: {0}")]
    NetworkDataParseError(String),

    #[error("JSON parsing failed: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Clipboard operation failed: {0}")]
    ClipboardError(String),

    #[error("Tray menu operation failed: {0}")]
    TrayMenuError(#[from] tauri::Error),

    #[error("Shell command failed with exit code {code}: {message}")]
    ShellCommandFailed { code: i32, message: String },

    #[allow(dead_code)]
    #[error("Authentication flow timeout")]
    AuthFlowTimeout,

    #[error("Invalid resource state: {0}")]
    InvalidResourceState(String),

    #[allow(dead_code)]
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[allow(dead_code)]
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Service connecting - authentication in progress")]
    ServiceConnecting,

    #[error("Authentication state transition in progress")]
    AuthStateTransition,

    #[error("Retry limit exceeded: {0}")]
    RetryLimitExceeded(String),

    #[error("Service initialization in progress - data not yet available")]
    ServiceInitializing,
}

pub type Result<T> = std::result::Result<T, TwingateError>;

impl From<arboard::Error> for TwingateError {
    fn from(err: arboard::Error) -> Self {
        TwingateError::ClipboardError(err.to_string())
    }
}
