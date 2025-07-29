use thiserror::Error;

/// Comprehensive error types for Twingate operations
#[derive(Error, Debug)]
pub enum TwingateError {
    // Service-related errors
    #[error("Twingate service is not running")]
    ServiceNotRunning,
    
    
    #[error("Service is connecting to Twingate network")]
    ServiceConnecting,
    
    #[error("Service requires authentication")]
    AuthenticationRequired,
    
    #[error("Authentication flow timed out after {seconds} seconds")]
    AuthenticationTimeout { seconds: u64 },
    
    // Command execution errors
    #[error("Shell command '{command}' failed with exit code {code}: {stderr}")]
    CommandFailed {
        command: String,
        code: i32,
        stderr: String,
    },
    
    #[error("Command execution error: {source}")]
    CommandExecutionError {
        #[from]
        source: tauri_plugin_shell::Error,
    },
    
    // Data processing errors
    
    #[error("JSON deserialization failed: {source}")]
    JsonError {
        #[from]
        source: serde_json::Error,
    },
    
    #[error("Invalid UTF-8 in command output")]
    InvalidUtf8,
    
    // Resource-related errors
    #[error("Resource '{id}' not found")]
    ResourceNotFound { id: String },
    
    #[error("Invalid resource ID format: {id}")]
    InvalidResourceId { id: String },
    
    // System integration errors
    #[error("Clipboard operation failed: {details}")]
    ClipboardError { details: String },
    
    #[error("System tray operation failed: {source}")]
    TrayError {
        #[from]
        source: tauri::Error,
    },
    
    
    // Retry and timeout errors
    #[error("Operation timed out after {attempts} attempts")]
    RetryLimitExceeded { attempts: u32 },
    
}

pub type Result<T> = std::result::Result<T, TwingateError>;

// Error conversions
impl From<arboard::Error> for TwingateError {
    fn from(err: arboard::Error) -> Self {
        Self::ClipboardError {
            details: err.to_string(),
        }
    }
}

impl From<std::str::Utf8Error> for TwingateError {
    fn from(_: std::str::Utf8Error) -> Self {
        Self::InvalidUtf8
    }
}

impl From<tauri_plugin_opener::Error> for TwingateError {
    fn from(err: tauri_plugin_opener::Error) -> Self {
        Self::TrayError {
            source: tauri::Error::InvalidIcon(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open URL: {}", err),
            )),
        }
    }
}

// Helper methods for common error scenarios
impl TwingateError {
    pub fn command_failed(command: impl Into<String>, code: i32, stderr: impl Into<String>) -> Self {
        Self::CommandFailed {
            command: command.into(),
            code,
            stderr: stderr.into(),
        }
    }
    
    pub fn resource_not_found(id: impl Into<String>) -> Self {
        Self::ResourceNotFound { id: id.into() }
    }
    
    pub fn invalid_resource_id(id: impl Into<String>) -> Self {
        Self::InvalidResourceId { id: id.into() }
    }
    
    
}
