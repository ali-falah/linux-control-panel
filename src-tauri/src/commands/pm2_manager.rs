use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2SystemStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub pm2_home: String,
    pub daemon_running: bool,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2Process {
    pub pm_id: u32,
    pub name: String,
    pub pid: Option<u32>,
    pub status: String,
    pub cpu: f64,
    pub memory: u64,
    pub uptime: Option<u64>,
    pub restarts: u32,
    pub unstable_restarts: u32,
    pub exec_mode: String,
    pub instances: i32,
    pub watch: bool,
    pub script_path: String,
    pub cwd: String,
    pub out_log_path: String,
    pub err_log_path: String,
    pub node_args: Vec<String>,
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub version: Option<String>,
    pub created_at: Option<u64>,
    pub namespace: String,
    pub interpreter: Option<String>,
    pub node_version: Option<String>,
    pub max_memory_restart: Option<String>,
    /// PM2 modules such as pm2-logrotate are processes too, but they are not
    /// part of the application fleet and must not trigger drift/restart alarms.
    pub is_module: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2DumpInfo {
    pub exists: bool,
    pub modified: Option<u64>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2StartOptions {
    pub script_path: String,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub instances: Option<i32>,
    pub exec_mode: Option<String>, // "fork" or "cluster"
    pub watch: Option<bool>,
    pub max_memory_restart: Option<String>, // e.g. "300M"
    pub args: Option<Vec<String>>,
    pub env_vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2EcosystemFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: String,
    pub app_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2StartupStatus {
    pub is_enabled: bool,
    pub service_name: Option<String>,
    pub user: String,
    pub startup_command_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2SavedDumpApp {
    pub name: String,
    pub namespace: String,
    pub script_path: String,
    pub cwd: String,
    pub exec_mode: String,
    pub instances: i32,
    pub watch: bool,
    pub max_memory_restart: Option<String>,
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub saved_status: String,
    pub is_currently_running: bool,
    pub current_pm_id: Option<u32>,
    pub created_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pm2LogFileInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pm2LogOverview {
    pub directory: String,
    pub total_size: u64,
    pub files: Vec<Pm2LogFileInfo>,
    pub hidden_files: usize,
    pub logrotate_installed: bool,
    pub logrotate_version: Option<String>,
}

const ECOSYSTEM_FILENAMES: &[&str] = &[
    "ecosystem.config.js",
    "ecosystem.config.cjs",
    "ecosystem.config.mjs",
    "ecosystem.json",
    "pm2.config.js",
    "pm2.config.cjs",
    "pm2.json",
];

/// PM2 accepts a numeric app id or the literal `all`; anything else could be
/// interpreted as an option by the CLI.
fn valid_target(target: &str) -> bool {
    target == "all"
        || (!target.is_empty() && target.chars().all(|c| c.is_ascii_digit()))
}

fn valid_process_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.len() <= 128
        && !name.chars().any(|c| c.is_control())
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_env_vars(env_vars: &HashMap<String, String>) -> Result<(), String> {
    for (key, value) in env_vars {
        if !valid_env_key(key) {
            return Err(format!("Invalid environment variable name: '{key}'"));
        }
        if value.contains('\0') {
            return Err(format!("Environment variable '{key}' contains a NUL byte"));
        }
    }
    Ok(())
}

fn valid_memory_size(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let mut digits = false;
    let mut dot = false;
    let mut unit = false;
    for c in value.chars() {
        match c {
            '0'..='9' => digits = true,
            '.' if !dot && !unit => dot = true,
            'K' | 'M' | 'G' if !unit => unit = true,
            _ => return false,
        }
    }
    digits
}

fn validate_script_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() {
        return Err("The application script must be an absolute path.".to_string());
    }
    if value.contains('\0') {
        return Err("The application path contains a NUL byte.".to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| format!("The application script does not exist: {value}"))?;
    if !canonical.is_file() {
        return Err("The application script is not a regular file.".to_string());
    }
    Ok(canonical)
}

fn validate_work_dir(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() {
        return Err("The working directory must be an absolute path.".to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| format!("The working directory does not exist: {value}"))?;
    if !canonical.is_dir() {
        return Err("The working directory is not a directory.".to_string());
    }
    Ok(canonical)
}

fn ecosystem_filename_allowed(name: &str) -> bool {
    ECOSYSTEM_FILENAMES.contains(&name)
}

/// Only recognised PM2 ecosystem config files may be read or written here.
/// This prevents the page from using the editor as a generic file read/write
/// primitive for secrets such as ~/.ssh/id_rsa.
fn validate_ecosystem_path(value: &str, for_write: bool) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() {
        return Err("The ecosystem file path must be absolute.".to_string());
    }
    if value.contains('\0') {
        return Err("The ecosystem file path contains a NUL byte.".to_string());
    }
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return Err("The ecosystem file path has no file name.".to_string());
    };
    if !ecosystem_filename_allowed(name) {
        return Err(format!(
            "Refused: '{name}' is not a recognised PM2 ecosystem config name."
        ));
    }
    if path
        .components()
        .any(|component| component.as_os_str() == "..")
    {
        return Err("The ecosystem file path must not contain '..'.".to_string());
    }
    if for_write {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                return Err("Refused: the ecosystem file is a symlink.".to_string());
            }
        }
    }
    Ok(path.to_path_buf())
}

/// Recognise a project whose nearest package.json declares ESM, so a
/// `.js` ecosystem config is checked with the same module mode Node will use.
fn js_ecosystem_uses_esm(path: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(current) = dir {
        let package_json = current.join("package.json");
        if let Ok(text) = fs::read_to_string(&package_json) {
            return serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| value.get("type").and_then(|v| v.as_str()).map(|v| v == "module"))
                .unwrap_or(false);
        }
        dir = current.parent();
    }
    false
}

fn compact_node_error(stderr: &str, label: &str) -> String {
    let stderr = stderr.replace("[stdin]", label);
    let mut message = String::new();
    for line in stderr.lines().take(8) {
        if !line.trim().is_empty() {
            message.push_str(line);
            message.push('\n');
        }
    }
    let trimmed = message.trim_end();
    if trimmed.is_empty() {
        "Syntax error".to_string()
    } else if trimmed.chars().count() > 1200 {
        let short: String = trimmed.chars().take(1200).collect();
        format!("{short}…")
    } else {
        trimmed.to_string()
    }
}

/// Parse-check an ecosystem config without executing it.
///
/// JSON is parsed directly. JavaScript is checked by Node with `--check`,
/// which only parses the file and never runs it. `.mjs` and `.js` files whose
/// nearest package.json is ESM use module input mode; `.cjs` and ordinary
/// `.js` files use CommonJS.
async fn validate_ecosystem_content(path: &Path, content: &str) -> Result<(), String> {
    if content.len() > 2 * 1024 * 1024 {
        return Err("The ecosystem file is too large to validate (2 MB limit).".to_string());
    }
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ecosystem config");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "json" => {
            let value: serde_json::Value = serde_json::from_str(content)
                .map_err(|error| format!("Invalid JSON in {label}:\n{error}"))?;
            if !value.is_object() && !value.is_array() {
                return Err(format!(
                    "Invalid JSON in {label}: expected an app object or array."
                ));
            }
            Ok(())
        }
        "js" | "cjs" | "mjs" => {
            let module = extension == "mjs"
                || (extension == "js" && js_ecosystem_uses_esm(path));
            let node = find_node_bin().unwrap_or_else(|| "node".to_string());
            let mut command = Command::new(node);
            if module {
                command.arg("--input-type=module");
            }
            command
                .arg("--check")
                .arg("-")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            let mut child = command.spawn().map_err(|error| {
                format!("Node.js is required to validate {label}: {error}")
            })?;
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(error) = stdin.write_all(content.as_bytes()).await {
                    let _ = child.kill().await;
                    return Err(format!("Failed to send {label} to Node: {error}"));
                }
            }
            let stderr_reader = child.stderr.take().map(|mut stderr| {
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let _ = stderr.read_to_end(&mut buffer).await;
                    buffer
                })
            });
            let status = match tokio::time::timeout(Duration::from_secs(15), child.wait()).await {
                Ok(result) => result
                    .map_err(|error| format!("Failed waiting for Node validation: {error}"))?,
                Err(_) => {
                    let _ = child.kill().await;
                    return Err(format!("Timed out while validating {label}."));
                }
            };
            let stderr = match stderr_reader {
                Some(handle) => handle.await.unwrap_or_default(),
                None => Vec::new(),
            };
            if status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&stderr);
                Err(format!(
                    "Syntax error in {label}:\n{}",
                    compact_node_error(&stderr, label)
                ))
            }
        }
        _ => Err("Unsupported ecosystem config type for validation.".to_string()),
    }
}

#[tauri::command]
pub async fn pm2_validate_ecosystem(
    path: String,
    content: Option<String>,
) -> Result<(), String> {
    let ecosystem = validate_ecosystem_path(&path, true)?;
    let text = match content {
        Some(value) => value,
        None => fs::read_to_string(&ecosystem)
            .map_err(|error| format!("Failed to read the ecosystem file: {error}"))?,
    };
    validate_ecosystem_content(&ecosystem, &text).await
}

fn pm2_logs_dir() -> PathBuf {
    PathBuf::from(get_pm2_home()).join("logs")
}

/// Log files registered by PM2 itself, beyond the default logs folder. PM2
/// writes resolved `pm_out_log_path` / `pm_err_log_path` values into its dump,
/// so an ecosystem config with a custom `out_file` can still be read.
fn pm2_registered_log_paths() -> Vec<PathBuf> {
    let dump = PathBuf::from(get_pm2_home()).join("dump.pm2");
    let Ok(content) = fs::read_to_string(&dump) else {
        return Vec::new();
    };
    let Ok(entries) = parse_dump_array(&content) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for item in entries {
        for key in ["pm_out_log_path", "pm_err_log_path", "pm_log_path"] {
            let Some(value) = item.get(key).and_then(|value| value.as_str()) else {
                continue;
            };
            let Ok(canonical) = Path::new(value).canonicalize() else {
                continue;
            };
            if !canonical.is_file() {
                continue;
            }
            if !paths.contains(&canonical) {
                paths.push(canonical);
            }
        }
    }
    paths
}

/// Log files are user-controlled data. Restrict reads and truncation to the
/// PM2 log directory so a compromised renderer cannot read or erase arbitrary
/// files through the log viewer.
fn validate_log_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() {
        return Err("The log path must be absolute.".to_string());
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err("The log path has no file name.".to_string());
    };
    if file_name.is_empty() || file_name == "." || file_name == ".." {
        return Err("The log path has no valid file name.".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "The log path has no parent folder.".to_string())?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| format!("The log folder does not exist: {}", parent.display()))?;
    let candidate = canonical_parent.join(file_name);

    let logs_root = pm2_logs_dir();
    let logs_root = logs_root.canonicalize().unwrap_or(logs_root);
    let home_root = PathBuf::from(get_pm2_home());
    let home_root = home_root.canonicalize().unwrap_or(home_root);

    // The daemon's own log lives directly in PM2_HOME, beside the logs folder.
    let is_home_log = candidate.parent() == Some(home_root.as_path())
        && matches!(file_name, "pm2.log" | "pm2-out.log" | "pm2-error.log");

    let canonical_candidate = candidate.canonicalize().ok();
    let registered = pm2_registered_log_paths();
    let is_registered = canonical_candidate
        .as_ref()
        .map(|canonical| registered.iter().any(|path| path == canonical))
        .unwrap_or(false);

    if !candidate.starts_with(&logs_root) && !is_home_log && !is_registered {
        return Err(
            "Refused: only files inside the PM2 logs directory can be accessed.".to_string(),
        );
    }

    if let Some(canonical) = canonical_candidate {
        // Resolve symlinks: a link planted inside the logs folder must not be
        // able to point the reader at an arbitrary file.
        let still_allowed = canonical.starts_with(&logs_root)
            || (is_home_log && canonical.parent() == Some(home_root.as_path()))
            || registered.iter().any(|path| path == &canonical);
        if !still_allowed {
            return Err(
                "Refused: only files inside the PM2 logs directory can be accessed.".to_string(),
            );
        }
        if !canonical.is_file() {
            return Err("The log path is not a regular file.".to_string());
        }
        Ok(canonical)
    } else {
        // A stopped app may not have written its log yet; the caller decides
        // whether a missing file is an error.
        Ok(candidate)
    }
}

/// Helper to get user's PM2 home directory (~/.pm2)
fn get_pm2_home() -> String {
    if let Ok(val) = std::env::var("PM2_HOME") {
        if !val.trim().is_empty() {
            return val;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{}/.pm2", home);
    }
    "~/.pm2".to_string()
}

/// Helper to resolve binary path for pm2
fn find_pm2_bin() -> Option<String> {
    // Check common locations directly first
    for loc in &[
        "/usr/local/bin/pm2",
        "/usr/bin/pm2",
        "/bin/pm2",
    ] {
        if Path::new(loc).exists() {
            return Some(loc.to_string());
        }
    }

    // Check system PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("pm2");
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    // Check NVM / user local bins
    if let Ok(home) = std::env::var("HOME") {
        let nvm_dir = PathBuf::from(&home).join(".nvm/versions/node");
        if nvm_dir.exists() {
            if let Ok(entries) = fs::read_dir(&nvm_dir) {
                for entry in entries.flatten() {
                    let pm2_candidate = entry.path().join("bin/pm2");
                    if pm2_candidate.is_file() {
                        return Some(pm2_candidate.to_string_lossy().to_string());
                    }
                }
            }
        }
        let local_bin = PathBuf::from(&home).join(".local/bin/pm2");
        if local_bin.is_file() {
            return Some(local_bin.to_string_lossy().to_string());
        }
    }

    None
}

/// Resolve a Node.js binary. The desktop session often has a minimal PATH, so
/// look next to the resolved pm2 binary and inside NVM before giving up.
fn find_node_bin() -> Option<String> {
    if let Some(pm2) = find_pm2_bin() {
        if let Some(parent) = Path::new(&pm2).parent() {
            let candidate = parent.join("node");
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("node");
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let nvm_dir = PathBuf::from(&home).join(".nvm/versions/node");
        if let Ok(entries) = fs::read_dir(&nvm_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("bin/node");
                if candidate.is_file() {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
        let local_bin = PathBuf::from(&home).join(".local/bin/node");
        if local_bin.is_file() {
            return Some(local_bin.to_string_lossy().to_string());
        }
    }

    None
}

/// Helper to create a configured Command for pm2
fn create_pm2_cmd() -> Command {
    let bin = find_pm2_bin().unwrap_or_else(|| "pm2".to_string());
    let mut cmd = Command::new(bin);
    cmd.env("PM2_SILENT", "true");
    cmd.env("NO_COLOR", "1");
    cmd
}

#[tauri::command]
pub async fn pm2_get_system_status() -> Result<Pm2SystemStatus, String> {
    let pm2_path = find_pm2_bin();
    let pm2_home = get_pm2_home();

    // Check pm2 version
    let mut version = None;
    let mut installed = false;

    if pm2_path.is_some() {
        let out = Command::new(pm2_path.as_ref().unwrap())
            .args(["-v"])
            .env("PM2_SILENT", "true")
            .output()
            .await;
        if let Ok(o) = out {
            if o.status.success() {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !v.is_empty() {
                    version = Some(v);
                    installed = true;
                }
            }
        }
    }

    // Check node version
    let mut node_version = None;
    if let Ok(o) = Command::new("node").args(["-v"]).output().await {
        if o.status.success() {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !v.is_empty() {
                node_version = Some(v);
            }
        }
    }

    // Check npm version
    let mut npm_version = None;
    if let Ok(o) = Command::new("npm").args(["-v"]).output().await {
        if o.status.success() {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !v.is_empty() {
                npm_version = Some(v);
            }
        }
    }

    // Check if PM2 daemon is running (check pm2.pid in PM2_HOME or via ping)
    let mut daemon_running = false;
    let pid_file = PathBuf::from(&pm2_home).join("pm2.pid");
    if pid_file.exists() {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                // Check if process exists in /proc
                if Path::new(&format!("/proc/{}", pid)).exists() {
                    daemon_running = true;
                }
            }
        }
    }

    Ok(Pm2SystemStatus {
        installed,
        version,
        node_version,
        npm_version,
        pm2_home,
        daemon_running,
        executable_path: pm2_path,
    })
}

#[tauri::command]
pub async fn pm2_list_processes() -> Result<Vec<Pm2Process>, String> {
    let out = create_pm2_cmd().args(["jlist"]).output().await;

    let output = match out {
        Ok(o) => o,
        Err(e) => return Err(format!("Failed to execute pm2 jlist: {}", e)),
    };

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PM2 returned error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(Vec::new());
    }

    // Parse raw JSON array
    let raw_val: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            // Find JSON array brackets if pm2 printed banner text before JSON
            if let Some(start) = trimmed.find('[') {
                if let Some(end) = trimmed.rfind(']') {
                    let slice = &trimmed[start..=end];
                    serde_json::from_str(slice)
                        .map_err(|e2| format!("Failed to parse PM2 JSON: {} ({})", e2, e))?
                } else {
                    return Err(format!("Invalid JSON format from pm2: {}", e));
                }
            } else {
                return Err(format!("Invalid JSON from pm2: {}", e));
            }
        }
    };

    let mut result = Vec::new();

    if let Some(array) = raw_val.as_array() {
        for item in array {
            let pm_id = item.get("pm_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("app").to_string();
            let pid = item.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);

            // monit: { memory, cpu }
            let monit = item.get("monit");
            let memory = monit.and_then(|m| m.get("memory")).and_then(|v| v.as_u64()).unwrap_or(0);
            let cpu = monit.and_then(|m| m.get("cpu")).and_then(|v| v.as_f64()).unwrap_or(0.0);

            // pm2_env
            let env_obj = item.get("pm2_env");
            let status = env_obj.and_then(|e| e.get("status")).and_then(|v| v.as_str()).unwrap_or("stopped").to_string();
            let uptime = env_obj.and_then(|e| e.get("pm_uptime")).and_then(|v| v.as_u64());
            let restarts = env_obj.and_then(|e| e.get("restart_time")).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let unstable_restarts = env_obj.and_then(|e| e.get("unstable_restarts")).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let exec_mode = env_obj.and_then(|e| e.get("exec_mode")).and_then(|v| v.as_str()).unwrap_or("fork_mode").to_string();
            let instances = env_obj.and_then(|e| e.get("instances")).and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let watch = env_obj.and_then(|e| e.get("watch")).and_then(|v| v.as_bool()).unwrap_or(false);
            let script_path = env_obj.and_then(|e| e.get("pm_exec_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let cwd = env_obj.and_then(|e| e.get("pm_cwd")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let out_log_path = env_obj.and_then(|e| e.get("pm_out_log_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let err_log_path = env_obj.and_then(|e| e.get("pm_err_log_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let version = env_obj.and_then(|e| e.get("version")).and_then(|v| v.as_str()).map(|s| s.to_string());
            let created_at = env_obj.and_then(|e| e.get("created_at")).and_then(|v| v.as_u64());
            let namespace = env_obj
                .and_then(|e| e.get("namespace"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let interpreter = env_obj
                .and_then(|e| e.get("exec_interpreter"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let node_version = env_obj
                .and_then(|e| e.get("node_version"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let max_memory_restart = env_obj
                .and_then(|e| e.get("max_memory_restart"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Node args
            let mut node_args = Vec::new();
            if let Some(na) = env_obj.and_then(|e| e.get("node_args")).and_then(|v| v.as_array()) {
                for arg in na {
                    if let Some(s) = arg.as_str() {
                        node_args.push(s.to_string());
                    }
                }
            }

            // Args
            let mut args = Vec::new();
            if let Some(ar) = env_obj.and_then(|e| e.get("args")) {
                if let Some(a_arr) = ar.as_array() {
                    for arg in a_arr {
                        if let Some(s) = arg.as_str() {
                            args.push(s.to_string());
                        }
                    }
                } else if let Some(s) = ar.as_str() {
                    args.push(s.to_string());
                }
            }

            // Env vars (filtered / clean)
            let mut env_vars = HashMap::new();
            if let Some(env_map) = env_obj.and_then(|e| e.get("env")).and_then(|v| v.as_object()) {
                for (k, v) in env_map {
                    // Skip bulky default node environment props if not needed, or include all strings
                    if let Some(val_str) = v.as_str() {
                        env_vars.insert(k.clone(), val_str.to_string());
                    } else if let Some(val_num) = v.as_i64() {
                        env_vars.insert(k.clone(), val_num.to_string());
                    } else if let Some(val_bool) = v.as_bool() {
                        env_vars.insert(k.clone(), val_bool.to_string());
                    }
                }
            }

            // `axm_options` exists on ordinary apps too (it carries PM2 agent
            // settings), so it must not be used as the module test. PM2
            // modules are named `pm2-*` and run from PM2's own node_modules.
            let is_module = name.starts_with("pm2-")
                || script_path.contains("/.pm2/node_modules/")
                || script_path.contains("/node_modules/pm2-");

            result.push(Pm2Process {
                pm_id,
                name,
                pid,
                status,
                cpu,
                memory,
                uptime,
                restarts,
                unstable_restarts,
                exec_mode,
                instances,
                watch,
                script_path,
                cwd,
                out_log_path,
                err_log_path,
                node_args,
                args,
                env_vars,
                version,
                created_at,
                namespace,
                interpreter,
                node_version,
                max_memory_restart,
                is_module,
            });
        }
    }

    // Sort by pm_id ascending
    result.sort_by_key(|p| p.pm_id);

    Ok(result)
}

#[tauri::command]
pub async fn pm2_process_action(action: String, target: String) -> Result<String, String> {
    let act = action.to_lowercase();
    let valid_actions = ["start", "stop", "restart", "reload", "delete", "reset"];
    if !valid_actions.contains(&act.as_str()) {
        return Err(format!("Unsupported PM2 action '{}'", action));
    }
    if !valid_target(&target) {
        return Err("The PM2 target must be a numeric process id or 'all'.".to_string());
    }

    let out = create_pm2_cmd()
        .args([act.as_str(), target.as_str()])
        .output()
        .await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok(format!("PM2 {} action on '{}' succeeded", act, target))
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                Err(format!("PM2 error: {} {}", err.trim(), stdout.trim()))
            }
        }
        Err(e) => Err(format!("Failed to execute PM2 command: {}", e)),
    }
}

#[tauri::command]
pub async fn pm2_start_custom_process(options: Pm2StartOptions) -> Result<String, String> {
    let script = validate_script_path(&options.script_path)?;
    if let Some(ref name) = options.name {
        if !name.trim().is_empty() && !valid_process_name(name) {
            return Err("The process name is invalid.".to_string());
        }
    }
    if let Some(ref cwd) = options.cwd {
        if !cwd.trim().is_empty() {
            validate_work_dir(cwd)?;
        }
    }
    if let Some(ref mode) = options.exec_mode {
        if mode != "fork" && mode != "cluster" {
            return Err("Execution mode must be 'fork' or 'cluster'.".to_string());
        }
    }
    if let Some(instances) = options.instances {
        if !(0..=64).contains(&instances) {
            return Err("Instances must be between 0 and 64.".to_string());
        }
    }
    if let Some(ref mem) = options.max_memory_restart {
        if !mem.trim().is_empty() && !valid_memory_size(mem) {
            return Err("The memory limit must look like 500M or 1G.".to_string());
        }
    }
    if let Some(ref env_vars) = options.env_vars {
        validate_env_vars(env_vars)?;
    }
    if let Some(ref args) = options.args {
        if args.iter().any(|arg| arg.contains('\0')) {
            return Err("Application arguments must not contain NUL bytes.".to_string());
        }
    }

    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(script.to_string_lossy().as_ref());

    if let Some(ref name) = options.name {
        if !name.trim().is_empty() {
            cmd.arg("--name").arg(name.trim());
        }
    }

    if let Some(ref cwd) = options.cwd {
        if !cwd.trim().is_empty() {
            cmd.arg("--cwd").arg(cwd.trim());
        }
    }

    if let Some(ref mode) = options.exec_mode {
        if mode == "cluster" {
            let instances = options.instances.unwrap_or(0);
            if instances == 0 {
                cmd.arg("-i").arg("max");
            } else {
                cmd.arg("-i").arg(instances.to_string());
            }
        }
    }

    if let Some(true) = options.watch {
        cmd.arg("--watch");
    }

    if let Some(ref max_mem) = options.max_memory_restart {
        if !max_mem.trim().is_empty() {
            cmd.arg("--max-memory-restart").arg(max_mem.trim());
        }
    }

    // Node args / App args
    if let Some(ref args) = options.args {
        if !args.is_empty() {
            cmd.arg("--");
            for a in args {
                cmd.arg(a);
            }
        }
    }

    // Set custom environment variables for the invocation
    if let Some(ref env_vars) = options.env_vars {
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
    }

    let out = cmd.output().await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok("Process successfully launched in PM2".to_string())
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                Err(format!("PM2 start failed: {} {}", err.trim(), stdout.trim()))
            }
        }
        Err(e) => Err(format!("Failed to start process: {}", e)),
    }
}

#[tauri::command]
pub async fn pm2_save_dump() -> Result<String, String> {
    let out = create_pm2_cmd().args(["save", "--force"]).output().await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok("PM2 process list successfully saved to dump file".to_string())
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                Err(format!("PM2 save failed: {}", err.trim()))
            }
        }
        Err(e) => Err(format!("Failed to execute pm2 save: {}", e)),
    }
}

#[tauri::command]
pub async fn pm2_resurrect_dump() -> Result<String, String> {
    let out = create_pm2_cmd().args(["resurrect"]).output().await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok("PM2 processes successfully resurrected from dump".to_string())
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                Err(format!("PM2 resurrect failed: {} {}", err.trim(), stdout.trim()))
            }
        }
        Err(e) => Err(format!("Failed to execute pm2 resurrect: {}", e)),
    }
}

/// PM2's dump is usually plain JSON, but it can carry banner text from a
/// crashed CLI or a hand-edited file. Read the array between the outermost
/// brackets so the reader and the editor agree on what is editable.
fn parse_dump_array(content: &str) -> Result<Vec<serde_json::Value>, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str(trimmed) {
        Ok(value) => Ok(value),
        Err(error) => {
            let end = trimmed
                .rfind(']')
                .ok_or_else(|| format!("Invalid dump.pm2 format: {error}"))?;
            // Banner text can contain brackets of its own ("[PM2] ..."), so try
            // every "[" start until one yields a valid array.
            for (start, _) in trimmed.match_indices('[') {
                if start >= end {
                    break;
                }
                if let Ok(value) = serde_json::from_str(&trimmed[start..=end]) {
                    return Ok(value);
                }
            }
            Err(format!("Failed to parse dump.pm2 JSON: {error}"))
        }
    }
}

#[tauri::command]
pub async fn pm2_get_saved_dump_apps() -> Result<Vec<Pm2SavedDumpApp>, String> {
    let pm2_home = get_pm2_home();
    let dump_file = PathBuf::from(&pm2_home).join("dump.pm2");

    if !dump_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&dump_file)
        .map_err(|e| format!("Failed to read dump.pm2 file: {}", e))?;

    let dump_array = parse_dump_array(&content)?;

    let mut saved_apps = Vec::new();

    for item in dump_array {
        let name = item["name"].as_str().unwrap_or("unknown").to_string();
        let namespace = item["namespace"].as_str().unwrap_or("default").to_string();
        let script_path = item["pm_exec_path"].as_str()
            .or_else(|| item["script"].as_str())
            .unwrap_or("")
            .to_string();
        let cwd = item["pm_cwd"].as_str()
            .or_else(|| item["cwd"].as_str())
            .unwrap_or("")
            .to_string();
        let exec_mode = item["exec_mode"].as_str().unwrap_or("fork_mode").to_string();
        let instances = item["instances"].as_i64().unwrap_or(1) as i32;
        let watch = item["watch"].as_bool().unwrap_or(false);
        let max_memory_restart = item["max_memory_restart"].as_str().map(|s| s.to_string());
        
        let args = if let Some(arr) = item["args"].as_array() {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        } else if let Some(s) = item["args"].as_str() {
            vec![s.to_string()]
        } else {
            Vec::new()
        };

        let mut env_vars = HashMap::new();
        if let Some(env_obj) = item["env"].as_object() {
            for (k, v) in env_obj {
                if let Some(v_str) = v.as_str() {
                    env_vars.insert(k.clone(), v_str.to_string());
                }
            }
        }

        let saved_status = item["status"].as_str().unwrap_or("stopped").to_string();
        let created_at = item["created_at"].as_u64();

        saved_apps.push(Pm2SavedDumpApp {
            name,
            namespace,
            script_path,
            cwd,
            exec_mode,
            instances,
            watch,
            max_memory_restart,
            args,
            env_vars,
            saved_status,
            // The live state is merged from the process list by the frontend
            // so this command does not run `pm2 jlist` a second time.
            is_currently_running: false,
            current_pm_id: None,
            created_at,
        });
    }

    Ok(saved_apps)
}

#[tauri::command]
pub async fn pm2_get_dump_info() -> Result<Pm2DumpInfo, String> {
    let dump_file = PathBuf::from(get_pm2_home()).join("dump.pm2");
    match fs::metadata(&dump_file) {
        Ok(meta) => Ok(Pm2DumpInfo {
            exists: true,
            modified: meta
                .modified()
                .ok()
                .and_then(|time| {
                    time.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_secs())
                }),
            size: meta.len(),
        }),
        Err(_) => Ok(Pm2DumpInfo {
            exists: false,
            modified: None,
            size: 0,
        }),
    }
}

/// Change the number of cluster instances for one application.
#[tauri::command]
pub async fn pm2_scale(name: String, instances: i32) -> Result<String, String> {
    let name = name.trim();
    if !valid_process_name(name) || name.starts_with('-') {
        return Err("The application name is invalid for scaling.".to_string());
    }
    if !(1..=64).contains(&instances) {
        return Err("Instances must be between 1 and 64.".to_string());
    }

    let out = create_pm2_cmd()
        .args(["scale", name, &instances.to_string()])
        .output()
        .await;
    match out {
        Ok(output) => {
            if output.status.success() {
                Ok(format!("Scaled '{name}' to {instances} instance(s)"))
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(format!("PM2 scale failed: {} {}", err.trim(), stdout.trim()))
            }
        }
        Err(error) => Err(format!("Failed to execute PM2 scale: {error}")),
    }
}

#[tauri::command]
pub async fn pm2_start_saved_app(
    name: String,
    script_path: String,
    namespace: Option<String>,
    cwd: Option<String>,
    exec_mode: Option<String>,
    instances: Option<i32>,
    watch: Option<bool>,
    max_memory_restart: Option<String>,
    args: Option<Vec<String>>,
    env_vars: Option<HashMap<String, String>>,
) -> Result<String, String> {
    if !valid_process_name(&name) {
        return Err("The saved application name is invalid.".to_string());
    }
    let namespace = namespace
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default")
        .to_string();
    if namespace.chars().any(|c| c.is_control()) || namespace.starts_with('-') {
        return Err("The saved application namespace is invalid.".to_string());
    }

    // If the process is registered in PM2 memory, start it by name. If the
    // name is reused by a different script, refuse instead of starting the
    // wrong app or creating a duplicate.
    let active_procs = pm2_list_processes().await?;
    if let Some(proc) = active_procs
        .iter()
        .find(|p| p.name == name && p.namespace == namespace)
    {
        if !script_path.trim().is_empty()
            && !proc.script_path.is_empty()
            && proc.script_path != script_path
        {
            return Err(format!(
                "A PM2 process named '{}' already exists with a different script ({}).",
                name, proc.script_path
            ));
        }
        return pm2_process_action("start".to_string(), proc.pm_id.to_string()).await;
    }

    let script = validate_script_path(&script_path)?;
    if let Some(ref cwd) = cwd {
        if !cwd.trim().is_empty() {
            validate_work_dir(cwd)?;
        }
    }
    if let Some(ref mode) = exec_mode {
        if mode != "fork" && mode != "cluster" {
            return Err("Execution mode must be 'fork' or 'cluster'.".to_string());
        }
    }
    if let Some(instances) = instances {
        if !(0..=64).contains(&instances) {
            return Err("Instances must be between 0 and 64.".to_string());
        }
    }
    if let Some(ref mem) = max_memory_restart {
        if !mem.trim().is_empty() && !valid_memory_size(mem) {
            return Err("The memory limit must look like 500M or 1G.".to_string());
        }
    }
    if let Some(ref env_vars) = env_vars {
        validate_env_vars(env_vars)?;
    }
    if let Some(ref arg_list) = args {
        if arg_list.iter().any(|arg| arg.contains('\0')) {
            return Err("Application arguments must not contain NUL bytes.".to_string());
        }
    }

    // Otherwise launch it fresh
    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(script.to_string_lossy().as_ref());
    cmd.arg("--name").arg(name.trim());
    if namespace != "default" {
        cmd.arg("--namespace").arg(&namespace);
    }

    if let Some(ref d) = cwd {
        if !d.trim().is_empty() {
            cmd.arg("--cwd").arg(d.trim());
        }
    }

    if let Some(ref m) = exec_mode {
        if m.contains("cluster") {
            let inst = instances.unwrap_or(0);
            if inst > 0 {
                cmd.arg("-i").arg(inst.to_string());
            } else {
                cmd.arg("-i").arg("max");
            }
        }
    }

    if watch == Some(true) {
        cmd.arg("--watch");
    }

    if let Some(ref mem) = max_memory_restart {
        if !mem.trim().is_empty() {
            cmd.arg("--max-memory-restart").arg(mem.trim());
        }
    }

    if let Some(ref arg_list) = args {
        if !arg_list.is_empty() {
            cmd.arg("--").args(arg_list);
        }
    }

    if let Some(ref env_vars) = env_vars {
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
    }

    let out = cmd.output().await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok(format!("Started application '{}' successfully", name))
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                let stdout = String::from_utf8_lossy(&o.stdout);
                Err(format!("Failed to start '{}': {} {}", name, err.trim(), stdout.trim()))
            }
        }
        Err(e) => Err(format!("Execution failed: {}", e)),
    }
}

/// PM2 identifies a saved app by name + namespace; the same name may exist in
/// more than one namespace.
fn dump_entry_matches(item: &serde_json::Value, name: &str, namespace: &str) -> bool {
    item["name"].as_str() == Some(name)
        && item["namespace"].as_str().unwrap_or("default") == namespace
}

#[tauri::command]
pub async fn pm2_delete_saved_app(
    name: String,
    namespace: Option<String>,
) -> Result<String, String> {
    if !valid_process_name(&name) {
        return Err("The saved application name is invalid.".to_string());
    }
    let target_namespace = namespace
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default")
        .to_string();
    if target_namespace.chars().any(|c| c.is_control()) {
        return Err("The saved application namespace is invalid.".to_string());
    }
    let pm2_home = get_pm2_home();
    let dump_file = PathBuf::from(&pm2_home).join("dump.pm2");

    if !dump_file.exists() {
        return Err("No dump.pm2 file found".to_string());
    }

    let content = fs::read_to_string(&dump_file)
        .map_err(|e| format!("Failed to read dump.pm2: {}", e))?;

    let mut dump_array: Vec<serde_json::Value> = parse_dump_array(&content)?;

    dump_array.retain(|item| !dump_entry_matches(item, &name, &target_namespace));

    let updated = serde_json::to_string_pretty(&dump_array)
        .map_err(|e| format!("Failed to serialize dump.pm2: {}", e))?;

    // Keep a backup and replace the dump atomically; a partially written dump
    // would make every saved app disappear on the next PM2 start.
    let backup_path = dump_file.with_extension("pm2.bak");
    if let Ok(meta) = fs::symlink_metadata(&backup_path) {
        if meta.file_type().is_symlink() {
            return Err("Refused: the dump backup path is a symlink.".to_string());
        }
    }
    fs::copy(&dump_file, &backup_path)
        .map_err(|e| format!("Failed to back up dump.pm2: {}", e))?;
    let tmp_path = dump_file.with_extension("pm2.tmp");
    fs::write(&tmp_path, updated)
        .map_err(|e| format!("Failed to write dump.pm2: {}", e))?;
    if let Err(error) = fs::rename(&tmp_path, &dump_file) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to replace dump.pm2: {}", error));
    }

    Ok(format!("Removed '{}' from saved dump", name))
}

#[tauri::command]
pub async fn pm2_flush_logs(target: Option<String>) -> Result<String, String> {
    let mut cmd = create_pm2_cmd();
    cmd.arg("flush");
    if let Some(t) = target {
        let t = t.trim();
        if !t.is_empty() {
            if !valid_target(t) {
                return Err("The flush target must be a numeric process id or 'all'.".to_string());
            }
            cmd.arg(t);
        }
    }

    let out = cmd.output().await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok("Logs successfully flushed".to_string())
            } else {
                let err = String::from_utf8_lossy(&o.stderr);
                Err(format!("Failed to flush logs: {}", err.trim()))
            }
        }
        Err(e) => Err(format!("Failed to execute pm2 flush: {}", e)),
    }
}

#[tauri::command]
pub async fn pm2_read_logs(log_path: String, lines: Option<usize>) -> Result<String, String> {
    // Validate confinement before revealing whether a path exists.
    let path = validate_log_path(&log_path)?;
    // A stopped app may not have produced a log file yet; that is not an error.
    if !path.exists() {
        return Ok(format!("[No log file found at {log_path}]"));
    }

    let line_count = lines.unwrap_or(250).min(2000);
    let path_text = path.to_string_lossy().to_string();

    // Use tail command for fast, memory-safe reading of large log files
    let out = Command::new("tail")
        .args(["-n", &line_count.to_string(), &path_text])
        .output()
        .await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                // Fallback to std::fs read if tail fails
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let all_lines: Vec<&str> = content.lines().collect();
                        let start = all_lines.len().saturating_sub(line_count);
                        Ok(all_lines[start..].join("\n"))
                    }
                    Err(e) => Err(format!("Failed to read log file: {}", e)),
                }
            }
        }
        Err(_) => {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    let all_lines: Vec<&str> = content.lines().collect();
                    let start = all_lines.len().saturating_sub(line_count);
                    Ok(all_lines[start..].join("\n"))
                }
                Err(e) => Err(format!("Failed to read log file: {}", e)),
            }
        }
    }
}

fn pm2_logrotate_version() -> (bool, Option<String>) {
    let home = PathBuf::from(get_pm2_home());
    let candidates = [
        home.join("node_modules").join("pm2-logrotate"),
        home.join("modules").join("pm2-logrotate"),
    ];
    for directory in candidates.iter() {
        if !directory.exists() {
            continue;
        }
        let package_json = directory.join("package.json");
        let version = fs::read_to_string(package_json)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(|version| version.to_string())
            });
        return (true, version);
    }
    (false, None)
}

#[tauri::command]
pub async fn pm2_get_log_overview() -> Result<Pm2LogOverview, String> {
    let directory = pm2_logs_dir();
    let mut total_size = 0u64;
    let mut files = Vec::new();

    match fs::read_dir(&directory) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                total_size = total_size.saturating_add(metadata.len());
                files.push(Pm2LogFileInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    size: metadata.len(),
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // No logs yet is a normal state.
        }
        Err(error) => {
            return Err(format!(
                "Failed to read the PM2 log directory {}: {error}",
                directory.display()
            ));
        }
    }

    files.sort_by(|left, right| right.size.cmp(&left.size).then(left.name.cmp(&right.name)));
    let hidden_files = files.len().saturating_sub(8);
    files.truncate(8);
    let (logrotate_installed, logrotate_version) = pm2_logrotate_version();

    Ok(Pm2LogOverview {
        directory: directory.to_string_lossy().to_string(),
        total_size,
        files,
        hidden_files,
        logrotate_installed,
        logrotate_version,
    })
}

fn safe_log_export_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("pm2-log");
    let extension: String = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("log")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    let extension = if extension.is_empty() {
        "log".to_string()
    } else {
        extension
    };
    let stem: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "pm2-log" } else { stem };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("pm2-{stem}-{timestamp}.{extension}")
}

#[tauri::command]
pub async fn pm2_export_log(log_path: String) -> Result<String, String> {
    let path = validate_log_path(&log_path)?;
    if !path.exists() {
        return Err("The log file does not exist yet.".to_string());
    }
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Failed to inspect the log file: {error}"))?;
    const MAX_EXPORT_BYTES: u64 = 100 * 1024 * 1024;
    if metadata.len() > MAX_EXPORT_BYTES {
        return Err(format!(
            "This log file is larger than 100 MB ({:.1} MB). Use \"Open log folder\" instead.",
            metadata.len() as f64 / (1024.0 * 1024.0)
        ));
    }
    let download_dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    tokio::fs::create_dir_all(&download_dir)
        .await
        .map_err(|error| format!("Failed to prepare the download folder: {error}"))?;
    let target = download_dir.join(safe_log_export_name(&path));
    tokio::fs::copy(&path, &target)
        .await
        .map_err(|error| format!("Failed to copy the log file: {error}"))?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn pm2_list_ecosystem_files(custom_dirs: Option<Vec<String>>) -> Result<Vec<Pm2EcosystemFile>, String> {
    let mut search_dirs = Vec::new();

    // Default directories to search
    if let Ok(home) = std::env::var("HOME") {
        search_dirs.push(PathBuf::from(&home));
        search_dirs.push(PathBuf::from(&home).join("Desktop"));
        search_dirs.push(PathBuf::from(&home).join("Projects"));
        search_dirs.push(PathBuf::from(&home).join("MyActiveCodes"));
    }
    search_dirs.push(PathBuf::from("/var/www"));

    if let Some(extras) = custom_dirs {
        for d in extras {
            let p = PathBuf::from(&d);
            if !p.is_absolute() {
                return Err("Custom ecosystem directories must be absolute paths.".to_string());
            }
            if p.is_dir() {
                search_dirs.push(p);
            }
        }
    }

    let mut found_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for dir in search_dirs {
        if !dir.exists() || !dir.is_dir() {
            continue;
        }

        // Search top level of the directory
        for &fname in ECOSYSTEM_FILENAMES {
            let p = dir.join(fname);
            if p.exists() && p.is_file() {
                let canonical = p.canonicalize().unwrap_or(p.clone());
                let p_str = canonical.to_string_lossy().to_string();
                if seen_paths.insert(p_str.clone()) {
                    if let Ok(meta) = fs::metadata(&canonical) {
                        let size = meta.len();
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .ok()
                                    .map(|d| d.as_secs().to_string())
                            })
                            .unwrap_or_default();

                        // Try extracting app names from file content (simple regex / substring scan)
                        let app_names = extract_app_names_from_file(&canonical);

                        found_files.push(Pm2EcosystemFile {
                            path: p_str,
                            name: fname.to_string(),
                            size,
                            modified,
                            app_names,
                        });
                    }
                }
            }
        }

        // Search 1-level deep subdirectories (e.g. ~/Projects/my-app/ecosystem.config.js)
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let sub_path = entry.path();
                        for &fname in ECOSYSTEM_FILENAMES {
                            let p = sub_path.join(fname);
                            if p.exists() && p.is_file() {
                                let canonical = p.canonicalize().unwrap_or(p.clone());
                                let p_str = canonical.to_string_lossy().to_string();
                                if seen_paths.insert(p_str.clone()) {
                                    if let Ok(meta) = fs::metadata(&canonical) {
                                        let size = meta.len();
                                        let modified = meta
                                            .modified()
                                            .ok()
                                            .and_then(|t| {
                                                t.duration_since(std::time::UNIX_EPOCH)
                                                    .ok()
                                                    .map(|d| d.as_secs().to_string())
                                            })
                                            .unwrap_or_default();

                                        let app_names = extract_app_names_from_file(&canonical);

                                        found_files.push(Pm2EcosystemFile {
                                            path: p_str,
                                            name: format!("{}/{}", sub_path.file_name().unwrap_or_default().to_string_lossy(), fname),
                                            size,
                                            modified,
                                            app_names,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(found_files)
}

fn extract_app_names_from_file(path: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(content) = fs::read_to_string(path) {
        // Look for name: 'foo' or "name": "foo"
        for line in content.lines() {
            let line = line.trim();
            if line.contains("name:") || line.contains("\"name\":") {
                if let Some(pos) = line.find(':') {
                    let val = line[pos + 1..].trim().trim_matches(|c| c == '\'' || c == '"' || c == ',' || c == ' ');
                    if !val.is_empty() && !names.contains(&val.to_string()) {
                        names.push(val.to_string());
                    }
                }
            }
        }
    }
    names
}

#[tauri::command]
pub async fn pm2_read_ecosystem_file(path: String) -> Result<String, String> {
    let p = validate_ecosystem_path(&path, false)?;
    if let Ok(meta) = fs::symlink_metadata(&p) {
        if meta.file_type().is_symlink() {
            return Err("Refused: the ecosystem file is a symlink.".to_string());
        }
        if !meta.is_file() {
            return Err("The ecosystem path is not a regular file.".to_string());
        }
        if meta.len() > 2 * 1024 * 1024 {
            return Err("The ecosystem file is too large to edit (2 MB limit).".to_string());
        }
    }
    fs::read_to_string(&p).map_err(|e| format!("Failed to read ecosystem file '{}': {}", path, e))
}

#[tauri::command]
pub async fn pm2_write_ecosystem_file(
    path: String,
    content: String,
    create_only: Option<bool>,
) -> Result<String, String> {
    let p = validate_ecosystem_path(&path, true)?;
    if content.len() > 2 * 1024 * 1024 {
        return Err("The ecosystem file is too large to save (2 MB limit).".to_string());
    }
    let existed = p.exists();
    if create_only.unwrap_or(false) && existed {
        return Err(
            "A file with this name already exists. Choose another name or edit the existing config."
                .to_string(),
        );
    }
    validate_ecosystem_content(&p, &content).await?;
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }
    }

    let previous_permissions = if existed {
        // Make a backup, but never follow a symlink planted at <config>.bak.
        let backup_path = PathBuf::from(format!("{}.bak", p.to_string_lossy()));
        if let Ok(meta) = fs::symlink_metadata(&backup_path) {
            if meta.file_type().is_symlink() {
                return Err("Refused: the backup file is a symlink.".to_string());
            }
        }
        fs::copy(&p, &backup_path)
            .map_err(|e| format!("Failed to back up the ecosystem file: {}", e))?;
        fs::metadata(&p).ok().map(|meta| meta.permissions())
    } else {
        None
    };

    // Write to a sibling temp file and rename, so an interrupted save cannot
    // truncate the existing config.
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let file_name = p
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ecosystem.config.js");
    let temp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let write_result = (|| -> Result<(), String> {
        fs::write(&temp_path, content.as_bytes())
            .map_err(|e| format!("Failed to write ecosystem file: {}", e))?;
        if let Some(permissions) = previous_permissions {
            let _ = fs::set_permissions(&temp_path, permissions);
        }
        fs::rename(&temp_path, &p).map_err(|e| format!("Failed to replace ecosystem file: {}", e))
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(format!("Saved ecosystem file to {}", path))
}

#[tauri::command]
pub async fn pm2_start_ecosystem(
    path: String,
    only: Option<String>,
    env_name: Option<String>,
) -> Result<String, String> {
    let ecosystem = validate_ecosystem_path(&path, false)?;
    if !ecosystem.is_file() {
        return Err("The ecosystem file does not exist.".to_string());
    }
    let content = fs::read_to_string(&ecosystem)
        .map_err(|error| format!("Failed to read the ecosystem file: {error}"))?;
    validate_ecosystem_content(&ecosystem, &content).await?;
    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(ecosystem.to_string_lossy().as_ref());

    if let Some(ref o) = only {
        let only = o.trim();
        if !only.is_empty() {
            if only.chars().any(|c| c.is_control()) || only.starts_with('-') {
                return Err("The --only value contains invalid characters.".to_string());
            }
            cmd.arg("--only").arg(only);
        }
    }

    if let Some(ref env) = env_name {
        let env = env.trim();
        if !env.is_empty() {
            if env.chars().any(|c| c.is_control()) || env.starts_with('-') {
                return Err("The --env value contains invalid characters.".to_string());
            }
            cmd.arg("--env").arg(env);
        }
    }

    let out = cmd.output().await;

    match out {
        Ok(res) => {
            if res.status.success() {
                Ok(format!("Started ecosystem file '{}' successfully", path))
            } else {
                let err = String::from_utf8_lossy(&res.stderr);
                let stdout = String::from_utf8_lossy(&res.stdout);
                Err(format!("Failed to start ecosystem file: {} {}", err.trim(), stdout.trim()))
            }
        }
        Err(e) => Err(format!("Execution failed: {}", e)),
    }
}

#[tauri::command]
pub async fn pm2_get_startup_status() -> Result<Pm2StartupStatus, String> {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let service_name = format!("pm2-{}", user);

    // Check systemctl is-enabled pm2-<user>
    let out = Command::new("systemctl")
        .args(["is-enabled", &service_name])
        .output()
        .await;

    let mut is_enabled = false;
    if let Ok(o) = out {
        let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if status == "enabled" {
            is_enabled = true;
        }
    }

    let startup_command_hint = format!("pm2 startup systemd -u {} --hp {}", user, std::env::var("HOME").unwrap_or_default());

    Ok(Pm2StartupStatus {
        is_enabled,
        service_name: Some(service_name),
        user,
        startup_command_hint,
    })
}

#[cfg(test)]
mod pm2_validation_tests {
    use super::{
        ecosystem_filename_allowed, valid_env_key, valid_memory_size, valid_process_name,
        valid_target, validate_ecosystem_path, validate_script_path,
    };

    fn test_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lcp-pm2-validation-{}-{}",
            std::process::id(),
            label
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pm2_targets_are_ids_or_all_only() {
        assert!(valid_target("all"));
        assert!(valid_target("0"));
        assert!(valid_target("1234"));
        assert!(!valid_target("--help"));
        assert!(!valid_target("-1"));
        assert!(!valid_target("my-app"));
        assert!(!valid_target(""));
    }

    #[test]
    fn process_names_and_env_keys_are_bounded() {
        assert!(valid_process_name("api"));
        assert!(valid_process_name("api-worker 1"));
        assert!(!valid_process_name(""));
        assert!(!valid_process_name("line\nbreak"));
        assert!(valid_env_key("NODE_ENV"));
        assert!(valid_env_key("_PRIVATE"));
        assert!(!valid_env_key("1INVALID"));
        assert!(!valid_env_key("HAS-DASH"));
        assert!(!valid_env_key("HAS SPACE"));
    }

    #[test]
    fn memory_sizes_are_strict() {
        assert!(valid_memory_size("500M"));
        assert!(valid_memory_size("1G"));
        assert!(valid_memory_size("256"));
        assert!(valid_memory_size("1.5G"));
        assert!(!valid_memory_size("--help"));
        assert!(!valid_memory_size("M"));
        assert!(!valid_memory_size("500 MB"));
    }

    #[test]
    fn only_ecosystem_config_filenames_are_editable() {
        assert!(ecosystem_filename_allowed("ecosystem.config.js"));
        assert!(ecosystem_filename_allowed("pm2.json"));
        assert!(!ecosystem_filename_allowed("id_rsa"));
        assert!(!ecosystem_filename_allowed("notes.txt"));

        let dir = test_dir("eco");
        let allowed = dir.join("ecosystem.config.js");
        std::fs::write(&allowed, "module.exports = { apps: [] }").unwrap();
        assert!(validate_ecosystem_path(&allowed.to_string_lossy(), false).is_ok());
        assert!(
            validate_ecosystem_path(&dir.join("id_rsa").to_string_lossy(), false).is_err()
        );
        assert!(validate_ecosystem_path("relative.pm2.json", false).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scale_rejects_invalid_inputs_before_starting_pm2() {
        assert!(super::pm2_scale("".to_string(), 2).await.is_err());
        assert!(super::pm2_scale("--help".to_string(), 2).await.is_err());
        assert!(super::pm2_scale("api".to_string(), 0).await.is_err());
        assert!(super::pm2_scale("api".to_string(), 65).await.is_err());
    }

    #[test]
    fn scripts_must_be_absolute_existing_files() {
        let dir = test_dir("script");
        let script = dir.join("app.js");
        std::fs::write(&script, "console.log('ok')").unwrap();
        assert!(validate_script_path(&script.to_string_lossy()).is_ok());
        assert!(validate_script_path("app.js").is_err());
        assert!(validate_script_path(&dir.join("missing.js").to_string_lossy()).is_err());
        assert!(validate_script_path(&dir.to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_export_names_are_sanitized() {
        let name = super::safe_log_export_name(std::path::Path::new(
            "/home/user/.pm2/logs/api out-err.log",
        ));
        assert!(name.starts_with("pm2-api-out-err-"), "{name}");
        assert!(name.ends_with(".log"), "{name}");
        assert!(!name.contains('/'), "{name}");
    }

    #[test]
    fn log_paths_are_confined_and_require_an_absolute_path() {
        assert!(super::validate_log_path("logs/app.log").is_err());
        assert!(super::validate_log_path("/tmp/../etc/passwd").is_err());
        assert!(super::validate_log_path("/etc/passwd").is_err());
        assert!(super::validate_log_path("/").is_err());
    }

    #[test]
    fn dump_entries_match_name_and_namespace() {
        let item = serde_json::json!({ "name": "api", "namespace": "production" });
        assert!(super::dump_entry_matches(&item, "api", "production"));
        assert!(!super::dump_entry_matches(&item, "api", "default"));
        assert!(!super::dump_entry_matches(&item, "worker", "production"));
        let legacy = serde_json::json!({ "name": "api" });
        assert!(super::dump_entry_matches(&legacy, "api", "default"));
    }

    #[test]
    fn dump_parser_tolerates_banner_text_around_the_array() {
        let parsed = super::parse_dump_array("[PM2] Resurrecting\n[{\"name\":\"api\"}]\n")
            .expect("banner text should not block parsing");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["name"], "api");
        assert!(super::parse_dump_array("not json").is_err());
        assert_eq!(super::parse_dump_array("   ").unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ecosystem_json_syntax_is_checked() {
        let dir = test_dir("ecosystem-json");
        let path = dir.join("ecosystem.json");
        assert!(
            super::validate_ecosystem_content(&path, r#"{"apps":[{"name":"api","script":"main.js"}]}"#)
                .await
                .is_ok()
        );
        assert!(
            super::validate_ecosystem_content(&path, r#"{"apps":[}"#)
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ecosystem_javascript_syntax_is_checked() {
        if tokio::process::Command::new("node")
            .arg("-v")
            .output()
            .await
            .is_err()
        {
            return;
        }
        let dir = test_dir("ecosystem-js");
        let cjs = dir.join("ecosystem.config.js");
        let mjs = dir.join("ecosystem.config.mjs");
        assert!(
            super::validate_ecosystem_content(&cjs, "module.exports = { apps: [] };")
                .await
                .is_ok()
        );
        assert!(
            super::validate_ecosystem_content(&cjs, "module.exports = { apps: [ };")
                .await
                .is_err()
        );
        assert!(
            super::validate_ecosystem_content(&mjs, "export default { apps: [] };")
                .await
                .is_ok()
        );
        let esm_dir = test_dir("ecosystem-esm");
        std::fs::write(esm_dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
        let esm_js = esm_dir.join("ecosystem.config.js");
        assert!(
            super::validate_ecosystem_content(&esm_js, "export default { apps: [] };")
                .await
                .is_ok()
        );
        let _ = std::fs::remove_dir_all(&esm_dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
