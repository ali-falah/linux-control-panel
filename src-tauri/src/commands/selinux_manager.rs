use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelinuxStatus {
    pub status: String,       // "enabled" or "disabled"
    pub current_mode: String, // "enforcing" or "permissive"
    pub config_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denial {
    pub raw: String,
    pub timestamp: String,
    pub scontext: String,
    pub tcontext: String,
    pub tclass: String,
}

#[tauri::command]
pub async fn get_selinux_status() -> Result<SelinuxStatus, String> {
    if !crate::binary_exists("sestatus").await {
        return Err("sestatus not found. Is SELinux installed?".to_string());
    }

    let output = Command::new("sestatus")
        .output()
        .await
        .map_err(|e| format!("Failed to run sestatus: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut status = String::new();
    let mut current_mode = String::new();
    let mut config_mode = String::new();

    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim().to_string();
            match key {
                "SELinux status" => status = val,
                "Current mode" => current_mode = val,
                "Mode from config file" => config_mode = val,
                _ => {}
            }
        }
    }

    Ok(SelinuxStatus {
        status,
        current_mode,
        config_mode,
    })
}

#[tauri::command]
pub async fn set_selinux_state(mode: String) -> Result<String, String> {
    // mode should be "enforcing", "permissive", or "disabled"

    // Set runtime state if possible
    if mode == "enforcing" || mode == "permissive" {
        let setenforce_val = if mode == "enforcing" { "1" } else { "0" };
        Command::new("pkexec")
            .args(["/usr/sbin/setenforce", setenforce_val])
            .output()
            .await
            .map_err(|e| format!("pkexec setenforce failed: {}", e))?;
    }

    // Set permanent state in /etc/selinux/config
    let script = format!(
        "sed -i 's/^SELINUX=.*/SELINUX={}/' /etc/selinux/config",
        mode
    );
    let output = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!("Successfully set SELinux to {}", mode))
}

#[tauri::command]
pub async fn get_selinux_denials() -> Result<Vec<Denial>, String> {
    // Use ausearch to get AVCs.
    let output = Command::new("pkexec")
        .args(["/usr/sbin/ausearch", "-m", "AVC", "-ts", "recent"])
        .output()
        .await
        .map_err(|e| format!("pkexec ausearch failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut denials = Vec::new();

    // Very naive parsing of ausearch output. Each event is separated by "----"
    for event in stdout.split("----") {
        let event = event.trim();
        if event.is_empty() || event == "<no matches>" {
            continue;
        }

        let mut scontext = String::new();
        let mut tcontext = String::new();
        let mut tclass = String::new();
        let mut timestamp = String::new();

        if let Some(msg_idx) = event.find("msg='audit(") {
            let ts_start = msg_idx + 11;
            if let Some(ts_end) = event[ts_start..].find(':') {
                timestamp = event[ts_start..ts_start + ts_end].to_string();
            }
        }

        if let Some(idx) = event.find("scontext=") {
            let rest = &event[idx + 9..];
            scontext = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        }
        if let Some(idx) = event.find("tcontext=") {
            let rest = &event[idx + 9..];
            tcontext = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        }
        if let Some(idx) = event.find("tclass=") {
            let rest = &event[idx + 7..];
            tclass = rest.split_whitespace().next().unwrap_or("").to_string();
        }

        if !scontext.is_empty() {
            denials.push(Denial {
                raw: event.to_string(),
                timestamp,
                scontext,
                tcontext,
                tclass,
            });
        }
    }

    Ok(denials)
}
