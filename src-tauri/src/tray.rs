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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Alias;

    fn create_test_resource() -> Resource {
        Resource {
            address: "192.168.1.100".to_string(),
            admin_url: "https://admin.twingate.com/resource/123".to_string(),
            alias: Some("my-server".to_string()),
            aliases: vec![
                Alias {
                    address: "server.internal".to_string(),
                    open_url: "https://server.internal".to_string(),
                }
            ],
            auth_expires_at: 1640995200,
            auth_flow_id: "flow-123".to_string(),
            auth_state: "authenticated".to_string(),
            can_open_in_browser: true,
            client_visibility: 1,
            id: "resource-123".to_string(),
            name: "My Server".to_string(),
            open_url: "https://server.internal".to_string(),
            resource_type: "tcp".to_string(),
        }
    }

    fn create_test_resource_without_browser() -> Resource {
        Resource {
            address: "192.168.1.101".to_string(),
            admin_url: "https://admin.twingate.com/resource/124".to_string(),
            alias: None,
            aliases: vec![],
            auth_expires_at: 0, // Requires authentication
            auth_flow_id: "flow-124".to_string(),
            auth_state: "not_authenticated".to_string(),
            can_open_in_browser: false,
            client_visibility: 1,
            id: "resource-124".to_string(),
            name: "Database Server".to_string(),
            open_url: "".to_string(),
            resource_type: "tcp".to_string(),
        }
    }

    #[test]
    fn test_menu_action_from_event_id_basic() {
        assert!(matches!(MenuAction::from_event_id(QUIT_ID), MenuAction::Quit));
        assert!(matches!(MenuAction::from_event_id(START_SERVICE_ID), MenuAction::StartService));
        assert!(matches!(MenuAction::from_event_id(STOP_SERVICE_ID), MenuAction::StopService));
        assert!(matches!(MenuAction::from_event_id(OPEN_AUTH_URL_ID), MenuAction::OpenAuthUrl));
        assert!(matches!(MenuAction::from_event_id(COPY_AUTH_URL_ID), MenuAction::CopyAuthUrl));
    }

    #[test]
    fn test_menu_action_from_event_id_copy_address() {
        let event_id = "copy_address-resource-123";
        match MenuAction::from_event_id(event_id) {
            MenuAction::CopyAddress(resource_id) => {
                assert_eq!(resource_id, "123"); // split("-").last() returns the last part
            }
            _ => panic!("Expected CopyAddress action"),
        }
    }

    #[test]
    fn test_menu_action_from_event_id_authenticate() {
        let event_id = "authenticate-resource-456";
        match MenuAction::from_event_id(event_id) {
            MenuAction::Authenticate(resource_id) => {
                assert_eq!(resource_id, "456"); // split("-").last() returns the last part
            }
            _ => panic!("Expected Authenticate action"),
        }
    }

    #[test]
    fn test_menu_action_from_event_id_open_in_browser() {
        let event_id = "open_in_browser-resource-789";
        match MenuAction::from_event_id(event_id) {
            MenuAction::OpenInBrowser(resource_id) => {
                assert_eq!(resource_id, "789"); // split("-").last() returns the last part
            }
            _ => panic!("Expected OpenInBrowser action"),
        }
    }

    #[test]
    fn test_menu_action_from_event_id_unknown() {
        let event_id = "unknown_event_type";
        match MenuAction::from_event_id(event_id) {
            MenuAction::Unknown(id) => {
                assert_eq!(id, "unknown_event_type");
            }
            _ => panic!("Expected Unknown action"),
        }
    }

    #[test]
    fn test_menu_action_debug_format() {
        let action = MenuAction::Quit;
        assert_eq!(format!("{:?}", action), "Quit");

        let action = MenuAction::CopyAddress("test".to_string());
        assert_eq!(format!("{:?}", action), "CopyAddress(\"test\")");
    }

    #[test]
    fn test_menu_action_clone() {
        let action = MenuAction::Authenticate("test-resource".to_string());
        let cloned = action.clone();
        match (action, cloned) {
            (MenuAction::Authenticate(id1), MenuAction::Authenticate(id2)) => {
                assert_eq!(id1, id2);
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_get_address_from_resource_with_alias() {
        let resource = create_test_resource();
        let address = get_address_from_resource(&resource);
        assert_eq!(address, "my-server");
    }

    #[test]
    fn test_get_address_from_resource_without_alias() {
        let resource = create_test_resource_without_browser();
        let address = get_address_from_resource(&resource);
        assert_eq!(address, "192.168.1.101");
    }

    #[test]
    fn test_get_address_from_resource_empty_alias() {
        let mut resource = create_test_resource();
        resource.alias = Some("".to_string()); // Empty alias should fall back to address
        let address = get_address_from_resource(&resource);
        assert_eq!(address, "192.168.1.100");
    }

    #[test]
    fn test_get_open_url_from_resource_with_browser_support() {
        let resource = create_test_resource();
        let url = get_open_url_from_resource(&resource);
        assert_eq!(url, Some(&"https://server.internal".to_string()));
    }

    #[test]
    fn test_get_open_url_from_resource_without_browser_support() {
        let resource = create_test_resource_without_browser();
        let url = get_open_url_from_resource(&resource);
        assert_eq!(url, None);
    }

    #[test]
    fn test_get_open_url_from_resource_empty_aliases() {
        let mut resource = create_test_resource();
        resource.aliases.clear();
        let url = get_open_url_from_resource(&resource);
        assert_eq!(url, None);
    }

    #[test]
    fn test_get_open_url_from_resource_empty_open_url() {
        let mut resource = create_test_resource();
        resource.aliases[0].open_url = "".to_string();
        let url = get_open_url_from_resource(&resource);
        assert_eq!(url, None);
    }

    #[test]
    fn test_constants() {
        assert_eq!(TWINGATE_TRAY_ID, "twingate_tray");
        assert_eq!(USER_STATUS_ID, "user_status");
        assert_eq!(START_SERVICE_ID, "start_service");
        assert_eq!(STOP_SERVICE_ID, "stop_service");
        assert_eq!(RESOURCE_ADDRESS_ID, "resource_address");
        assert_eq!(COPY_ADDRESS_ID, "copy_address");
        assert_eq!(AUTHENTICATE_ID, "authenticate");
        assert_eq!(OPEN_IN_BROWSER_ID, "open_in_browser");
        assert_eq!(OPEN_AUTH_URL_ID, "open_auth_url");
        assert_eq!(COPY_AUTH_URL_ID, "copy_auth_url");
        assert_eq!(QUIT_ID, "quit");
    }

    #[test]
    fn test_menu_action_from_event_id_edge_cases() {
        // Test with malformed IDs
        match MenuAction::from_event_id("copy_address-") {
            MenuAction::CopyAddress(id) => assert_eq!(id, ""),
            _ => panic!("Expected CopyAddress with empty ID"),
        }

        // "authenticate" contains "authenticate" so it will be parsed as Authenticate action
        match MenuAction::from_event_id("authenticate") {
            MenuAction::Authenticate(id) => assert_eq!(id, "authenticate"), // split("-").last() on "authenticate" returns "authenticate"
            _ => panic!("Expected Authenticate action"),
        }

        // Test with empty string
        match MenuAction::from_event_id("") {
            MenuAction::Unknown(id) => assert_eq!(id, ""),
            _ => panic!("Expected Unknown action for empty string"),
        }
    }

    #[test]
    fn test_resource_address_preference() {
        // Test that non-empty alias takes precedence over address
        let mut resource = create_test_resource();
        resource.alias = Some("preferred-name".to_string());
        resource.address = "192.168.1.100".to_string();
        
        let address = get_address_from_resource(&resource);
        assert_eq!(address, "preferred-name");

        // Test that address is used when alias is None
        resource.alias = None;
        let address = get_address_from_resource(&resource);
        assert_eq!(address, "192.168.1.100");
    }

    #[test]
    fn test_browser_url_selection() {
        let mut resource = create_test_resource();
        
        // Add multiple aliases with different open_url values
        resource.aliases = vec![
            Alias {
                address: "first.internal".to_string(),
                open_url: "".to_string(), // Empty - should be skipped
            },
            Alias {
                address: "second.internal".to_string(),
                open_url: "https://second.internal".to_string(), // This should be selected
            },
            Alias {
                address: "third.internal".to_string(),
                open_url: "https://third.internal".to_string(),
            },
        ];

        let url = get_open_url_from_resource(&resource);
        assert_eq!(url, Some(&"https://second.internal".to_string()));
    }

    #[test]
    fn test_menu_action_resource_id_extraction() {
        // Test complex resource IDs
        let complex_id = "copy_address-very-long-resource-id-with-many-dashes";
        match MenuAction::from_event_id(complex_id) {
            MenuAction::CopyAddress(resource_id) => {
                assert_eq!(resource_id, "dashes"); // Should get the last part after split
            }
            _ => panic!("Expected CopyAddress action"),
        }

        // Test single dash
        let single_dash = "authenticate-single";
        match MenuAction::from_event_id(single_dash) {
            MenuAction::Authenticate(resource_id) => {
                assert_eq!(resource_id, "single");
            }
            _ => panic!("Expected Authenticate action"),
        }
    }
}
