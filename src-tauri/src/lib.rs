use arboard::Clipboard;
use tauri::{tray::TrayIconBuilder, AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_single_instance::init as single_instance_init;

mod auth;
mod commands;
mod error;
mod models;
mod network;
mod state;
mod tray;

use auth::{handle_service_auth, start_resource_auth};
use commands::greet;
use error::{Result, TwingateError};
use network::{get_network_data, get_network_data_with_retry};
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

    // Check if refresh is needed and get current network data
    let state = app_handle.state::<AppStateType>();
    let (needs_refresh, current_network_data) = {
        let state_guard = state.lock().unwrap();
        (
            state_guard.should_refresh(std::time::Duration::from_secs(30)),
            state_guard.network().cloned(),
        )
    };

    // Get network data - refresh if needed, otherwise use cached
    let network_data = if needs_refresh {
        // Refresh network data without holding any locks
        match get_network_data(app_handle).await {
            Ok(fresh_data) => {
                // Update state with fresh data
                {
                    let mut state_guard = state.lock().unwrap();
                    state_guard.update_network(fresh_data.clone());
                }
                fresh_data
            }
            Err(e) => {
                eprintln!("Error: Failed to refresh network data: {}", e);
                return Err(e);
            }
        }
    } else {
        current_network_data
    };

    let n = network_data.ok_or_else(|| {
        eprintln!("Error: Twingate service is not running");
        TwingateError::ServiceNotRunning
    })?;

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
    // Check if refresh is needed and get current network data
    let state = app_handle.state::<AppStateType>();
    let (needs_refresh, current_network_data) = {
        let state_guard = state.lock().unwrap();
        (
            state_guard.should_refresh(std::time::Duration::from_secs(30)),
            state_guard.network().cloned(),
        )
    };

    // Get network data - refresh if needed, otherwise use cached
    let network_data = if needs_refresh {
        // Refresh network data without holding any locks
        match get_network_data(app_handle).await {
            Ok(fresh_data) => {
                // Update state with fresh data
                {
                    let mut state_guard = state.lock().unwrap();
                    state_guard.update_network(fresh_data.clone());
                }
                fresh_data
            }
            Err(e) => {
                eprintln!("Error: Failed to refresh network data: {}", e);
                return Err(e);
            }
        }
    } else {
        current_network_data
    };

    let n = network_data.ok_or_else(|| {
        eprintln!("Error: Twingate service is not running");
        TwingateError::ServiceNotRunning
    })?;

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

fn rebuild_tray_after_delay(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Use longer initial delay during authentication flow
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

        let mut retry_count = 0;
        const MAX_REBUILD_RETRIES: u32 = 3;
        const REBUILD_RETRY_DELAY_MS: u64 = 3000;

        loop {
            log::debug!(
                "Attempting tray rebuild (attempt {} of {})",
                retry_count + 1,
                MAX_REBUILD_RETRIES + 1
            );

            let network_data = match get_network_data(&app_handle).await {
                Ok(data) => {
                    // Update state with fresh data
                    let state = app_handle.state::<AppStateType>();
                    let mut state_guard = state.lock().unwrap();
                    state_guard.update_network(data.clone());

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
                        tokio::time::sleep(std::time::Duration::from_millis(
                            REBUILD_RETRY_DELAY_MS,
                        ))
                        .await;
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
            match build_tray_menu(&app_handle, network_data).await {
                Ok(menu) => match app_handle.tray_by_id(TWINGATE_TRAY_ID) {
                    Some(tray) => {
                        if let Err(e) = tray.set_menu(Some(menu)) {
                            log::error!("Failed to set tray menu: {}", e);
                        } else {
                            log::debug!("Successfully updated tray menu");
                        }
                    }
                    None => {
                        log::error!("Tray icon not found with ID: {}", TWINGATE_TRAY_ID);
                    }
                },
                Err(e) => {
                    log::error!("Failed to build tray menu: {}", e);
                }
            }

            break;
        }
    });
}

async fn handle_menu_action(app_handle: &AppHandle, action: MenuAction) -> Result<()> {
    match action {
        MenuAction::Quit => {
            println!("Quit menu item clicked - exiting application");
            app_handle.exit(0);
        }
        MenuAction::StartService => {
            println!("Starting Twingate service...");
            let shell = app_handle.shell();
            match shell
                .command("pkexec")
                .args(["twingate", "start"])
                .output()
                .await
            {
                Ok(output) => {
                    if output.status.success() {
                        println!("Successfully started Twingate service");
                        println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                        
                        // Check if authentication is required and handle it
                        log::debug!("Checking if service requires authentication");
                        if let Err(e) = handle_service_auth(app_handle).await {
                            log::error!("Failed to handle service authentication: {}", e);
                            eprintln!("Warning: Failed to handle service authentication: {}", e);
                            // Don't return error here - service is started, just auth failed
                        }
                    } else {
                        let error_msg = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "Error: Failed to start Twingate service. Exit code: {:?}",
                            output.status.code()
                        );
                        eprintln!("Error output: {}", error_msg);
                        return Err(TwingateError::command_failed(
                            "twingate start",
                            output.status.code().unwrap_or(-1),
                            error_msg,
                        ));
                    }
                    rebuild_tray_after_delay(app_handle.clone());
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute start service command: {}", e);
                    return Err(TwingateError::from(e));
                }
            }
        }
        MenuAction::StopService => {
            println!("Stopping Twingate service...");
            let shell = app_handle.shell();
            match shell
                .command("pkexec")
                .args(["twingate", "stop"])
                .output()
                .await
            {
                Ok(output) => {
                    if output.status.success() {
                        println!("Successfully stopped Twingate service");
                        println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    } else {
                        let error_msg = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "Error: Failed to stop Twingate service. Exit code: {:?}",
                            output.status.code()
                        );
                        eprintln!("Error output: {}", error_msg);
                        return Err(TwingateError::command_failed(
                            "twingate stop",
                            output.status.code().unwrap_or(-1),
                            error_msg,
                        ));
                    }
                    rebuild_tray_after_delay(app_handle.clone());
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute stop service command: {}", e);
                    return Err(TwingateError::from(e));
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
                        let state = app_handle.state::<AppStateType>();
                        let mut state_guard = state.lock().unwrap();
                        state_guard.update_network(data.clone());

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
                        let state = app_handle.state::<AppStateType>();
                        let mut state_guard = state.lock().unwrap();
                        state_guard.update_network(None);
                        
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
                                        let state = retry_app_handle.state::<AppStateType>();
                                        let mut state_guard = state.lock().unwrap();
                                        state_guard.update_network(Some(network));
                                    }
                                    
                                    // Rebuild tray with new data
                                    rebuild_tray_after_delay(retry_app_handle_clone);
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
