use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::utils::privilege::tokio::Command;
use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};
use crate::utils::root_policy::{
    restore_profile_sh_backup_shape, validate_profile_sh_content, validate_profile_sh_path,
    PROFILE_D_DIR,
};

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
    if let Ok(rd) = std::fs::read_dir(PROFILE_D_DIR) {
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
    backup_file_into(path, &backup_dir()).await
}

async fn backup_file_into(path: &str, backup_path_dir: &Path) -> Result<String, String> {
    let file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_path = backup_path_dir.join(format!("{file_name}.{timestamp}.bak"));

    if is_system_file(Path::new(path)) {
        // No standalone privileged copy: the root owned helper keeps its
        // revision `.bak` itself exactly when it publishes the write
        // (backup-on-write in `install_root_owned_file`), and the script
        // channel of `write_file_as_root` keeps one for the legacy /etc
        // targets. Nothing here may ever run with root rights, so the empty
        // return only records "the originating write keeps the backup".
        return Ok(String::new());
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

/// Is `path` a direct child of the root-owned profile directory? Only those
/// paths are allowed to take the privileged `lcp-admin` route; everything else
/// (the user's own `~/.bashrc` and friends) keeps the unprivileged writer.
fn profile_d_name(path: &str) -> Option<String> {
    let candidate = Path::new(path);
    if candidate.parent() != Some(Path::new(PROFILE_D_DIR)) {
        return None;
    }
    candidate
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// Turn a bare file name into the one privileged destination this panel may
/// write. The name is rejected before any path is built (a separator can not
/// smuggle in a second component) and the resulting path is then checked by
/// the same `validate_profile_sh_path` the helper uses: fixed parent, `.sh`
/// suffix, safe characters, no relative parts, no symlink in any component.
fn profile_d_target(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The profile file name must not be empty.".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "'{name}' is not a plain profile file name (separators are not allowed)."
        ));
    }
    let path = format!("{PROFILE_D_DIR}/{name}");
    validate_profile_sh_path(&path)?;
    Ok(path)
}

/// Parse the helper's stdout: exactly one line, the absolute path of the
/// revision backup, or an empty stdout for a brand-new file. Anything else is
/// a protocol violation and is refused rather than displayed.
fn helper_backup_path(stdout: &str) -> Result<Option<String>, String> {
    let line = stdout.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if line.lines().count() != 1 {
        return Err(format!(
            "The administrator helper returned unexpected output instead of one backup path: {line}"
        ));
    }
    let candidate = Path::new(line);
    if candidate.parent() != Some(Path::new(PROFILE_D_DIR)) || !line.ends_with(".bak") {
        return Err(format!(
            "The administrator helper reported an unexpected backup path: {line}"
        ));
    }
    Ok(Some(line.to_string()))
}

/// The exact line the PATH tab appends. Kept in one place so the content
/// policy test checks precisely what the command writes.
fn path_export_line(directory: &str) -> String {
    format!("export PATH=\"$PATH:{directory}\"")
}

/// Append one PATH extension, preserving the file's final newline shape.
fn content_with_path_entry(content: &str, directory: &str) -> String {
    let new_line = path_export_line(directory);
    if content.is_empty() || content.ends_with('\n') {
        format!("{content}{new_line}\n")
    } else {
        format!("{content}\n{new_line}\n")
    }
}

/// Drop every uncommented `PATH` line mentioning `directory`.
fn content_without_path_entry(content: &str, directory: &str) -> String {
    content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !(t.contains("PATH") && t.contains(directory) && !t.starts_with('#'))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Write one `/etc/profile.d/<name>.sh` file through the dedicated root
/// helper action. The name goes through `validate_profile_sh_path`, the content
/// through `validate_profile_sh_content`, both **before** the privileged call;
/// the helper validates both again. The content travels on stdin, so no value
/// is ever part of a command string. Returns the helper-made backup path
/// (`None` when the file did not exist yet).
async fn write_profile_d_as_root(name: &str, content: &str) -> Result<Option<String>, String> {
    let path = profile_d_target(name)?;
    validate_profile_sh_content(content)?;

    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the profile file was not written."
            .to_string()
    })?;
    let args = vec![String::from("write_profile_sh"), path];

    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, content).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    helper_backup_path(&String::from_utf8_lossy(&output.stdout))
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

    // `/etc/profile.d` is written by the root helper, which makes its own
    // backup; every other profile file is written directly as the user.
    let profile_file = profile_d_name(&path);
    if profile_file.is_none() {
        backup_file(&path).await?;
    }

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

    match profile_file {
        Some(file_name) => write_profile_d_as_root(&file_name, &new_content).await.map(|_| ()),
        None => write_file_content(&path, &new_content).await,
    }
}

#[tauri::command]
pub async fn shell_delete_var(path: String, raw_line: String) -> Result<(), String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let new_content = content
        .lines()
        .filter(|l| l.trim() != raw_line.trim())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    match profile_d_name(&path) {
        Some(file_name) => write_profile_d_as_root(&file_name, &new_content).await.map(|_| ()),
        None => {
            backup_file(&path).await?;
            write_file_content(&path, &new_content).await
        }
    }
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
    let file_name = format!("{clean}.sh");
    // Validates the name and the destination before any privileged call.
    let path = profile_d_target(&file_name)?;
    let content = "#!/usr/bin/env bash\n# Created by Linux Control Panel\n";
    // The helper installs the file as 0644 itself; no separate root chmod.
    write_profile_d_as_root(&file_name, content).await?;
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

/// A user-owned backup must be one this app created itself: it must live in
/// the panel's own backups directory and be named exactly
/// `<basename-of-original>.<YYYYMMDD>_<HHMMSS>.bak`. Nothing that merely
/// claims to be "a backup" is followed — the name must derive from the
/// destination and carry the strict timestamp shape `backup_file` writes.
fn valid_home_backup_in(
    original_path: &str,
    backup_path: &str,
    backups_dir: &Path,
) -> Result<(), String> {
    let digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    let candidate = Path::new(backup_path);
    if candidate.parent() != Some(backups_dir) {
        return Err(format!(
            "'{backup_path}' is not a backup this panel keeps in its own backups directory."
        ));
    }
    let original_name = Path::new(original_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let backup_name = candidate
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(revision) = backup_name
        .strip_prefix(&original_name)
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rev| rev.strip_suffix(".bak"))
    else {
        return Err(format!(
            "The backup file name {backup_name:?} must start with the destination's file name {original_name:?} and carry a '.<timestamp>.bak' revision."
        ));
    };
    // <YYYYMMDD>_<HHMMSS> — exactly what `backup_file` writes
    let Some((date, time)) = revision.split_once('_') else {
        return Err(format!(
            "The backup revision {revision:?} must be <YYYYMMDD>_<HHMMSS>."
        ));
    };
    if date.len() != 8 || time.len() != 6 || !digits(date) || !digits(time) {
        return Err(format!(
            "The backup revision {revision:?} must be <YYYYMMDD>_<HHMMSS>: 8 digits, '_', 6 digits."
        ));
    }
    Ok(())
}

/// Keep the current content of a user-owned profile file before overwriting
/// it — unprivileged, into the panel's own backups directory.
fn backup_current_home_file(path: &str, backups_dir: &Path) -> Result<String, String> {
    let file_name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_path = backups_dir.join(format!("{file_name}.{timestamp}.bak"));
    std::fs::copy(path, &backup_path).map_err(|e| format!("Failed to backup: {e}"))?;
    Ok(backup_path.to_string_lossy().to_string())
}

/// Restore one *user-owned* profile file. Unprivileged by design: both paths
/// are re-derived against the allowlist of managed home files and the panel's
/// own backups directory, the current content is kept as a fresh `backup_file`
/// backup, and the destination is written with a plain `std::fs::copy` — no
/// pkexec, no helper, nothing privileged.
async fn restore_home_profile_file(
    original_path: &str,
    backup_path: &str,
    backups_dir: &Path,
    allowed: &[PathBuf],
) -> Result<(), String> {
    if !allowed.iter().any(|path| path.as_path() == Path::new(original_path)) {
        return Err(format!(
            "'{original_path}' is not a profile file this panel manages; refusing to restore."
        ));
    }
    valid_home_backup_in(original_path, backup_path, backups_dir)?;
    backup_current_home_file(original_path, backups_dir)?;
    std::fs::copy(backup_path, original_path)
        .map_err(|e| format!("Failed to restore: {e}"))?;
    Ok(())
}

/// Validate one *root-owned* profile.d restore pair from the app side. This
/// is only the pre-filter: the helper re-validates the destination, the
/// backup sibling, its ownership and the backup's bytes itself, and the
/// content never travels through the app.
fn restore_root_pair_valid(original_path: &str, backup_path: &str) -> Result<(), String> {
    validate_profile_sh_path(original_path)?;
    restore_profile_sh_backup_shape(original_path, backup_path)
}

/// Restore one root-owned profile.d file through the helper's dedicated
/// `restore_profile_sh` action. The app passes two validated paths and never
/// reads the backup's bytes; the helper reads the sibling itself, re-checks
/// the historical content against the current policy, keeps the current
/// content as a fresh revision backup (so no separate pre-restore backup is
/// needed here), and publishes atomically.
async fn restore_root_profile_file(original_path: &str, backup_path: &str) -> Result<(), String> {
    restore_root_pair_valid(original_path, backup_path)?;
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the profile file was not restored."
            .to_string()
    })?;
    let args = vec![
        String::from("restore_profile_sh"),
        original_path.to_string(),
        backup_path.to_string(),
    ];
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, "").await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn shell_restore_backup(backup_path: String, original_path: String) -> Result<(), String> {
    // Root-owned targets: exact profile.d children only, restored exclusively
    // through the helper action.
    if profile_d_name(&original_path).is_some() {
        return restore_root_profile_file(&original_path, &backup_path).await;
    }
    // User-owned targets: the known home profile files only, unprivileged.
    let allowed = user_profile_files();
    if allowed.iter().any(|path| path.as_path() == Path::new(&original_path)) {
        return restore_home_profile_file(&original_path, &backup_path, &backup_dir(), &allowed)
            .await;
    }
    Err(format!(
        "'{original_path}' is not a profile file this panel manages; refusing to restore."
    ))
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
    let content = std::fs::read_to_string(&profile_path).unwrap_or_default();
    let new_content = content_with_path_entry(&content, &directory);
    match profile_d_name(&profile_path) {
        Some(file_name) => write_profile_d_as_root(&file_name, &new_content).await.map(|_| ()),
        None => {
            backup_file(&profile_path).await?;
            write_file_content(&profile_path, &new_content).await
        }
    }
}

#[tauri::command]
pub async fn shell_remove_path_entry(directory: String, profile_path: String) -> Result<(), String> {
    let content = std::fs::read_to_string(&profile_path).map_err(|e| e.to_string())?;
    let new_content = content_without_path_entry(&content, &directory);
    match profile_d_name(&profile_path) {
        Some(file_name) => write_profile_d_as_root(&file_name, &new_content).await.map(|_| ()),
        None => {
            backup_file(&profile_path).await?;
            write_file_content(&profile_path, &new_content).await
        }
    }
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

// ─── tests: write_profile_d_as_root (name-only, validated, helper action) ───

#[cfg(test)]
mod profile_d_write_tests {
    use super::*;

    #[test]
    fn profile_d_target_accepts_only_a_plain_sh_name() {
        std::fs::create_dir_all(PROFILE_D_DIR).unwrap();
        assert_eq!(
            profile_d_target("10-lang.sh").unwrap(),
            format!("{PROFILE_D_DIR}/10-lang.sh")
        );

        // a separator or a relative component can never become a second path
        // element, and the same validator the helper uses is applied after
        for name in [
            "../shadow.sh",
            "sub/10-lang.sh",
            "..",
            ".",
            "",
            "   ",
            ".hidden.sh",
            "10-lang.conf",
            "10 lang.sh",
            "10-lang.sh/",
            "x\\y.sh",
        ] {
            assert!(
                profile_d_target(name).is_err(),
                "name {name:?} must be refused"
            );
        }
    }

    #[tokio::test]
    async fn write_profile_d_as_root_refuses_non_export_content_before_any_privileged_call() {
        std::fs::create_dir_all(PROFILE_D_DIR).unwrap();
        // No helper is invoked for any of these: content validation runs
        // before the approved-name lookup and long before the pkexec hop.
        for content in [
            "rm -rf /\n",
            "export X=\"$(touch /tmp/pwned)\"\n",
            "export 1BAD=\"x\"\n",
            "export PATH=\"$PATH:/opt/bin;rm -rf /\"\n",
        ] {
            let error = write_profile_d_as_root("10-lang.sh", content)
                .await
                .expect_err("non-export content must be refused");
            assert!(
                !error.contains("helper is not installed"),
                "content must be refused before any privileged call: {error}"
            );
        }

        // a bad name is refused before the (here invalid) content is looked at
        let error = write_profile_d_as_root("../shadow.sh", "export X=\"1\"\n")
            .await
            .expect_err("a traversal name must be refused");
        assert!(
            error.contains("separators are not allowed"),
            "the name must be refused first: {error}"
        );
    }

    #[test]
    fn generated_path_entries_pass_the_root_content_policy() {
        // What shell_add_path_entry puts into an /etc/profile.d file must
        // satisfy exactly the policy write_profile_d_as_root enforces.
        for directory in ["/opt/bin", "/usr/local/foo", "/opt/my_tools-1.2/bin"] {
            let added = content_with_path_entry("", directory);
            assert_eq!(added, format!("export PATH=\"$PATH:{directory}\"\n"));
            assert!(
                validate_profile_sh_content(&added).is_ok(),
                "the add shape must pass the policy: {added:?}"
            );
        }

        let first = content_with_path_entry("export FOO=\"bar\"\n", "/opt/bin");
        let both = content_with_path_entry(&first, "/usr/local/foo");
        assert!(validate_profile_sh_content(&both).is_ok());

        let removed = content_without_path_entry(&both, "/opt/bin");
        assert_eq!(
            removed,
            "export FOO=\"bar\"\nexport PATH=\"$PATH:/usr/local/foo\"\n"
        );
        assert!(validate_profile_sh_content(&removed).is_ok());

        // removing the last entry leaves a still-valid file
        let empty = content_without_path_entry(&first, "/opt/bin");
        assert_eq!(empty, "export FOO=\"bar\"\n");
        assert!(validate_profile_sh_content(&empty).is_ok());
    }

    #[test]
    fn generated_path_entries_still_refuse_a_hostile_directory() {        // The directory is not validated at the call site, so the content
        // grammar is the defence: none of these can reach /etc/profile.d.
        for directory in [
            "/opt/bin;touch /tmp/x",
            "/opt/bin\"$(touch /tmp/x)\"",
            "/opt/bin extra",
            "/opt/bin`id`",
            "/opt/bin|cat",
            "/opt/bin&&true",
            "/opt/bin$(id)",
        ] {
            let added = content_with_path_entry("", directory);
            assert!(
                validate_profile_sh_content(&added).is_err(),
                "a hostile directory must be refused by the content policy: {added:?}"
            );
        }
    }

    #[test]
    fn helper_backup_path_accepts_only_one_backup_sibling() {
        let backup = format!("{PROFILE_D_DIR}/10-lang.sh.1700000000_1.2.bak");
        assert_eq!(
            helper_backup_path(&format!("{backup}\n")).unwrap().as_deref(),
            Some(backup.as_str())
        );
        assert_eq!(helper_backup_path("").unwrap(), None);
        assert_eq!(helper_backup_path("  \n").unwrap(), None);

        // not a sibling, not a .bak, or more than one line is a protocol
        // violation, never something to show in the UI
        for output in [
            String::from("/tmp/evil.bak\n"),
            format!("{PROFILE_D_DIR}/10-lang.sh\n"),
            format!("{backup}\n{backup}\n"),
        ] {
            assert!(
                helper_backup_path(&output).is_err(),
                "output {output:?} must be refused"
            );
        }
    }

    #[test]
    fn profile_d_name_matches_only_the_root_owned_directory() {
        assert_eq!(
            profile_d_name(&format!("{PROFILE_D_DIR}/10-lang.sh")).as_deref(),
            Some("10-lang.sh")
        );
        for path in [
            "/etc/bash.bashrc",
            "/etc/profile.d/sub/10-lang.sh",
            "/home/user/.bashrc",
            "/etc/profile.d/../10-lang.sh",
        ] {
            assert!(profile_d_name(path).is_none(), "{path} is not a direct child");
        }
    }
}

// ─── tests: shell_restore_backup (own-backup-only, allowlisted destinations) ──

#[cfg(test)]
mod profile_restore_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lcp-restore-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 1: a legitimate, own-made home backup — the current content is kept
    /// as a fresh backup and the chosen backup is copied back byte-exact.
    #[tokio::test]
    async fn a_legitimate_own_made_home_backup_restores() {
        let backups_dir = unique_dir("backups");
        let home = unique_dir("home");
        let original = home.join(".bashrc");
        std::fs::write(&original, "current content\n").unwrap();
        let backup = backups_dir.join(".bashrc.20240102_030405.bak");
        std::fs::write(&backup, "previous content\n").unwrap();

        let allowed = vec![original.clone()];
        restore_home_profile_file(
            original.to_str().unwrap(),
            backup.to_str().unwrap(),
            &backups_dir,
            &allowed,
        )
        .await
        .expect("a legitimate own-made backup must restore");

        assert_eq!(
            std::fs::read_to_string(&original).unwrap(),
            "previous content\n"
        );
        // the current content was kept before the restore overwrote it — as
        // a fresh `.bashrc.<its own timestamp>.bak` sibling
        let mut kept = Vec::new();
        for entry in std::fs::read_dir(&backups_dir).unwrap().flatten() {
            if entry.path() != backup {
                kept.push(entry.path());
            }
        }
        assert_eq!(kept.len(), 1, "exactly one fresh backup: {kept:?}");
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            "current content\n"
        );
    }

    /// 2: a backup outside the panel's own backups directory is refused,
    /// no matter its name.
    #[tokio::test]
    async fn a_backup_outside_the_app_backup_dir_is_refused() {
        let backups_dir = unique_dir("backups");
        let home = unique_dir("home");
        let stranger = unique_dir("stranger");
        let original = home.join(".bashrc");
        std::fs::write(&original, "current content\n").unwrap();
        let backup = stranger.join(".bashrc.20240102_030405.bak");
        std::fs::write(&backup, "previous content\n").unwrap();

        let allowed = vec![original.clone()];
        let error = restore_home_profile_file(
            original.to_str().unwrap(),
            backup.to_str().unwrap(),
            &backups_dir,
            &allowed,
        )
        .await
        .expect_err("a foreign directory must be refused");
        assert!(error.contains("backups directory"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&original).unwrap(),
            "current content\n",
            "nothing may be copied for a refused backup"
        );
    }

    /// 3: right directory, wrong identity — the backup name must derive from
    /// the destination and carry the exact <YYYYMMDD>_<HHMMSS> shape.
    #[test]
    fn a_backup_whose_name_does_not_derive_from_the_destination_is_refused() {
        let backups_dir = unique_dir("backups");
        let original = unique_dir("home").join(".bashrc");

        for wrong in [
            ".profile.20240102_030405.bak", // another file's backup
            ".bashrc.bak",                  // no revision at all
            ".bashrc.notatime.bak",         // free-form revision
            ".bashrc.020240102_030405.bak", // 9 digits
            ".bashrc.20240102030405.bak",   // missing the underscore
            ".bashrc.20240102_0304.bak",    // 4 digits in the time
        ] {
            let backup = backups_dir.join(wrong).to_str().unwrap().to_string();
            let error = valid_home_backup_in(
                original.to_str().unwrap(),
                &backup,
                &backups_dir,
            )
            .expect_err("a wrong backup shape must be refused");
            assert!(
                error.contains("backup"),
                "{wrong:?} must be refused with the backup reason: {error}"
            );
        }

        // the exact shape backup_file writes is the one accepted
        valid_home_backup_in(
            &original.to_str().unwrap(),
            &backups_dir.join(".bashrc.20240102_030405.bak").to_str().unwrap(),
            &backups_dir,
        )
        .expect("the app's own backup shape must be accepted");
    }

    /// 4: a destination outside the allowed targets is refused at the
    /// command boundary — nothing is spawned, nothing is copied.
    #[tokio::test]
    async fn a_destination_outside_the_managed_targets_is_refused() {
        let error = crate::commands::shell_env::shell_restore_backup(
            "/tmp/pwned.bak".to_string(),
            "/etc/environment".to_string(),
        )
        .await
        .expect_err("an unmanaged /etc file must be refused");
        assert!(error.contains("not a profile file this panel manages"), "{error}");

        let error = crate::commands::shell_env::shell_restore_backup(
            "/etc/shadow.bak".to_string(),
            "/etc/shadow".to_string(),
        )
        .await
        .expect_err("an arbitrary /etc destination must be refused");
        assert!(error.contains("not a profile file this panel manages"), "{error}");

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
        let error = crate::commands::shell_env::shell_restore_backup(
            format!("{}/stranger.bak", home.display()),
            format!("{}/../otheruser/.bashrc", home.display()),
        )
        .await
        .expect_err("a home file outside the allowlist must be refused");
        assert!(error.contains("not a profile file this panel manages"), "{error}");
    }

    /// 6 (root side, app pre-filter): the backup must be the helper's exact
    /// revision-sibling shape — foreign directory, foreign name or a
    /// non-numeric revision are refused before any privileged call exists.
    #[tokio::test]
    async fn the_root_restore_refuses_a_non_helper_shaped_backup() {
        let dest = format!("{PROFILE_D_DIR}/10-lang.sh");

        // foreign directory
        let error = crate::commands::shell_env::shell_restore_backup(
            "/tmp/hostile/target.bak".to_string(),
            dest.clone(),
        )
        .await
        .expect_err("a backup outside /etc/profile.d must be refused");
        assert!(error.contains("revision sibling"), "{error}");

        // foreign name / cross-destination
        let error = crate::commands::shell_env::shell_restore_backup(
            format!("{PROFILE_D_DIR}/19-other.sh.1700000000_1.2.bak"),
            dest.clone(),
        )
        .await
        .expect_err("another destination's backup must be refused");
        assert!(error.contains("must start with the destination name"), "{error}");

        // no numeric revision
        let error = crate::commands::shell_env::shell_restore_backup(
            format!("{PROFILE_D_DIR}/10-lang.sh.bak"),
            dest.clone(),
        )
        .await
        .expect_err("a name without a numeric revision must be refused");
        assert!(error.contains("revision"), "{error}");

        // the exact shape the helper creates is accepted by the pre-filter
        restore_root_pair_valid(
            &dest,
            &format!("{PROFILE_D_DIR}/10-lang.sh.1700000000_1.2.bak"),
        )
        .expect("the helper's own shape must pass the app pre-filter");
    }

    /// The allowlist the home branch reuses is the same list of managed home
    /// profile files every other command reads — nothing invented here.
    #[test]
    fn the_home_restores_reuse_the_managed_profile_file_list() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
        let expected = vec![
            home.join(".bashrc"),
            home.join(".bash_profile"),
            home.join(".profile"),
        ];
        assert_eq!(user_profile_files(), expected);
    }
}
