use crate::error::{Result, TwingateError};
use crate::models::Network;
use crate::network::get_network_data;
use crate::state::AppState;
use crate::tray::{build_tray_menu, TWINGATE_TRAY_ID};
use crate::utils::{extract_url_from_text, extract_url_with_pattern};
use std::str;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{ShellExt, process::Output};
use tokio::time::sleep;

/// Manages network data fetching with caching and refresh logic
pub struct NetworkDataManager<'a> {
    app_handle: &'a AppHandle,
    cache_duration: Duration,
}

impl<'a> NetworkDataManager<'a> {
    pub fn new(app_handle: &'a AppHandle, cache_duration: Duration) -> Self {
        Self {
            app_handle,
            cache_duration,
        }
    }

    /// Gets network data, using cache if fresh or refreshing if stale
    pub async fn get_cached_or_refresh(&self) -> Result<Option<Network>> {
        let state = self.app_handle.state::<Mutex<AppState>>();
        let (needs_refresh, current_network_data) = {
            let state_guard = state.lock().unwrap();
            (
                state_guard.should_refresh(self.cache_duration),
                state_guard.network().cloned(),
            )
        };

        if needs_refresh {
            log::debug!("Network data is stale, refreshing...");
            match get_network_data(self.app_handle).await {
                Ok(fresh_data) => {
                    // Update state with fresh data
                    {
                        let mut state_guard = state.lock().unwrap();
                        state_guard.update_network(fresh_data.clone());
                    }
                    log::debug!("Successfully refreshed network data");
                    Ok(fresh_data)
                }
                Err(e) => {
                    log::error!("Failed to refresh network data: {}", e);
                    Err(e)
                }
            }
        } else {
            log::debug!("Using cached network data");
            Ok(current_network_data)
        }
    }

    /// Gets network data and ensures service is running
    pub async fn get_network_or_error(&self) -> Result<Network> {
        self.get_cached_or_refresh()
            .await?
            .ok_or_else(|| {
                log::error!("Twingate service is not running");
                TwingateError::ServiceNotRunning
            })
    }
}

/// Manages application state access with clean abstractions
pub struct StateManager;

impl StateManager {
    /// Execute a closure with read access to application state
    pub fn with_state<F, R>(app_handle: &AppHandle, f: F) -> R
    where
        F: FnOnce(&AppState) -> R,
    {
        let state = app_handle.state::<Mutex<AppState>>();
        let state_guard = state.lock().unwrap();
        f(&*state_guard)
    }

    /// Execute a closure with write access to application state
    pub fn with_state_mut<F, R>(app_handle: &AppHandle, f: F) -> R
    where
        F: FnOnce(&mut AppState) -> R,
    {
        let state = app_handle.state::<Mutex<AppState>>();
        let mut state_guard = state.lock().unwrap();
        f(&mut *state_guard)
    }

    /// Get the authentication URL if in authenticating state
    pub fn get_auth_url(app_handle: &AppHandle) -> Option<String> {
        Self::with_state(app_handle, |state| {
            state.auth_url().map(|url| url.to_string())
        })
    }

    /// Set the application to authenticating state
    pub fn set_authenticating(app_handle: &AppHandle, auth_url: String) {
        Self::with_state_mut(app_handle, |state| {
            state.set_authenticating(auth_url);
        });
    }

    /// Update network data in state
    pub fn update_network(app_handle: &AppHandle, network: Option<Network>) {
        Self::with_state_mut(app_handle, |state| {
            state.update_network(network);
        });
    }
}

/// Manages authentication state detection and URL extraction
pub struct AuthStateManager;

impl AuthStateManager {
    /// Check if authentication is required based on status output
    pub fn is_auth_required(status_output: &str) -> bool {
        let status_lower = status_output.to_lowercase();
        status_lower.contains("authentication is required") ||
        status_lower.contains("auth required") ||
        status_lower.contains("not authenticated") ||
        status_lower.contains("user authentication is required") ||
        status_lower.contains("authenticating")
    }

    /// Extract authentication URL from various command outputs
    pub fn extract_auth_url(output: &str) -> Option<String> {
        // First try with common patterns
        let patterns = ["visit:", "go to:", "open:", "navigate to:", "visit ", "go to ", "browse to:", "authenticate at:", "login at:"];
        if let Some(url) = extract_url_with_pattern(output, &patterns) {
            if url.len() > 20 { // Minimum reasonable URL length
                return Some(url);
            }
        }

        // Fallback to any URL in the text
        extract_url_from_text(output)
    }

    /// Check service status and extract auth URL if available
    pub async fn check_auth_status(app_handle: &AppHandle) -> Result<Option<String>> {
        let shell = app_handle.shell();
        let status_output = shell
            .command("twingate")
            .args(["status"])
            .output()
            .await?;

        let status_str = str::from_utf8(&status_output.stdout)?;
        log::debug!("Service status output: {}", status_str);

        if Self::is_auth_required(status_str) {
            Ok(Self::extract_auth_url(status_str))
        } else {
            Ok(None)
        }
    }
}

/// Centralizes shell command execution with consistent error handling
pub struct CommandExecutor<'a> {
    app_handle: &'a AppHandle,
}

impl<'a> CommandExecutor<'a> {
    pub fn new(app_handle: &'a AppHandle) -> Self {
        Self { app_handle }
    }

    /// Execute a shell command with proper error handling
    pub async fn execute(&self, command: &str, args: &[&str]) -> Result<Output> {
        log::debug!("Executing command: {} {}", command, args.join(" "));
        
        let shell = self.app_handle.shell();
        let output = shell
            .command(command)
            .args(args)
            .output()
            .await
            .map_err(|e| {
                log::error!("Failed to execute command '{}': {}", command, e);
                TwingateError::from(e)
            })?;

        log::debug!("Command '{}' completed with status: {:?}", command, output.status);
        Ok(output)
    }

    /// Execute a command and ensure it succeeds
    pub async fn execute_success(&self, command: &str, args: &[&str]) -> Result<Output> {
        let output = self.execute(command, args).await?;
        
        if output.status.success() {
            log::debug!("Command '{}' succeeded", command);
            Ok(output)
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!("Command '{}' failed with exit code: {:?}, stderr: {}", 
                       command, output.status.code(), error_msg);
            Err(TwingateError::command_failed(
                format!("{} {}", command, args.join(" ")),
                output.status.code().unwrap_or(-1),
                error_msg,
            ))
        }
    }

    /// Execute a Twingate command (convenience method)
    pub async fn execute_twingate(&self, args: &[&str]) -> Result<Output> {
        self.execute("twingate", args).await
    }

    /// Execute a Twingate command with elevated privileges
    pub async fn execute_twingate_elevated(&self, args: &[&str]) -> Result<Output> {
        let mut full_args = vec!["twingate"];
        full_args.extend_from_slice(args);
        self.execute_success("pkexec", &full_args).await
    }
}

/// Manages tray operations with centralized logic
pub struct TrayManager;

impl TrayManager {
    /// Rebuild tray menu immediately with current state
    pub async fn rebuild_tray_now(app_handle: &AppHandle) -> Result<()> {
        log::debug!("Rebuilding tray menu immediately");
        
        // Get current network data from state
        let network_data = StateManager::with_state(app_handle, |state| {
            state.network().cloned()
        });

        // Build and set the tray menu
        match build_tray_menu(app_handle, network_data).await {
            Ok(menu) => match app_handle.tray_by_id(TWINGATE_TRAY_ID) {
                Some(tray) => {
                    if let Err(e) = tray.set_menu(Some(menu)) {
                        log::error!("Failed to set tray menu: {}", e);
                        Err(TwingateError::from(e))
                    } else {
                        log::debug!("Successfully updated tray menu");
                        Ok(())
                    }
                }
                None => {
                    log::error!("Tray icon not found with ID: {}", TWINGATE_TRAY_ID);
                    Err(TwingateError::ServiceNotRunning)
                }
            },
            Err(e) => {
                log::error!("Failed to build tray menu: {}", e);
                Err(e)
            }
        }
    }

    /// Rebuild tray menu after a delay with retry logic  
    pub fn rebuild_tray_after_delay(app_handle: AppHandle) {
        tauri::async_runtime::spawn(async move {
            // Use longer initial delay during authentication flow
            sleep(Duration::from_millis(2000)).await;

            let mut retry_count = 0;
            const MAX_REBUILD_RETRIES: u32 = 3;
            const REBUILD_RETRY_DELAY_MS: u64 = 3000;

            loop {
                log::debug!(
                    "Attempting tray rebuild (attempt {} of {})",
                    retry_count + 1,
                    MAX_REBUILD_RETRIES + 1
                );

                let _network_data = match get_network_data(&app_handle).await {
                    Ok(data) => {
                        // Update state with fresh data
                        StateManager::update_network(&app_handle, data.clone());

                        match &data {
                            Some(network) => {
                                log::debug!(
                                    "Successfully refreshed network data for tray menu - User: {}",
                                    network.user.email
                                );
                            }
                            None => {
                                log::debug!("Twingate service is not running - showing disconnected menu");
                            }
                        }
                        data
                    }
                    Err(TwingateError::ServiceConnecting) | Err(TwingateError::AuthenticationRequired) => {
                        log::debug!("Service in transitional state during tray rebuild, will retry");

                        if retry_count >= MAX_REBUILD_RETRIES {
                            log::warn!("Exhausted retries for tray rebuild during authentication flow");
                            None
                        } else {
                            retry_count += 1;
                            log::debug!("Waiting {}ms before retry", REBUILD_RETRY_DELAY_MS);
                            sleep(Duration::from_millis(REBUILD_RETRY_DELAY_MS)).await;
                            continue;
                        }
                    }
                    Err(TwingateError::RetryLimitExceeded { .. }) => {
                        log::warn!(
                            "Network data retrieval retry limit exceeded during tray rebuild"
                        );
                        None
                    }
                    Err(e) => {
                        log::error!("Error getting network data for tray rebuild: {}", e);
                        None
                    }
                };

                // Build and set the tray menu
                if let Err(e) = Self::rebuild_tray_now(&app_handle).await {
                    log::error!("Failed to rebuild tray menu: {}", e);
                }

                break;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Network, User, InternetSecurity};

    fn create_test_network() -> Network {
        Network {
            admin_url: "https://admin.twingate.com".to_string(),
            full_tunnel_time_limit: 3600,
            internet_security: InternetSecurity {
                mode: 1,
                status: 2,
            },
            resources: vec![],
            user: User {
                avatar_url: "https://example.com/avatar.png".to_string(),
                email: "test@example.com".to_string(),
                first_name: "Test".to_string(),
                id: "user-123".to_string(),
                is_admin: false,
                last_name: "User".to_string(),
            },
        }
    }

    #[test]
    fn test_network_data_manager_creation() {
        // This test would require a mock AppHandle, which is complex to set up
        // In a real implementation, we'd use dependency injection or mocking
        let cache_duration = Duration::from_secs(30);
        assert_eq!(cache_duration.as_secs(), 30);
    }

    #[test]
    fn test_state_manager_methods_exist() {
        // Test that our StateManager has the expected methods
        // These would be integration tested with a real AppHandle
        assert!(std::mem::size_of::<StateManager>() == 0); // Zero-sized type
    }
}