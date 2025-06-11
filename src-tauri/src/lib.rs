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

use auth::start_resource_auth;
use commands::greet;
use error::{Result, TwingateError};
use network::get_network_data;
use state::{AppState, AppStateType};
use tray::{
    build_tray_menu, get_address_from_resource, MenuAction, AUTHENTICATE_ID, COPY_ADDRESS_ID,
    TWINGATE_TRAY_ID,
};

async fn handle_copy_address(app_handle: &AppHandle, address_id: &str) -> Result<()> {
    let resource_id = address_id.split("-").last().ok_or_else(|| {
        eprintln!("Error: Invalid address ID format: {}", address_id);
        TwingateError::InvalidResourceState("Invalid address ID format".to_string())
    })?;

    // Check if refresh is needed and get current network data
    let state = app_handle.state::<AppStateType>();
    let (needs_refresh, current_network_data) = {
        let state_guard = state.lock().unwrap();
        (
            state_guard.should_refresh(std::time::Duration::from_secs(30)),
            state_guard.get_network().cloned(),
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
            TwingateError::ResourceNotFound(resource_id.to_string())
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

fn rebuild_tray_after_delay(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let network_data = match get_network_data(&app_handle).await {
            Ok(data) => {
                // Update state with fresh data
                let state = app_handle.state::<AppStateType>();
                let mut state_guard = state.lock().unwrap();
                state_guard.update_network(data.clone());
                if data.is_some() {
                    println!("Successfully refreshed network data for tray menu");
                } else {
                    println!("Twingate service is not running - showing disconnected menu");
                }
                data
            }
            Err(e) => {
                eprintln!("Error: Failed to get network data for tray rebuild: {}", e);
                None
            }
        };

        match build_tray_menu(&app_handle, network_data).await {
            Ok(menu) => match app_handle.tray_by_id(TWINGATE_TRAY_ID) {
                Some(tray) => {
                    if let Err(e) = tray.set_menu(Some(menu)) {
                        eprintln!("Error: Failed to set tray menu: {}", e);
                    } else {
                        println!("Successfully updated tray menu");
                    }
                }
                None => {
                    eprintln!("Error: Tray icon not found with ID: {}", TWINGATE_TRAY_ID);
                }
            },
            Err(e) => {
                eprintln!("Error: Failed to build tray menu: {}", e);
            }
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
                    } else {
                        let error_msg = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "Error: Failed to start Twingate service. Exit code: {:?}",
                            output.status.code()
                        );
                        eprintln!("Error output: {}", error_msg);
                        return Err(TwingateError::ShellCommandFailed {
                            code: output.status.code().unwrap_or(-1),
                            message: error_msg.to_string(),
                        });
                    }
                    rebuild_tray_after_delay(app_handle.clone());
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute start service command: {}", e);

                    return Err(TwingateError::ShellPluginError(e));
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
                        return Err(TwingateError::ShellCommandFailed {
                            code: output.status.code().unwrap_or(-1),
                            message: error_msg.to_string(),
                        });
                    }
                    rebuild_tray_after_delay(app_handle.clone());
                }
                Err(e) => {
                    eprintln!("Error: Failed to execute stop service command: {}", e);

                    return Err(TwingateError::ShellPluginError(e));
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

            let app_handle = app.app_handle().clone();
            let network_data = tauri::async_runtime::block_on(async {
                match get_network_data(&app_handle).await {
                    Ok(data) => {
                        // Initialize state with network data
                        let state = app_handle.state::<AppStateType>();
                        let mut state_guard = state.lock().unwrap();
                        state_guard.update_network(data.clone());

                        match &data {
                            Some(network) => {
                                println!(
                                    "Successfully connected to Twingate - User: {}",
                                    network.user.email
                                );
                                println!("Found {} resources", network.resources.len());
                            }
                            None => {
                                println!(
                                    "Twingate service is not running - will show disconnected menu"
                                );
                            }
                        }
                        data
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to get network data during setup: {}", e);
                        eprintln!("Application will start with disconnected menu");
                        None
                    }
                }
            });

            let menu =
                tauri::async_runtime::block_on(build_tray_menu(app.app_handle(), network_data))
                    .map_err(|e| {
                        eprintln!("Error: Failed to build initial tray menu: {}", e);
                        e
                    })?;

            let icon = app
                .default_window_icon()
                .ok_or_else(|| {
                    eprintln!("Error: No default window icon found");
                    tauri::Error::InvalidIcon(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No default window icon",
                    ))
                })?
                .clone();

            let tray_builder = TrayIconBuilder::with_id(TWINGATE_TRAY_ID)
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(true);

            let tray_builder = create_menu_event_handler(tray_builder);

            tray_builder.build(app).map_err(|e| {
                eprintln!("Error: Failed to build tray icon: {}", e);
                e
            })?;

            println!("Twingate Linux application initialized successfully");

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
