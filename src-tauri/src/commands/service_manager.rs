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
    pub is_protected: bool,
    pub protection_level: String,
    pub protection_reason: Option<String>,
}

pub fn get_service_protection(unit_name: &str) -> (bool, String, Option<String>) {
    let clean_name = unit_name.trim_end_matches(".service").to_lowercase();

    // 1. Critical Core System Services (Immortal: Cannot Stop, Disable, or Mask)
    let critical_services = [
        "dbus",
        "dbus-broker",
        "polkit",
        "systemd-journald",
        "systemd-logind",
        "systemd-udevd",
        "systemd-sysctl",
        "systemd-remount-fs",
        "systemd-tmpfiles-setup",
        "systemd-tmpfiles-setup-dev",
        "systemd-user-sessions",
        "systemd-modules-load",
        "emergency",
        "rescue",
        "init",
    ];

    if critical_services.iter().any(|&s| clean_name == s || clean_name.starts_with("systemd-journald") || clean_name.starts_with("systemd-udevd") || clean_name.starts_with("dbus")) {
        return (
            true,
            "critical".to_string(),
            Some("Critical operating system core component. Stopping, disabling, or masking will cause system instability or lockup.".to_string())
        );
    }

    // 2. Essential System Infrastructure (Guarded: Cannot Disable or Mask)
    let essential_services = [
        "networkmanager",
        "systemd-networkd",
        "systemd-resolved",
        "systemd-timesyncd",
        "sshd",
        "firewalld",
        "nftables",
        "iptables",
        "auditd",
        "chronyd",
        "chrony",
        "gdm",
        "sddm",
        "lightdm",
        "systemd-homed",
    ];

    if essential_services.iter().any(|&s| clean_name == s || clean_name.starts_with("networkmanager") || clean_name.starts_with("firewalld")) {
        return (
            true,
            "essential".to_string(),
            Some("Essential system infrastructure service. Masking or disabling will cause loss of network, security, or display access.".to_string())
        );
    }

    (false, String::new(), None)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameEntry {
    pub time_ms: u64,
    pub time_str: String,
    pub name: String,
    pub unit_type: String,
    pub is_service: bool,
    pub is_protected: bool,
    pub protection_level: String,
    pub protection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootTimeBreakdown {
    pub firmware_ms: u64,
    pub firmware_str: String,
    pub loader_ms: u64,
    pub loader_str: String,
    pub kernel_ms: u64,
    pub kernel_str: String,
    pub initrd_ms: u64,
    pub initrd_str: String,
    pub userspace_ms: u64,
    pub userspace_str: String,
    pub total_ms: u64,
    pub total_str: String,
    pub target_reached_str: String,
    pub raw_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalChainEntry {
    pub line: String,
    pub unit: String,
    pub active_at: String,
    pub duration: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceUnitStatus {
    pub name: String,
    pub active_state: String,
    pub sub_state: String,
    pub unit_file_state: String,
}

use std::sync::Mutex;
use std::time::{Duration, Instant};

static SYSTEM_UNIT_FILE_CACHE: Mutex<Option<(std::collections::HashMap<String, String>, Instant)>> = Mutex::new(None);
static USER_UNIT_FILE_CACHE: Mutex<Option<(std::collections::HashMap<String, String>, Instant)>> = Mutex::new(None);
const UNIT_FILE_CACHE_TTL: Duration = Duration::from_secs(60);

pub fn invalidate_unit_file_cache() {
    if let Ok(mut g) = SYSTEM_UNIT_FILE_CACHE.lock() { *g = None; }
    if let Ok(mut g) = USER_UNIT_FILE_CACHE.lock() { *g = None; }
}

fn resolve_service_alias(unit_name: &str, is_user: bool) -> Option<String> {
    let dirs = if is_user {
        vec![
            dirs::config_dir().map(|p| p.join("systemd/user")),
            Some(std::path::PathBuf::from("/etc/systemd/user")),
            Some(std::path::PathBuf::from("/usr/lib/systemd/user")),
        ]
    } else {
        vec![
            Some(std::path::PathBuf::from("/etc/systemd/system")),
            Some(std::path::PathBuf::from("/usr/lib/systemd/system")),
            Some(std::path::PathBuf::from("/lib/systemd/system")),
        ]
    };

    for dir_opt in dirs {
        if let Some(dir) = dir_opt {
            let path = dir.join(unit_name);
            if let Ok(target) = std::fs::read_link(&path) {
                if let Some(filename) = target.file_name().and_then(|f| f.to_str()) {
                    return Some(filename.to_string());
                }
            }
        }
    }
    None
}

/// Query specific services status quickly via systemctl show (instant ~15ms)
#[tauri::command]
pub async fn get_services_status(names: Vec<String>, user_mode: Option<bool>) -> Result<Vec<ServiceUnitStatus>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let is_user = user_mode.unwrap_or(false);
    let mut cmd = Command::new("systemctl");
    if is_user {
        cmd.arg("--user");
    }
    cmd.arg("show");
    cmd.arg("-p");
    cmd.arg("Id,Names,ActiveState,SubState,UnitFileState");
    for name in &names {
        cmd.arg(name);
    }

    let output = cmd.output().await.map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut results = Vec::new();
    let mut cur_names: Vec<String> = Vec::new();
    let mut cur_id = String::new();
    let mut cur_active = "inactive".to_string();
    let mut cur_sub = "dead".to_string();
    let mut cur_file_state = "unknown".to_string();

    let flush_block = |res: &mut Vec<ServiceUnitStatus>, id: &str, names: &[String], active: &str, sub: &str, file_state: &str| {
        let mut all_target_names = names.to_vec();
        if !id.is_empty() && !all_target_names.contains(&id.to_string()) {
            all_target_names.push(id.to_string());
        }
        for n in all_target_names {
            res.push(ServiceUnitStatus {
                name: n,
                active_state: active.to_string(),
                sub_state: sub.to_string(),
                unit_file_state: file_state.to_string(),
            });
        }
    };

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !cur_id.is_empty() || !cur_names.is_empty() {
                flush_block(&mut results, &cur_id, &cur_names, &cur_active, &cur_sub, &cur_file_state);
                cur_id.clear();
                cur_names.clear();
                cur_active = "inactive".to_string();
                cur_sub = "dead".to_string();
                cur_file_state = "unknown".to_string();
            }
            continue;
        }

        if let Some((k, v)) = line.split_once('=') {
            match k {
                "Id" => {
                    cur_id = v.to_string();
                    if !cur_id.is_empty() && !cur_names.contains(&cur_id) {
                        cur_names.push(cur_id.clone());
                    }
                }
                "Names" => {
                    for n in v.split_whitespace() {
                        let name_str = n.to_string();
                        if !cur_names.contains(&name_str) {
                            cur_names.push(name_str);
                        }
                    }
                }
                "ActiveState" => cur_active = v.to_string(),
                "SubState" => cur_sub = v.to_string(),
                "UnitFileState" => cur_file_state = v.to_string(),
                _ => {}
            }
        }
    }

    if !cur_id.is_empty() || !cur_names.is_empty() {
        flush_block(&mut results, &cur_id, &cur_names, &cur_active, &cur_sub, &cur_file_state);
    }

    Ok(results)
}

/// List all systemd units (system or user scope) with high performance concurrent parsing & cached unit-file states
#[tauri::command]
pub async fn list_all_units(filter: Option<String>, user_mode: Option<bool>) -> Result<Vec<ServiceUnit>, String> {
    use std::collections::HashMap;
    let is_user = user_mode.unwrap_or(false);

    // Check cached unit-file states
    let cached_files: Option<HashMap<String, String>> = {
        let guard = if is_user { USER_UNIT_FILE_CACHE.lock() } else { SYSTEM_UNIT_FILE_CACHE.lock() };
        if let Ok(ref g) = guard {
            if let Some((ref map, ref instant)) = **g {
                if instant.elapsed() < UNIT_FILE_CACHE_TTL {
                    Some(map.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    let (files_map, output_units) = match cached_files {
        Some(cached) => {
            // Fast-path: Only fetch active/loaded units (takes ~50-100ms!)
            let mut args_units = vec!["list-units", "--all", "--type=service", "--no-pager", "--no-legend", "--plain"];
            if is_user {
                args_units.push("--user");
            }
            let output_units = Command::new("systemctl")
                .args(&args_units)
                .output()
                .await
                .map_err(|e| e.to_string())?;

            (cached, output_units)
        }
        None => {
            // Cold path: Fetch both concurrently via tokio::join!
            let mut args_files = vec!["list-unit-files", "--type=service", "--no-pager", "--no-legend"];
            let mut args_units = vec!["list-units", "--all", "--type=service", "--no-pager", "--no-legend", "--plain"];
            if is_user {
                args_files.push("--user");
                args_units.push("--user");
            }

            let mut cmd_files = Command::new("systemctl");
            cmd_files.args(&args_files);
            let mut cmd_units = Command::new("systemctl");
            cmd_units.args(&args_units);

            let (res_files, res_units) = tokio::join!(
                cmd_files.output(),
                cmd_units.output()
            );

            let out_files = res_files.map_err(|e| e.to_string())?;
            let out_units = res_units.map_err(|e| e.to_string())?;

            let mut map = HashMap::with_capacity(512);
            let stdout_files = String::from_utf8_lossy(&out_files.stdout);
            for line in stdout_files.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(name), Some(state)) = (parts.next(), parts.next()) {
                    map.insert(name.to_string(), state.to_string());
                }
            }

            // Store in cache
            if let Ok(mut g) = if is_user { USER_UNIT_FILE_CACHE.lock() } else { SYSTEM_UNIT_FILE_CACHE.lock() } {
                *g = Some((map.clone(), Instant::now()));
            }

            (map, out_units)
        }
    };

    let mut all_services: HashMap<String, ServiceUnit> = HashMap::with_capacity(files_map.len() + 64);

    // Populate all known unit files
    for (name, state) in files_map.iter() {
        let (is_protected, protection_level, protection_reason) = if !is_user {
            get_service_protection(name)
        } else {
            (false, String::new(), None)
        };

        all_services.insert(
            name.clone(),
            ServiceUnit {
                name: name.clone(),
                load_state: "loaded".to_string(),
                active_state: "inactive".to_string(),
                sub_state: "dead".to_string(),
                description: String::new(),
                unit_file_state: state.clone(),
                is_protected,
                protection_level,
                protection_reason,
            },
        );
    }

    // Merge active/loaded units state and descriptions
    let stdout_units = String::from_utf8_lossy(&output_units.stdout);
    for line in stdout_units.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0];
        let load_state = parts[1];
        let active_state = parts[2];
        let sub_state = parts[3];
        let description = parts[4..].join(" ");

        if let Some(unit) = all_services.get_mut(name) {
            unit.load_state = load_state.to_string();
            unit.active_state = active_state.to_string();
            unit.sub_state = sub_state.to_string();
            unit.description = description;
        } else {
            let (is_protected, protection_level, protection_reason) = if !is_user {
                get_service_protection(name)
            } else {
                (false, String::new(), None)
            };

            all_services.insert(
                name.to_string(),
                ServiceUnit {
                    name: name.to_string(),
                    load_state: load_state.to_string(),
                    active_state: active_state.to_string(),
                    sub_state: sub_state.to_string(),
                    description,
                    unit_file_state: "generated".to_string(),
                    is_protected,
                    protection_level,
                    protection_reason,
                },
            );
        }
    }

    // Synchronize alias units with their target canonical services
    let alias_names: Vec<String> = all_services
        .iter()
        .filter(|(_, u)| u.unit_file_state == "alias" || u.active_state == "inactive")
        .map(|(k, _)| k.clone())
        .collect();

    for alias_name in alias_names {
        if let Some(target_name) = resolve_service_alias(&alias_name, is_user) {
            if let Some(target_unit) = all_services.get(&target_name) {
                let target_active = target_unit.active_state.clone();
                let target_sub = target_unit.sub_state.clone();
                let target_load = target_unit.load_state.clone();
                let target_desc = target_unit.description.clone();

                if let Some(alias_unit) = all_services.get_mut(&alias_name) {
                    alias_unit.active_state = target_active;
                    alias_unit.sub_state = target_sub;
                    alias_unit.load_state = target_load;
                    if alias_unit.description.is_empty() && !target_desc.is_empty() {
                        alias_unit.description = format!("{} (Alias -> {})", target_desc, target_name);
                    }
                }
            }
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

    // ── Security Protection Enforcement ──────────────────────────────────────────
    if !is_user {
        let (is_protected, protection_level, reason) = get_service_protection(&name);
        if is_protected {
            if protection_level == "critical" && matches!(action, ServiceAction::Mask | ServiceAction::Disable | ServiceAction::Stop) {
                let reason_msg = reason.unwrap_or_else(|| "This is a critical core system unit.".to_string());
                log_to_file("WARN", &format!("Blocked destructive action {action_str} on critical unit {name}"));
                return Err(format!("Security Protection: Action '{action_str}' is strictly blocked on critical system service '{name}'. {reason_msg}"));
            }

            if protection_level == "essential" && matches!(action, ServiceAction::Mask | ServiceAction::Disable) {
                let reason_msg = reason.unwrap_or_else(|| "This is an essential infrastructure service.".to_string());
                log_to_file("WARN", &format!("Blocked {action_str} on essential unit {name}"));
                return Err(format!("Security Protection: Disabling or masking '{name}' is restricted to prevent system lockout. {reason_msg}"));
            }
        }
    }

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

    if matches!(action, ServiceAction::Enable | ServiceAction::Disable | ServiceAction::Mask | ServiceAction::Unmask) {
        invalidate_unit_file_cache();
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

    if !is_user {
        let (is_protected, protection_level, _) = get_service_protection(&name);
        if is_protected && protection_level == "critical" && content.trim().is_empty() {
            return Err(format!("Security Violation: Writing empty override to critical system unit '{name}' is blocked."));
        }
    }

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

static BOOT_BLAME_CACHE: Mutex<Option<Vec<BlameEntry>>> = Mutex::new(None);
static BOOT_TIMES_CACHE: Mutex<Option<BootTimeBreakdown>> = Mutex::new(None);
static BOOT_CHAIN_CACHE: Mutex<Option<Vec<CriticalChainEntry>>> = Mutex::new(None);

fn get_current_boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn load_disk_boot_blame() -> Option<Vec<BlameEntry>> {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() {
        return None;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_blame_{}.json", boot_id));
    if let Ok(data) = std::fs::read_to_string(&cache_file) {
        if let Ok(entries) = serde_json::from_str::<Vec<BlameEntry>>(&data) {
            if !entries.is_empty() {
                return Some(entries);
            }
        }
    }
    None
}

fn save_disk_boot_blame(entries: &[BlameEntry]) {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() || entries.is_empty() {
        return;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_blame_{}.json", boot_id));
    if let Ok(json) = serde_json::to_string(entries) {
        let _ = std::fs::write(&cache_file, json);
    }
}

fn load_disk_boot_times() -> Option<BootTimeBreakdown> {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() {
        return None;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_times_{}.json", boot_id));
    if let Ok(data) = std::fs::read_to_string(&cache_file) {
        return serde_json::from_str(&data).ok();
    }
    None
}

fn save_disk_boot_times(times: &BootTimeBreakdown) {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() {
        return;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_times_{}.json", boot_id));
    if let Ok(json) = serde_json::to_string(times) {
        let _ = std::fs::write(&cache_file, json);
    }
}

fn load_disk_boot_chain() -> Option<Vec<CriticalChainEntry>> {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() {
        return None;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_chain_{}.json", boot_id));
    if let Ok(data) = std::fs::read_to_string(&cache_file) {
        return serde_json::from_str(&data).ok();
    }
    None
}

fn save_disk_boot_chain(chain: &[CriticalChainEntry]) {
    let boot_id = get_current_boot_id();
    if boot_id.is_empty() || chain.is_empty() {
        return;
    }
    let cache_file = std::env::temp_dir().join(format!("lcp_chain_{}.json", boot_id));
    if let Ok(json) = serde_json::to_string(chain) {
        let _ = std::fs::write(&cache_file, json);
    }
}

pub fn invalidate_boot_cache() {
    if let Ok(mut g) = BOOT_BLAME_CACHE.lock() { *g = None; }
    if let Ok(mut g) = BOOT_TIMES_CACHE.lock() { *g = None; }
    if let Ok(mut g) = BOOT_CHAIN_CACHE.lock() { *g = None; }
}

/// Fetch boot latency profiling data using systemd-analyze blame
#[tauri::command]
pub async fn get_boot_blame(force: Option<bool>) -> Result<Vec<BlameEntry>, String> {
    let is_force = force.unwrap_or(false);
    if !is_force {
        if let Ok(guard) = BOOT_BLAME_CACHE.lock() {
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        if let Some(disk_cached) = load_disk_boot_blame() {
            if let Ok(mut guard) = BOOT_BLAME_CACHE.lock() {
                *guard = Some(disk_cached.clone());
            }
            return Ok(disk_cached);
        }
    } else {
        if let Ok(mut g) = BOOT_BLAME_CACHE.lock() { *g = None; }
    }

    if !crate::binary_exists("systemd-analyze").await {
        return Err("systemd-analyze is not available on this system".to_string());
    }

    let cmd_future = async {
        Command::new("systemd-analyze")
            .arg("--no-pager")
            .arg("blame")
            .output()
            .await
    };

    let output = match ::tokio::time::timeout(std::time::Duration::from_secs(90), cmd_future).await {
        Ok(res) => res.map_err(|e| e.to_string())?,
        Err(_) => {
            // Fallback to disk or in-memory cache if timeout occurred
            if let Some(disk_cached) = load_disk_boot_blame() {
                return Ok(disk_cached);
            }
            if let Ok(guard) = BOOT_BLAME_CACHE.lock() {
                if let Some(ref cached) = *guard {
                    return Ok(cached.clone());
                }
            }
            return Err("systemd-analyze blame command timed out. Please try refreshing again.".to_string());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Failed to") || line.starts_with("Warning:") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let name = parts.last().unwrap().to_string();
        let time_part = parts[..parts.len() - 1].join(" ");
        let time_ms = parse_blame_time(&time_part);

        // Skip non-time lines that might be error outputs
        if time_ms == 0 && !time_part.contains("ms") && !time_part.contains('s') {
            continue;
        }

        // Determine unit type
        let unit_type = if name.ends_with(".service") {
            "service".to_string()
        } else if name.ends_with(".device") {
            "device".to_string()
        } else if name.ends_with(".mount") {
            "mount".to_string()
        } else if name.ends_with(".target") {
            "target".to_string()
        } else if name.ends_with(".socket") {
            "socket".to_string()
        } else if name.ends_with(".slice") {
            "slice".to_string()
        } else {
            "other".to_string()
        };

        let is_service = unit_type == "service";

        // Determine protection level
        let (is_protected, protection_level, protection_reason) = if !is_service {
            (
                true,
                "critical".to_string(),
                Some(format!("System {} unit. Core infrastructure cannot be disabled.", unit_type))
            )
        } else {
            get_service_protection(&name)
        };

        entries.push(BlameEntry {
            time_ms,
            time_str: time_part,
            name,
            unit_type,
            is_service,
            is_protected,
            protection_level,
            protection_reason,
        });
    }

    if entries.is_empty() && !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        // Fallback to cache if available
        if let Some(disk_cached) = load_disk_boot_blame() {
            return Ok(disk_cached);
        }
        if let Ok(guard) = BOOT_BLAME_CACHE.lock() {
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        return Err(if err_msg.is_empty() { "systemd-analyze blame returned empty output".to_string() } else { err_msg.to_string() });
    }

    save_disk_boot_blame(&entries);

    if let Ok(mut guard) = BOOT_BLAME_CACHE.lock() {
        *guard = Some(entries.clone());
    }

    Ok(entries)
}

/// Fetch boot time breakdown from systemd-analyze time
#[tauri::command]
pub async fn get_boot_times(force: Option<bool>) -> Result<BootTimeBreakdown, String> {
    let is_force = force.unwrap_or(false);
    if !is_force {
        if let Ok(guard) = BOOT_TIMES_CACHE.lock() {
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        if let Some(disk_cached) = load_disk_boot_times() {
            if let Ok(mut guard) = BOOT_TIMES_CACHE.lock() {
                *guard = Some(disk_cached.clone());
            }
            return Ok(disk_cached);
        }
    } else {
        if let Ok(mut g) = BOOT_TIMES_CACHE.lock() { *g = None; }
    }

    if !crate::binary_exists("systemd-analyze").await {
        return Err("systemd-analyze is not available".to_string());
    }

    let cmd_future = async {
        Command::new("systemd-analyze")
            .arg("--no-pager")
            .arg("time")
            .output()
            .await
    };

    let output = match ::tokio::time::timeout(std::time::Duration::from_secs(12), cmd_future).await {
        Ok(res) => res.map_err(|e| e.to_string())?,
        Err(_) => {
            if let Some(disk_cached) = load_disk_boot_times() {
                return Ok(disk_cached);
            }
            if let Ok(guard) = BOOT_TIMES_CACHE.lock() {
                if let Some(ref cached) = *guard {
                    return Ok(cached.clone());
                }
            }
            return Err("systemd-analyze time command timed out".to_string());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut firmware_ms = 0;
    let mut firmware_str = "0ms".to_string();
    let mut loader_ms = 0;
    let mut loader_str = "0ms".to_string();
    let mut kernel_ms = 0;
    let mut kernel_str = "0ms".to_string();
    let mut initrd_ms = 0;
    let mut initrd_str = "0ms".to_string();
    let mut userspace_ms = 0;
    let mut userspace_str = "0ms".to_string();
    let mut total_ms = 0;
    let mut total_str = "0ms".to_string();
    let mut target_reached_str = String::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Startup finished in") {
            if let Some((_, rest)) = trimmed.split_once("Startup finished in ") {
                if let Some((phases, total_part)) = rest.split_once(" = ") {
                    total_str = total_part.trim().to_string();
                    total_ms = parse_blame_time(&total_str);

                    for phase in phases.split('+') {
                        let p = phase.trim();
                        if p.contains("(firmware)") {
                            let val = p.replace("(firmware)", "").trim().to_string();
                            firmware_ms = parse_blame_time(&val);
                            firmware_str = val;
                        } else if p.contains("(loader)") {
                            let val = p.replace("(loader)", "").trim().to_string();
                            loader_ms = parse_blame_time(&val);
                            loader_str = val;
                        } else if p.contains("(kernel)") {
                            let val = p.replace("(kernel)", "").trim().to_string();
                            kernel_ms = parse_blame_time(&val);
                            kernel_str = val;
                        } else if p.contains("(initrd)") {
                            let val = p.replace("(initrd)", "").trim().to_string();
                            initrd_ms = parse_blame_time(&val);
                            initrd_str = val;
                        } else if p.contains("(userspace)") {
                            let val = p.replace("(userspace)", "").trim().to_string();
                            userspace_ms = parse_blame_time(&val);
                            userspace_str = val;
                        }
                    }
                }
            }
        } else if trimmed.contains("target reached after") || trimmed.contains("reached after") {
            target_reached_str = trimmed.to_string();
        }
    }

    let res = BootTimeBreakdown {
        firmware_ms,
        firmware_str,
        loader_ms,
        loader_str,
        kernel_ms,
        kernel_str,
        initrd_ms,
        initrd_str,
        userspace_ms,
        userspace_str,
        total_ms,
        total_str,
        target_reached_str,
        raw_summary: stdout.trim().to_string(),
    };

    save_disk_boot_times(&res);

    if let Ok(mut guard) = BOOT_TIMES_CACHE.lock() {
        *guard = Some(res.clone());
    }

    Ok(res)
}

/// Fetch critical chain dependency breakdown
#[tauri::command]
pub async fn get_boot_critical_chain(force: Option<bool>) -> Result<Vec<CriticalChainEntry>, String> {
    let is_force = force.unwrap_or(false);
    if !is_force {
        if let Ok(guard) = BOOT_CHAIN_CACHE.lock() {
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        if let Some(disk_cached) = load_disk_boot_chain() {
            if let Ok(mut guard) = BOOT_CHAIN_CACHE.lock() {
                *guard = Some(disk_cached.clone());
            }
            return Ok(disk_cached);
        }
    } else {
        if let Ok(mut g) = BOOT_CHAIN_CACHE.lock() { *g = None; }
    }

    if !crate::binary_exists("systemd-analyze").await {
        return Err("systemd-analyze is not available".to_string());
    }

    let cmd_future = async {
        Command::new("systemd-analyze")
            .arg("--no-pager")
            .arg("critical-chain")
            .output()
            .await
    };

    let output = match ::tokio::time::timeout(std::time::Duration::from_secs(15), cmd_future).await {
        Ok(res) => res.map_err(|e| e.to_string())?,
        Err(_) => {
            if let Some(disk_cached) = load_disk_boot_chain() {
                return Ok(disk_cached);
            }
            if let Ok(guard) = BOOT_CHAIN_CACHE.lock() {
                if let Some(ref cached) = *guard {
                    return Ok(cached.clone());
                }
            }
            return Err("systemd-analyze critical-chain timed out".to_string());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("The time") || trimmed.is_empty() || trimmed.starts_with("Failed to") {
            continue;
        }

        // Count leading indentation / tree symbols safely with char vectors
        let mut depth = 0;
        let mut char_idx = 0;
        let chars: Vec<char> = line.chars().collect();
        while char_idx < chars.len() {
            let c = chars[char_idx];
            if c == ' ' || c == '│' || c == '└' || c == '─' || c == '├' || c == '|' || c == '`' || c == '-' {
                if c == '└' || c == '├' || c == '`' {
                    depth += 1;
                }
                char_idx += 1;
            } else {
                break;
            }
        }

        let content_str: String = chars[char_idx..].iter().collect();
        let content = content_str.trim();
        if content.is_empty() {
            continue;
        }

        let mut unit = content.to_string();
        let mut active_at = String::new();
        let mut duration = String::new();

        if let Some((u, rest)) = content.split_once('@') {
            unit = u.trim().to_string();
            if let Some((act, dur)) = rest.split_once('+') {
                active_at = act.trim().to_string();
                duration = dur.trim().to_string();
            } else {
                active_at = rest.trim().to_string();
            }
        }

        entries.push(CriticalChainEntry {
            line: line.to_string(),
            unit,
            active_at,
            duration,
            depth,
        });
    }

    save_disk_boot_chain(&entries);

    if let Ok(mut guard) = BOOT_CHAIN_CACHE.lock() {
        *guard = Some(entries.clone());
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
