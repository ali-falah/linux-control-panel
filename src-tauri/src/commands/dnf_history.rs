use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfHistoryEntry {
    pub id: u32,
    pub command: String,
    pub date: String,
    pub action: String,
    pub altered: u32,
}

/// Parse `dnf history list` output into structured entries
#[tauri::command]
pub async fn list_dnf_history() -> Result<Vec<DnfHistoryEntry>, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("dnf")
        .args(["history", "list"])
        .output()
        .await
        .map_err(|e| format!("Failed to run dnf: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("dnf history list failed: {err}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let entries = parse_dnf_history(&stdout);

    Ok(entries)
}

fn parse_dnf_history(output: &str) -> Vec<DnfHistoryEntry> {
    let mut entries = Vec::new();
    let re = regex::Regex::new(
        r"^\s*(\d+)\s+(.+?)\s+(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})(?:\s+(.*?))?\s+(\d+)\s*$",
    )
    .unwrap();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.starts_with("ID") {
            continue;
        }

        if let Some(caps) = re.captures(line) {
            let id = caps
                .get(1)
                .map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            let command = caps.get(2).map_or("", |m| m.as_str()).to_string();
            let date = caps.get(3).map_or("", |m| m.as_str()).to_string();
            let action = caps.get(4).map_or("", |m| m.as_str()).to_string();
            let altered = caps
                .get(5)
                .map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));

            entries.push(DnfHistoryEntry {
                id,
                command,
                date,
                action,
                altered,
            });
        }
    }

    entries
}

/// Undo a DNF transaction by ID
#[tauri::command]
pub async fn undo_transaction(id: u32) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let id_str = id.to_string();

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "history", "undo", &id_str, "-y"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("undo_transaction {id} failed: {stderr}"));
        return Err(format!("dnf history undo failed: {stderr}"));
    }

    log_to_file("INFO", &format!("Undid transaction #{id}"));
    Ok(stdout)
}

#[tauri::command]
pub async fn dnf_search_packages(query: String) -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("dnf")
        .args(["search", &query])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

#[tauri::command]
pub async fn dnf_package_info(pkg: String) -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("dnf")
        .args(["info", &pkg])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

#[tauri::command]
pub async fn dnf_list_versions(pkg: String) -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("dnf")
        .args(["list", "--showduplicates", &pkg])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

#[tauri::command]
pub async fn dnf_clean_all() -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "clean", "all"])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn dnf_autoremove() -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "autoremove", "-y"])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

#[tauri::command]
pub async fn dnf_check() -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("dnf")
        .arg("check")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

#[tauri::command]
pub async fn dnf_makecache_cmd() -> Result<String, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "makecache"])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}
