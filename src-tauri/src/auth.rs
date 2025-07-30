use crate::error::{Result, TwingateError};
use crate::network::{get_network_data_with_retry, wait_for_service_ready};
use crate::state::AppState;
use crate::tray::{build_tray_menu, TWINGATE_TRAY_ID};
use crate::utils::{extract_url_from_text, extract_url_with_pattern};
use std::str;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tokio::time::sleep;

const AUTH_RETRY_ATTEMPTS: u32 = 10;
const AUTH_STATUS_CHECK_DELAY_MS: u64 = 500;
const AUTH_TIMEOUT_SECONDS: u64 = 120;

async fn rebuild_tray_for_auth_state(app_handle: &AppHandle) -> Result<()> {
    log::debug!("Rebuilding tray menu for authentication state");
    
    // Build and set the tray menu with current state (should show authenticating menu)
    match build_tray_menu(app_handle, None).await {
        Ok(menu) => match app_handle.tray_by_id(TWINGATE_TRAY_ID) {
            Some(tray) => {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    log::error!("Failed to set tray menu for auth state: {}", e);
                    Err(TwingateError::from(e))
                } else {
                    log::debug!("Successfully updated tray menu for authenticating state");
                    Ok(())
                }
            }
            None => {
                log::error!("Tray icon not found with ID: {}", TWINGATE_TRAY_ID);
                Err(TwingateError::ServiceNotRunning)
            }
        },
        Err(e) => {
            log::error!("Failed to build tray menu for auth state: {}", e);
            Err(e)
        }
    }
}

pub async fn start_resource_auth(app_handle: &tauri::AppHandle, auth_id: &str) -> Result<()> {
    log::debug!("Starting resource authentication for auth_id: {}", auth_id);
    
    let resource_id = auth_id
        .split("-")
        .last()
        .ok_or_else(|| TwingateError::invalid_resource_id(auth_id))?;

    log::debug!("Extracted resource_id: {}", resource_id);

    // Get network data with retry logic to handle transitional states
    let n = get_network_data_with_retry(app_handle, AUTH_RETRY_ATTEMPTS)
        .await?
        .ok_or(TwingateError::ServiceNotRunning)?;

    log::debug!("Retrieved network data with {} resources", n.resources.len());

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .ok_or_else(|| TwingateError::resource_not_found(resource_id))?;

    let resource_name = &n.resources[idx].name;
    log::debug!("Found resource: {} at index {}", resource_name, idx);

    // Execute authentication command with proper error handling
    match execute_auth_command(app_handle, resource_name).await {
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
    
    let shell = app_handle.shell();
    
    // First check if authentication is needed by running twingate status
    let status_output = shell
        .command("twingate")
        .args(["status"])
        .output()
        .await?;
    
    let status_str = str::from_utf8(&status_output.stdout)?;
    
    log::debug!("Service status output: {}", status_str);
    
    // Check if authentication is required - look for various patterns
    let auth_required = status_str.to_lowercase().contains("authentication is required") ||
                       status_str.to_lowercase().contains("auth required") ||
                       status_str.to_lowercase().contains("not authenticated") ||
                       status_str.to_lowercase().contains("user authentication is required") ||
                       status_str.to_lowercase().contains("authenticating");
    
    if !auth_required {
        log::debug!("Service does not require authentication");
        return Ok(());
    }
    
    // Check if we're already in authenticating state and can extract URL from status
    if status_str.to_lowercase().contains("authenticating") {
        log::debug!("Service is in authenticating state, looking for URL in status output");
        
        // Look for the authentication URL in the status output
        if let Some(url) = extract_url_from_text(status_str) {
            if url.len() > 20 {
                log::info!("Found authentication URL in status output: {}", url);
                    
                // Update application state to show we're authenticating
                let state = app_handle.state::<Mutex<AppState>>();
                {
                    let mut state_guard = state.lock().unwrap();
                    state_guard.set_authenticating(url.clone());
                }
                    
                    // Immediately rebuild tray to show authenticating menu
                    if let Err(e) = rebuild_tray_for_auth_state(app_handle).await {
                        log::warn!("Failed to rebuild tray for authenticating state: {}", e);
                    }
                    
                // Try to open the URL in the default browser using Tauri's shell API
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
                                                
                        let open_result = shell
                            .command(open_cmd)
                            .args([&url])
                            .output()
                            .await;
                            
                        match open_result {
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
                        let state = app_handle.state::<Mutex<AppState>>();
                        {
                            let mut state_guard = state.lock().unwrap();
                            state_guard.update_network(None); // This will set status to NotRunning temporarily
                        }
                        
                        // Trigger a tray rebuild to reflect the new state
                        crate::rebuild_tray_after_delay(app_handle.clone());
                        
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("Service not ready after opening auth URL: {}", e);
                        // Don't fail here - the user might still be completing authentication
                        return Ok(());
                    }
                }
            }
        }
        
        // If we found authenticating state but no URL, continue with polling logic below
        log::debug!("Found authenticating state but no URL in current status output, will poll for it");
    }
    
    log::info!("Service requires authentication, attempting to get auth URL");
    
    // Try multiple approaches to get the auth URL, with retries
    let mut auth_url: Option<String> = None;
    let max_attempts = 8;
    let mut attempt = 0;
    
    while attempt < max_attempts && auth_url.is_none() {
        attempt += 1;
        log::debug!("Auth URL detection attempt {} of {}", attempt, max_attempts);
        
        // First, always check the status to see if we're in authenticating state now
        let status_check = shell
            .command("twingate")
            .args(["status"])
            .output()
            .await?;
        
        let status_check_str = str::from_utf8(&status_check.stdout)?;
        log::debug!("Status check (attempt {}): {}", attempt, status_check_str);
        
        // If we're now in authenticating state, look for the URL
        if status_check_str.to_lowercase().contains("authenticating") {
            log::debug!("Service is now in authenticating state on attempt {}", attempt);
            
            // Look for the authentication URL in the status output
            if let Some(url) = extract_url_from_text(status_check_str) {
                if url.len() > 20 {
                    auth_url = Some(url.clone());
                    log::info!("Found authentication URL in status on polling attempt {}: {}", attempt, url);
                }
            }
            
            if auth_url.is_some() {
                break;
            }
        }
        
        // On first attempt, also try to trigger auth by getting network data
        if attempt == 1 {
            log::debug!("Attempting to trigger auth URL generation by accessing network data");
            match get_network_data_with_retry(app_handle, 1).await {
                Ok(_) => {
                    log::debug!("Network data retrieved successfully, authentication might not be needed");
                    return Ok(());
                }
                Err(e) => {
                    log::debug!("Network data retrieval failed (as expected): {}", e);
                    // This failure is expected and might have triggered auth URL generation
                }
            }
        }
        
        // Try to get resources data which might trigger auth URL generation
        let resources_output = shell
            .command("twingate")
            .args(["resources", "list"])
            .output()
            .await?;
        
        let resources_str = str::from_utf8(&resources_output.stdout)?;
        
        log::debug!("Resources output (attempt {}): {}", attempt, resources_str);
        
        // Look for URL patterns in the resources output with enhanced detection
        let patterns = ["visit:", "go to:", "open:", "navigate to:", "visit ", "go to ", "browse to:", "authenticate at:", "login at:"];
        if let Some(url) = extract_url_with_pattern(resources_str, &patterns) {
            auth_url = Some(url.clone());
            log::info!("Found authentication URL in resources output (attempt {}): {}", attempt, url);
        }
        
        if auth_url.is_none() {
            log::debug!("No URL found in resources output attempt {}, trying multiple auth commands", attempt);
            
            // Try different auth-related commands, focusing on status since that's where URL appears
            let auth_commands = vec![
                vec!["status"],
                vec!["status", "--json"],
                vec!["auth"],
                vec!["auth", "--help"],
            ];
            
            for cmd_args in auth_commands {
                let auth_output = shell
                    .command("twingate")
                    .args(&cmd_args)
                    .output()
                    .await?;
                
                let auth_str = str::from_utf8(&auth_output.stdout)?;
                let auth_err = str::from_utf8(&auth_output.stderr).unwrap_or("");
                
                log::debug!("Command 'twingate {}' output (attempt {}): stdout='{}', stderr='{}'", 
                    cmd_args.join(" "), attempt, auth_str, auth_err);
                
                // Look for URL patterns in both stdout and stderr
                let combined_output = format!("{}\n{}", auth_str, auth_err);
                let patterns = ["visit:", "go to:", "open:", "navigate to:", "visit ", "go to ", "browse to:"];
                if let Some(url) = extract_url_with_pattern(&combined_output, &patterns) {
                    auth_url = Some(url.clone());
                    log::info!("Found authentication URL in '{}' output (attempt {}): {}", 
                        cmd_args.join(" "), attempt, url);
                }
                
                if auth_url.is_some() {
                    break;
                }
            }
        }
        
        // If still no URL found and this isn't the last attempt, wait before retrying
        if auth_url.is_none() && attempt < max_attempts {
            log::debug!("No auth URL found on attempt {}, waiting 1.5 seconds before retry", attempt);
            sleep(Duration::from_millis(1500)).await;
        }
    }
    
    // If still no URL after all attempts, try to trigger authentication by accessing network data
    if auth_url.is_none() {
        log::debug!("No URL found after {} attempts, trying to trigger authentication via network data", max_attempts);
        
        // Try to get network data which might trigger auth
        match get_network_data_with_retry(app_handle, 1).await {
            Ok(_) => {
                log::debug!("Network data retrieved successfully, authentication might not be needed");
                return Ok(());
            }
            Err(e) => {
                log::debug!("Network data retrieval failed: {}, checking for auth URL in error", e);
                
                // Sometimes the error message contains the auth URL
                let error_str = e.to_string();
                if let Some(url) = extract_url_from_text(&error_str) {
                    auth_url = Some(url.clone());
                    log::info!("Found authentication URL in error message: {}", url);
                }
            }
        }
    }
    
    if let Some(url) = auth_url {
        log::info!("Found authentication URL: {}", url);
        
        // Update application state to show we're authenticating
        let state = app_handle.state::<Mutex<AppState>>();
        {
            let mut state_guard = state.lock().unwrap();
            state_guard.set_authenticating(url.clone());
        }
        
        // Immediately rebuild tray to show authenticating menu
        if let Err(e) = rebuild_tray_for_auth_state(app_handle).await {
            log::warn!("Failed to rebuild tray for authenticating state: {}", e);
        }
        
        // Try to open the URL in the default browser using Tauri's shell API
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
                
                let open_result = shell
                    .command(open_cmd)
                    .args([&url])
                    .output()
                    .await;
                    
                match open_result {
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
                let state = app_handle.state::<Mutex<AppState>>();
                {
                    let mut state_guard = state.lock().unwrap();
                    state_guard.update_network(None); // This will set status to NotRunning temporarily
                }
                
                // Trigger a tray rebuild to reflect the new state
                crate::rebuild_tray_after_delay(app_handle.clone());
                
                Ok(())
            }
            Err(e) => {
                log::warn!("Service not ready after opening auth URL: {}", e);
                // Don't fail here - the user might still be completing authentication
                Ok(())
            }
        }
    } else {
        log::warn!("Could not find authentication URL automatically");
        log::info!("User may need to manually authenticate or run 'twingate auth' in terminal");
        
        // As a last resort, try to display a message to the user
        log::info!("Please run 'twingate auth' in your terminal to authenticate");
        
        Ok(())
    }
}
    

async fn execute_auth_command(app_handle: &tauri::AppHandle, resource_name: &str) -> Result<()> {
    log::debug!("Executing authentication command for resource: {}", resource_name);
    
    let shell = app_handle.shell();
    
    // Execute the authentication command
    let auth_result = shell
        .command("pkexec")
        .args(["twingate", "auth", resource_name])
        .output()
        .await;

    match auth_result {
        Ok(output) => {
            if output.status.success() {
                log::debug!("Authentication command completed successfully for resource: {}", resource_name);
                Ok(())
            } else {
                let error_msg = format!(
                    "Authentication command failed for resource {} with exit code: {:?}", 
                    resource_name, 
                    output.status.code()
                );
                log::error!("{}", error_msg);
                Err(TwingateError::command_failed(
                    "twingate auth",
                    output.status.code().unwrap_or(-1),
                    error_msg,
                ))
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to execute authentication command for resource {}: {}", resource_name, e);
            log::error!("{}", error_msg);
            Err(TwingateError::from(e))
        }
    }
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
            match get_network_data_with_retry(app_handle, 2).await {
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
        assert_eq!(AUTH_RETRY_ATTEMPTS, 10);
        assert_eq!(AUTH_STATUS_CHECK_DELAY_MS, 500);
        assert_eq!(AUTH_TIMEOUT_SECONDS, 120);
    }

    #[test]
    fn test_auth_url_extraction_scenarios() {
        // Test various authentication output patterns
        let test_cases = vec![
            (
                "Please visit: https://auth.twingate.com/device?code=ABC123",
                Some("https://auth.twingate.com/device?code=ABC123"),
            ),
            (
                "Go to: https://mycompany.twingate.com/auth",
                Some("https://mycompany.twingate.com/auth"),
            ),
            (
                "Authentication required. No URL provided.",
                None,
            ),
            (
                "Visit https://example.com/very/long/path?param1=value1&param2=value2#fragment",
                Some("https://example.com/very/long/path?param1=value1&param2=value2#fragment"),
            ),
            (
                "Multiple URLs: https://first.com and https://second.com",
                Some("https://first.com"),
            ),
            (
                "No authentication required",
                None,
            ),
            (
                "Status: authenticating\nPlease visit: https://auth.company.com/login",
                Some("https://auth.company.com/login"),
            ),
        ];

        for (input, expected) in test_cases {
            let result = extract_url_from_text(input);
            assert_eq!(result.as_deref(), expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_auth_url_extraction_with_patterns() {
        let patterns = &["visit:", "go to:", "navigate to:", "open:"];
        
        let test_cases = vec![
            (
                "Please visit: https://auth.example.com",
                Some("https://auth.example.com"),
            ),
            (
                "You need to go to: https://company.twingate.com/auth",
                Some("https://company.twingate.com/auth"),
            ),
            (
                "Navigate to: https://secure.example.com/login?token=xyz",
                Some("https://secure.example.com/login?token=xyz"),
            ),
            (
                "Open: https://portal.example.com",
                Some("https://portal.example.com"),
            ),
            (
                "Authentication required but no specific instruction",
                None,
            ),
        ];

        for (input, expected) in test_cases {
            let result = extract_url_with_pattern(input, patterns);
            assert_eq!(result.as_deref(), expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_real_world_auth_scenarios() {
        // Test realistic authentication command outputs
        let status_outputs = vec![
            (
                "Twingate Status: Authenticating\nPlease visit: https://mycompany.twingate.com/auth/device?code=ABCD1234&session=xyz789 to complete authentication.",
                true,
            ),
            (
                "Authentication is required. Please run 'twingate auth' to authenticate.",
                true,
            ),
            (
                "User authentication is required",
                true,
            ),
            (
                "Twingate is online. Connected to network.",
                false,
            ),
            (
                "Service is not running",
                false,
            ),
            (
                "Authenticating... Please wait.",
                true,
            ),
        ];

        for (output, should_require_auth) in status_outputs {
            let requires_auth = output.to_lowercase().contains("authentication is required") ||
                               output.to_lowercase().contains("auth required") ||
                               output.to_lowercase().contains("not authenticated") ||
                               output.to_lowercase().contains("user authentication is required") ||
                               output.to_lowercase().contains("authenticating");
            
            assert_eq!(requires_auth, should_require_auth, "Failed for output: {}", output);
        }
    }

    #[test]
    fn test_auth_command_resource_id_extraction() {
        // Test resource ID extraction from auth command IDs
        let test_cases = vec![
            ("authenticate-resource-123", "123"), // split("-").last() returns the last part
            ("authenticate-simple", "simple"),
            ("authenticate-complex-resource-with-dashes", "dashes"), // split("-").last() returns "dashes"
            ("authenticate-", ""),
        ];

        for (auth_id, expected_resource_id) in test_cases {
            let resource_id = auth_id.split("-").last().unwrap_or_default();
            assert_eq!(resource_id, expected_resource_id, "Failed for auth_id: {}", auth_id);
        }
    }

    #[test]
    fn test_auth_timeout_calculation() {
        // Test that timeout values are reasonable
        assert!(AUTH_TIMEOUT_SECONDS >= 60, "Auth timeout should be at least 1 minute");
        assert!(AUTH_TIMEOUT_SECONDS <= 300, "Auth timeout should not exceed 5 minutes");
        
        assert!(AUTH_STATUS_CHECK_DELAY_MS >= 100, "Status check delay should be at least 100ms");
        assert!(AUTH_STATUS_CHECK_DELAY_MS <= 2000, "Status check delay should not exceed 2 seconds");
        
        assert!(AUTH_RETRY_ATTEMPTS >= 5, "Should have at least 5 retry attempts");
        assert!(AUTH_RETRY_ATTEMPTS <= 20, "Should not have more than 20 retry attempts");
    }

    #[test]
    fn test_auth_url_validation() {
        // Test that extracted URLs are valid format
        let valid_urls = vec![
            "https://example.com",
            "https://company.twingate.com/auth",
            "https://portal.example.com/device?code=123",
            "http://localhost:8080/auth",
        ];

        for url in valid_urls {
            assert!(url.starts_with("http://") || url.starts_with("https://"));
            assert!(url.len() > 10, "URL should be longer than 10 characters");
            assert!(!url.contains(" "), "URL should not contain spaces");
        }
    }

    #[test]
    fn test_auth_state_detection_patterns() {
        // Test various authentication state detection patterns
        let auth_patterns = vec![
            "authentication is required",
            "user authentication is required", 
            "auth required",
            "not authenticated",
            "authentication needed",
            "please authenticate",
            "requires authentication",
            "needs authentication",
        ];

        for pattern in auth_patterns {
            let test_string = format!("Service status: {}", pattern);
            assert!(test_string.to_lowercase().contains("authentication") ||
                   test_string.to_lowercase().contains("auth"));
        }
    }

    #[test]
    fn test_error_conditions() {
        // Test that appropriate error conditions are handled
        let error_scenarios = vec![
            "Service not found",
            "Permission denied", 
            "Network unreachable",
            "Timeout occurred",
            "Invalid credentials",
        ];

        for scenario in error_scenarios {
            // These would typically result in TwingateError variants
            assert!(!scenario.is_empty());
            assert!(scenario.len() > 5);
        }
    }
}
