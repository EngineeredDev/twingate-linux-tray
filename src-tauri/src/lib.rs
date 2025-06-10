use std::{process::Command, str};

use arboard::Clipboard;
use regex::Regex;
use serde::Deserialize;
use serde_json::from_slice;
use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Deserialize)]
pub struct Network {
    pub admin_url: String,
    pub full_tunnel_time_limit: u64,
    pub internet_security: InternetSecurity,
    pub resources: Vec<Resource>,
    pub user: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InternetSecurity {
    pub mode: i32,
    pub status: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resource {
    pub address: String,
    pub admin_url: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    pub auth_expires_at: i64,
    pub auth_flow_id: String,
    pub auth_state: String,
    pub can_open_in_browser: bool,
    pub client_visibility: i32,
    pub id: String,
    pub name: String,
    pub open_url: String,
    #[serde(rename = "type")]
    pub resource_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Alias {
    pub address: String,
    pub open_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub avatar_url: String,
    pub email: String,
    pub first_name: String,
    pub id: String,
    pub is_admin: bool,
    pub last_name: String,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

const TWINGATE_TRAY_ID: &str = "twingate_tray";
const USER_STATUS_ID: &str = "user_status";
const START_SERVICE_ID: &str = "start_service";
const STOP_SERVICE_ID: &str = "stop_service";
const NUMBER_RESOURCES_ID: &str = "num_resources";
const RESOURCE_ADDRESS_ID: &str = "resource_address";
const COPY_ADDRESS_ID: &str = "copy_address";
const AUTHENTICATE_ID: &str = "authenticate";
const QUIT_ID: &str = "quit";

fn start_resource_auth(auth_id: &str) {
    let resource_id = auth_id.split("-").last().unwrap();

    let n = get_network_data().unwrap();

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .unwrap();

    // TODO: what do we do if pkexec isn't there?
    Command::new("pkexec")
        .args(["twingate", "auth", &n.resources[idx].name])
        .spawn()
        .unwrap();
}

fn get_address_from_resource(resource: &Resource) -> &String {
    resource
        .alias
        .as_ref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&resource.address)
}

fn build_resource_menu(resource: &Resource, app: &AppHandle) -> Submenu<tauri::Wry> {
    let submenu = Submenu::with_id(app, &resource.id, &resource.name, true).unwrap();

    let address_to_use = get_address_from_resource(&resource);

    submenu
        .append(
            &MenuItem::with_id(
                app,
                format!("{}-{}", RESOURCE_ADDRESS_ID, &resource.id),
                &address_to_use,
                false,
                None::<&str>,
            )
            .unwrap(),
        )
        .unwrap();

    submenu
        .append(
            &MenuItem::with_id(
                app,
                format!("{}-{}", COPY_ADDRESS_ID, &resource.id),
                "Copy Address",
                true,
                None::<&str>,
            )
            .unwrap(),
        )
        .unwrap();

    submenu
        .append(&PredefinedMenuItem::separator(app).unwrap())
        .unwrap();

    let auth_menu_items = build_auth_menu(resource, app);
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = auth_menu_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<tauri::Wry>)
        .collect();

    submenu.append_items(&refs).unwrap();

    return submenu;
}

fn build_auth_menu(resource: &Resource, app: &AppHandle) -> Vec<MenuItem<tauri::Wry>> {
    match resource.auth_expires_at == 0 {
        true => {
            vec![
                MenuItem::with_id(
                    app,
                    "auth_required",
                    "Authentication Required",
                    false,
                    None::<&str>,
                )
                .unwrap(),
                MenuItem::with_id(
                    app,
                    format!("{}-{}", AUTHENTICATE_ID, &resource.id),
                    "Authenticate...",
                    true,
                    None::<&str>,
                )
                .unwrap(),
            ]
        }
        false => {
            vec![MenuItem::with_id(
                app,
                "resource_auth_header",
                format!(
                    "Auth expires in {} days",
                    // TODO: needs to show hours for between 0 and 1 day left
                    chrono::Duration::milliseconds(resource.auth_expires_at.clone()).num_days()
                ),
                false,
                None::<&str>,
            )
            .unwrap()]
            // TODO: add renew session support?
        }
    }
}


async fn check_auth_flow() -> bool {
    let mut opened_url = false;

    loop {
        let output_str = str::from_utf8(
            &Command::new("twingate-notifier")
                .arg("resources")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap_or_default()
        .to_lowercase();

        match output_str {
            s if s.contains("not-running") => {
                return true;
            }
            ref s if s.contains("authenticating") => {
                let re = Regex::new(
                    r"https?://[\w.-]+(?:\.[\w\.-]+)+[\w\-\._~:/?#[\]@!$&'()*+,;=]+",
                )
                .expect("Failed to compile regex");
                if let Some(caps) = re.captures(&output_str) {
                    if let Some(url) = caps.get(0) {
                        if opened_url == false {
                            let _ = Command::new("xdg-open")
                                .arg(url.as_str())
                                .output()
                                .expect("failed to execute process");
                            opened_url = true;
                        }
                    } else {
                        println!("No URL found.");
                    }
                } else {
                    println!("No URL found.");
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                return true;
            }
            _ => return true,
        }
    }
}
    

fn get_network_data() -> Option<Network> {
    let status_cmd = &Command::new("twingate").arg("status").output().unwrap();
    let status = std::str::from_utf8(&status_cmd.stdout).unwrap();

    // TODO: should check for other status. Just assuming only not-running and online
    if status.contains("not-running") {
        return None;
    }


    tauri::async_runtime::block_on(check_auth_flow());
    

    let output = Command::new("twingate-notifier")
        .arg("resources")
        .output()
        .unwrap();
    let output_str = str::from_utf8(&output.stdout).unwrap_or_default();

    match from_slice(&output_str.as_bytes()) {
        Ok(network) => Some(network),
        Err(_) => {
            println!("Failed to parse JSON. {}", &output_str);
            None
        }
    }
}

fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, tauri::Error> {
    // app.remove_tray_by_id(TWINGATE_TRAY_ID);
    let menu: Menu<tauri::Wry>;
    match get_network_data() {
        Some(n) => {
            let visible_resources: Vec<_> = n
                .resources
                .iter()
                .filter(|r| r.client_visibility != 0)
                .collect();

            let user_status_menu_item = MenuItem::with_id(
                app,
                USER_STATUS_ID,
                n.user.email.clone(),
                true,
                None::<&str>,
            )?;

            let stop_service_menu_item = MenuItem::with_id(
                app,
                STOP_SERVICE_ID,
                "Log Out and Disconnect",
                true,
                None::<&str>,
            )?;

            let total_resources_menu_item = MenuItem::with_id(
                app,
                "resource_total_count",
                format!("{} Resources", visible_resources.len()),
                false,
                None::<&str>,
            )?;

            let quit_menu_item = MenuItem::with_id(app, QUIT_ID, "Close Tray", true, None::<&str>)?;

            let separator_menu_item = PredefinedMenuItem::separator(app)?;

            let mut menu_items: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![&user_status_menu_item];

            let security_mode_menu_item = if n.internet_security.mode > 0 {
                Some(MenuItem::with_id(
                    app,
                    "security_mode",
                    "Security Enabled",
                    false,
                    None::<&str>,
                )?)
            } else {
                None
            };

            if let Some(ref security) = security_mode_menu_item {
                menu_items.push(security);
            }

            menu_items.push(&stop_service_menu_item);
            menu_items.push(&separator_menu_item);

            let resource_submenus: Vec<_> = visible_resources
                .iter()
                .map(|r| build_resource_menu(r, app))
                .collect();

            menu_items.push(&total_resources_menu_item);

            for submenu in &resource_submenus {
                menu_items.push(submenu);
            }

            menu_items.push(&separator_menu_item);

            menu_items.push(&quit_menu_item);

            menu = Menu::with_items(app, &menu_items[..])?;
        }
        None => {
            menu = Menu::with_items(
                app,
                &[&MenuItem::with_id(
                    app,
                    START_SERVICE_ID,
                    "Start Twingate",
                    true,
                    None::<&str>,
                )?],
            )?;
        }
    }

    return Ok(menu);

    // Ok(TrayIconBuilder::with_id(TWINGATE_TRAY_ID)
    //     .menu(&menu)
    //     .show_menu_on_left_click(true))
}

fn handle_copy_address(address_id: &str) {
    let resource_id = address_id.split("-").last().unwrap();

    // TODO: should check for None but technically shouldn't happen
    let n = get_network_data().unwrap();

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .unwrap();

    let mut clipboard = Clipboard::new().unwrap();

    clipboard
        .set_text(get_address_from_resource(&n.resources[idx]))
        .unwrap()
}

fn rebuild_tray_after_delay(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        match build_tray_menu(&app_handle) {
            Ok(menu) => {
                match app_handle.tray_by_id(TWINGATE_TRAY_ID) {
                    Some(tray) => {
                        // TODO: handle error case
                        let _ = tray.set_menu(Some(menu));
                    }
                    None => {
                        // TODO: create entire tray?
                    }
                }
                // Set the same on_menu_event handler
                // let builder = create_menu_event_handler(builder);
                //
                // // Build the new tray
                // if let Err(e) = builder.build(&app_handle) {
                //     eprintln!("Failed to rebuild tray: {}", e);
                // }
            }
            Err(e) => {
                eprintln!("Failed to build tray menu: {}", e);
            }
        }
    });
}

fn create_menu_event_handler(builder: TrayIconBuilder<tauri::Wry>) -> TrayIconBuilder<tauri::Wry> {
    builder.on_menu_event(|app, event| match event.id.as_ref() {
        "quit" => {
            println!("quit menu item was clicked");
            app.exit(0);
        }
        START_SERVICE_ID => {
            let shell = app.shell();
            let output = tauri::async_runtime::block_on(async move {
                shell
                    .command("pkexec")
                    .args(["twingate", "start"])
                    .output()
                    .await
                    .unwrap()
            });
            if output.status.success() {
                println!("Result: {:?}", String::from_utf8(output.stdout));
            } else {
                println!("Exit with code: {}", output.status.code().unwrap());
            }

            rebuild_tray_after_delay(app.app_handle().clone());
        }
        STOP_SERVICE_ID => {
            rebuild_tray_after_delay(app.app_handle().clone());
        }
        address_id if address_id.contains(COPY_ADDRESS_ID) => {
            handle_copy_address(address_id);
        }
        auth_id if auth_id.contains(AUTHENTICATE_ID) => {
            start_resource_auth(auth_id);
        }
        _ => {
            println!("menu item {:?} not handled", event.id);
        }
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            let menu = build_tray_menu(&app.app_handle())?;

            let tray_builder = TrayIconBuilder::with_id(TWINGATE_TRAY_ID)
                .menu(&menu)
                .show_menu_on_left_click(true);

            let tray_builder = create_menu_event_handler(tray_builder);

            tray_builder.build(app)?;

            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
