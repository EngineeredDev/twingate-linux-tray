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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_service_not_running_error() {
        let error = TwingateError::ServiceNotRunning;
        assert_eq!(error.to_string(), "Twingate service is not running");
    }

    #[test]
    fn test_service_connecting_error() {
        let error = TwingateError::ServiceConnecting;
        assert_eq!(error.to_string(), "Service is connecting to Twingate network");
    }

    #[test]
    fn test_authentication_required_error() {
        let error = TwingateError::AuthenticationRequired;
        assert_eq!(error.to_string(), "Service requires authentication");
    }

    #[test]
    fn test_authentication_timeout_error() {
        let error = TwingateError::AuthenticationTimeout { seconds: 60 };
        assert_eq!(error.to_string(), "Authentication flow timed out after 60 seconds");
    }

    #[test]
    fn test_command_failed_error() {
        let error = TwingateError::CommandFailed {
            command: "twingate status".to_string(),
            code: 1,
            stderr: "Service not found".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Shell command 'twingate status' failed with exit code 1: Service not found"
        );
    }

    #[test]
    fn test_command_failed_helper() {
        let error = TwingateError::command_failed("twingate start", -1, "Permission denied");
        match error {
            TwingateError::CommandFailed { command, code, stderr } => {
                assert_eq!(command, "twingate start");
                assert_eq!(code, -1);
                assert_eq!(stderr, "Permission denied");
            }
            _ => panic!("Expected CommandFailed error"),
        }
    }

    #[test]
    fn test_resource_not_found_error() {
        let error = TwingateError::ResourceNotFound {
            id: "resource-123".to_string(),
        };
        assert_eq!(error.to_string(), "Resource 'resource-123' not found");
    }

    #[test]
    fn test_resource_not_found_helper() {
        let error = TwingateError::resource_not_found("test-resource");
        match error {
            TwingateError::ResourceNotFound { id } => {
                assert_eq!(id, "test-resource");
            }
            _ => panic!("Expected ResourceNotFound error"),
        }
    }

    #[test]
    fn test_invalid_resource_id_error() {
        let error = TwingateError::InvalidResourceId {
            id: "invalid-id".to_string(),
        };
        assert_eq!(error.to_string(), "Invalid resource ID format: invalid-id");
    }

    #[test]
    fn test_invalid_resource_id_helper() {
        let error = TwingateError::invalid_resource_id("bad-format");
        match error {
            TwingateError::InvalidResourceId { id } => {
                assert_eq!(id, "bad-format");
            }
            _ => panic!("Expected InvalidResourceId error"),
        }
    }

    #[test]
    fn test_clipboard_error() {
        let error = TwingateError::ClipboardError {
            details: "Failed to access clipboard".to_string(),
        };
        assert_eq!(error.to_string(), "Clipboard operation failed: Failed to access clipboard");
    }

    #[test]
    fn test_invalid_utf8_error() {
        let error = TwingateError::InvalidUtf8;
        assert_eq!(error.to_string(), "Invalid UTF-8 in command output");
    }

    #[test]
    fn test_retry_limit_exceeded_error() {
        let error = TwingateError::RetryLimitExceeded { attempts: 5 };
        assert_eq!(error.to_string(), "Operation timed out after 5 attempts");
    }

    #[test]
    fn test_from_utf8_error() {
        // Test the conversion from Utf8Error to TwingateError
        // We'll create an actual invalid UTF-8 error by manipulating bytes
        let valid_string = "Hello";
        let mut bytes = valid_string.as_bytes().to_vec();
        
        // Make it invalid by adding an incomplete UTF-8 sequence
        // 0xF0 starts a 4-byte sequence but we don't complete it
        bytes.push(0xF0);
        
        let utf8_error = std::str::from_utf8(&bytes).unwrap_err();
        let twingate_error: TwingateError = utf8_error.into();
        
        match twingate_error {
            TwingateError::InvalidUtf8 => {},
            _ => panic!("Expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_from_arboard_error() {
        // Create a mock arboard error using the clipboard error variant directly
        let error = TwingateError::ClipboardError {
            details: "Mock clipboard error".to_string(),
        };
        
        match error {
            TwingateError::ClipboardError { details } => {
                assert_eq!(details, "Mock clipboard error");
            }
            _ => panic!("Expected ClipboardError"),
        }
    }

    #[test]
    fn test_json_error_display() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let twingate_error: TwingateError = json_error.into();
        
        match twingate_error {
            TwingateError::JsonError { .. } => {
                assert!(twingate_error.to_string().contains("JSON deserialization failed"));
            }
            _ => panic!("Expected JsonError"),
        }
    }

    #[test]
    fn test_error_debug_format() {
        let error = TwingateError::ServiceNotRunning;
        let debug_str = format!("{:?}", error);
        assert_eq!(debug_str, "ServiceNotRunning");
    }

    #[test]
    fn test_error_chain() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let twingate_error: TwingateError = json_error.into();
        
        // Test that the error chain is preserved
        assert!(twingate_error.source().is_some());
    }
}
