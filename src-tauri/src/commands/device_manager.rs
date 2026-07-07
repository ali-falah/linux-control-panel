use crate::utils::privilege::Command;

#[tauri::command]
pub fn device_get_all() -> Result<String, String> {
    // Run lshw without root to avoid annoying polkit prompts on page load.
    // It still provides great information for a device manager.
    let output = Command::new("lshw")
        .arg("-json")
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
