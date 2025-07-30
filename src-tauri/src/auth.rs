use crate::error::{Result, TwingateError};
use crate::managers::{AuthStateManager, CommandExecutor, NetworkDataManager, StateManager, TrayManager};
use crate::network::wait_for_service_ready;
use std::time::Duration;
use tauri::AppHandle;
use tokio::time::sleep;

const AUTH_STATUS_CHECK_DELAY_MS: u64 = 500;
const AUTH_TIMEOUT_SECONDS: u64 = 120;

async fn rebuild_tray_for_auth_state(app_handle: &AppHandle) -> Result<()> {
    log::debug!("Rebuilding tray menu for authentication state");
    TrayManager::rebuild_tray_now(app_handle).await
}

pub async fn start_resource_auth(app_handle: &tauri::AppHandle, auth_id: &str) -> Result<()> {
    log::debug!("Starting resource authentication for auth_id: {}", auth_id);
    
    let resource_id = auth_id
        .split("-")
        .last()
        .ok_or_else(|| TwingateError::invalid_resource_id(auth_id))?;

    log::debug!("Extracted resource_id: {}", resource_id);

    // Get network data with retry logic to handle transitional states
    let network_manager = NetworkDataManager::new(app_handle, Duration::from_secs(30));
    let n = network_manager.get_network_or_error().await?;

    log::debug!("Retrieved network data with {} resources", n.resources.len());

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .ok_or_else(|| TwingateError::resource_not_found(resource_id))?;

    let resource_name = &n.resources[idx].name;
    log::debug!("Found resource: {} at index {}", resource_name, idx);

    // Execute authentication command with proper error handling
    let executor = CommandExecutor::new(app_handle);
    match executor.execute_twingate_elevated(&["auth", resource_name]).await {
        Ok(_) => {
            log::debug!("Authentication command executed successfully for resource: {}", resource_name);
            
            // Wait for authentication to complete and service to be ready
            match wait_for_auth_completion(app_handle).await {
                Ok(_) => {
                    log::debug!("Authentication completed successfully for resource: {}", resource_name);
                    Ok(())
                }
                Err(e) => {
                    log::warn!("Authentication completion check failed for resource {}: {}", resource_name, e);
                    Err(e)
                }
            }
        }
        Err(e) => {
            log::error!("Failed to execute authentication command for resource {}: {}", resource_name, e);
            Err(e)
        }
    }
}

pub async fn handle_service_auth(app_handle: &tauri::AppHandle) -> Result<()> {
    log::debug!("Checking if service-level authentication is required");
    
    // Use AuthStateManager to check authentication status
    match AuthStateManager::check_auth_status(app_handle).await? {
        None => {
            log::debug!("Service does not require authentication");
            return Ok(());
        }
        Some(url) if url.len() > 20 => {
            log::info!("Found authentication URL in status: {}", url);
            return handle_auth_flow(app_handle, url).await;
        }
        Some(_) => {
            log::debug!("Authentication required but no URL found in status, will search for it");
        }
    }

    log::info!("Service requires authentication, attempting to get auth URL");
    
    // Try multiple approaches to get the auth URL, with retries
    if let Some(url) = find_auth_url_with_retry(app_handle).await? {
        return handle_auth_flow(app_handle, url).await;
    }

    log::warn!("Could not find authentication URL automatically");
    log::info!("User may need to manually authenticate or run 'twingate auth' in terminal");
    Ok(())
}

async fn handle_auth_flow(app_handle: &AppHandle, url: String) -> Result<()> {
    log::info!("Starting authentication flow with URL: {}", url);
    
    // Update application state to show we're authenticating
    StateManager::set_authenticating(app_handle, url.clone());
    
    // Immediately rebuild tray to show authenticating menu
    if let Err(e) = rebuild_tray_for_auth_state(app_handle).await {
        log::warn!("Failed to rebuild tray for authenticating state: {}", e);
    }
    
    // Try to open the URL in the default browser
    match tauri_plugin_opener::open_url(url.clone(), None::<&str>) {
        Ok(_) => {
            log::debug!("Successfully opened authentication URL");
        }
        Err(e) => {
            log::error!("Failed to open authentication URL: {}", e);
            // Try alternative method using shell command
            log::info!("Trying alternative method to open URL");
            
            #[cfg(target_os = "linux")]
            let open_cmd = "xdg-open";
            
            let executor = CommandExecutor::new(app_handle);
            match executor.execute(open_cmd, &[&url]).await {
                Ok(output) => {
                    if !output.status.success() {
                        log::warn!("xdg-open failed with status: {:?}", output.status);
                        log::info!("URL is available in tray menu for manual opening");
                    } else {
                        log::debug!("Successfully opened authentication URL with alternative method");
                    }
                }
                Err(e) => {
                    log::warn!("Alternative method also failed: {}", e);
                    log::info!("URL is available in tray menu for manual opening");
                }
            }
        }
    }
    
    // Wait a bit for the authentication to start
    sleep(Duration::from_millis(3000)).await;

    // Wait for the service to be ready after authentication
    match wait_for_service_ready(app_handle, AUTH_TIMEOUT_SECONDS).await {
        Ok(_) => {
            log::info!("Service is ready after authentication");
            
            // Clear the authenticating state since authentication is complete
            StateManager::update_network(app_handle, None); // This will set status to NotRunning temporarily
            
            // Trigger a tray rebuild to reflect the new state
            TrayManager::rebuild_tray_after_delay(app_handle.clone());
            
            Ok(())
        }
        Err(e) => {
            log::warn!("Service not ready after opening auth URL: {}", e);
            // Don't fail here - the user might still be completing authentication
            Ok(())
        }
    }
}

async fn find_auth_url_with_retry(app_handle: &AppHandle) -> Result<Option<String>> {
    let mut auth_url: Option<String> = None;
    let max_attempts = 8;
    let mut attempt = 0;
    
    let executor = CommandExecutor::new(app_handle);
    
    while attempt < max_attempts && auth_url.is_none() {
        attempt += 1;
        log::debug!("Auth URL detection attempt {} of {}", attempt, max_attempts);
        
        // Check various command outputs for auth URL
        let outputs_to_check = vec![
            executor.execute_twingate(&["status"]).await,
            executor.execute_twingate(&["resources", "list"]).await,
            executor.execute_twingate(&["auth"]).await,
        ];
        
        for output_result in outputs_to_check {
            if let Ok(output) = output_result {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);
                
                if let Some(url) = AuthStateManager::extract_auth_url(&combined) {
                    auth_url = Some(url);
                    break;
                }
            }
        }
        
        if auth_url.is_none() && attempt < max_attempts {
            log::debug!("No auth URL found on attempt {}, waiting before retry", attempt);
            sleep(Duration::from_millis(1500)).await;
        }
    }
    
    Ok(auth_url)
}

async fn wait_for_auth_completion(app_handle: &tauri::AppHandle) -> Result<()> {
    log::debug!("Waiting for authentication completion");
    
    // First, wait a short delay to allow the authentication process to start
    sleep(Duration::from_millis(AUTH_STATUS_CHECK_DELAY_MS)).await;
    
    // Wait for the service to be ready with a timeout
    match wait_for_service_ready(app_handle, AUTH_TIMEOUT_SECONDS).await {
        Ok(_) => {
            log::debug!("Service is ready after authentication");
            
            // Additional verification: try to get network data to confirm everything is working
            let network_manager = NetworkDataManager::new(app_handle, Duration::from_secs(30));
            match network_manager.get_cached_or_refresh().await {
                Ok(Some(_)) => {
                    log::debug!("Network data retrieval successful after authentication");
                    Ok(())
                }
                Ok(None) => {
                    log::warn!("Service not running after authentication completion");
                    Err(TwingateError::ServiceNotRunning)
                }
                Err(e) => {
                    log::warn!("Failed to verify network data after authentication: {}", e);
                    // Don't fail here - the authentication might have succeeded even if we can't immediately get data
                    Ok(())
                }
            }
        }
        Err(e) => {
            log::error!("Timeout or error waiting for authentication completion: {}", e);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_values() {
        assert_eq!(AUTH_STATUS_CHECK_DELAY_MS, 500);
        assert_eq!(AUTH_TIMEOUT_SECONDS, 120);
    }

    #[test]
    fn test_auth_timeout_calculation() {
        // Test that timeout values are reasonable
        assert!(AUTH_TIMEOUT_SECONDS >= 60, "Auth timeout should be at least 1 minute");
        assert!(AUTH_TIMEOUT_SECONDS <= 300, "Auth timeout should not exceed 5 minutes");
        
        assert!(AUTH_STATUS_CHECK_DELAY_MS >= 100, "Status check delay should be at least 100ms");
        assert!(AUTH_STATUS_CHECK_DELAY_MS <= 2000, "Status check delay should not exceed 2 seconds");
        
        // Retry attempts are now managed in individual functions
    }

    #[test]
    fn test_resource_id_extraction() {
        // Test resource ID extraction from auth command IDs
        let test_cases = vec![
            ("authenticate-resource-123", "123"),
            ("authenticate-simple", "simple"),
            ("authenticate-complex-resource-with-dashes", "dashes"),
            ("authenticate-", ""),
        ];

        for (auth_id, expected_resource_id) in test_cases {
            let resource_id = auth_id.split("-").last().unwrap_or_default();
            assert_eq!(resource_id, expected_resource_id, "Failed for auth_id: {}", auth_id);
        }
    }
}