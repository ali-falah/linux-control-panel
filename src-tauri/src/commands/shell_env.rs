use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::utils::privilege::tokio::Command;

// ─── Structs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellVar {
    pub name: String,
    pub value: String,
    pub raw_line: String,
    pub line_number: usize,
    pub source_path: String,
    pub live_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellVarGroup {
    pub source_path: String,
    pub display_name: String,
    pub is_system: bool,
    pub vars: Vec<ShellVar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileFile {
    pub path: String,
    pub display_name: String,
    pub last_modified: String,
    pub writable: bool,
    pub line_count: usize,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathEntry {
    pub directory: String,
    pub source_path: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellBackup {
    pub backup_path: String,
    pub original_path: String,
    pub timestamp: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveEnvVar {
    pub name: String,
    pub live_value: String,
    pub defined_value: Option<String>,
    pub in_sync: bool,
    pub source_path: Option<String>,
}

// ─── Constants ────────────────────────────────────────────────────────────────

const CRITICAL_VARS: &[&str] = &["PATH", "HOME", "USER", "SHELL", "LOGNAME", "UID", "GID"];
const PROFILE_D: &str = "/etc/profile.d";

fn user_profile_files() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
    vec![
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".profile"),
    ]
}

fn all_profile_files() -> Vec<PathBuf> {
    let mut files = user_profile_files();
    // append /etc/profile.d/*.sh
    if let Ok(rd) = std::fs::read_dir(PROFILE_D) {
        let mut sys: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "sh").unwrap_or(false))
            .collect();
        sys.sort();
        files.extend(sys);
    }
    files
}

fn is_system_file(path: &Path) -> bool {
    path.starts_with("/etc")
}

fn display_name(path: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
    if let Ok(rel) = path.strip_prefix(&home) {
        format!("~/{}", rel.display())
    } else {
        path.display().to_string()
    }
}

/// Parse `export KEY=VALUE` or `export KEY="VALUE"` lines from a string.
fn parse_exports_from_content(content: &str, source_path: &str) -> Vec<ShellVar> {
    let mut vars = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // match "export KEY=..." (with or without quotes, with or without spaces)
        if let Some(rest) = trimmed.strip_prefix("export ") {
            let rest = rest.trim();
            if let Some(eq_pos) = rest.find('=') {
                let name = rest[..eq_pos].trim().to_string();
                // Skip lines like  export PATH="$PATH:..."  — those are PATH extensions handled in PATH tab
                if name == "PATH" {
                    continue;
                }
                // Validate name is a legal identifier
                if !is_valid_var_name(&name) {
                    continue;
                }
                let raw_value = rest[eq_pos + 1..].trim().to_string();
                let value = raw_value
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                vars.push(ShellVar {
                    name,
                    value,
                    raw_line: trimmed.to_string(),
                    line_number: idx + 1,
                    source_path: source_path.to_string(),
                    live_value: None,
                });
            }
        }
    }
    vars
}

fn is_valid_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn backup_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    let dir = base.join("control-panel").join("backups");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

async fn backup_file(path: &str) -> Result<String, String> {
    let file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_path = backup_dir().join(format!("{file_name}.{timestamp}.bak"));

    if is_system_file(Path::new(path)) {
        let out = Command::new("pkexec")
            .args(["cp", path, backup_path.to_str().unwrap_or("")])
            .output()
            .await
            .map_err(|e| format!("Failed to backup: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
    } else {
        std::fs::copy(path, &backup_path)
            .map_err(|e| format!("Failed to backup: {e}"))?;
    }

    Ok(backup_path.to_str().unwrap_or("").to_string())
}

async fn write_file_content(path: &str, content: &str) -> Result<(), String> {
    if is_system_file(Path::new(path)) {
        crate::utils::privilege::write_file_as_root(path, content).await
    } else {
        std::fs::write(path, content).map_err(|e| format!("Failed to write file: {e}"))
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn shell_list_profile_files() -> Result<Vec<ProfileFile>, String> {
    let mut result = Vec::new();
    for path in all_profile_files() {
        let path_str = path.to_str().unwrap_or("").to_string();
        let meta = std::fs::metadata(&path);
        let (last_modified, line_count, writable) = if let Ok(m) = &meta {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    let secs = d.as_secs() as i64;
                    chrono::DateTime::from_timestamp(secs, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "—".to_string())
                })
                .unwrap_or_else(|| "—".to_string());
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let lc = content.lines().count();
            // writable: user files always true; system files — check if we'd need pkexec
            let w = !is_system_file(&path);
            (modified, lc, w)
        } else {
            ("(not found)".to_string(), 0, false)
        };
        result.push(ProfileFile {
            display_name: display_name(&path),
            is_system: is_system_file(&path),
            path: path_str,
            last_modified,
            writable,
            line_count,
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn shell_read_profile_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))
}

#[tauri::command]
pub async fn shell_parse_all_exports() -> Result<Vec<ShellVarGroup>, String> {
    let mut groups = Vec::new();
    for path in all_profile_files() {
        let path_str = path.to_str().unwrap_or("").to_string();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let vars = parse_exports_from_content(&content, &path_str);
        if !vars.is_empty() || path.exists() {
            groups.push(ShellVarGroup {
                display_name: display_name(&path),
                is_system: is_system_file(&path),
                source_path: path_str,
                vars,
            });
        }
    }
    Ok(groups)
}

#[tauri::command]
pub async fn shell_get_live_value(name: String) -> Result<String, String> {
    let out = Command::new("printenv")
        .arg(&name)
        .output()
        .await
        .map_err(|e| format!("Failed to run printenv: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tauri::command]
pub async fn shell_write_var(
    path: String,
    name: String,
    value: String,
    old_line: Option<String>,
) -> Result<(), String> {
    // Safety: validate name
    if !is_valid_var_name(&name) {
        return Err(format!("Invalid variable name: '{name}'. Must match [A-Za-z_][A-Za-z0-9_]*"));
    }
    if CRITICAL_VARS.contains(&name.as_str()) && name == "PATH" {
        return Err(format!("'{name}' is a critical system variable. Use the PATH Manager tab to modify it."));
    }
    if !std::path::Path::new(&path).exists()
        && !std::path::Path::new(&path).starts_with("/etc")
    {
        // create user file if not exists
        std::fs::write(&path, "").map_err(|e| format!("Cannot create {path}: {e}"))?;
    }

    backup_file(&path).await?;

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let new_line = format!("export {name}=\"{value}\"");

    let new_content = if let Some(old) = &old_line {
        // Rewrite the specific line
        content
            .lines()
            .map(|l| if l.trim() == old.trim() { new_line.as_str() } else { l })
            .collect::<Vec<_>>()
            .join("\n")
            + if content.ends_with('\n') { "\n" } else { "" }
    } else {
        // Append to end
        if content.is_empty() || content.ends_with('\n') {
            format!("{content}{new_line}\n")
        } else {
            format!("{content}\n{new_line}\n")
        }
    };

    write_file_content(&path, &new_content).await
}

#[tauri::command]
pub async fn shell_delete_var(path: String, raw_line: String) -> Result<(), String> {
    backup_file(&path).await?;
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let new_content = content
        .lines()
        .filter(|l| l.trim() != raw_line.trim())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_file_content(&path, &new_content).await
}

#[tauri::command]
pub async fn shell_write_profile_file(path: String, content: String) -> Result<(), String> {
    backup_file(&path).await?;
    write_file_content(&path, &content).await
}

#[tauri::command]
pub async fn shell_create_profile_d_file(name: String) -> Result<String, String> {
    // Enforce .sh extension
    let clean = name.trim().trim_end_matches(".sh");
    if clean.is_empty() {
        return Err("File name cannot be empty".to_string());
    }
    let path = format!("{PROFILE_D}/{clean}.sh");
    let content = "#!/usr/bin/env bash\n# Created by Linux Control Panel\n";
    write_file_content(&path, content).await?;
    let _ = crate::utils::privilege::tokio::Command::new("pkexec")
        .args(["chmod", "644", &path])
        .output()
        .await;
    Ok(path)
}

#[tauri::command]
pub async fn shell_list_backups() -> Result<Vec<ShellBackup>, String> {
    let dir = backup_dir();
    let mut backups = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let bpath = entry.path();
            if bpath.extension().map(|e| e == "bak").unwrap_or(false) {
                let filename = bpath.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                // filename is like ".bashrc.20240101_123456.bak"
                // try to extract original filename (everything before the last two dot-segments)
                let parts: Vec<&str> = filename.rsplitn(3, '.').collect();
                let original_name = if parts.len() >= 3 { parts[2].to_string() } else { filename.clone() };
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
                let original_path = home.join(&original_name).to_str().unwrap_or("").to_string();

                let meta = entry.metadata().ok();
                let timestamp = meta.as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                            .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "—".to_string())
                    })
                    .unwrap_or_else(|| "—".to_string());

                backups.push(ShellBackup {
                    backup_path: bpath.to_str().unwrap_or("").to_string(),
                    original_path,
                    timestamp,
                    filename,
                });
            }
        }
    }
    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(backups)
}

#[tauri::command]
pub async fn shell_restore_backup(backup_path: String, original_path: String) -> Result<(), String> {
    backup_file(&original_path).await?;
    if is_system_file(Path::new(&original_path)) {
        let out = Command::new("pkexec")
            .args(["cp", &backup_path, &original_path])
            .output()
            .await
            .map_err(|e| format!("pkexec cp failed: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
    } else {
        std::fs::copy(&backup_path, &original_path)
            .map_err(|e| format!("Failed to restore: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn shell_parse_path() -> Result<Vec<PathEntry>, String> {
    let path_val = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));

    // Try to find which profile file each entry comes from
    let all_contents: Vec<(String, String)> = all_profile_files()
        .into_iter()
        .filter_map(|p| {
            let s = p.to_str().unwrap_or("").to_string();
            std::fs::read_to_string(&p).ok().map(|c| (s, c))
        })
        .collect();

    let entries: Vec<PathEntry> = path_val
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|dir| {
            // Expand ~ manually
            let expanded = if dir.starts_with("~/") {
                home.join(&dir[2..]).to_str().unwrap_or(dir).to_string()
            } else {
                dir.to_string()
            };
            let exists = std::path::Path::new(&expanded).exists();

            // Find source file
            let source = all_contents.iter().find_map(|(path, content)| {
                for line in content.lines() {
                    let t = line.trim();
                    if t.contains("PATH") && t.contains(dir) && !t.starts_with('#') {
                        return Some(path.clone());
                    }
                }
                None
            });

            PathEntry {
                directory: expanded,
                source_path: source,
                exists,
            }
        })
        .collect();

    Ok(entries)
}

#[tauri::command]
pub async fn shell_add_path_entry(directory: String, profile_path: String) -> Result<(), String> {
    backup_file(&profile_path).await?;
    let content = std::fs::read_to_string(&profile_path).unwrap_or_default();
    let new_line = format!("export PATH=\"$PATH:{directory}\"");
    let new_content = if content.is_empty() || content.ends_with('\n') {
        format!("{content}{new_line}\n")
    } else {
        format!("{content}\n{new_line}\n")
    };
    write_file_content(&profile_path, &new_content).await
}

#[tauri::command]
pub async fn shell_remove_path_entry(directory: String, profile_path: String) -> Result<(), String> {
    backup_file(&profile_path).await?;
    let content = std::fs::read_to_string(&profile_path).map_err(|e| e.to_string())?;
    let new_content = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !(t.contains("PATH") && t.contains(&directory) && !t.starts_with('#'))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_file_content(&profile_path, &new_content).await
}

#[tauri::command]
pub async fn shell_get_live_env() -> Result<Vec<LiveEnvVar>, String> {
    // Run `bash -l -c env` to get a login-shell environment
    let out = Command::new("bash")
        .args(["-l", "-c", "env"])
        .output()
        .await
        .map_err(|e| format!("Failed to run env: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();

    // Also get all defined vars from profile files for comparison
    let mut defined_map: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    for path in all_profile_files() {
        let path_str = path.to_str().unwrap_or("").to_string();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        for var in parse_exports_from_content(&content, &path_str) {
            defined_map.entry(var.name.clone()).or_insert((var.value.clone(), path_str.clone()));
        }
    }

    let vars: Vec<LiveEnvVar> = stdout
        .lines()
        .filter_map(|line| {
            let eq = line.find('=')?;
            let name = line[..eq].to_string();
            let live_value = line[eq + 1..].to_string();
            let (defined_value, source_path) = if let Some((v, s)) = defined_map.get(&name) {
                (Some(v.clone()), Some(s.clone()))
            } else {
                (None, None)
            };
            let in_sync = defined_value.as_ref().map(|d| d == &live_value).unwrap_or(true);
            Some(LiveEnvVar { name, live_value, defined_value, in_sync, source_path })
        })
        .collect();

    Ok(vars)
}

#[tauri::command]
pub async fn shell_source_file(path: String) -> Result<Vec<LiveEnvVar>, String> {
    let out = Command::new("bash")
        .args(["-c", &format!("source {path} && env")])
        .output()
        .await
        .map_err(|e| format!("Failed to source {path}: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let vars = stdout
        .lines()
        .filter_map(|line| {
            let eq = line.find('=')?;
            Some(LiveEnvVar {
                name: line[..eq].to_string(),
                live_value: line[eq + 1..].to_string(),
                defined_value: None,
                in_sync: true,
                source_path: Some(path.clone()),
            })
        })
        .collect();

    Ok(vars)
}
