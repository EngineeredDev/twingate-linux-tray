use arboard::Clipboard;
use tauri::{tray::TrayIconBuilder, AppHandle, Manager};
use tauri_plugin_single_instance::init as single_instance_init;

mod auth;
mod commands;
mod error;
mod managers;
mod models;
mod network;
mod state;
mod tray;
mod utils;

use auth::{handle_service_auth, start_resource_auth};
use commands::greet;
use error::{Result, TwingateError};
use managers::{CommandExecutor, NetworkDataManager, StateManager, TrayManager};
use network::get_network_data_with_retry;
use state::AppState;
use std::sync::Mutex;

// Compatibility type alias for gradual migration
type AppStateType = Mutex<AppState>;
use tray::{
    build_tray_menu, build_disconnected_menu, get_address_from_resource, get_open_url_from_resource, MenuAction, AUTHENTICATE_ID, COPY_ADDRESS_ID,
    TWINGATE_TRAY_ID,
};

async fn handle_copy_address(app_handle: &AppHandle, address_id: &str) -> Result<()> {
    let resource_id = address_id.split("-").last().ok_or_else(|| {
        eprintln!("Error: Invalid address ID format: {}", address_id);
        TwingateError::invalid_resource_id(address_id)
    })?;

    // Use NetworkDataManager to get network data with caching
    let network_manager = NetworkDataManager::new(app_handle, std::time::Duration::from_secs(30));
    let n = network_manager.get_network_or_error().await?;

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .ok_or_else(|| {
            eprintln!("Error: Resource not found: {}", resource_id);
            TwingateError::resource_not_found(resource_id)
        })?;

    let mut clipboard = Clipboard::new().map_err(|e| {
        eprintln!("Error: Failed to access clipboard: {}", e);
        e
    })?;

    let address = get_address_from_resource(&n.resources[idx]);
    clipboard.set_text(address).map_err(|e| {
        eprintln!("Error: Failed to copy address to clipboard: {}", e);
        e
    })?;

    println!("Successfully copied address to clipboard: {}", address);
    Ok(())
}

async fn handle_open_in_browser(app_handle: &AppHandle, resource_id: &str) -> Result<()> {
    // Use NetworkDataManager to get network data with caching
    let network_manager = NetworkDataManager::new(app_handle, std::time::Duration::from_secs(30));
    let n = network_manager.get_network_or_error().await?;

    let resource = n
        .resources
        .iter()
        .find(|x| x.id == resource_id)
        .ok_or_else(|| {
            eprintln!("Error: Resource not found: {}", resource_id);
            TwingateError::resource_not_found(resource_id)
        })?;

    let open_url = get_open_url_from_resource(resource).ok_or_else(|| {
        eprintln!("Error: Resource does not support opening in browser: {}", resource_id);
        TwingateError::invalid_resource_id(resource_id)
    })?;

    println!("Opening URL in browser: {}", open_url);
    
    match tauri_plugin_opener::open_url(open_url, None::<String>) {
        Ok(_) => {
            println!("Successfully opened URL in browser: {}", open_url);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: Failed to open URL in browser: {}", e);
            Err(TwingateError::from(e))
        }
    }
}

async fn handle_open_auth_url(app_handle: &AppHandle) -> Result<()> {
    let auth_url = StateManager::get_auth_url(app_handle);

    if let Some(url) = auth_url {
        println!("Opening authentication URL: {}", url);
        
        match tauri_plugin_opener::open_url(url.clone(), None::<String>) {
            Ok(_) => {
                println!("Successfully opened authentication URL in browser");
                Ok(())
            }
            Err(e) => {
                eprintln!("Error: Failed to open authentication URL: {}", e);
                Err(TwingateError::from(e))
            }
        }
    } else {
        eprintln!("Error: No authentication URL available");
        Err(TwingateError::ServiceNotRunning)
    }
}

async fn handle_copy_auth_url(app_handle: &AppHandle) -> Result<()> {
    let auth_url = StateManager::get_auth_url(app_handle);

    if let Some(url) = auth_url {
        let mut clipboard = Clipboard::new().map_err(|e| {
            eprintln!("Error: Failed to access clipboard: {}", e);
            e
        })?;

        clipboard.set_text(url.clone()).map_err(|e| {
            eprintln!("Error: Failed to copy authentication URL to clipboard: {}", e);
            e
        })?;

        println!("Successfully copied authentication URL to clipboard: {}", url);
        Ok(())
    } else {
        eprintln!("Error: No authentication URL available");
        Err(TwingateError::ServiceNotRunning)
    }
}



async fn handle_menu_action(app_handle: &AppHandle, action: MenuAction) -> Result<()> {
    match action {
        MenuAction::Quit => {
            println!("Quit menu item clicked - exiting application");
            app_handle.exit(0);
        }
        MenuAction::StartService => {
            println!("Starting Twingate service...");
            let executor = CommandExecutor::new(app_handle);
            match executor.execute_twingate_elevated(&["start"]).await {
                Ok(output) => {
                    println!("Successfully started Twingate service");
                    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    
                    // Check if authentication is required and handle it
                    log::debug!("Checking if service requires authentication");
                    match handle_service_auth(app_handle).await {
                        Ok(_) => {
                            // Check if we're now in authenticating state
                            let is_authenticating = StateManager::with_state(app_handle, |state| {
                                matches!(state.service_status(), crate::state::ServiceStatus::Authenticating(_))
                            });
                            
                            // Only call rebuild_tray_after_delay if not authenticating
                            // (if authenticating, the tray was already rebuilt immediately)
                            if !is_authenticating {
                                TrayManager::rebuild_tray_after_delay(app_handle.clone());
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to handle service authentication: {}", e);
                            eprintln!("Warning: Failed to handle service authentication: {}", e);
                            // Don't return error here - service is started, just auth failed
                            TrayManager::rebuild_tray_after_delay(app_handle.clone());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute start service command: {}", e);
                    return Err(e);
                }
            }
        }
        MenuAction::StopService => {
            println!("Stopping Twingate service...");
            let executor = CommandExecutor::new(app_handle);
            match executor.execute_twingate_elevated(&["stop"]).await {
                Ok(output) => {
                    println!("Successfully stopped Twingate service");
                    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    TrayManager::rebuild_tray_after_delay(app_handle.clone());
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute stop service command: {}", e);
                    return Err(e);
                }
            }
        }

        MenuAction::CopyAddress(resource_id) => {
            println!("Copying address for resource: {}", resource_id);
            let address_id = format!("{}-{}", COPY_ADDRESS_ID, resource_id);
            handle_copy_address(app_handle, &address_id).await?;
        }
        MenuAction::Authenticate(resource_id) => {
            println!("Starting authentication for resource: {}", resource_id);
            let auth_id = format!("{}-{}", AUTHENTICATE_ID, resource_id);
            start_resource_auth(app_handle, &auth_id)
                .await
                .map_err(|e| {
                    eprintln!(
                        "Error: Failed to start authentication for resource {}: {}",
                        resource_id, e
                    );
                    e
                })?;
        }
        MenuAction::OpenInBrowser(resource_id) => {
            println!("Opening resource in browser: {}", resource_id);
            handle_open_in_browser(app_handle, &resource_id).await?;
        }
        MenuAction::OpenAuthUrl => {
            println!("Opening authentication URL...");
            handle_open_auth_url(app_handle).await?;
        }
        MenuAction::CopyAuthUrl => {
            println!("Copying authentication URL to clipboard...");
            handle_copy_auth_url(app_handle).await?;
        }
        MenuAction::Unknown(event_id) => {
            eprintln!("Warning: Unhandled menu item: {}", event_id);
        }
    }
    Ok(())
}

fn create_menu_event_handler(builder: TrayIconBuilder<tauri::Wry>) -> TrayIconBuilder<tauri::Wry> {
    builder.on_menu_event(|app, event| {
        let event_id = event.id.clone();
        let app_handle = app.app_handle().clone();
        let action = MenuAction::from_event_id(event_id.as_ref());

        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle_menu_action(&app_handle, action).await {
                eprintln!(
                    "Error: Failed to handle menu action for event '{:?}': {}",
                    event_id, e
                );
            }
        });
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(single_instance_init(|_app, _argv, _cwd| {
            println!("Second instance attempted - ignoring");
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppStateType::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            println!("Initializing Twingate Linux application...");
            log::info!("Starting Twingate Linux application setup");

            let app_handle = app.app_handle().clone();
            
            log::debug!("Attempting to retrieve initial network data");
            
            // Allow more time for service initialization during startup
            let network_data = tauri::async_runtime::block_on(async {
                // Initial delay to allow service to start if it was just launched
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                
                log::debug!("Starting initial network data retrieval with extended timeout");
                
                // Use extended retry count for initial startup
                match get_network_data_with_retry(&app_handle, 10).await {
                    Ok(data) => {
                        // Initialize state with network data
                        StateManager::update_network(&app_handle, data.clone());

                        match &data {
                            Some(network) => {
                                log::debug!(
                                    "Successfully connected to Twingate during startup - User: {}",
                                    network.user.email
                                );
                                log::debug!("Found {} resources", network.resources.len());
                                println!(
                                    "Successfully connected to Twingate - User: {}",
                                    network.user.email
                                );
                                println!("Found {} resources", network.resources.len());
                            }
                            None => {
                                log::debug!(
                                    "Twingate service is not running - will show disconnected menu"
                                );
                                println!(
                                    "Twingate service is not running - will show disconnected menu"
                                );
                            }
                        }
                        data
                    }
                    Err(e) => {
                        log::warn!("Failed to get network data during startup: {}", e);
                        log::debug!("Application will start with disconnected menu and retry in background");
                        eprintln!("Warning: Failed to get network data during setup: {}", e);
                        eprintln!("Application will start with disconnected menu");
                        
                        // Initialize state with no network data
                        StateManager::update_network(&app_handle, None);
                        
                        // Schedule background retry for network data
                        let retry_app_handle = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            log::debug!("Starting background network data retry");
                            tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
                            
                            match get_network_data_with_retry(&retry_app_handle, 5).await {
                                Ok(Some(network)) => {
                                    log::debug!("Background retry successful - updating state and rebuilding tray");
                                    let retry_app_handle_clone = retry_app_handle.clone();
                                    {
                                        StateManager::update_network(&retry_app_handle, Some(network));
                                    }
                                    
                                    // Rebuild tray with new data
                                    TrayManager::rebuild_tray_after_delay(retry_app_handle_clone);
                                }
                                Ok(None) => {
                                    log::debug!("Background retry confirmed service not running");
                                }
                                Err(e) => {
                                    log::warn!("Background network data retry failed: {}", e);
                                }
                            }
                        });
                        
                        None
                    }
                }
            });

            log::debug!("Building initial tray menu");
            let menu = match tauri::async_runtime::block_on(build_tray_menu(app.app_handle(), network_data)) {
                Ok(m) => {
                    log::debug!("Successfully built initial tray menu");
                    m
                }
                Err(e) => {
                    log::error!("Failed to build initial tray menu: {}", e);
                    eprintln!("Error: Failed to build initial tray menu: {}", e);
                    // Create a minimal fallback menu
                    log::debug!("Creating minimal fallback menu");
                    let fallback_result = tauri::async_runtime::block_on(async {
                        build_disconnected_menu(app.app_handle()).await
                    });
                    
                    match fallback_result {
                        Ok(m) => m,
                        Err(e2) => {
                            log::error!("Failed to build even fallback menu: {}", e2);
                            eprintln!("Critical: Failed to build fallback menu: {}", e2);
                            return Err(Box::new(tauri::Error::InvalidIcon(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Failed to build fallback menu: {}", e2),
                            ))));
                        }
                    }
                }
            };

            log::debug!("Getting default window icon");
            let icon = app
                .default_window_icon()
                .ok_or_else(|| {
                    log::error!("No default window icon found");
                    eprintln!("Error: No default window icon found");
                    tauri::Error::InvalidIcon(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No default window icon",
                    ))
                })?
                .clone();

            log::debug!("Building tray icon");
            let tray_builder = TrayIconBuilder::with_id(TWINGATE_TRAY_ID)
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(true);

            let tray_builder = create_menu_event_handler(tray_builder);

            match tray_builder.build(app) {
                Ok(_) => {
                    log::info!("Successfully created tray icon");
                    println!("Twingate Linux application initialized successfully");
                }
                Err(e) => {
                    log::error!("Failed to build tray icon: {}", e);
                    eprintln!("Error: Failed to build tray icon: {}", e);
                    return Err(Box::new(e));
                }
            }

            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                    println!("Development tools opened");
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
