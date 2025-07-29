use crate::error::{Result, TwingateError};
use crate::network::{get_network_data_with_retry, wait_for_service_ready};
use std::str;
use std::time::Duration;
use tauri_plugin_shell::ShellExt;
use tokio::time::sleep;

const AUTH_RETRY_ATTEMPTS: u32 = 10;
const AUTH_STATUS_CHECK_DELAY_MS: u64 = 500;
const AUTH_TIMEOUT_SECONDS: u64 = 120;

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
                       status_str.to_lowercase().contains("user authentication is required");
    
    if !auth_required {
        log::debug!("Service does not require authentication");
        return Ok(());
    }
    
    log::info!("Service requires authentication, attempting to get auth URL");
    
    // Try to get resources data which might trigger auth URL generation
    let resources_output = shell
        .command("twingate")
        .args(["resources", "list"])
        .output()
        .await?;
    
    let resources_str = str::from_utf8(&resources_output.stdout)?;
    
    log::debug!("Resources output: {}", resources_str);
    
    // Check if the resources command output contains an auth URL
    let mut auth_url: Option<String> = None;
    
    // Look for URL patterns in the resources output
    for line in resources_str.lines() {
        if let Some(url_start) = line.find("http") {
            let url_part = &line[url_start..];
            // Find the end of the URL (whitespace or end of line)
            let url_end = url_part.find(char::is_whitespace).unwrap_or(url_part.len());
            let url = url_part[..url_end].trim();
            
            if !url.is_empty() && (url.starts_with("https://") || url.starts_with("http://")) {
                auth_url = Some(url.to_string());
                log::info!("Found authentication URL in resources output: {}", url);
                break;
            }
        }
    }
    
    // If no URL found in resources, try the auth command
    if auth_url.is_none() {
        log::debug!("No URL found in resources output, trying auth command");
        
        let auth_output = shell
            .command("twingate")
            .args(["auth"])
            .output()
            .await?;
        
        let auth_str = str::from_utf8(&auth_output.stdout)?;
        
        log::debug!("Auth command output: {}", auth_str);
        
        // Look for URL patterns in the auth output
        for line in auth_str.lines() {
            if let Some(url_start) = line.find("http") {
                let url_part = &line[url_start..];
                let url_end = url_part.find(char::is_whitespace).unwrap_or(url_part.len());
                let url = url_part[..url_end].trim();
                
                if !url.is_empty() && (url.starts_with("https://") || url.starts_with("http://")) {
                    auth_url = Some(url.to_string());
                    log::info!("Found authentication URL in auth output: {}", url);
                    break;
                }
            }
        }
    }
    
    // If still no URL, try to trigger authentication by accessing network data
    if auth_url.is_none() {
        log::debug!("No URL found, trying to trigger authentication");
        
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
                if let Some(url_start) = error_str.find("http") {
                    let url_part = &error_str[url_start..];
                    let url_end = url_part.find(char::is_whitespace).unwrap_or(url_part.len());
                    let url = url_part[..url_end].trim();
                    
                    if !url.is_empty() && (url.starts_with("https://") || url.starts_with("http://")) {
                        auth_url = Some(url.to_string());
                        log::info!("Found authentication URL in error message: {}", url);
                    }
                }
            }
        }
    }
    
    if let Some(url) = auth_url {
        log::info!("Opening authentication URL: {}", url);
        
        // Open the URL in the default browser using Tauri's shell API
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
                #[cfg(target_os = "macos")]
                let open_cmd = "open";
                
                let open_result = shell
                    .command(open_cmd)
                    .args([&url])
                    .output()
                    .await;
                
                if let Err(e) = open_result {
                    log::error!("Alternative method also failed: {}", e);
                    return Err(TwingateError::from(e));
                }
            }
        }
        
        // Wait a bit for the authentication to start
        sleep(Duration::from_millis(3000)).await;

        // Wait for the service to be ready after authentication
        match wait_for_service_ready(app_handle, AUTH_TIMEOUT_SECONDS).await {
            Ok(_) => {
                log::info!("Service is ready after authentication");
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


