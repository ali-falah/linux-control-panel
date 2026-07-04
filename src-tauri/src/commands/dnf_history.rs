use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tauri::{AppHandle, Emitter};

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfHistoryEntry {
    pub id: u32,
    pub command: String,
    pub date: String,
    pub action: String,
    pub altered: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnfUpdateEntry {
    pub package: String,
    pub arch: String,
    pub version: String,
    pub repo: String,
    pub size: String,
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

#[tauri::command]
pub fn dnf_read_log() -> Result<String, String> {
    std::fs::read_to_string("/var/log/dnf.log").map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dnf_check_updates() -> Result<Vec<DnfUpdateEntry>, String> {
    if !crate::binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }
    let output = Command::new("dnf")
        .arg("info")
        .arg("--upgrades")
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut entries = Vec::new();
    let mut current_entry = DnfUpdateEntry::default();

    let extract_value = |line: &str| -> String {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            parts[1].trim().to_string()
        } else {
            String::new()
        }
    };

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("Name ") || line.starts_with("Name\t") || line.starts_with("Name:") || line.starts_with("Name ") {
            let val = extract_value(line);
            if !val.is_empty() {
                if !current_entry.package.is_empty() {
                    entries.push(current_entry.clone());
                    current_entry = DnfUpdateEntry::default();
                }
                current_entry.package = val;
            }
        } else if line.starts_with("Architecture") {
            current_entry.arch = extract_value(line);
        } else if line.starts_with("Version") {
            current_entry.version = extract_value(line);
        } else if line.starts_with("Release") {
            let rel = extract_value(line);
            if !current_entry.version.is_empty() {
                current_entry.version.push('-');
                current_entry.version.push_str(&rel);
            }
        } else if line.starts_with("Repository") {
            current_entry.repo = extract_value(line);
        } else if line.starts_with("Download size") || line.starts_with("Installed size") || line.starts_with("Size") {
            if current_entry.size.is_empty() {
                current_entry.size = extract_value(line);
            }
        }
    }
    if !current_entry.package.is_empty() {
        entries.push(current_entry);
    }
    Ok(entries)
}

#[tauri::command]
pub async fn dnf_run_upgrade(app: AppHandle, packages: Vec<String>) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (is_none, pw_opt) = {
        let guard = crate::utils::privilege::SUDO_PASSWORD.lock().unwrap();
        (guard.is_none(), if guard.is_some() { Some(guard.clone().unwrap()) } else { None })
    };
    if is_none {
        return Err("Root privileges are required to perform upgrades. Please enable Root in the Control Panel.".to_string());
    }
    let pw = pw_opt.unwrap();

    let mut cmd = tokio::process::Command::new("sudo");
    cmd.arg("-S")
       .arg("--prompt=")
       .arg("python3")
       .arg("-c")
       .arg("import pty; import sys; pty.spawn(sys.argv[1:])")
       .arg("dnf")
       .arg("upgrade")
       .arg("-y")
       .args(&packages);
    
    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut p = pw;
        p.push('\n');
        tokio::spawn(async move {
            let _ = stdin.write_all(p.as_bytes()).await;
        });
    }

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_clone.emit("dnf-upgrade-output", text);
                }
                Err(_) => break,
            }
        }
    });

    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_clone.emit("dnf-upgrade-output", text);
                }
                Err(_) => break,
            }
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = app.emit("dnf-upgrade-finished", status.success());

    if !status.success() {
        return Err("Upgrade process failed".to_string());
    }
    Ok(())
}
