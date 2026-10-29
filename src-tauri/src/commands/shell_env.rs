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

/// The profile files the preview may execute. This is exactly the list the
/// UI shows (`shell_list_profile_files`), so the renderer can not name an
/// arbitrary file to be sourced.
fn ensure_approved_source_file(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Refusing to source an empty path.".to_string());
    }
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(format!("Refusing to source '{path}': the path must be absolute."));
    }
    if candidate
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir | std::path::Component::CurDir))
    {
        return Err(format!(
            "Refusing to source '{path}': the path contains relative components."
        ));
    }
    if !all_profile_files().iter().any(|approved| approved == candidate) {
        return Err(format!(
            "Refusing to source '{path}': not one of the profile files this panel manages."
        ));
    }
    if !candidate.is_file() {
        return Err(format!("Refusing to source '{path}': not a regular file."));
    }
    Ok(())
}

/// The argv for the preview shell. The file path is a **positional parameter**
/// (`$1`) of the script, never spliced into the command text — shell
/// metacharacters, separators and substitutions in a path can not be
/// interpreted.
fn source_file_argv(path: &str) -> Vec<String> {
    vec![
        String::from("-c"),
        String::from("source \"$1\" && env"),
        String::from("lcp-source"), // $0
        path.to_string(),           // $1
    ]
}

/// Turn `env` output into preview rows: each value's definition is taken from
/// the sourced file and `in_sync` is a real comparison — never a hard-coded
/// `true`. Same rule `shell_get_live_env` uses for its preview.
fn live_env_with_definitions(
    stdout: &str,
    definitions: &[(String, String)],
    path: &str,
) -> Vec<LiveEnvVar> {
    stdout
        .lines()
        .filter_map(|line| {
            let eq = line.find('=')?;
            let name = line[..eq].to_string();
            let live_value = line[eq + 1..].to_string();
            let defined_value = definitions
                .iter()
                .find(|(defined_name, _)| defined_name == &name)
                .map(|(_, value)| value.clone());
            let in_sync = defined_value
                .as_ref()
                .map(|defined| defined == &live_value)
                .unwrap_or(true);
            Some(LiveEnvVar {
                name,
                live_value,
                defined_value,
                in_sync,
                source_path: Some(path.to_string()),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn shell_source_file(path: String) -> Result<Vec<LiveEnvVar>, String> {
    // Only the panel's own profile files may be executed by the preview.
    ensure_approved_source_file(&path)?;

    // The path travels as a positional argument, so it is never parsed by a
    // shell (previously: `format!("source {path} && env")` — injection).
    let out = Command::new("bash")
        .args(source_file_argv(&path))
        .output()
        .await
        .map_err(|e| format!("Failed to source {path}: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read the sourced file {path}: {e}"))?;
    let definitions: Vec<(String, String)> = parse_exports_from_content(&content, &path)
        .into_iter()
        .map(|var| (var.name, var.value))
        .collect();

    Ok(live_env_with_definitions(
        &String::from_utf8_lossy(&out.stdout),
        &definitions,
        &path,
    ))
}

// ─── tests: shell_source_file (no shell interpolation, real in_sync) ────────

#[cfg(test)]
mod shell_source_file_tests {
    use super::*;

    #[test]
    fn shell_source_file_argv_keeps_the_path_out_of_the_command_text() {
        let payload = "/tmp/a; touch /tmp/pwned $(id) `id` $HOME 'q' \"dq\" && rm -rf /";
        let argv = source_file_argv(payload);
        assert_eq!(argv.len(), 4);
        assert_eq!(argv[0], "-c");
        assert_eq!(argv[1], "source \"$1\" && env");
        assert!(
            !argv[1].contains(payload) && !argv[1].contains("; touch"),
            "the path must never be spliced into the script text: {:?}",
            argv[1]
        );
        assert_eq!(argv[2], "lcp-source");
        assert_eq!(argv[3], payload, "the path is passed verbatim as $1");
    }

    #[tokio::test]
    async fn shell_source_file_missing_or_unapproved_path_returns_err() {
        for path in [
            String::new(),
            String::from("/nonexistent/lcp-definitely-missing.sh"),
            String::from("relative/profile.sh"),
            String::from("/etc/profile.d/../../etc/shadow"),
            String::from("/etc/shadow"),
            String::from("/tmp/attacker-supplied.sh"),
        ] {
            let result = shell_source_file(path.clone()).await;
            assert!(result.is_err(), "must refuse {path:?}, got {result:?}");
        }
    }

    #[tokio::test]
    async fn shell_source_file_does_not_execute_metacharacters_in_the_path() {
        let marker = std::env::temp_dir().join(format!(
            "lcp-source-injection-marker-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        for payload in [
            format!("/tmp/x.sh; touch {}", marker.display()),
            format!("/tmp/x.sh && touch {}", marker.display()),
            format!("/tmp/x.sh $(touch {})", marker.display()),
            format!("/tmp/x.sh `touch {}`", marker.display()),
            format!("/tmp/x.sh | touch {}", marker.display()),
        ] {
            assert!(shell_source_file(payload).await.is_err());
            assert!(
                !marker.exists(),
                "a shell metacharacter in the path was executed"
            );
        }
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn shell_source_file_computes_in_sync_from_the_sourced_definitions() {
        let stdout = "LCP_MATCH=one\nLCP_DIFF=two\nLCP_UNDEFINED=three\n";
        let definitions = vec![
            (String::from("LCP_MATCH"), String::from("one")),
            (String::from("LCP_DIFF"), String::from("different")),
        ];
        let vars = live_env_with_definitions(stdout, &definitions, "/etc/profile.d/x.sh");
        let by = |name: &str| vars.iter().find(|var| var.name == name).unwrap();

        assert!(by("LCP_MATCH").in_sync, "defined == live must be in sync");
        assert!(!by("LCP_DIFF").in_sync, "defined != live must not be in sync");
        assert!(by("LCP_UNDEFINED").in_sync, "no definition means nothing to diverge from");
        assert_eq!(by("LCP_DIFF").defined_value.as_deref(), Some("different"));
        assert_eq!(by("LCP_MATCH").source_path.as_deref(), Some("/etc/profile.d/x.sh"));
    }

    #[test]
    fn shell_source_file_definitions_come_from_real_export_lines() {
        let content = "# comment\nexport LCP_ALPHA=\"one\"\nexport PATH=\"$PATH:/opt/bin\"\nexport LCP_BETA=two\n";
        let definitions: Vec<(String, String)> =
            parse_exports_from_content(content, "/etc/profile.d/x.sh")
                .into_iter()
                .map(|var| (var.name, var.value))
                .collect();
        assert!(definitions.iter().any(|(name, _)| name == "LCP_ALPHA"));
        assert!(definitions.iter().any(|(name, _)| name == "LCP_BETA"));
        assert!(
            !definitions.iter().any(|(name, _)| name == "PATH"),
            "PATH extensions are handled by the PATH tab"
        );

        let vars = live_env_with_definitions("LCP_ALPHA=one\nLCP_BETA=two\n", &definitions, "/x");
        assert!(vars.iter().all(|var| var.in_sync));
    }
}
