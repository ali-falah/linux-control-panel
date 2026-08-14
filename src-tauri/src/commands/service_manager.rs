use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;
use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceUnit {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub description: String,
    pub unit_file_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
    Mask,
    Unmask,
    Reload,
}

impl std::fmt::Display for ServiceAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceAction::Start => write!(f, "start"),
            ServiceAction::Stop => write!(f, "stop"),
            ServiceAction::Restart => write!(f, "restart"),
            ServiceAction::Enable => write!(f, "enable"),
            ServiceAction::Disable => write!(f, "disable"),
            ServiceAction::Mask => write!(f, "mask"),
            ServiceAction::Unmask => write!(f, "unmask"),
            ServiceAction::Reload => write!(f, "reload"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BlameEntry {
    pub time_ms: u64,
    pub time_str: String,
    pub name: String,
}

/// List all systemd units (system or user scope)
#[tauri::command]
pub async fn list_all_units(filter: Option<String>, user_mode: Option<bool>) -> Result<Vec<ServiceUnit>, String> {
    if !crate::binary_exists("systemctl").await {
        return Err("systemctl is not available on this system".to_string());
    }

    use std::collections::HashMap;
    let mut all_services: HashMap<String, ServiceUnit> = HashMap::new();
    let is_user = user_mode.unwrap_or(false);

    // 1. Get all installed unit files
    let mut args_files = vec!["list-unit-files", "--type=service", "--no-pager", "--no-legend"];
    if is_user {
        args_files.push("--user");
    }

    let output_files = Command::new("systemctl")
        .args(&args_files)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let stdout_files = String::from_utf8_lossy(&output_files.stdout);
    for line in stdout_files.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let name = parts[0].to_string();
        let state = parts[1].to_string();

        all_services.insert(
            name.clone(),
            ServiceUnit {
                name,
                load_state: "loaded".to_string(),
                active_state: "inactive".to_string(),
                sub_state: "dead".to_string(),
                description: String::new(),
                unit_file_state: state,
            },
        );
    }

    // 2. Get active/loaded units state and descriptions
    let mut args_units = vec!["list-units", "--all", "--type=service", "--no-pager", "--no-legend", "--plain"];
    if is_user {
        args_units.push("--user");
    }

    let output_units = Command::new("systemctl")
        .args(&args_units)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let stdout_units = String::from_utf8_lossy(&output_units.stdout);
    for line in stdout_units.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0].to_string();
        let load_state = parts[1].to_string();
        let active_state = parts[2].to_string();
        let sub_state = parts[3].to_string();
        let description = parts[4..].join(" ");

        if let Some(unit) = all_services.get_mut(&name) {
            unit.load_state = load_state;
            unit.active_state = active_state;
            unit.sub_state = sub_state;
            unit.description = description;
        } else {
            all_services.insert(
                name.clone(),
                ServiceUnit {
                    name,
                    load_state,
                    active_state,
                    sub_state,
                    description,
                    unit_file_state: "generated".to_string(),
                },
            );
        }
    }

    let mut units: Vec<ServiceUnit> = all_services.into_values().collect();

    if let Some(f) = filter {
        let f = f.to_lowercase();
        units.retain(|u| {
            u.name.to_lowercase().contains(&f) || u.description.to_lowercase().contains(&f)
        });
    }

    units.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(units)
}

/// Perform an action on a systemd service (start/stop/restart/enable/disable/mask/unmask)
#[tauri::command]
pub async fn unit_action(name: String, action: ServiceAction, user_mode: Option<bool>) -> Result<String, String> {
    if !binary_exists("systemctl").await {
        return Err("systemctl is not available".to_string());
    }

    let action_str = action.to_string();
    let is_user = user_mode.unwrap_or(false);

    let output = if is_user {
        Command::new("systemctl")
            .args(["--user", &action_str, &name])
            .output()
            .await
            .map_err(|e| format!("Failed to run systemctl --user: {e}"))?
    } else {
        let needs_elevation = matches!(
            action,
            ServiceAction::Enable
                | ServiceAction::Disable
                | ServiceAction::Mask
                | ServiceAction::Unmask
                | ServiceAction::Start
                | ServiceAction::Stop
                | ServiceAction::Restart
                | ServiceAction::Reload
        );

        if needs_elevation {
            Command::new("pkexec")
                .args(["systemctl", &action_str, &name])
                .output()
                .await
                .map_err(|e| format!("Failed to run pkexec: {e}"))?
        } else {
            Command::new("systemctl")
                .args([&action_str, &name])
                .output()
                .await
                .map_err(|e| format!("Failed to run systemctl: {e}"))?
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let scope_str = if is_user { "user" } else { "system" };
        log_to_file(
            "ERROR",
            &format!("unit_action ({scope_str}) {action_str} {name} failed: {stderr}"),
        );
        return Err(format!("systemctl {action_str} {name} failed: {stderr}"));
    }

    log_to_file("INFO", &format!("Service action: {action_str} {name}"));
    Ok(if stdout.is_empty() {
        format!("Successfully ran: systemctl {action_str} {name}")
    } else {
        stdout
    })
}

/// Get service logs from journalctl
#[tauri::command]
pub async fn get_service_logs(name: String, lines: Option<u32>, user_mode: Option<bool>) -> Result<String, String> {
    if !binary_exists("journalctl").await {
        return Err("journalctl is not available on this system".to_string());
    }

    let lines_str = lines.unwrap_or(100).to_string();
    let is_user = user_mode.unwrap_or(false);

    let output = if is_user {
        Command::new("journalctl")
            .args([
                "--user",
                "-u",
                &name,
                "-n",
                &lines_str,
                "--no-pager",
                "--output=short-precise",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run journalctl: {e}"))?
    } else {
        Command::new("journalctl")
            .args([
                "-u",
                &name,
                "-n",
                &lines_str,
                "--no-pager",
                "--output=short-precise",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run journalctl: {e}"))?
    };

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("journalctl failed: {err}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Read the unit file content via `systemctl cat`
#[tauri::command]
pub async fn read_unit_file(name: String, user_mode: Option<bool>) -> Result<String, String> {
    if !binary_exists("systemctl").await {
        return Err("systemctl is not available".to_string());
    }

    let is_user = user_mode.unwrap_or(false);
    let output = if is_user {
        Command::new("systemctl")
            .args(["--user", "cat", &name])
            .output()
            .await
            .map_err(|e| format!("Failed to run systemctl --user cat: {e}"))?
    } else {
        Command::new("systemctl")
            .args(["cat", &name])
            .output()
            .await
            .map_err(|e| format!("Failed to run systemctl cat: {e}"))?
    };

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("systemctl cat {name} failed: {err}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Write a drop-in override for a unit file
#[tauri::command]
pub async fn write_unit_file(name: String, content: String, user_mode: Option<bool>) -> Result<(), String> {
    let is_user = user_mode.unwrap_or(false);

    if is_user {
        let home = std::env::var("HOME").map_err(|_| "Could not find HOME directory".to_string())?;
        let unit_stem = name.trim_end_matches(".service");
        let drop_in_dir = format!("{}/.config/systemd/user/{}.d", home, unit_stem);
        let drop_in_file = format!("{}/override.conf", drop_in_dir);

        // Create directory
        std::fs::create_dir_all(&drop_in_dir)
            .map_err(|e| format!("Failed to create user systemd dir: {e}"))?;

        // Write file
        std::fs::write(&drop_in_file, &content)
            .map_err(|e| format!("Failed to write user systemd override: {e}"))?;

        // Reload user daemon
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .await;

        log_to_file("INFO", &format!("Wrote user unit file override for {name}"));
        Ok(())
    } else {
        let unit_stem = name.trim_end_matches(".service");
        let drop_in_dir = format!("/etc/systemd/system/{}.d", unit_stem);
        let drop_in_file = format!("{}/override.conf", drop_in_dir);

        // Create drop-in directory via pkexec
        let mkdir_out = Command::new("pkexec")
            .args(["mkdir", "-p", &drop_in_dir])
            .output()
            .await
            .map_err(|e| format!("Failed to create drop-in dir: {e}"))?;

        if !mkdir_out.status.success() {
            let err = String::from_utf8_lossy(&mkdir_out.stderr).to_string();
            return Err(format!("Failed to create drop-in directory: {err}"));
        }

        // Write content safely as root
        crate::utils::privilege::write_file_as_root(&drop_in_file, &content).await?;

        // Reload systemd daemon
        let _ = Command::new("pkexec")
            .args(["systemctl", "daemon-reload"])
            .output()
            .await;

        log_to_file("INFO", &format!("Wrote unit file override for {name}"));
        Ok(())
    }
}

/// Fetch boot latency profiling data using systemd-analyze blame
#[tauri::command]
pub async fn get_boot_blame() -> Result<Vec<BlameEntry>, String> {
    if !crate::binary_exists("systemd-analyze").await {
        return Err("systemd-analyze is not available on this system".to_string());
    }

    let output = Command::new("systemd-analyze")
        .arg("blame")
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let name = parts.last().unwrap().to_string();
        let time_part = parts[..parts.len() - 1].join(" ");
        let time_ms = parse_blame_time(&time_part);

        entries.push(BlameEntry {
            time_ms,
            time_str: time_part,
            name,
        });
    }

    Ok(entries)
}

fn parse_blame_time(s: &str) -> u64 {
    let mut total_ms = 0;
    for part in s.split_whitespace() {
        if part.ends_with("min") {
            if let Ok(m) = part.trim_end_matches("min").parse::<f64>() {
                total_ms += (m * 60000.0) as u64;
            }
        } else if part.ends_with("ms") {
            if let Ok(ms) = part.trim_end_matches("ms").parse::<f64>() {
                total_ms += ms as u64;
            }
        } else if part.ends_with('s') {
            if let Ok(sec) = part.trim_end_matches('s').parse::<f64>() {
                total_ms += (sec * 1000.0) as u64;
            }
        }
    }
    total_ms
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDependencies {
    pub requires: Vec<String>,
    pub wants: Vec<String>,
    pub after: Vec<String>,
    pub before: Vec<String>,
}

/// Query systemd dependencies for a unit (Requires, Wants, After, Before)
#[tauri::command]
pub async fn get_unit_dependencies(name: String, user_mode: Option<bool>) -> Result<UnitDependencies, String> {
    let is_user = user_mode.unwrap_or(false);
    let mut cmd = Command::new("systemctl");
    if is_user {
        cmd.arg("--user");
    }
    cmd.args(["show", &name, "--property=Requires,Wants,After,Before", "--no-pager"]);
    let output = cmd.output().await.map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut requires = Vec::new();
    let mut wants = Vec::new();
    let mut after = Vec::new();
    let mut before = Vec::new();

    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let list: Vec<String> = v.split_whitespace().map(|s| s.to_string()).collect();
            match k {
                "Requires" => requires = list,
                "Wants" => wants = list,
                "After" => after = list,
                "Before" => before = list,
                _ => {}
            }
        }
    }

    Ok(UnitDependencies { requires, wants, after, before })
}
