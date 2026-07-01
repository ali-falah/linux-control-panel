use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdUnit {
    pub name: String,
    pub unit_type: String, // "service", "timer", etc.
    pub state: String,     // "enabled", "disabled", "masked", "static"
    pub scope: String,     // "system" or "user"
    pub vendor_preset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutostartEntry {
    pub name: String,
    pub exec: String,
    pub comment: String,
    pub enabled: bool,
    pub file_path: String,
    pub icon: Option<String>,
}

/// List systemd unit files (both user and system)
#[tauri::command]
pub async fn list_systemd_units() -> Result<Vec<SystemdUnit>, String> {
    if !binary_exists("systemctl").await {
        return Err("systemctl is not available on this system".to_string());
    }

    let mut units = Vec::new();

    // System units
    let system_output = Command::new("systemctl")
        .args([
            "list-unit-files",
            "--type=service",
            "--no-pager",
            "--no-legend",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run systemctl: {e}"))?;

    if system_output.status.success() {
        let stdout = String::from_utf8_lossy(&system_output.stdout).to_string();
        parse_unit_files(&stdout, "system", &mut units);
    }

    // User units
    let user_output = Command::new("systemctl")
        .args([
            "--user",
            "list-unit-files",
            "--type=service",
            "--no-pager",
            "--no-legend",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run systemctl --user: {e}"))?;

    if user_output.status.success() {
        let stdout = String::from_utf8_lossy(&user_output.stdout).to_string();
        parse_unit_files(&stdout, "user", &mut units);
    }

    Ok(units)
}

fn parse_unit_files(output: &str, scope: &str, units: &mut Vec<SystemdUnit>) {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let unit_file = parts[0];
        let state = parts[1].to_string();
        let vendor_preset = parts.get(2).unwrap_or(&"").to_string();

        // Extract unit type from the filename
        let unit_type = if unit_file.ends_with(".service") {
            "service"
        } else if unit_file.ends_with(".timer") {
            "timer"
        } else if unit_file.ends_with(".socket") {
            "socket"
        } else {
            "other"
        };

        // Skip non-service types for the startup manager
        let name = unit_file.to_string();

        units.push(SystemdUnit {
            name,
            unit_type: unit_type.to_string(),
            state,
            scope: scope.to_string(),
            vendor_preset,
        });
    }
}

/// List XDG autostart entries from ~/.config/autostart/
#[tauri::command]
pub async fn list_autostart_entries() -> Result<Vec<AutostartEntry>, String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let autostart_dir = home.join(".config/autostart");

    if !autostart_dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();

    let dir_entries =
        fs::read_dir(&autostart_dir).map_err(|e| format!("Failed to read autostart dir: {e}"))?;

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut name = String::new();
        let mut exec = String::new();
        let mut comment = String::new();
        let mut enabled = true;
        let mut icon: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if let Some((key, val)) = trimmed.split_once('=') {
                match key.trim() {
                    "Name" => name = val.trim().to_string(),
                    "Exec" => exec = val.trim().to_string(),
                    "Comment" => comment = val.trim().to_string(),
                    "X-GNOME-Autostart-enabled" => enabled = val.trim() != "false",
                    "Hidden" => {
                        if val.trim() == "true" {
                            enabled = false
                        }
                    }
                    "Icon" => icon = Some(val.trim().to_string()),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
        }

        entries.push(AutostartEntry {
            name,
            exec,
            comment,
            enabled,
            file_path: path.to_string_lossy().to_string(),
            icon,
        });
    }

    Ok(entries)
}

/// Enable or disable a systemd service unit
#[tauri::command]
pub async fn toggle_service_unit(name: String, enabled: bool, scope: String) -> Result<(), String> {
    if !binary_exists("systemctl").await {
        return Err("systemctl is not available".to_string());
    }

    let action = if enabled { "enable" } else { "disable" };

    let mut args = vec!["systemctl"];
    if scope == "user" {
        // User services don't need pkexec
        let output = Command::new("systemctl")
            .args(["--user", action, &name])
            .output()
            .await
            .map_err(|e| format!("Failed to run systemctl: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            log_to_file(
                "ERROR",
                &format!("toggle_service_unit {name} failed: {err}"),
            );
            return Err(format!("systemctl {action} failed: {err}"));
        }
    } else {
        // System services need elevation
        args.extend(&[action, &name]);
        let output = Command::new("pkexec")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to run pkexec: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            log_to_file(
                "ERROR",
                &format!("toggle_service_unit {name} failed: {err}"),
            );
            return Err(format!("systemctl {action} failed: {err}"));
        }
    }

    log_to_file("INFO", &format!("Service {name} {action}d (scope={scope})"));
    Ok(())
}

/// Toggle an XDG autostart entry by writing X-GNOME-Autostart-enabled
#[tauri::command]
pub async fn toggle_autostart(file_path: String, enabled: bool) -> Result<(), String> {
    let content =
        fs::read_to_string(&file_path).map_err(|e| format!("Failed to read desktop file: {e}"))?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found_enabled = false;
    let mut found_hidden = false;

    for line in &mut lines {
        let trimmed = line.trim().to_string();
        if trimmed.starts_with("X-GNOME-Autostart-enabled=") {
            *line = format!(
                "X-GNOME-Autostart-enabled={}",
                if enabled { "true" } else { "false" }
            );
            found_enabled = true;
        }
        if trimmed.starts_with("Hidden=") {
            *line = format!("Hidden={}", if enabled { "false" } else { "true" });
            found_hidden = true;
        }
    }

    if !found_enabled {
        // Find the [Desktop Entry] section and insert after it
        let mut insert_pos = lines.len();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "[Desktop Entry]" {
                insert_pos = i + 1;
                break;
            }
        }
        lines.insert(
            insert_pos,
            format!(
                "X-GNOME-Autostart-enabled={}",
                if enabled { "true" } else { "false" }
            ),
        );
    }

    if !found_hidden {
        // Mark Hidden as false when enabling
        if !enabled {
            lines.push("Hidden=true".to_string());
        }
    }

    let new_content = lines.join("\n");

    fs::write(&file_path, new_content).map_err(|e| format!("Failed to write desktop file: {e}"))?;

    log_to_file(
        "INFO",
        &format!("Autostart entry {file_path} enabled={enabled}"),
    );
    Ok(())
}
