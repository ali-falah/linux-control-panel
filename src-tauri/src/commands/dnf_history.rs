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

/// Parse `dnf history list` output into structured entries.
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

// ─── Maintenance Commands ─────────────────────────────────────────────────────

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
        None => return Err("No DNF log file found in /var/log/".to_string()),
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
    Ok(entries)
}

// ─── Lock Detection ───────────────────────────────────────────────────────────

const LOCK_PATHS: &[&str] = &[
    "/run/dnf.pid",
    "/run/dnf.lock",
    "/var/cache/dnf/dnf.pid",
];

/// Check if DNF is currently locked by another process.
/// Returns lock info including PID and process name if locked.
#[tauri::command]
pub async fn dnf_check_lock_status() -> Result<DnfLockInfo, String> {
    for path in LOCK_PATHS {
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
    }
    Ok(DnfLockInfo { locked: false, pid: None, process_name: None, lock_path: None })
}

/// Remove a stale DNF lock file. Only works if the owning process is NOT running.
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
        log_to_file("INFO", &format!("Removed stale DNF lock: {lock_path}"));
        Ok(format!("Stale lock file removed: {lock_path}\nRun 'dnf check' to verify database integrity."))
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
        log_to_file("INFO", &format!("Sent SIGTERM to DNF upgrade process (PID {pid})"));
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

/// Stop PackageKit to prevent it from holding the DNF lock during upgrade.
/// Returns Ok(true) if PackageKit was running and was stopped, Ok(false) if not running.
async fn pause_packagekit() -> bool {
    // Check if running first
    let is_running = std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "packagekit"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if is_running {
        let _ = std::process::Command::new("systemctl")
            .args(["stop", "packagekit"])
            .status();
        log_to_file("INFO", "Paused PackageKit before DNF upgrade");
        true
    } else {
        false
    }
}

// ─── Main Upgrade Command (with all safety guards) ───────────────────────────

#[tauri::command]
pub async fn dnf_run_upgrade(app: AppHandle, packages: Vec<String>) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // ── Guard 1: Root required ───────────────────────────────────────────────
    let (is_none, pw_opt) = {
        let guard = crate::utils::privilege::SUDO_PASSWORD.lock().unwrap();
        (guard.is_none(), guard.clone())
    };
    if is_none {
        return Err("Root privileges are required. Please enable Root in the status bar before upgrading.".to_string());
    }
    let pw = pw_opt.unwrap();

    // ── Guard 2: DNF lock check ──────────────────────────────────────────────
    let lock_info = dnf_check_lock_status().await?;
    if lock_info.locked {
        return Err(format!(
            "DNF is locked by '{}' (PID {}). Another package operation is in progress.\n\nWait for it to finish, or use Maintenance → Kill DNF Lock if the process is stuck.",
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

    // ── Guard 4: Pause PackageKit ────────────────────────────────────────────
    let packagekit_paused = pause_packagekit().await;
    if packagekit_paused {
        let _ = app.emit("dnf-upgrade-output", "ℹ PackageKit paused to prevent lock conflicts.\n");
    }

    // ── Launch upgrade process ───────────────────────────────────────────────
    let mut cmd = tokio::process::Command::new("sudo");
    cmd.arg("-S")
       .arg("--prompt=")
       .arg("stdbuf")
       .arg("-oL")
       .arg("-eL")
       .arg("dnf")
       .arg("upgrade")
       .arg("-y");

    // Put package names BEFORE flags so `dnf history list` displays
    // exact package names instead of truncating flags in UI output
    if !packages.is_empty() {
        cmd.args(&packages);
    }

    cmd.arg("--allowerasing")
       .arg("--nogpgcheck")
       .arg("--setopt=timeout=120");

    cmd.env("PYTHONUNBUFFERED", "1");

    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    // Store PID for cancel support
    if let Some(pid) = child.id() {
        let mut guard = UPGRADE_PID.lock().unwrap();
        *guard = Some(pid);
    }

    // Write password to stdin
    if let Some(mut stdin) = child.stdin.take() {
        let mut p = pw;
        p.push('\n');
        tokio::spawn(async move {
            let _ = stdin.write_all(p.as_bytes()).await;
        });
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

// ─── DNF Dry Run Data Types & Command ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfPackageDiff {
    pub name: String,
    pub arch: String,
    pub old_version: Option<String>,
    pub new_version: String,
    pub repo: String,
    pub size: String,
    pub action: String, // "Upgrade" | "Install" | "Remove" | "Downgrade" | "Obsolete"
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

fn parse_dnf_dry_run(raw_text: &str) -> DnfDryRunResult {
    let clean_text = strip_ansi_codes(raw_text);
    let mut packages: Vec<DnfPackageDiff> = Vec::new();
    let mut current_action: Option<String> = None;
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
        } else if lower.starts_with("transaction summary:") || lower.starts_with("transaction summary") {
            current_action = None;
            continue;
        }

        if lower.contains("need to download") {
            if let Some(pos) = line.find("Need to download") {
                total_download_size = line[pos + "Need to download".len()..].trim().trim_end_matches('.').to_string();
            }
        } else if lower.starts_with("total download size:") {
            total_download_size = trimmed["Total download size:".len()..].trim().to_string();
        } else if lower.starts_with("after this operation,") {
            disk_space_change = trimmed.to_string();
        }

        if let Some(ref action) = current_action {
            // Replacement line in DNF5: "replacing bluez x86_64 0:5.87-4.fc44 updates 3.9 MiB"
            if trimmed.starts_with("replacing ") || trimmed.starts_with("obsoleted by ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 {
                    let old_ver = parts[3].to_string();
                    if let Some(last_pkg) = packages.last_mut() {
                        last_pkg.old_version = Some(old_ver);
                    }
                }
                continue;
            }

            if trimmed.starts_with("Package ") || trimmed.starts_with("===") || trimmed.starts_with("---") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 4 {
                let name = parts[0].to_string();
                let arch = parts[1].to_string();
                let version = parts[2].to_string();
                let repo = parts[3].to_string();
                let size = if parts.len() >= 5 { parts[4..].join(" ") } else { "-".to_string() };

                match action.as_str() {
                    "Upgrade" => to_upgrade_count += 1,
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
        raw_output: raw_text.to_string(),
    }
}

#[tauri::command]
pub async fn dnf_dry_run_upgrade(packages: Vec<String>) -> Result<DnfDryRunResult, String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let pw_opt = {
        let guard = crate::utils::privilege::SUDO_PASSWORD.lock().unwrap();
        guard.clone()
    };

    let mut cmd = if let Some(ref _pw) = pw_opt {
        let mut c = tokio::process::Command::new("sudo");
        c.arg("-S").arg("--prompt=").arg("dnf");
        c
    } else {
        tokio::process::Command::new("dnf")
    };

    cmd.arg("--color=never")
       .arg("upgrade")
       .arg("--assumeno");

    if !packages.is_empty() {
        cmd.args(&packages);
    }

    cmd.arg("--allowerasing")
       .arg("--nogpgcheck")
       .arg("--setopt=timeout=60");

    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn DNF dry-run: {e}"))?;

    if let (Some(pw), Some(mut stdin)) = (pw_opt, child.stdin.take()) {
        let mut p = pw;
        p.push('\n');
        let _ = stdin.write_all(p.as_bytes()).await;
    }

    let output = child.wait_with_output().await.map_err(|e| format!("Error waiting for DNF dry-run: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let full_text = format!("{}\n{}", stdout, stderr);

    let parsed = parse_dnf_dry_run(&full_text);

    // If no packages were parsed, check if DNF failed with a conflict or problem
    if parsed.packages.is_empty() {
        let has_critical_error = full_text.lines().any(|l| {
            let tr = l.trim();
            tr.starts_with("Error: ") || tr.starts_with("Problem: ") || tr.starts_with("Errors during downloading")
        });

        if has_critical_error {
            let errors: Vec<&str> = full_text
                .lines()
                .map(|l| l.trim())
                .filter(|l| l.starts_with("Error: ") || l.starts_with("Problem: ") || l.starts_with("Errors during downloading"))
                .collect();
            if !errors.is_empty() {
                return Err(errors.join("\n"));
            }
        }
    }

    Ok(parsed)
}
