use crate::error::{Result, TwingateError};
use crate::network::get_network_data;
use std::str;
use tauri_plugin_shell::ShellExt;

pub async fn start_resource_auth(app_handle: &tauri::AppHandle, auth_id: &str) -> Result<()> {
    let resource_id = auth_id
        .split("-")
        .last()
        .ok_or_else(|| TwingateError::InvalidResourceState("Invalid auth ID format".to_string()))?;

    let n = get_network_data(app_handle)
        .await?
        .ok_or(TwingateError::ServiceNotRunning)?;

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .ok_or_else(|| TwingateError::ResourceNotFound(resource_id.to_string()))?;

    let shell = app_handle.shell();
    shell
        .command("pkexec")
        .args(["twingate", "auth", &n.resources[idx].name])
        .spawn()?;

    Ok(())
}
