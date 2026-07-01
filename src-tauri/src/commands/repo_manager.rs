use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub id: String,
    pub name: String,
    pub baseurl: String,
    pub enabled: bool,
    pub file_path: String,
    pub metalink: Option<String>,
    pub mirrorlist: Option<String>,
    pub gpgcheck: bool,
}

/// Parse all .repo files from /etc/yum.repos.d/
#[tauri::command]
pub async fn list_repos() -> Result<Vec<RepoEntry>, String> {
    let repo_dir = PathBuf::from("/etc/yum.repos.d");
    if !repo_dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();

    let dir_entries =
        fs::read_dir(&repo_dir).map_err(|e| format!("Failed to read /etc/yum.repos.d: {e}"))?;

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("repo") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log_to_file("WARN", &format!("Could not read {:?}: {e}", path));
                continue;
            }
        };

        let file_path = path.to_string_lossy().to_string();
        let mut current_id = String::new();
        let mut current_name = String::new();
        let mut current_baseurl = String::new();
        let mut current_enabled = true;
        let mut current_metalink: Option<String> = None;
        let mut current_mirrorlist: Option<String> = None;
        let mut current_gpgcheck = true;
        let mut in_section = false;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                // Save previous section
                if in_section && !current_id.is_empty() {
                    entries.push(RepoEntry {
                        id: current_id.clone(),
                        name: if current_name.is_empty() {
                            current_id.clone()
                        } else {
                            current_name.clone()
                        },
                        baseurl: current_baseurl.clone(),
                        enabled: current_enabled,
                        file_path: file_path.clone(),
                        metalink: current_metalink.clone(),
                        mirrorlist: current_mirrorlist.clone(),
                        gpgcheck: current_gpgcheck,
                    });
                }
                current_id = line[1..line.len() - 1].to_string();
                current_name = String::new();
                current_baseurl = String::new();
                current_enabled = true;
                current_metalink = None;
                current_mirrorlist = None;
                current_gpgcheck = true;
                in_section = true;
            } else if in_section {
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim().to_lowercase();
                    let val = val.trim().to_string();
                    match key.as_str() {
                        "name" => current_name = val,
                        "baseurl" => current_baseurl = val,
                        "enabled" => current_enabled = val == "1" || val == "true",
                        "metalink" => current_metalink = Some(val),
                        "mirrorlist" => current_mirrorlist = Some(val),
                        "gpgcheck" => current_gpgcheck = val == "1",
                        _ => {}
                    }
                }
            }
        }

        // Save last section
        if in_section && !current_id.is_empty() {
            entries.push(RepoEntry {
                id: current_id.clone(),
                name: if current_name.is_empty() {
                    current_id.clone()
                } else {
                    current_name.clone()
                },
                baseurl: current_baseurl.clone(),
                enabled: current_enabled,
                file_path: file_path.clone(),
                metalink: current_metalink.clone(),
                mirrorlist: current_mirrorlist.clone(),
                gpgcheck: current_gpgcheck,
            });
        }
    }

    Ok(entries)
}

/// Enable or disable a specific repo section in its .repo file via pkexec + sed
#[tauri::command]
pub async fn toggle_repo(repo_id: String, enabled: bool, file_path: String) -> Result<(), String> {
    let enabled_str = if enabled { "1" } else { "0" };

    // Use sed to update enabled= under the specific [repo_id] section
    let script = format!(
        r#"/^\[{}\]/,/^\[/ s/^enabled\s*=.*/enabled={}/;"#,
        regex::escape(&repo_id),
        enabled_str
    );

    let output = Command::new("pkexec")
        .args(["sed", "-i", &script, &file_path])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        log_to_file("ERROR", &format!("toggle_repo failed: {err}"));
        return Err(format!("Failed to toggle repo: {err}"));
    }

    log_to_file("INFO", &format!("Toggled repo {repo_id} enabled={enabled}"));
    Ok(())
}

/// Add a repo by URL using dnf config-manager
#[tauri::command]
pub async fn add_repo(url: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "config-manager", "--add-repo", &url])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("add_repo failed: {stderr}"));
        return Err(format!("Failed to add repo: {stderr}"));
    }

    log_to_file("INFO", &format!("Added repo: {url}"));
    Ok(stdout)
}

/// Run dnf makecache to refresh metadata
#[tauri::command]
pub async fn run_makecache() -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "makecache"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("makecache failed: {stderr}"));
        return Err(format!("dnf makecache failed: {stderr}"));
    }

    log_to_file("INFO", "Ran dnf makecache");
    Ok(stdout)
}
