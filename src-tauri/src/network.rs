use crate::error::{Result, TwingateError};
use crate::models::Network;
use serde_json::from_slice;
use std::str;

use tauri_plugin_shell::ShellExt;

pub async fn get_network_data(app_handle: &tauri::AppHandle) -> Result<Option<Network>> {
    let shell = app_handle.shell();

    let status_output = shell.command("twingate").args(["status"]).output().await?;

    let status = std::str::from_utf8(&status_output.stdout).map_err(|_| {
        TwingateError::NetworkDataParseError("Invalid UTF-8 in status output".to_string())
    })?;

    if status.contains("not-running") {
        return Ok(None);
    }

    let resources_output = shell
        .command("twingate-notifier")
        .args(["resources"])
        .output()
        .await?;

    let output_str = str::from_utf8(&resources_output.stdout).map_err(|_| {
        TwingateError::NetworkDataParseError("Invalid UTF-8 in resources output".to_string())
    })?;

    match from_slice(output_str.as_bytes()) {
        Ok(network) => Ok(Some(network)),
        Err(e) => {
            println!("Failed to parse JSON. {}", &output_str);
            Err(TwingateError::JsonParseError(e))
        }
    }
}
