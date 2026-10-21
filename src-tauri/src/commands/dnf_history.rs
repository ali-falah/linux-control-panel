use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;
use tauri::{AppHandle, Emitter};
use std::sync::Mutex;

use crate::{binary_exists, log_to_file};

// ─── Shared child PID for cancel support ────────────────────────────────────

/// Holds the PID of the currently running dnf upgrade child process.
static UPGRADE_PID: Mutex<Option<u32>> = Mutex::new(None);

// ─── Data Types ──────────────────────────────────────────────────────────────

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfLockInfo {
    pub locked: bool,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub lock_path: Option<String>,
}

// ─── History ─────────────────────────────────────────────────────────────────

/// Parse `dnf history list` or `/var/log/apt/history.log` into structured entries.
#[tauri::command]
pub async fn list_dnf_history() -> Result<Vec<DnfHistoryEntry>, String> {
    if binary_exists("dnf").await {
        let output = Command::new("dnf")
            .args(["history", "list"])
            .output()
            .await
            .map_err(|e| format!("Failed to run dnf: {e}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            return Ok(parse_dnf_history(&stdout));
        }
    }

    if std::path::Path::new("/var/log/apt/history.log").exists() || binary_exists("apt").await {
        return Ok(parse_apt_history());
    }

    Err("Neither dnf nor apt history is available on this system".to_string())
}

fn parse_apt_history() -> Vec<DnfHistoryEntry> {
    let content = match std::fs::read_to_string("/var/log/apt/history.log") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    let mut current_date = String::new();
    let mut current_cmd = String::new();
    let mut current_action = String::new();
    let mut current_altered: u32 = 0;
    let mut id = 1;

    let flush_entry = |entries: &mut Vec<DnfHistoryEntry>, date: &str, cmd: &str, action: &str, altered: u32, id: u32| {
        if !date.is_empty() || !cmd.is_empty() {
            entries.push(DnfHistoryEntry {
                id,
                command: if cmd.is_empty() { "apt".to_string() } else { cmd.to_string() },
                date: date.to_string(),
                action: if action.is_empty() { "Transaction".to_string() } else { action.to_string() },
                altered,
            });
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_entry(&mut entries, &current_date, &current_cmd, &current_action, current_altered, id);
            id += 1;
            current_date.clear();
            current_cmd.clear();
            current_action.clear();
            current_altered = 0;
            continue;
        }

        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            match key {
                "Start-Date" => current_date = val.to_string(),
                "Commandline" => current_cmd = val.to_string(),
                "Install" => {
                    current_action = "Install".to_string();
                    current_altered += val.split(',').count() as u32;
                }
                "Upgrade" => {
                    if current_action.is_empty() {
                        current_action = "Upgrade".to_string();
                    }
                    current_altered += val.split(',').count() as u32;
                }
                "Remove" => {
                    if current_action.is_empty() {
                        current_action = "Remove".to_string();
                    }
                    current_altered += val.split(',').count() as u32;
                }
                _ => {}
            }
        }
    }

    flush_entry(&mut entries, &current_date, &current_cmd, &current_action, current_altered, id);
    entries.reverse();
    entries
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
            let id = caps.get(1).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            let command = caps.get(2).map_or("", |m| m.as_str()).to_string();
            let date = caps.get(3).map_or("", |m| m.as_str()).to_string();
            let action = caps.get(4).map_or("", |m| m.as_str()).to_string();
            let altered = caps.get(5).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            entries.push(DnfHistoryEntry { id, command, date, action, altered });
        }
    }
    entries
}

/// Undo a DNF transaction by ID.
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

// ─── Package Queries ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn dnf_search_packages(query: String) -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("dnf")
            .args(["search", &query])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("apt-cache").await {
        let output = Command::new("apt-cache")
            .args(["search", &query])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    Err("Neither dnf nor apt-cache is available".to_string())
}

#[tauri::command]
pub async fn dnf_package_info(pkg: String) -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("dnf")
            .args(["info", &pkg])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("apt-cache").await {
        let output = Command::new("apt-cache")
            .args(["show", &pkg])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    Err("Neither dnf nor apt-cache is available".to_string())
}

#[tauri::command]
pub async fn dnf_list_versions(pkg: String) -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("dnf")
            .args(["list", "--showduplicates", &pkg])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("apt-cache").await {
        let output = Command::new("apt-cache")
            .args(["madison", &pkg])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    Err("Neither dnf nor apt-cache is available".to_string())
}

// ─── Maintenance Commands ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn dnf_clean_all() -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/dnf", "clean", "all"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    if crate::binary_exists("apt-get").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/apt-get", "clean"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return Ok(String::from_utf8_lossy(&output.stdout).to_string() + "\nAPT cache cleaned successfully.");
    }
    Err("Package manager is not available".to_string())
}

#[tauri::command]
pub async fn dnf_autoremove() -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/dnf", "autoremove", "-y"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("apt-get").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/apt-get", "autoremove", "-y"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    Err("Package manager is not available".to_string())
}

#[tauri::command]
pub async fn dnf_check() -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("dnf")
            .arg("check")
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("dpkg").await {
        let output = Command::new("dpkg")
            .arg("--audit")
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let res = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        return Ok(if res.trim().is_empty() { "No broken or half-installed packages found (dpkg audit OK).".to_string() } else { res });
    }
    Err("Package manager check tool not available".to_string())
}

#[tauri::command]
pub async fn dnf_makecache_cmd() -> Result<String, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/dnf", "makecache"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    if crate::binary_exists("apt-get").await {
        let output = Command::new("pkexec")
            .args(["/usr/bin/apt-get", "update"])
            .output()
            .await
            .map_err(|e| format!("pkexec failed: {e}"))?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr));
    }
    Err("Package manager not available".to_string())
}

fn format_log_timestamp(raw: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z") {
        let local_dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from(dt);
        return local_dt.format("%Y-%m-%d %I:%M:%S %p").to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_str(&format!("{raw}+0000"), "%Y-%m-%dT%H:%M:%S%z") {
        let local_dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from(dt);
        return local_dt.format("%Y-%m-%d %I:%M:%S %p").to_string();
    }
    raw.replace('T', " ")
}

#[tauri::command]
pub fn dnf_read_log() -> Result<String, String> {
    let log_paths = [
        "/var/log/dnf5.log",
        "/var/log/dnf.log",
        "/var/log/dnf.rpm.log",
        "/var/log/dpkg.log",
        "/var/log/apt/history.log",
    ];

    let mut found_path = None;
    for path in &log_paths {
        if std::path::Path::new(path).exists() {
            found_path = Some(*path);
            break;
        }
    }

    let path = match found_path {
        Some(p) => p,
        None => return Err("No package manager log file found in /var/log/".to_string()),
    };

    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

    let iso_re = regex::Regex::new(r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:[+-]\d{4}|Z)?)(.*)$").unwrap();

    let mut formatted_lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = iso_re.captures(trimmed) {
            let raw_ts = caps.get(1).unwrap().as_str();
            let rest = caps.get(2).unwrap().as_str();
            let human_ts = format_log_timestamp(raw_ts);
            formatted_lines.push(format!("[{human_ts}]{rest}"));
        } else {
            formatted_lines.push(trimmed.to_string());
        }
    }

    // Reverse line order so newest entries appear at the top
    formatted_lines.reverse();

    Ok(formatted_lines.join("\n"))
}

// ─── Check Updates ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn dnf_check_updates() -> Result<Vec<DnfUpdateEntry>, String> {
    if crate::binary_exists("dnf").await {
        let output = Command::new("dnf")
            .arg("info")
            .arg("--upgrades")
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(format!("DNF could not list available upgrades: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut entries = Vec::new();
        let mut current_entry = DnfUpdateEntry::default();

        let extract_value = |line: &str| -> String {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 { parts[1].trim().to_string() } else { String::new() }
        };

        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("Name ") || line.starts_with("Name\t") || line.starts_with("Name:") {
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
        return Ok(entries);
    }

    if crate::binary_exists("apt").await {
        let output = Command::new("apt")
            .args(["list", "--upgradable"])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(format!("APT could not list available upgrades: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut entries = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("Listing") || !line.contains("[upgradable from:") {
                continue;
            }
            // format: pkg/suite version arch [upgradable from: ...]
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let (pkg, repo) = match parts[0].split_once('/') {
                    Some((p, r)) => (p.to_string(), r.to_string()),
                    None => (parts[0].to_string(), "apt".to_string()),
                };
                let version = parts[1].to_string();
                let arch = parts[2].to_string();

                entries.push(DnfUpdateEntry {
                    package: pkg,
                    arch,
                    version,
                    repo,
                    size: String::new(),
                });
            }
        }
        return Ok(entries);
    }

    Err("Neither dnf nor apt is available".to_string())
}

// ─── Lock Detection ───────────────────────────────────────────────────────────

const LOCK_PATHS: &[&str] = &[
    "/run/dnf.pid",
    "/run/dnf.lock",
    "/var/cache/dnf/dnf.pid",
    "/var/lib/dpkg/lock-frontend",
    "/var/lib/dpkg/lock",
    "/var/lib/apt/lists/lock",
];

/// Check if DNF or APT/DPKG is currently locked by another process.
/// Returns lock info including PID and process name if locked.
#[tauri::command]
pub async fn dnf_check_lock_status() -> Result<DnfLockInfo, String> {
    for path in LOCK_PATHS {
        if !std::path::Path::new(path).exists() {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            let pid_str = content.trim().to_string();
            if let Ok(pid) = pid_str.parse::<u32>() {
                // Verify the process is actually running
                let proc_exists = std::path::Path::new(&format!("/proc/{pid}")).exists();
                if proc_exists {
                    // Try to read the process name
                    let proc_name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                        .unwrap_or_else(|_| "unknown".to_string())
                        .trim()
                        .to_string();
                    return Ok(DnfLockInfo {
                        locked: true,
                        pid: Some(pid),
                        process_name: Some(proc_name),
                        lock_path: Some(path.to_string()),
                    });
                }
                // Stale lock — process is dead but file remains
                return Ok(DnfLockInfo {
                    locked: false,
                    pid: Some(pid),
                    process_name: None,
                    lock_path: Some(path.to_string()),
                });
            }
        }

        // For dpkg/apt file locks, test with fuser if available
        if let Ok(output) = std::process::Command::new("fuser").arg(path).output() {
            if output.status.success() {
                let pids = String::from_utf8_lossy(&output.stdout);
                if let Some(pid_str) = pids.split_whitespace().next() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        let proc_name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                            .unwrap_or_else(|_| "apt/dpkg".to_string())
                            .trim()
                            .to_string();
                        return Ok(DnfLockInfo {
                            locked: true,
                            pid: Some(pid),
                            process_name: Some(proc_name),
                            lock_path: Some(path.to_string()),
                        });
                    }
                }
            }
        }
    }
    Ok(DnfLockInfo { locked: false, pid: None, process_name: None, lock_path: None })
}

/// Remove a stale DNF or APT lock file. Only works if the owning process is NOT running.
/// Safety: blocked if the PID is still alive (can't forcibly remove a live lock).
#[tauri::command]
pub async fn dnf_kill_lock() -> Result<String, String> {
    let lock_info = dnf_check_lock_status().await?;

    if lock_info.locked {
        // Live process owns the lock — refuse to remove it
        return Err(format!(
            "Cannot remove lock: process '{}' (PID {}) is still running and owns the lock. Wait for it to finish.",
            lock_info.process_name.unwrap_or("unknown".to_string()),
            lock_info.pid.unwrap_or(0)
        ));
    }

    // Only proceed if we found a stale lock (file exists but PID is dead)
    if let Some(lock_path) = lock_info.lock_path {
        std::fs::remove_file(&lock_path)
            .map_err(|e| format!("Failed to remove lock file '{lock_path}': {e}. Try running as root."))?;
        log_to_file("INFO", &format!("Removed stale lock: {lock_path}"));
        Ok(format!("Stale lock file removed: {lock_path}"))
    } else {
        Ok("No stale lock file found. Nothing to remove.".to_string())
    }
}

// ─── Cancel Running Upgrade ───────────────────────────────────────────────────

/// Cancel the currently running DNF upgrade process (sends SIGTERM).
/// This is a graceful termination — DNF will clean up its transaction.
#[tauri::command]
pub fn dnf_cancel_upgrade() -> Result<(), String> {
    let pid_opt = {
        let guard = UPGRADE_PID.lock().unwrap();
        *guard
    };
    if let Some(pid) = pid_opt {
        // SIGTERM (signal 15) — graceful termination
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        log_to_file("INFO", &format!("Sent SIGTERM to upgrade process (PID {pid})"));
        Ok(())
    } else {
        Err("No active upgrade process to cancel.".to_string())
    }
}

// ─── Pre-flight Safety Checks ─────────────────────────────────────────────────

/// Check available disk space on / in bytes.
fn check_disk_space() -> Result<u64, String> {
    let output = std::process::Command::new("df")
        .args(["--output=avail", "-B1", "/"])
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // df output: header line + data line
    stdout.lines()
        .nth(1)
        .and_then(|l| l.trim().parse::<u64>().ok())
        .ok_or_else(|| "Failed to parse disk space".to_string())
}

const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024; // 500 MiB

fn validate_update_package_names(packages: &[String]) -> Result<(), String> {
    if packages.iter().any(|package| {
        package.is_empty()
            || !package.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | ':' | '@' | '-'))
            || !package.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
    }) {
        return Err("Invalid package selection. Select packages from the update list and try again.".to_string());
    }
    Ok(())
}

// ─── Main Upgrade Command (with all safety guards) ───────────────────────────

#[tauri::command]
pub async fn dnf_run_upgrade(
    app: AppHandle,
    packages: Vec<String>,
    expected_packages: Vec<DnfPackageDiff>,
) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;

    validate_update_package_names(&packages)?;

    // ── Guard 2: Package manager lock check ───────────────────────────────────
    let lock_info = dnf_check_lock_status().await?;
    if lock_info.locked {
        return Err(format!(
            "Package manager is locked by '{}' (PID {}). Another package operation is in progress.\n\nWait for it to finish, or inspect the lock in Advanced → Updates if the process is stuck.",
            lock_info.process_name.unwrap_or("unknown".to_string()),
            lock_info.pid.unwrap_or(0)
        ));
    }

    // ── Guard 3: Disk space ──────────────────────────────────────────────────
    match check_disk_space() {
        Ok(free_bytes) if free_bytes < MIN_FREE_BYTES => {
            let free_mb = free_bytes / (1024 * 1024);
            return Err(format!(
                "Insufficient disk space: only {free_mb} MB free on /.\nAt least 500 MB is required to safely upgrade packages.\nFree up space before retrying."
            ));
        }
        Err(e) => log_to_file("WARN", &format!("Could not check disk space: {e}")),
        _ => {}
    }

    let is_dnf = crate::binary_exists("dnf").await;
    let is_apt = crate::binary_exists("apt-get").await;

    if !is_dnf && !is_apt {
        return Err("No supported package manager (dnf or apt-get) found.".to_string());
    }

    if is_dnf {
        let preview = dnf_dry_run_upgrade(packages.clone()).await?;
        if preview.packages.is_empty() {
            return Err("No updates are available in the current transaction. No changes were made.".to_string());
        }
        if !same_package_transaction(&preview.packages, &expected_packages) {
            return Err("The DNF transaction changed after the preview. Review the new transaction and confirm again.".to_string());
        }
        let protected_removals: Vec<&str> = preview.packages.iter()
            .filter(|package| package.action == "Remove" && crate::commands::flatpak_rpm::is_protected_package(&package.name))
            .map(|package| package.name.as_str())
            .collect();
        if !protected_removals.is_empty() {
            return Err(format!(
                "Update blocked because the transaction would remove protected system package(s): {}.",
                protected_removals.join(", ")
            ));
        }
    } else {
        let mut preview_cmd = tokio::process::Command::new("apt-get");
        preview_cmd.arg("-s");
        preview_cmd.env("LC_ALL", "C").env("LANG", "C");
        if packages.is_empty() {
            preview_cmd.arg("dist-upgrade");
        } else {
            preview_cmd.args(["install", "--only-upgrade"]);
            preview_cmd.args(&packages);
        }
        let preview = preview_cmd.output().await.map_err(|e| format!("Could not preview the package transaction: {e}"))?;
        let preview_stdout = String::from_utf8_lossy(&preview.stdout);
        let removals: Vec<&str> = preview_stdout
            .lines()
            .filter(|line| line.starts_with("Remv "))
            .collect();
        if !preview.status.success() || !removals.is_empty() {
            return Err(if removals.is_empty() {
                "Could not safely preview this package transaction. No changes were made.".to_string()
            } else {
                format!("This upgrade would remove installed packages:\n{}. Resolve the conflict before applying it.", removals.join("\n"))
            });
        }
    }

    // ── Launch upgrade process through the system authorization agent ────────
    let mut cmd = tokio::process::Command::new("pkexec");
    cmd.arg("/usr/bin/stdbuf")
       .arg("-oL")
       .arg("-eL");

    if is_dnf {
        cmd.arg("/usr/bin/dnf")
           .arg("upgrade")
           .arg("--best")
           .arg("-y");

        // Put package names BEFORE flags so `dnf history list` displays
        // exact package names instead of truncating flags in UI output
        if !packages.is_empty() {
            cmd.args(&packages);
        }

        cmd.arg("--setopt=timeout=120");

        cmd.env("PYTHONUNBUFFERED", "1");
    } else {
        cmd.arg("/usr/bin/apt-get");
        if !packages.is_empty() {
            cmd.arg("install").arg("--only-upgrade").arg("-y");
            cmd.args(&packages);
        } else {
            cmd.arg("dist-upgrade").arg("-y");
        }
        cmd.env("DEBIAN_FRONTEND", "noninteractive");
    }

    cmd.stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    // Store PID for cancel support
    if let Some(pid) = child.id() {
        let mut guard = UPGRADE_PID.lock().unwrap();
        *guard = Some(pid);
    }

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    // Stream stdout
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

    // Stream stderr
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

    // Clear the PID
    {
        let mut guard = UPGRADE_PID.lock().unwrap();
        *guard = None;
    }

    let _ = app.emit("dnf-upgrade-finished", status.success());

    if !status.success() {
        log_to_file("ERROR", "DNF upgrade process failed");
        return Err("Upgrade process failed. Check the terminal output above for details.".to_string());
    }

    log_to_file("INFO", "DNF upgrade completed successfully");
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnfPackageDiff {
    pub name: String,
    pub arch: String,
    pub old_version: Option<String>,
    pub new_version: String,
    pub repo: String,
    pub size: String,
    pub action: String, // "Upgrade" | "Install" | "Remove" | "Downgrade" | "Obsolete"
}

fn same_package_transaction(current: &[DnfPackageDiff], expected: &[DnfPackageDiff]) -> bool {
    let mut current = current.to_vec();
    let mut expected = expected.to_vec();
    let sort_packages = |packages: &mut Vec<DnfPackageDiff>| {
        packages.sort_by(|a, b| (&a.action, &a.name, &a.arch, &a.new_version).cmp(&(&b.action, &b.name, &b.arch, &b.new_version)));
    };
    sort_packages(&mut current);
    sort_packages(&mut expected);
    current == expected
}

fn dnf_preview_detail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().rev().take(18).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n").chars().take(2400).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfDryRunResult {
    pub packages: Vec<DnfPackageDiff>,
    pub total_download_size: String,
    pub disk_space_change: String,
    pub to_upgrade_count: usize,
    pub to_install_count: usize,
    pub to_remove_count: usize,
    pub to_downgrade_count: usize,
    pub raw_output: String,
}

fn strip_ansi_codes(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until ending letter
            if let Some(&'[') = chars.peek() {
                chars.next();
                for next_c in chars.by_ref() {
                    if next_c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_known_architecture(arch: &str) -> bool {
    matches!(
        arch,
        "x86_64" | "i686" | "i386" | "noarch" | "aarch64" | "armv7hl" | "armv6hl" | "ppc64le" | "s390x" | "riscv64" | "src"
    )
}

fn parse_dnf_dry_run(stdout_text: &str, full_raw_text: &str) -> DnfDryRunResult {
    let clean_text = strip_ansi_codes(stdout_text);
    let mut packages: Vec<DnfPackageDiff> = Vec::new();
    let mut current_action: Option<String> = None;
    let mut in_summary_section = false;
    let mut total_download_size = String::new();
    let mut disk_space_change = String::new();

    let mut to_upgrade_count = 0;
    let mut to_install_count = 0;
    let mut to_remove_count = 0;
    let mut to_downgrade_count = 0;

    for line in clean_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();

        // 1. Transaction Summary Section Boundary
        if lower.contains("transaction summary") {
            in_summary_section = true;
            current_action = None;
            continue;
        }

        // 2. Metrics Extractors (works anywhere in output)
        if lower.contains("need to download") {
            if let Some(pos) = line.find("Need to download") {
                total_download_size = line[pos + "Need to download".len()..].trim().trim_end_matches('.').to_string();
            }
            continue;
        } else if lower.starts_with("total download size:") {
            total_download_size = trimmed["Total download size:".len()..].trim().to_string();
            continue;
        } else if lower.starts_with("after this operation") {
            disk_space_change = trimmed.to_string();
            continue;
        }

        // Once inside the summary section, NEVER parse packages
        if in_summary_section {
            continue;
        }

        // 3. Action Section Headers (Package List Zone only)
        if lower.starts_with("upgrading:") || lower == "upgrades" {
            current_action = Some("Upgrade".to_string());
            continue;
        } else if lower.starts_with("installing dependencies:") || lower.starts_with("installing:") || lower == "installing" {
            current_action = Some("Install".to_string());
            continue;
        } else if lower.starts_with("removing:") || lower == "removing" {
            current_action = Some("Remove".to_string());
            continue;
        } else if lower.starts_with("downgrading:") || lower == "downgrading" {
            current_action = Some("Downgrade".to_string());
            continue;
        } else if lower.starts_with("obsoleting:") || lower == "obsoleting" {
            current_action = Some("Obsolete".to_string());
            continue;
        }

        // 4. Package Rows within active action section
        if let Some(ref action) = current_action {
            // Replacement tracking (DNF5 `replacing <pkg> <arch> <ver>` or DNF4 `obsoleted by`)
            if trimmed.starts_with("replacing ") || trimmed.starts_with("obsoleted by ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let old_ver = if parts.len() >= 4 && is_known_architecture(parts[2]) {
                    Some(parts[3].to_string())
                } else if parts.len() >= 3 && parts[2].chars().any(|c| c.is_ascii_digit()) {
                    Some(parts[2].to_string())
                } else if parts.len() >= 4 {
                    Some(parts[3].to_string())
                } else {
                    None
                };
                if let Some(ver) = old_ver {
                    if let Some(last_pkg) = packages.last_mut() {
                        last_pkg.old_version = Some(ver);
                    }
                }
                continue;
            }

            // Skip headers or divider lines
            if trimmed.starts_with("Package ") || trimmed.starts_with("===") || trimmed.starts_with("---") {
                continue;
            }

            // Skip common informational sentence lines
            if lower.starts_with("updating and")
                || lower.starts_with("repositories loaded")
                || lower.starts_with("operation aborted")
                || lower.starts_with("nothing to do")
                || lower.starts_with("complete!")
            {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            // A genuine package line MUST have at least: [name, arch, version, repo]
            if parts.len() >= 4 {
                let name = parts[0].to_string();
                let arch = parts[1].to_string();
                let version = parts[2].to_string();
                let repo = parts[3].to_string();
                let size = if parts.len() >= 5 { parts[4..].join(" ") } else { "-".to_string() };

                // STRICT VALIDATION:
                // parts[1] must be a recognized architecture (e.g. x86_64, noarch, aarch64)
                // parts[2] must contain numbers (e.g. 1.94.117-1, 0:5.87-6.fc44)
                let is_arch = is_known_architecture(&arch);
                let is_version = version.chars().any(|c| c.is_ascii_digit());

                if is_arch && is_version {
                    match action.as_str() {
                        "Upgrade" | "Obsolete" => to_upgrade_count += 1,
                        "Install" => to_install_count += 1,
                        "Remove" => to_remove_count += 1,
                        "Downgrade" => to_downgrade_count += 1,
                        _ => {}
                    }

                    packages.push(DnfPackageDiff {
                        name,
                        arch,
                        old_version: None,
                        new_version: version,
                        repo,
                        size,
                        action: action.clone(),
                    });
                }
            }
        }
    }

    if total_download_size.is_empty() {
        total_download_size = "0 B".to_string();
    }

    DnfDryRunResult {
        packages,
        total_download_size,
        disk_space_change,
        to_upgrade_count,
        to_install_count,
        to_remove_count,
        to_downgrade_count,
        raw_output: full_raw_text.to_string(),
    }
}

#[tauri::command]
pub async fn dnf_dry_run_upgrade(packages: Vec<String>) -> Result<DnfDryRunResult, String> {
    use std::process::Stdio;
    validate_update_package_names(&packages)?;
    let mut cmd = tokio::process::Command::new("dnf");

    cmd.arg("--color=never")
       .arg("upgrade")
       .arg("--best")
       .arg("--assumeno");
    cmd.env("LC_ALL", "C").env("LANG", "C");

    if !packages.is_empty() {
        cmd.args(&packages);
    }

    cmd.arg("--setopt=timeout=60");

    cmd.stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn DNF dry-run: {e}"))?;

    let output = child.wait_with_output().await.map_err(|e| format!("Error waiting for DNF dry-run: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let full_text = format!("{}\n{}", stdout, stderr);

    let parsed = parse_dnf_dry_run(&stdout, &full_text);
    let lower_text = full_text.to_lowercase();
    let has_error = ["error:", "problem:", "errors during downloading", "failed to download", "cannot download"]
        .iter()
        .any(|marker| lower_text.contains(marker));
    if has_error {
        return Err(format!(
            "DNF could not safely preview this update transaction. No changes were made.\n\n{}",
            dnf_preview_detail(&full_text)
        ));
    }

    let known_noop = lower_text.lines().any(|line| {
        let line = line.trim();
        line.starts_with("nothing to do") || line.starts_with("no packages marked for upgrade")
    });
    let has_summary = lower_text.contains("transaction summary");
    let expected_assumeno_exit = lower_text.contains("operation aborted");
    let has_remove_section = lower_text.lines().any(|line| line.trim().starts_with("removing:"));
    if (known_noop && !output.status.success())
        || (!known_noop && (parsed.packages.is_empty() || !has_summary || (!output.status.success() && !expected_assumeno_exit)))
        || (has_remove_section && parsed.to_remove_count == 0)
    {
        return Err(format!(
            "DNF did not return a complete update preview. No changes were made.\n\n{}",
            dnf_preview_detail(&full_text)
        ));
    }

    // If no packages were parsed, check if DNF failed with a conflict or problem
    if parsed.packages.is_empty() {
        let has_critical_error = full_text.lines().any(|l| {
            let tr = l.trim();
            tr.starts_with("Error: ") || tr.starts_with("Problem: ") || tr.starts_with("Errors during downloading")
        });

        if has_critical_error {
            let mut errors = Vec::new();
            let mut in_err = false;
            for line in full_text.lines() {
                let tr = line.trim();
                if tr.starts_with("Error: ") || tr.starts_with("Problem: ") || tr.starts_with("Errors during downloading") {
                    in_err = true;
                }
                if in_err {
                    if tr.is_empty() && errors.len() >= 3 {
                        in_err = false;
                    } else if !tr.is_empty() {
                        errors.push(tr);
                    }
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("\n"));
            }
        }
    }

    Ok(parsed)
}
