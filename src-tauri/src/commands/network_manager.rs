use crate::utils::privilege::Command;
use crate::utils::privilege::Command as PrivCommand;

#[tauri::command]
pub fn network_get_interfaces() -> Result<String, String> {
    let output = Command::new("ip")
        .args(["-j", "a"])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_get_dns() -> Result<String, String> {
    let output = Command::new("resolvectl")
        .arg("status")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        }
        _ => {
            // Fallback to reading /etc/resolv.conf
            let resolv = std::fs::read_to_string("/etc/resolv.conf").map_err(|e| e.to_string())?;
            Ok(resolv)
        }
    }
}

#[tauri::command]
pub fn network_list_connections() -> Result<String, String> {
    let mut cmd = Command::new("nmcli");
    cmd.args(["-t", "-c", "no", "-f", "UUID,NAME,TYPE,DEVICE,STATE", "connection", "show"]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_get_connection(uuid: String) -> Result<String, String> {
    let mut cmd = Command::new("nmcli");
    cmd.args(["-t", "-c", "no", "-f", "ipv4,ipv6,connection", "connection", "show", &uuid]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_save_connection(uuid: String, settings: std::collections::HashMap<String, String>) -> Result<String, String> {
    let is_new = uuid.is_empty();
    let mut cmd = PrivCommand::new("pkexec");
    cmd.arg("nmcli").arg("connection");
    if is_new {
        cmd.arg("add");
    } else {
        cmd.arg("modify").arg(&uuid);
    }
    
    for (k, v) in settings {
        cmd.arg(&k).arg(&v);
    }
    
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_delete_connection(uuid: String) -> Result<String, String> {
    let mut cmd = PrivCommand::new("pkexec");
    cmd.arg("nmcli");
    cmd.args(["connection", "delete", &uuid]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_up_connection(uuid: String) -> Result<String, String> {
    let mut cmd = PrivCommand::new("pkexec");
    cmd.arg("nmcli");
    cmd.args(["connection", "up", &uuid]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_down_connection(uuid: String) -> Result<String, String> {
    let mut cmd = PrivCommand::new("pkexec");
    cmd.arg("nmcli");
    cmd.args(["connection", "down", &uuid]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn network_set_interface_state(iface: String, up: bool) -> Result<String, String> {
    let mut cmd = PrivCommand::new("pkexec");
    cmd.arg("ip");
    cmd.args(["link", "set", "dev", &iface, if up { "up" } else { "down" }]);
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
