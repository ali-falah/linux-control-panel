use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallState {
    pub is_running: bool,
    pub is_panic: bool,
    pub active_zones: Vec<String>,
    pub default_zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRules {
    pub zone: String,
    pub services: Vec<String>,
    pub ports: Vec<String>,
}

#[tauri::command]
pub async fn get_firewall_state() -> Result<FirewallState, String> {
    if !crate::binary_exists("firewall-cmd").await {
        return Err("firewall-cmd is not available".to_string());
    }

    let state_out = Command::new("firewall-cmd")
        .arg("--state")
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let is_running = String::from_utf8_lossy(&state_out.stdout).trim() == "running";

    if !is_running {
        return Ok(FirewallState {
            is_running: false,
            is_panic: false,
            active_zones: vec![],
            default_zone: String::new(),
        });
    }

    let panic_out = Command::new("firewall-cmd")
        .arg("--query-panic")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let is_panic = String::from_utf8_lossy(&panic_out.stdout).trim() == "yes";

    let default_zone = Command::new("firewall-cmd")
        .arg("--get-default-zone")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let active_zones_out = Command::new("firewall-cmd")
        .arg("--get-active-zones")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut active_zones = Vec::new();
    for line in active_zones_out.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            active_zones.push(line.trim().to_string());
        }
    }

    if active_zones.is_empty() && !default_zone.is_empty() {
        active_zones.push(default_zone.clone());
    }

    Ok(FirewallState {
        is_running,
        is_panic,
        active_zones,
        default_zone,
    })
}

#[tauri::command]
pub async fn get_zone_rules(zone: String) -> Result<ZoneRules, String> {
    let services_out = Command::new("firewall-cmd")
        .args(["--zone", &zone, "--list-services"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let services: Vec<String> = String::from_utf8_lossy(&services_out.stdout)
        .trim()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let ports_out = Command::new("firewall-cmd")
        .args(["--zone", &zone, "--list-ports"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let ports: Vec<String> = String::from_utf8_lossy(&ports_out.stdout)
        .trim()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    Ok(ZoneRules {
        zone,
        services,
        ports,
    })
}

#[tauri::command]
pub async fn modify_firewall_rule(
    zone: String,
    rule_type: String, // "service" or "port"
    value: String,     // e.g. "http" or "8080/tcp"
    add: bool,
) -> Result<String, String> {
    let action = if add { "--add" } else { "--remove" };
    let arg = format!("{}-{}={}", action, rule_type, value);

    // Apply to runtime
    let out1 = Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", "--zone", &zone, &arg])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {e}"))?;

    if !out1.status.success() {
        return Err(String::from_utf8_lossy(&out1.stderr).to_string());
    }

    // Apply permanently
    Command::new("pkexec")
        .args([
            "/usr/bin/firewall-cmd",
            "--zone",
            &zone,
            &arg,
            "--permanent",
        ])
        .output()
        .await
        .map_err(|e| format!("pkexec permanent failed: {e}"))?;

    Ok(format!(
        "Successfully {}ed {} {}",
        if add { "add" } else { "remov" },
        rule_type,
        value
    ))
}

#[tauri::command]
pub async fn toggle_panic_mode(enable: bool) -> Result<String, String> {
    let arg = if enable { "--panic-on" } else { "--panic-off" };
    let output = Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", arg])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!(
        "Panic mode turned {}",
        if enable { "ON" } else { "OFF" }
    ))
}
