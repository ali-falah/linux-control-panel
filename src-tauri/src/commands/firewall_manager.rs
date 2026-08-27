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

    let state_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .arg("--state")
            .output()
    ).await.map_err(|_| "firewall-cmd --state timed out".to_string())?
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

    let panic_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .arg("--query-panic")
            .output()
    ).await.map_err(|_| "firewall-cmd --query-panic timed out".to_string())?
    .map_err(|e| e.to_string())?;
    let is_panic = String::from_utf8_lossy(&panic_out.stdout).trim() == "yes";

    let default_zone_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .arg("--get-default-zone")
            .output()
    ).await;
    let default_zone = match default_zone_out {
        Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    };

    let active_zones_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .arg("--get-active-zones")
            .output()
    ).await;
    let active_zones_raw = match active_zones_out {
        Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    };

    let mut active_zones = Vec::new();
    for line in active_zones_raw.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            let mut zone = line.trim().to_string();
            if zone.ends_with(" (default)") {
                zone = zone.replace(" (default)", "");
            }
            active_zones.push(zone);
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
    let services_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .args(["--zone", &zone, "--list-services"])
            .output()
    ).await;
    
    let services: Vec<String> = match services_out {
        Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout)
            .trim()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    };

    let ports_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .args(["--zone", &zone, "--list-ports"])
            .output()
    ).await;
    
    let ports: Vec<String> = match ports_out {
        Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout)
            .trim()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    };

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

#[tauri::command]
pub async fn firewall_get_rich_rules(zone: String) -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .args(["--zone", &zone, "--list-rich-rules"])
            .output()
    ).await.map_err(|_| "firewall-cmd --list-rich-rules timed out".to_string())?
    .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let rules = stdout.lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(rules)
}

#[tauri::command]
pub async fn firewall_get_zone_interfaces(zone: String) -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("firewall-cmd")
            .args(["--zone", &zone, "--list-interfaces"])
            .output()
    ).await.map_err(|_| "firewall-cmd --list-interfaces timed out".to_string())?
    .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let interfaces = stdout.trim()
        .split_whitespace()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(interfaces)
}

#[tauri::command]
pub async fn firewall_get_all_interfaces() -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("ip")
            .args(["-o", "link", "show"])
            .output()
    ).await.map_err(|_| "ip link show timed out".to_string())?
    .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1].trim_end_matches(':').to_string();
            if name != "lo" {
                interfaces.push(name);
            }
        }
    }
    Ok(interfaces)
}

#[tauri::command]
pub async fn firewall_modify_rich_rule(
    zone: String,
    rule: String,
    add: bool,
) -> Result<String, String> {
    let arg = format!("--{}-rich-rule={}", if add { "add" } else { "remove" }, rule);

    let out1 = Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", "--zone", &zone, &arg])
        .output()
        .await
        .map_err(|e| format!("pkexec runtime failed: {e}"))?;

    if !out1.status.success() {
        return Err(String::from_utf8_lossy(&out1.stderr).to_string());
    }

    Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", "--zone", &zone, &arg, "--permanent"])
        .output()
        .await
        .map_err(|e| format!("pkexec permanent failed: {e}"))?;

    Ok(format!(
        "Successfully {}ed rich rule '{}'",
        if add { "add" } else { "remov" },
        rule
    ))
}

#[tauri::command]
pub async fn firewall_change_interface_zone(
    zone: String,
    interface: String,
) -> Result<String, String> {
    let out1 = Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", "--zone", &zone, &format!("--change-interface={}", interface), "--permanent"])
        .output()
        .await
        .map_err(|e| format!("pkexec permanent change failed: {e}"))?;

    if !out1.status.success() {
        return Err(String::from_utf8_lossy(&out1.stderr).to_string());
    }

    let out2 = Command::new("pkexec")
        .args(["/usr/bin/firewall-cmd", "--zone", &zone, &format!("--change-interface={}", interface)])
        .output()
        .await
        .map_err(|e| format!("pkexec runtime change failed: {e}"))?;

    if !out2.status.success() {
        return Err(String::from_utf8_lossy(&out2.stderr).to_string());
    }

    Ok(format!(
        "Successfully bound interface {} to zone {}",
        interface, zone
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortListenerInfo {
    pub port: u16,
    pub proto: String,
    pub is_listening: bool,
    pub process_name: String,
    pub pid: String,
}

/// Check if a local process is currently listening on the specified port
#[tauri::command]
pub async fn firewall_check_port_listener(port: u16, proto: String) -> Result<PortListenerInfo, String> {
    let flag = if proto.to_lowercase().contains("udp") { "-ulpn" } else { "-tlpn" };
    let output = Command::new("ss")
        .args([flag, "-H"])
        .output()
        .await;

    let mut is_listening = false;
    let mut process_name = String::new();
    let mut pid = String::new();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let target = format!(":{}", port);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[3].ends_with(&target) {
                is_listening = true;
                if parts.len() >= 6 {
                    let proc_info = parts[5];
                    if let Some(name_start) = proc_info.find("(\"") {
                        if let Some(name_end) = proc_info[name_start + 2..].find('"') {
                            process_name = proc_info[name_start + 2..name_start + 2 + name_end].to_string();
                        }
                    }
                    if let Some(pid_start) = proc_info.find("pid=") {
                        let rest = &proc_info[pid_start + 4..];
                        let pid_end = rest.find(',').unwrap_or(rest.len());
                        pid = rest[..pid_end].to_string();
                    }
                }
                break;
            }
        }
    }

    Ok(PortListenerInfo {
        port,
        proto,
        is_listening,
        process_name,
        pid,
    })
}
