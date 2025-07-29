use crate::error::Result;
use crate::models::{Network, Resource};
use crate::state::{AppState, ServiceStatus};
use std::sync::Mutex;
use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager,
};

#[derive(Debug, Clone)]
pub enum MenuAction {
    StartService,
    StopService,
    CopyAddress(String),
    Authenticate(String),
    OpenInBrowser(String),
    OpenAuthUrl,
    CopyAuthUrl,
    Quit,
    Unknown(String),
}

impl MenuAction {
    pub fn from_event_id(event_id: &str) -> Self {
        match event_id {
            QUIT_ID => MenuAction::Quit,
            START_SERVICE_ID => MenuAction::StartService,
            STOP_SERVICE_ID => MenuAction::StopService,
            OPEN_AUTH_URL_ID => MenuAction::OpenAuthUrl,
            COPY_AUTH_URL_ID => MenuAction::CopyAuthUrl,
            id if id.contains(COPY_ADDRESS_ID) => {
                let resource_id = id.split("-").last().unwrap_or_default();
                MenuAction::CopyAddress(resource_id.to_string())
            }
            id if id.contains(AUTHENTICATE_ID) => {
                let resource_id = id.split("-").last().unwrap_or_default();
                MenuAction::Authenticate(resource_id.to_string())
            }
            id if id.contains(OPEN_IN_BROWSER_ID) => {
                let resource_id = id.split("-").last().unwrap_or_default();
                MenuAction::OpenInBrowser(resource_id.to_string())
            }
            _ => MenuAction::Unknown(event_id.to_string()),
        }
    }
}

pub const TWINGATE_TRAY_ID: &str = "twingate_tray";
pub const USER_STATUS_ID: &str = "user_status";
pub const START_SERVICE_ID: &str = "start_service";
pub const STOP_SERVICE_ID: &str = "stop_service";
pub const RESOURCE_ADDRESS_ID: &str = "resource_address";
pub const COPY_ADDRESS_ID: &str = "copy_address";
pub const AUTHENTICATE_ID: &str = "authenticate";
pub const OPEN_IN_BROWSER_ID: &str = "open_in_browser";
pub const OPEN_AUTH_URL_ID: &str = "open_auth_url";
pub const COPY_AUTH_URL_ID: &str = "copy_auth_url";
pub const QUIT_ID: &str = "quit";

pub fn get_address_from_resource(resource: &Resource) -> &String {
    resource
        .alias
        .as_ref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&resource.address)
}

pub fn get_open_url_from_resource(resource: &Resource) -> Option<&String> {
    if !resource.can_open_in_browser {
        return None;
    }
    
    resource
        .aliases
        .iter()
        .find(|alias| !alias.open_url.is_empty())
        .map(|alias| &alias.open_url)
}

pub fn build_resource_menu(resource: &Resource, app: &AppHandle) -> Result<Submenu<tauri::Wry>> {
    let submenu = Submenu::with_id(app, &resource.id, &resource.name, true)?;

    let address_to_use = get_address_from_resource(resource);

    submenu.append(&MenuItem::with_id(
        app,
        format!("{}-{}", RESOURCE_ADDRESS_ID, &resource.id),
        address_to_use,
        false,
        None::<&str>,
    )?)?;

    submenu.append(&MenuItem::with_id(
        app,
        format!("{}-{}", COPY_ADDRESS_ID, &resource.id),
        "Copy Address",
        true,
        None::<&str>,
    )?)?;

    // Add "Open in Browser" menu item if resource supports it
    if let Some(_open_url) = get_open_url_from_resource(resource) {
        submenu.append(&MenuItem::with_id(
            app,
            format!("{}-{}", OPEN_IN_BROWSER_ID, &resource.id),
            "Open in Browser...",
            true,
            None::<&str>,
        )?)?;
    }

    submenu.append(&PredefinedMenuItem::separator(app)?)?;

    let auth_menu_items = build_auth_menu(resource, app)?;
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = auth_menu_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<tauri::Wry>)
        .collect();

    submenu.append_items(&refs)?;

    Ok(submenu)
}

pub fn build_auth_menu(resource: &Resource, app: &AppHandle) -> Result<Vec<MenuItem<tauri::Wry>>> {
    match resource.auth_expires_at == 0 {
        true => Ok(vec![
            MenuItem::with_id(
                app,
                "auth_required",
                "Authentication Required",
                false,
                None::<&str>,
            )?,
            MenuItem::with_id(
                app,
                format!("{}-{}", AUTHENTICATE_ID, &resource.id),
                "Authenticate...",
                true,
                None::<&str>,
            )?,
        ]),
        false => Ok(vec![MenuItem::with_id(
            app,
            "resource_auth_header",
            format!(
                "Auth expires in {} days",
                chrono::Duration::milliseconds(resource.auth_expires_at).num_days()
            ),
            false,
            None::<&str>,
        )?]),
    }
}

pub async fn build_tray_menu(
    app: &AppHandle,
    network_data: Option<Network>,
) -> Result<Menu<tauri::Wry>> {
    // Check application state to determine if we're in authenticating mode
    let service_status = {
        let app_state = app.state::<Mutex<AppState>>();
        let state_guard = app_state.lock().unwrap();
        state_guard.service_status().clone()
    };
    
    match service_status {
        ServiceStatus::Authenticating(auth_url) => build_authenticating_menu(app, &auth_url).await,
        _ => match network_data {
            Some(n) => build_connected_menu(app, &n).await,
            None => build_disconnected_menu(app).await,
        }
    }
}

async fn build_connected_menu(app: &AppHandle, network: &Network) -> Result<Menu<tauri::Wry>> {
    let visible_resources: Vec<_> = network
        .resources
        .iter()
        .filter(|r| r.client_visibility != 0)
        .collect();

    let mut menu_items: Vec<&dyn IsMenuItem<tauri::Wry>> = Vec::new();

    // User status section
    let user_status_items = build_user_status_section(app, network)?;
    for item in &user_status_items {
        menu_items.push(item);
    }

    // Separator
    let separator = PredefinedMenuItem::separator(app)?;
    menu_items.push(&separator);

    // Resources section
    let (resource_count_item, resource_submenus) =
        build_resources_section(app, &visible_resources)?;
    menu_items.push(&resource_count_item);

    for submenu in &resource_submenus {
        menu_items.push(submenu);
    }

    // Final separator and quit
    menu_items.push(&separator);
    let quit_item = MenuItem::with_id(app, QUIT_ID, "Close Tray", true, None::<&str>)?;
    menu_items.push(&quit_item);

    Ok(Menu::with_items(app, &menu_items[..])?)
}

pub async fn build_disconnected_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>> {
    let start_item =
        MenuItem::with_id(app, START_SERVICE_ID, "Start Twingate", true, None::<&str>)?;
    
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "Close Tray", true, None::<&str>)?;

    Ok(Menu::with_items(app, &[&start_item, &separator, &quit_item])?)
}

pub async fn build_authenticating_menu(app: &AppHandle, _auth_url: &str) -> Result<Menu<tauri::Wry>> {
    let auth_status = MenuItem::with_id(
        app,
        "auth_status",
        "Authenticating...",
        false,
        None::<&str>,
    )?;

    let separator1 = PredefinedMenuItem::separator(app)?;

    let open_auth_url_item = MenuItem::with_id(
        app,
        OPEN_AUTH_URL_ID,
        "Open Authentication URL",
        true,
        None::<&str>,
    )?;

    let copy_auth_url_item = MenuItem::with_id(
        app,
        COPY_AUTH_URL_ID,
        "Copy Authentication URL",
        true,
        None::<&str>,
    )?;

    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "Close Tray", true, None::<&str>)?;

    Ok(Menu::with_items(app, &[
        &auth_status,
        &separator1,
        &open_auth_url_item,
        &copy_auth_url_item,
        &separator2,
        &quit_item
    ])?)
}

fn build_user_status_section(
    app: &AppHandle,
    network: &Network,
) -> Result<Vec<MenuItem<tauri::Wry>>> {
    let mut items = Vec::new();

    let user_status_item = MenuItem::with_id(
        app,
        USER_STATUS_ID,
        network.user.email.clone(),
        true,
        None::<&str>,
    )?;
    items.push(user_status_item);

    if network.internet_security.mode > 0 {
        let security_item = MenuItem::with_id(
            app,
            "security_mode",
            "Security Enabled",
            false,
            None::<&str>,
        )?;
        items.push(security_item);
    }

    let stop_service_item = MenuItem::with_id(
        app,
        STOP_SERVICE_ID,
        "Log Out and Disconnect",
        true,
        None::<&str>,
    )?;
    items.push(stop_service_item);

    Ok(items)
}

fn build_resources_section(
    app: &AppHandle,
    visible_resources: &[&Resource],
) -> Result<(MenuItem<tauri::Wry>, Vec<Submenu<tauri::Wry>>)> {
    let total_resources_item = MenuItem::with_id(
        app,
        "resource_total_count",
        format!("{} Resources", visible_resources.len()),
        false,
        None::<&str>,
    )?;

    let resource_submenus: Result<Vec<_>> = visible_resources
        .iter()
        .map(|r| build_resource_menu(r, app))
        .collect();
    let resource_submenus = resource_submenus?;

    Ok((total_resources_item, resource_submenus))
}
