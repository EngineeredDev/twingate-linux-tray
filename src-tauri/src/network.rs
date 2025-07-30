use crate::error::{Result, TwingateError};
use crate::models::Network;
use serde_json::from_slice;
use std::str;
use std::time::Duration;
use tauri_plugin_shell::ShellExt;
use tokio::time::sleep;

const MAX_RETRIES: u32 = 8;
const BASE_DELAY_MS: u64 = 1000;
const MAX_DELAY_MS: u64 = 10000;

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceState {
    NotRunning,
    Starting,
    Connecting,
    Connected,
    AuthRequired,
}

impl ServiceState {
    fn from_status_output(output: &str) -> Self {
        let output = output.trim().to_lowercase();
        
        log::debug!("Parsing service state from output: '{}'", output);
        
        // Check for authentication required states first (highest priority)
        if output.contains("authentication is required") ||
           output.contains("auth required") ||
           output.contains("authentication required") ||
           output.contains("user authentication is required") ||
           output.contains("needs authentication") ||
           output.contains("not authenticated") ||
           output.contains("authentication needed") ||
           output.contains("please authenticate") ||
           output.contains("requires authentication") ||
           (output.contains("auth") && (output.contains("required") || output.contains("needed") || output.contains("expired"))) {
            log::debug!("Service state detected: AuthRequired");
            return ServiceState::AuthRequired;
        }
        
        if output.contains("not-running") || output.contains("offline") || output.contains("stopped") || 
           output.contains("not running") || output.contains("inactive") || output.contains("dead") {
            log::debug!("Service state detected: NotRunning");
            ServiceState::NotRunning
        } else if output.contains("starting") || output.contains("initializing") || output.contains("booting") ||
                  output.contains("loading") || output.contains("launching") {
            log::debug!("Service state detected: Starting");
            ServiceState::Starting
        } else if output.contains("connecting") || output.contains("authenticating") || output.contains("handshake") ||
                  output.contains("establishing") || output.contains("negotiating") {
            log::debug!("Service state detected: Connecting");
            ServiceState::Connecting
        } else if output.contains("online") || output.contains("connected") || output.contains("ready") ||
                  output.contains("active") || output.contains("established") {
            log::debug!("Service state detected: Connected");
            ServiceState::Connected
        } else {
            log::debug!("Service state unknown, defaulting to Connecting for: '{}'", output);
            ServiceState::Connecting
        }
    }
}

async fn get_service_state(app_handle: &tauri::AppHandle) -> Result<ServiceState> {
    log::debug!("Checking Twingate service status");
    let shell = app_handle.shell();
    
    let status_output = shell.command("twingate").args(["status"]).output().await?;
    
    let status = std::str::from_utf8(&status_output.stdout)?;
    
    log::debug!("Raw twingate status output: '{}'", status.trim());
    
    let state = ServiceState::from_status_output(status);
    log::debug!("Determined service state: {:?}", state);
    
    Ok(state)
}

async fn try_get_resources_data(app_handle: &tauri::AppHandle) -> Result<Option<Network>> {
    log::debug!("Attempting to fetch resources data");
    let shell = app_handle.shell();
    
    let resources_output = shell
        .command("twingate-notifier")
        .args(["resources"])
        .output()
        .await?;
    
    let output_str = str::from_utf8(&resources_output.stdout)?;
    
    let trimmed_output = output_str.trim();
    log::debug!("Raw resources command output (length: {}): '{}'", trimmed_output.len(), trimmed_output);
    
    // Check for common non-JSON responses during authentication flow
    if trimmed_output.is_empty() {
        log::debug!("Empty resources output - service may be starting or not ready");
        return Err(TwingateError::ServiceConnecting);
    }
    
    // Check for authentication-related responses
    if trimmed_output.to_lowercase().contains("authentication") ||
       trimmed_output.to_lowercase().contains("auth required") ||
       trimmed_output.to_lowercase().contains("not authenticated") ||
       trimmed_output.to_lowercase().contains("please authenticate") ||
       trimmed_output.to_lowercase().contains("login required") {
        log::debug!("Authentication required based on resources output: '{}'", trimmed_output);
        return Err(TwingateError::AuthenticationRequired);
    }
    
    // Check for known transitional state responses
    let transitional_responses = [
        "not connected", "offline", "connecting", "authenticating", 
        "starting", "initializing", "waiting", "loading", "establishing",
        "handshaking", "negotiating", "not ready", "unavailable"
    ];
    
    for response in &transitional_responses {
        if trimmed_output.eq_ignore_ascii_case(response) {
            log::debug!("Service in transitional state: '{}'", trimmed_output);
            return Err(TwingateError::ServiceConnecting);
        }
    }
    
    // Check if output looks like JSON (starts with { or [)
    if !trimmed_output.starts_with('{') && !trimmed_output.starts_with('[') {
        log::warn!("Resources output doesn't appear to be JSON: '{}'", trimmed_output);
        
        // If it contains authentication keywords, return AuthRequired
        if trimmed_output.to_lowercase().contains("auth") {
            return Err(TwingateError::AuthenticationRequired);
        }
        
        return Err(TwingateError::ServiceConnecting);
    }

    match from_slice::<Option<Network>>(trimmed_output.as_bytes()) {
        Ok(network) => {
            log::debug!("Successfully parsed network data with {} resources", 
                if let Some(ref n) = network { n.resources.len() } else { 0 });
            Ok(network)
        }
        Err(e) => {
            log::warn!("Failed to parse JSON from resources output: '{}'. Parse error: {}", 
                trimmed_output, e);
            Err(TwingateError::from(e))
        }
    }
}

pub async fn get_network_data(app_handle: &tauri::AppHandle) -> Result<Option<Network>> {
    get_network_data_with_retry(app_handle, MAX_RETRIES).await
}

pub async fn get_network_data_with_retry(app_handle: &tauri::AppHandle, max_retries: u32) -> Result<Option<Network>> {
    let mut retry_count = 0;
    let mut delay_ms = BASE_DELAY_MS;
    
    log::debug!("Starting network data retrieval with up to {} retries", max_retries);
    
    loop {
        log::debug!("Network data attempt {} of {}", retry_count + 1, max_retries + 1);
        

        // First check the service state for better decision making
        match get_service_state(app_handle).await {
            Ok(ServiceState::NotRunning) => {
                log::debug!("Service not running - returning None");
                return Ok(None);
            }
            Ok(ServiceState::AuthRequired) => {
                log::debug!("Service requires authentication");
                return Err(TwingateError::AuthenticationRequired);
            }
            Ok(ServiceState::Connected) => {
                log::debug!("Service reports connected state, attempting to get resources");
                // Service claims to be connected, try to get resources
                match try_get_resources_data(app_handle).await {
                    Ok(network) => {
                        log::debug!("Successfully retrieved network data on attempt {}", retry_count + 1);
                        return Ok(network);
                    }
                    Err(TwingateError::AuthenticationRequired) => {
                        log::debug!("Resources indicate authentication required");
                        return Err(TwingateError::AuthenticationRequired);
                    }
                    Err(TwingateError::ServiceConnecting) => {
                        log::debug!("Resources not ready despite connected status, service may still be initializing");
                        // Fall through to retry logic
                    }
                    Err(e) => {
                        log::error!("Error retrieving resources despite connected status: {}", e);
                        return Err(e);
                    }
                }
            }
            Ok(ServiceState::Starting) | Ok(ServiceState::Connecting) => {
                log::debug!("Service in transitional state, will retry after delay");
                // Service is in a transitional state, we should retry
            }
            Err(e) => {
                log::warn!("Failed to get service state: {}. Attempting resources as fallback", e);
                // If we can't get status, try resources anyway as a fallback
                match try_get_resources_data(app_handle).await {
                    Ok(network) => {
                        log::debug!("Fallback resources retrieval successful on attempt {}", retry_count + 1);
                        return Ok(network);
                    }
                    Err(TwingateError::AuthenticationRequired) => {
                        log::debug!("Fallback resources indicate authentication required");
                        return Err(TwingateError::AuthenticationRequired);
                    }
                    Err(TwingateError::ServiceConnecting) => {
                        log::debug!("Fallback resources not ready, will retry");
                        // Fall through to retry logic
                    }
                    Err(resources_err) => {
                        log::error!("Both status and resources failed. Status error: {}, Resources error: {}",
                            e, resources_err);
                        return Err(e);
                    }
                }
            }
        }

        
        // Check if we've exhausted retries
        if retry_count >= max_retries {
            log::warn!("Exhausted {} retries attempting to get network data", max_retries);
            log::debug!("Final service state before giving up: {:?}", get_service_state(app_handle).await);
            return Err(TwingateError::RetryLimitExceeded { 
                attempts: max_retries + 1 
            });
        }
        
        // Wait before retrying with exponential backoff
        log::debug!("Waiting {}ms before retry attempt {}", delay_ms, retry_count + 2);
        sleep(Duration::from_millis(delay_ms)).await;
        
        retry_count += 1;
        delay_ms = std::cmp::min(delay_ms * 2, MAX_DELAY_MS);
    }
}

pub async fn wait_for_service_ready(app_handle: &tauri::AppHandle, timeout_seconds: u64) -> Result<()> {
    let start_time = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(timeout_seconds);
    
    log::debug!("Waiting for service to be ready (timeout: {}s)", timeout_seconds);
    
    while start_time.elapsed() < timeout_duration {
        match get_service_state(app_handle).await {
            Ok(ServiceState::Connected) => {
                log::debug!("Service is ready");
                return Ok(());
            }
            Ok(state) => {
                log::debug!("Service state: {:?}, continuing to wait", state);
            }
            Err(e) => {
                log::debug!("Error checking service state: {}, continuing to wait", e);
            }
        }
        
        sleep(Duration::from_millis(1000)).await;
    }
    
    log::warn!("Timeout waiting for service to be ready");
    Err(TwingateError::AuthenticationTimeout { seconds: timeout_seconds })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_state_from_status_output_not_running() {
        let test_cases = vec![
            "not-running",
            "offline",
            "stopped",
            "not running",
            "inactive",
            "dead",
            "Service is not-running",
            "Status: offline",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::NotRunning, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_starting() {
        let test_cases = vec![
            "starting",
            "initializing",
            "booting",
            "loading",
            "launching",
            "Service is starting",
            "Status: initializing",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::Starting, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_connecting() {
        let test_cases = vec![
            "connecting",
            "authenticating",
            "handshake",
            "establishing",
            "negotiating",
            "Service is connecting",
            "Status: authenticating",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::Connecting, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_connected() {
        let test_cases = vec![
            "online",
            "connected",
            "ready",
            "active",
            "established",
            "Service is online",
            "Status: connected",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::Connected, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_auth_required() {
        let test_cases = vec![
            "authentication is required",
            "auth required",
            "authentication required",
            "user authentication is required",
            "needs authentication",
            "not authenticated",
            "authentication needed",
            "please authenticate",
            "requires authentication",
            "auth expired",
            "Authentication is Required",
            "AUTH REQUIRED",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::AuthRequired, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_auth_required_priority() {
        // Auth required should take priority over other states
        let test_cases = vec![
            "connected but authentication is required",
            "online, auth required",
            "ready - user authentication is required",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::AuthRequired, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_unknown() {
        let test_cases = vec![
            "unknown status",
            "weird state",
            "unexpected output",
            "",
            "12345",
            "random text",
        ];

        for output in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, ServiceState::Connecting, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_from_status_output_case_insensitive() {
        let test_cases = vec![
            ("NOT-RUNNING", ServiceState::NotRunning),
            ("ONLINE", ServiceState::Connected),
            ("STARTING", ServiceState::Starting),
            ("CONNECTING", ServiceState::Connecting),
            ("Authentication Is Required", ServiceState::AuthRequired),
        ];

        for (output, expected) in test_cases {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, expected, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_service_state_debug_format() {
        assert_eq!(format!("{:?}", ServiceState::NotRunning), "NotRunning");
        assert_eq!(format!("{:?}", ServiceState::Starting), "Starting");
        assert_eq!(format!("{:?}", ServiceState::Connecting), "Connecting");
        assert_eq!(format!("{:?}", ServiceState::Connected), "Connected");
        assert_eq!(format!("{:?}", ServiceState::AuthRequired), "AuthRequired");
    }

    #[test]
    fn test_service_state_equality() {
        assert_eq!(ServiceState::NotRunning, ServiceState::NotRunning);
        assert_eq!(ServiceState::Connected, ServiceState::Connected);
        assert_ne!(ServiceState::NotRunning, ServiceState::Connected);
        assert_ne!(ServiceState::Starting, ServiceState::Connecting);
    }

    #[test]
    fn test_service_state_clone() {
        let state = ServiceState::AuthRequired;
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }

    #[test]
    fn test_constants_values() {
        assert_eq!(MAX_RETRIES, 8);
        assert_eq!(BASE_DELAY_MS, 1000);
        assert_eq!(MAX_DELAY_MS, 10000);
    }

    #[test]
    fn test_service_state_from_complex_output() {
        // Test more realistic status outputs
        let complex_outputs = vec![
            ("Twingate is not-running. Run 'twingate start' to start.", ServiceState::NotRunning),
            ("Twingate is online. Resources: 5", ServiceState::Connected),
            ("Twingate is starting... Please wait.", ServiceState::Starting),
            ("Twingate is connecting to network...", ServiceState::Connecting),
            ("Twingate is ready but user authentication is required. Please run 'twingate auth'.", ServiceState::AuthRequired),
        ];

        for (output, expected) in complex_outputs {
            let state = ServiceState::from_status_output(output);
            assert_eq!(state, expected, "Failed for complex output: {}", output);
        }
    }

    #[test]
    fn test_service_state_multiline_output() {
        let multiline_output = "Twingate Status:\nState: authentication is required\nResources: 0";
        let state = ServiceState::from_status_output(multiline_output);
        assert_eq!(state, ServiceState::AuthRequired);
    }

    #[test]
    fn test_service_state_whitespace_handling() {
        let outputs_with_whitespace = vec![
            "  authentication is required  ",
            "\tconnected\t",
            "\nstarting\n",
            "   not-running   ",
        ];

        let expected = vec![
            ServiceState::AuthRequired,
            ServiceState::Connected,
            ServiceState::Starting,
            ServiceState::NotRunning,
        ];

        for (output, expected_state) in outputs_with_whitespace.iter().zip(expected.iter()) {
            let state = ServiceState::from_status_output(output);
            assert_eq!(&state, expected_state, "Failed for output with whitespace: '{}'", output);
        }
    }
}
