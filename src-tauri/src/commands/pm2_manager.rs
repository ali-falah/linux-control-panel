use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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
    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(&options.script_path);

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

#[tauri::command]
pub async fn pm2_get_saved_dump_apps() -> Result<Vec<Pm2SavedDumpApp>, String> {
    let pm2_home = get_pm2_home();
    let dump_file = PathBuf::from(&pm2_home).join("dump.pm2");

    if !dump_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&dump_file)
        .map_err(|e| format!("Failed to read dump.pm2 file: {}", e))?;

    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(Vec::new());
    }

    let dump_array: Vec<serde_json::Value> = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            if let Some(start) = trimmed.find('[') {
                if let Some(end) = trimmed.rfind(']') {
                    serde_json::from_str(&trimmed[start..=end])
                        .map_err(|e2| format!("Failed to parse dump.pm2 JSON: {} ({})", e2, e))?
                } else {
                    return Err(format!("Invalid dump.pm2 format: {}", e));
                }
            } else {
                return Err(format!("Invalid dump.pm2 format: {}", e));
            }
        }
    };

    // Query active processes to determine live running state
    let active_procs = pm2_list_processes().await.unwrap_or_default();

    let mut saved_apps = Vec::new();

    for item in dump_array {
        let name = item["name"].as_str().unwrap_or("unknown").to_string();
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

        let running_match = active_procs.iter().find(|p| p.name == name || (p.script_path == script_path && !script_path.is_empty()));
        let is_currently_running = running_match.map(|p| p.status == "online").unwrap_or(false);
        let current_pm_id = running_match.map(|p| p.pm_id);

        saved_apps.push(Pm2SavedDumpApp {
            name,
            script_path,
            cwd,
            exec_mode,
            instances,
            watch,
            max_memory_restart,
            args,
            env_vars,
            saved_status,
            is_currently_running,
            current_pm_id,
            created_at,
        });
    }

    Ok(saved_apps)
}

#[tauri::command]
pub async fn pm2_start_saved_app(
    name: String,
    script_path: String,
    cwd: Option<String>,
    exec_mode: Option<String>,
    instances: Option<i32>,
    watch: Option<bool>,
    args: Option<Vec<String>>,
) -> Result<String, String> {
    // If the process is registered in PM2 memory, start it by name
    let active_procs = pm2_list_processes().await.unwrap_or_default();
    if let Some(proc) = active_procs.iter().find(|p| p.name == name) {
        return pm2_process_action("start".to_string(), proc.pm_id.to_string()).await;
    }

    // Otherwise launch it fresh
    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(&script_path);
    cmd.arg("--name").arg(&name);

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

    if let Some(ref arg_list) = args {
        if !arg_list.is_empty() {
            cmd.arg("--").args(arg_list);
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

#[tauri::command]
pub async fn pm2_delete_saved_app(name: String) -> Result<String, String> {
    let pm2_home = get_pm2_home();
    let dump_file = PathBuf::from(&pm2_home).join("dump.pm2");

    if !dump_file.exists() {
        return Err("No dump.pm2 file found".to_string());
    }

    let content = fs::read_to_string(&dump_file)
        .map_err(|e| format!("Failed to read dump.pm2: {}", e))?;

    let mut dump_array: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse dump.pm2: {}", e))?;

    dump_array.retain(|item| {
        item["name"].as_str() != Some(&name)
    });

    let updated = serde_json::to_string_pretty(&dump_array)
        .map_err(|e| format!("Failed to serialize dump.pm2: {}", e))?;

    fs::write(&dump_file, updated)
        .map_err(|e| format!("Failed to write dump.pm2: {}", e))?;

    Ok(format!("Removed '{}' from saved dump", name))
}

#[tauri::command]
pub async fn pm2_flush_logs(target: Option<String>) -> Result<String, String> {
    let mut cmd = create_pm2_cmd();
    cmd.arg("flush");
    if let Some(t) = target {
        if !t.trim().is_empty() {
            cmd.arg(t.trim());
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
    let path = Path::new(&log_path);
    if !path.exists() {
        return Ok(format!("[No log file found at {}]", log_path));
    }

    let line_count = lines.unwrap_or(250).min(2000);

    // Use tail command for fast, memory-safe reading of large log files
    let out = Command::new("tail")
        .args(["-n", &line_count.to_string(), &log_path])
        .output()
        .await;

    match out {
        Ok(o) => {
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                // Fallback to std::fs read if tail fails
                match fs::read_to_string(path) {
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
            match fs::read_to_string(path) {
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

#[tauri::command]
pub async fn pm2_clear_logs(log_path: String) -> Result<String, String> {
    let path = Path::new(&log_path);
    if path.exists() {
        if let Err(e) = fs::write(path, "") {
            return Err(format!("Failed to truncate log file: {}", e));
        }
    }
    Ok("Log file cleared".to_string())
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
            search_dirs.push(PathBuf::from(d));
        }
    }

    let filenames = [
        "ecosystem.config.js",
        "ecosystem.config.cjs",
        "ecosystem.config.mjs",
        "ecosystem.json",
        "pm2.config.js",
        "pm2.config.cjs",
        "pm2.json",
    ];

    let mut found_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for dir in search_dirs {
        if !dir.exists() || !dir.is_dir() {
            continue;
        }

        // Search top level of the directory
        for fname in &filenames {
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
                        for fname in &filenames {
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
    fs::read_to_string(&path).map_err(|e| format!("Failed to read ecosystem file '{}': {}", path, e))
}

#[tauri::command]
pub async fn pm2_write_ecosystem_file(path: String, content: String) -> Result<String, String> {
    let p = Path::new(&path);
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }
    }

    // Make backup if exists
    if p.exists() {
        let backup_path = format!("{}.bak", path);
        let _ = fs::copy(p, backup_path);
    }

    fs::write(p, &content).map_err(|e| format!("Failed to write ecosystem file: {}", e))?;
    Ok(format!("Saved ecosystem file to {}", path))
}

#[tauri::command]
pub async fn pm2_start_ecosystem(
    path: String,
    only: Option<String>,
    env_name: Option<String>,
) -> Result<String, String> {
    let mut cmd = create_pm2_cmd();
    cmd.arg("start");
    cmd.arg(&path);

    if let Some(ref o) = only {
        if !o.trim().is_empty() {
            cmd.arg("--only").arg(o.trim());
        }
    }

    if let Some(ref env) = env_name {
        if !env.trim().is_empty() {
            cmd.arg("--env").arg(env.trim());
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
