use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::process::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatpakApp {
    pub name: String,
    pub app_id: String,
    pub version: String,
    pub origin: String,
    pub installation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpmPackage {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateEntry {
    pub common_name: String,
    pub flatpak: Option<FlatpakApp>,
    pub rpm: Option<RpmPackage>,
    pub recommendation: String,
}

/// List all installed Flatpak apps
#[tauri::command]
pub async fn list_flatpaks() -> Result<Vec<FlatpakApp>, String> {
    if !binary_exists("flatpak").await {
        return Ok(vec![]);
    }

    let output = Command::new("flatpak")
        .args([
            "list",
            "--app",
            "--columns=name,application,version,origin,installation",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run flatpak: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("flatpak list failed: {err}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let apps = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            FlatpakApp {
                name: parts.first().unwrap_or(&"").trim().to_string(),
                app_id: parts.get(1).unwrap_or(&"").trim().to_string(),
                version: parts.get(2).unwrap_or(&"").trim().to_string(),
                origin: parts.get(3).unwrap_or(&"").trim().to_string(),
                installation: parts.get(4).unwrap_or(&"system").trim().to_string(),
            }
        })
        .filter(|a| !a.app_id.is_empty())
        .collect();

    Ok(apps)
}

/// List all installed RPM packages
#[tauri::command]
pub async fn list_rpms() -> Result<Vec<RpmPackage>, String> {
    if !binary_exists("rpm").await {
        return Err("rpm is not available on this system".to_string());
    }

    let output = Command::new("rpm")
        .args([
            "-qa",
            "--queryformat",
            "%{NAME}\t%{VERSION}\t%{ARCH}\t%{SUMMARY}\n",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run rpm: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("rpm -qa failed: {err}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let packages = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, '\t').collect();
            RpmPackage {
                name: parts.first().unwrap_or(&"").trim().to_string(),
                version: parts.get(1).unwrap_or(&"").trim().to_string(),
                arch: parts.get(2).unwrap_or(&"").trim().to_string(),
                summary: parts.get(3).unwrap_or(&"").trim().to_string(),
            }
        })
        .filter(|p| !p.name.is_empty())
        .collect();

    Ok(packages)
}

/// Detect packages available as both Flatpak and RPM
#[tauri::command]
pub async fn detect_duplicates() -> Result<Vec<DuplicateEntry>, String> {
    let flatpaks = list_flatpaks().await?;
    let rpms = list_rpms().await?;

    // Build a normalized name map for RPMs
    let rpm_map: HashMap<String, &RpmPackage> =
        rpms.iter().map(|r| (normalize_name(&r.name), r)).collect();

    let mut duplicates = Vec::new();

    for flatpak in &flatpaks {
        let flat_norm = normalize_name(&flatpak.name);

        // Try to find a matching RPM
        if let Some(rpm) = rpm_map.get(&flat_norm) {
            let recommendation = determine_recommendation(flatpak, rpm);
            duplicates.push(DuplicateEntry {
                common_name: flatpak.name.clone(),
                flatpak: Some(flatpak.clone()),
                rpm: Some((*rpm).clone()),
                recommendation,
            });
        } else {
            // Try matching on last part of app_id (e.g., org.gnome.Calculator -> calculator)
            let id_norm = flatpak
                .app_id
                .split('.')
                .last()
                .map(|s| normalize_name(s))
                .unwrap_or_default();

            if !id_norm.is_empty() {
                if let Some(rpm) = rpm_map.get(&id_norm) {
                    let recommendation = determine_recommendation(flatpak, rpm);
                    duplicates.push(DuplicateEntry {
                        common_name: flatpak.name.clone(),
                        flatpak: Some(flatpak.clone()),
                        rpm: Some((*rpm).clone()),
                        recommendation,
                    });
                }
            }
        }
    }

    Ok(duplicates)
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .replace(['-', '_', '.', ' '], "")
        .trim_start_matches("lib")
        .to_string()
}

fn determine_recommendation(flatpak: &FlatpakApp, _rpm: &RpmPackage) -> String {
    // Sandbox preference: Flatpak is sandboxed, better for untrusted apps
    // RPM integrates better with system; prefer for system tools
    let system_tool_keywords = [
        "systemd", "dnf", "rpm", "gtk", "qt", "lib", "python", "perl", "kernel",
    ];

    let name_lower = flatpak.name.to_lowercase();
    let is_system_tool = system_tool_keywords.iter().any(|k| name_lower.contains(k));

    if is_system_tool {
        "Keep RPM — better system integration for system tools".to_string()
    } else {
        "Keep Flatpak — sandboxed with automatic updates".to_string()
    }
}

/// Remove a Flatpak app
#[tauri::command]
pub async fn remove_flatpak(app_id: String, system_wide: bool) -> Result<String, String> {
    if !binary_exists("flatpak").await {
        return Err("flatpak is not available".to_string());
    }

    let mut args = vec!["/usr/bin/flatpak", "uninstall", "-y", &app_id];
    if system_wide {
        args.push("--system");
    } else {
        args.push("--user");
    }

    let output = Command::new("pkexec")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file(
            "ERROR",
            &format!("remove_flatpak {app_id} failed: {stderr}"),
        );
        return Err(format!("Failed to remove Flatpak: {stderr}"));
    }

    log_to_file("INFO", &format!("Removed Flatpak: {app_id}"));
    Ok(stdout)
}

/// Remove an RPM package
#[tauri::command]
pub async fn remove_rpm(name: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "remove", &name, "-y"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("remove_rpm {name} failed: {stderr}"));
        return Err(format!("Failed to remove RPM: {stderr}"));
    }

    log_to_file("INFO", &format!("Removed RPM: {name}"));
    Ok(stdout)
}
