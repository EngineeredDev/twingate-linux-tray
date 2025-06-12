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
        
        if output.contains("not-running") || output.contains("offline") || output.contains("stopped") {
            log::debug!("Service state detected: NotRunning");
            ServiceState::NotRunning
        } else if output.contains("starting") || output.contains("initializing") || output.contains("booting") {
            log::debug!("Service state detected: Starting");
            ServiceState::Starting
        } else if output.contains("connecting") || output.contains("authenticating") || output.contains("handshake") {
            log::debug!("Service state detected: Connecting");
            ServiceState::Connecting
        } else if output.contains("online") || output.contains("connected") || output.contains("ready") {
            log::debug!("Service state detected: Connected");
            ServiceState::Connected
        } else if output.contains("auth") && (output.contains("required") || output.contains("needed") || output.contains("expired")) {
            log::debug!("Service state detected: AuthRequired");
            ServiceState::AuthRequired
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
    
    let status = std::str::from_utf8(&status_output.stdout).map_err(|e| {
        log::error!("Invalid UTF-8 in status output: {}", e);
        TwingateError::NetworkDataParseError("Invalid UTF-8 in status output".to_string())
    })?;
    
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
    
    let output_str = str::from_utf8(&resources_output.stdout).map_err(|e| {
        log::error!("Invalid UTF-8 in resources output: {}", e);
        TwingateError::NetworkDataParseError("Invalid UTF-8 in resources output".to_string())
    })?;
    
    let trimmed_output = output_str.trim();
    log::debug!("Raw resources command output (length: {}): '{}'", trimmed_output.len(), trimmed_output);
    
    // Check for common non-JSON responses during authentication flow
    if trimmed_output.is_empty() {
        log::debug!("Empty resources output - service may be starting or not ready");
        return Err(TwingateError::ServiceConnecting);
    }
    
    // Check for known transitional state responses
    let transitional_responses = [
        "not connected", "offline", "connecting", "authenticating", 
        "starting", "initializing", "waiting", "loading"
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
             Err(TwingateError::JsonParseError(e))
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
            Ok(ServiceState::Connected) => {
                log::debug!("Service reports connected state, attempting to get resources");
                // Service claims to be connected, try to get resources
                match try_get_resources_data(app_handle).await {
                    Ok(network) => {
                        log::debug!("Successfully retrieved network data on attempt {}", retry_count + 1);
                        return Ok(network);
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
            Ok(ServiceState::Starting) | Ok(ServiceState::Connecting) | Ok(ServiceState::AuthRequired) => {
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
            return Err(TwingateError::RetryLimitExceeded(
                format!("Failed to get network data after {} attempts", max_retries + 1)
            ));
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
    Err(TwingateError::AuthFlowTimeout)
}
