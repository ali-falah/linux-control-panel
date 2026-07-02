use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::log_to_file;

// ─── Data Structures ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxInstallInfo {
    pub installed: bool,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxServiceStatus {
    pub active: bool,
    pub status: String,
    pub since: String,
    pub sub_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxTestResult {
    pub passed: bool,
    pub output: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxSite {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub source: String, // "conf.d" | "sites-available"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxStats {
    pub sites_available: usize,
    pub sites_enabled: usize,
    pub sites_disabled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxConfigFile {
    pub name: String,
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxBackup {
    pub original_path: String,
    pub backup_path: String,
    pub timestamp: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WwwEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub children: Vec<WwwEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslCert {
    pub domain: String,
    pub cert_path: String,
    pub expiry: String,
    pub days_until_expiry: i64,
    pub status: String, // "valid" | "expiring" | "expired"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSiteConfig {
    pub server_name: String,
    pub root_dir: String,
    pub port: u16,
    pub is_proxy: bool,
    pub proxy_url: String,
    pub index_file: String,
    pub enable_404: bool,
    pub enable_50x: bool,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn nginx_installed() -> bool {
    crate::binary_exists("nginx").await
}

async fn run_nginx_test() -> NginxTestResult {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // nginx -t needs root: it opens /var/log/nginx/error.log and /run/nginx.pid
    let output = Command::new("pkexec")
        .args(["nginx", "-t"])
        .output()
        .await;

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let combined = format!("{}{}", stdout, stderr).trim().to_string();
            NginxTestResult {
                passed: o.status.success(),
                output: combined,
                timestamp,
            }
        }
        Err(e) => NginxTestResult {
            passed: false,
            output: format!("Failed to run nginx -t: {e}"),
            timestamp,
        },
    }
}

/// Create a timestamped backup of a file via pkexec cp
async fn backup_file(original_path: &str) -> Result<String, String> {
    // Ensure backup dir exists
    let backup_dir = "/etc/nginx/backups";
    let mkdir_out = Command::new("pkexec")
        .args(["mkdir", "-p", backup_dir])
        .output()
        .await
        .map_err(|e| format!("Failed to create backup dir: {e}"))?;

    if !mkdir_out.status.success() {
        let err = String::from_utf8_lossy(&mkdir_out.stderr).to_string();
        return Err(format!("Failed to create backup directory: {err}"));
    }

    let filename = Path::new(original_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_path = format!("{backup_dir}/{filename}.{timestamp}.bak");

    let cp_out = Command::new("pkexec")
        .args(["cp", original_path, &backup_path])
        .output()
        .await
        .map_err(|e| format!("Failed to backup file: {e}"))?;

    if !cp_out.status.success() {
        let err = String::from_utf8_lossy(&cp_out.stderr).to_string();
        return Err(format!("Failed to copy to backup: {err}"));
    }

    Ok(backup_path)
}

/// Write content to a path via pkexec tee
async fn pkexec_write(path: &str, content: &str) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .args(["tee", path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pkexec tee: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(content.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to stdin: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait for pkexec: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("pkexec tee failed: {err}"));
    }

    Ok(())
}

// ─── Commands: Install Check ───────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_check_installed() -> NginxInstallInfo {
    if !nginx_installed().await {
        return NginxInstallInfo {
            installed: false,
            version: String::new(),
        };
    }

    // nginx -v also triggers a log-file open; run via pkexec to avoid the
    // "could not open error log" alert polluting the version string.
    let ver = Command::new("pkexec")
        .args(["nginx", "-v"])
        .output()
        .await
        .ok()
        .map(|o| {
            // nginx prints version to stderr
            let s = String::from_utf8_lossy(&o.stderr).to_string();
            s.trim().to_string()
        })
        .unwrap_or_default();

    NginxInstallInfo {
        installed: true,
        version: ver,
    }
}

// ─── Commands: Service Control ────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_service_status() -> Result<NginxServiceStatus, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let output = Command::new("systemctl")
        .args(["show", "nginx", "--no-pager",
               "--property=ActiveState,SubState,ActiveEnterTimestamp"])
        .output()
        .await
        .map_err(|e| format!("systemctl show failed: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let mut active_state = "unknown".to_string();
    let mut sub_state = "unknown".to_string();
    let mut since = String::new();

    for line in text.lines() {
        if let Some(v) = line.strip_prefix("ActiveState=") {
            active_state = v.to_string();
        } else if let Some(v) = line.strip_prefix("SubState=") {
            sub_state = v.to_string();
        } else if let Some(v) = line.strip_prefix("ActiveEnterTimestamp=") {
            since = v.to_string();
        }
    }

    Ok(NginxServiceStatus {
        active: active_state == "active",
        status: active_state,
        since,
        sub_state,
    })
}

#[tauri::command]
pub async fn nginx_service_action(action: String) -> Result<String, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let allowed = ["start", "stop", "restart", "reload"];
    if !allowed.contains(&action.as_str()) {
        return Err(format!("Invalid action: {action}"));
    }

    let output = Command::new("pkexec")
        .args(["systemctl", &action, "nginx"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        log_to_file("ERROR", &format!("nginx_service_action {action} failed: {stderr}"));
        return Err(format!("systemctl {action} nginx failed: {stderr}"));
    }

    log_to_file("INFO", &format!("nginx service action: {action}"));
    Ok(format!("Successfully ran: systemctl {action} nginx"))
}

// ─── Commands: Config Test ────────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_test_config() -> Result<NginxTestResult, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }
    Ok(run_nginx_test().await)
}

// ─── Commands: Stats ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_get_stats() -> Result<NginxStats, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let sites = nginx_list_sites_internal().await;
    let available = sites.len();
    let enabled = sites.iter().filter(|s| s.enabled).count();
    let disabled = available - enabled;

    Ok(NginxStats {
        sites_available: available,
        sites_enabled: enabled,
        sites_disabled: disabled,
    })
}

// ─── Commands: Sites Manager ──────────────────────────────────────────────────

async fn nginx_list_sites_internal() -> Vec<NginxSite> {
    let mut sites = Vec::new();

    let dirs = [
        ("/etc/nginx/conf.d", "conf.d"),
        ("/etc/nginx/sites-available", "sites-available"),
    ];

    for (dir, source) in &dirs {
        let entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut entries = entries;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !name.ends_with(".conf") && *source == "conf.d" {
                continue;
            }

            // Check if symlink exists in sites-enabled
            let enabled = if *source == "sites-available" {
                let symlink = format!("/etc/nginx/sites-enabled/{name}");
                tokio::fs::symlink_metadata(&symlink).await.is_ok()
            } else {
                // conf.d files are always "enabled"
                true
            };

            sites.push(NginxSite {
                name,
                path: path.to_string_lossy().to_string(),
                enabled,
                source: source.to_string(),
            });
        }
    }

    sites.sort_by(|a, b| a.name.cmp(&b.name));
    sites
}

#[tauri::command]
pub async fn nginx_list_sites() -> Result<Vec<NginxSite>, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }
    Ok(nginx_list_sites_internal().await)
}

#[tauri::command]
pub async fn nginx_toggle_site(name: String, enable: bool) -> Result<NginxTestResult, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let source_path = format!("/etc/nginx/sites-available/{name}");
    let symlink_path = format!("/etc/nginx/sites-enabled/{name}");

    // Ensure sites-enabled dir exists
    let _ = Command::new("pkexec")
        .args(["mkdir", "-p", "/etc/nginx/sites-enabled"])
        .output()
        .await;

    if enable {
        let out = Command::new("pkexec")
            .args(["ln", "-sf", &source_path, &symlink_path])
            .output()
            .await
            .map_err(|e| format!("Failed to create symlink: {e}"))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            return Err(format!("Failed to enable site: {err}"));
        }
    } else {
        let out = Command::new("pkexec")
            .args(["rm", "-f", &symlink_path])
            .output()
            .await
            .map_err(|e| format!("Failed to remove symlink: {e}"))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            return Err(format!("Failed to disable site: {err}"));
        }
    }

    // Test nginx config
    let test = run_nginx_test().await;

    if !test.passed {
        // Revert
        if enable {
            let _ = Command::new("pkexec")
                .args(["rm", "-f", &symlink_path])
                .output()
                .await;
        } else {
            let _ = Command::new("pkexec")
                .args(["ln", "-sf", &source_path, &symlink_path])
                .output()
                .await;
        }
        return Ok(test);
    }

    // Reload nginx
    let _ = Command::new("pkexec")
        .args(["systemctl", "reload", "nginx"])
        .output()
        .await;

    log_to_file("INFO", &format!("nginx_toggle_site {name} enable={enable}"));
    Ok(test)
}

#[tauri::command]
pub async fn nginx_create_site(config: NewSiteConfig) -> Result<String, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let safe_name = config
        .server_name
        .replace(['/', ' ', '\0'], "_");

    let path = format!("/etc/nginx/sites-available/{safe_name}.conf");

    let content = if config.is_proxy {
        format!(
            r#"server {{
    listen {port};
    server_name {server_name};

    location / {{
        proxy_pass {proxy_url};
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
{error_pages}}}
"#,
            port = config.port,
            server_name = config.server_name,
            proxy_url = config.proxy_url,
            error_pages = build_error_pages(config.enable_404, config.enable_50x),
        )
    } else {
        format!(
            r#"server {{
    listen {port};
    server_name {server_name};

    root {root};
    index {index};

    location / {{
        try_files $uri $uri/ =404;
    }}
{error_pages}}}
"#,
            port = config.port,
            server_name = config.server_name,
            root = config.root_dir,
            index = config.index_file,
            error_pages = build_error_pages(config.enable_404, config.enable_50x),
        )
    };

    // Ensure dirs exist
    let _ = Command::new("pkexec")
        .args(["mkdir", "-p", "/etc/nginx/sites-available"])
        .output()
        .await;

    pkexec_write(&path, &content).await?;

    let symlink = format!("/etc/nginx/sites-enabled/{safe_name}.conf");
    let _ = Command::new("pkexec")
        .args(["mkdir", "-p", "/etc/nginx/sites-enabled"])
        .output()
        .await;

    let _ = Command::new("pkexec")
        .args(["ln", "-sf", &path, &symlink])
        .output()
        .await;

    let test = run_nginx_test().await;
    if !test.passed {
        let _ = Command::new("pkexec").args(["rm", "-f", &path]).output().await;
        let _ = Command::new("pkexec").args(["rm", "-f", &symlink]).output().await;
        return Err(format!("nginx -t failed after site creation:\n{}", test.output));
    }

    let _ = Command::new("pkexec")
        .args(["systemctl", "reload", "nginx"])
        .output()
        .await;

    log_to_file("INFO", &format!("nginx_create_site {safe_name}"));
    Ok(path)
}

fn build_error_pages(e404: bool, e50x: bool) -> String {
    let mut s = String::new();
    if e404 {
        s.push_str("    error_page 404 /404.html;\n");
    }
    if e50x {
        s.push_str("    error_page 500 502 503 504 /50x.html;\n");
    }
    s
}

#[tauri::command]
pub async fn nginx_delete_site(name: String, path: String) -> Result<(), String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let symlink = format!("/etc/nginx/sites-enabled/{name}");
    let _ = Command::new("pkexec")
        .args(["rm", "-f", &symlink])
        .output()
        .await;

    let out = Command::new("pkexec")
        .args(["rm", "-f", &path])
        .output()
        .await
        .map_err(|e| format!("Failed to delete site: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("Failed to delete site config: {err}"));
    }

    let test = run_nginx_test().await;
    if test.passed {
        let _ = Command::new("pkexec")
            .args(["systemctl", "reload", "nginx"])
            .output()
            .await;
    }

    log_to_file("INFO", &format!("nginx_delete_site {name}"));
    Ok(())
}

// ─── Commands: Config Editor ──────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_list_configs() -> Result<Vec<NginxConfigFile>, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    let mut files = Vec::new();
    let dirs = [
        ("/etc/nginx", "nginx"),
        ("/etc/nginx/conf.d", "conf.d"),
        ("/etc/nginx/sites-available", "sites-available"),
    ];

    for (dir, source) in &dirs {
        let entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut entries = entries;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !name.ends_with(".conf") {
                continue;
            }

            files.push(NginxConfigFile {
                name,
                path: path.to_string_lossy().to_string(),
                source: source.to_string(),
            });
        }
    }

    files.sort_by(|a, b| a.source.cmp(&b.source).then(a.name.cmp(&b.name)));
    Ok(files)
}

#[tauri::command]
pub async fn nginx_read_config(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read {path}: {e}"))
}

#[tauri::command]
pub async fn nginx_write_config(path: String, content: String) -> Result<NginxTestResult, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    // Backup first
    backup_file(&path)
        .await
        .map_err(|e| format!("Backup failed: {e}"))?;

    // Write to temp, rename to real path
    let tmp = format!("{path}.tmp_nginx_test");
    pkexec_write(&tmp, &content).await?;

    let mv_out = Command::new("pkexec")
        .args(["mv", &tmp, &path])
        .output()
        .await
        .map_err(|e| format!("mv failed: {e}"))?;

    if !mv_out.status.success() {
        let err = String::from_utf8_lossy(&mv_out.stderr).to_string();
        return Err(format!("Failed to move temp file: {err}"));
    }

    let test = run_nginx_test().await;

    if !test.passed {
        // Restore from latest backup
        let backup_dir = "/etc/nginx/backups";
        let filename = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Ok(mut dir) = tokio::fs::read_dir(backup_dir).await {
            let mut backups: Vec<String> = Vec::new();
            while let Ok(Some(entry)) = dir.next_entry().await {
                let n = entry.file_name().to_string_lossy().to_string();
                if n.starts_with(&filename) && n.ends_with(".bak") {
                    backups.push(entry.path().to_string_lossy().to_string());
                }
            }
            backups.sort();
            if let Some(latest) = backups.last() {
                let _ = Command::new("pkexec")
                    .args(["cp", latest, &path])
                    .output()
                    .await;
            }
        }

        return Ok(test);
    }

    let _ = Command::new("pkexec")
        .args(["systemctl", "reload", "nginx"])
        .output()
        .await;

    log_to_file("INFO", &format!("nginx_write_config {path}"));
    Ok(test)
}

// ─── Commands: Backups ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_list_backups() -> Result<Vec<NginxBackup>, String> {
    let backup_dir = "/etc/nginx/backups";
    let mut backups = Vec::new();

    let mut entries = match tokio::fs::read_dir(backup_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if !filename.ends_with(".bak") {
            continue;
        }

        // Parse: <original_name>.<timestamp>.bak
        let without_bak = filename.trim_end_matches(".bak");
        let parts: Vec<&str> = without_bak.rsplitn(2, '.').collect();
        let (ts, orig_name) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            (String::new(), without_bak.to_string())
        };

        backups.push(NginxBackup {
            original_path: format!("/etc/nginx/{orig_name}"),
            backup_path: path.to_string_lossy().to_string(),
            timestamp: ts,
            filename,
        });
    }

    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(backups)
}

#[tauri::command]
pub async fn nginx_restore_backup(backup_path: String, original_path: String) -> Result<NginxTestResult, String> {
    if !nginx_installed().await {
        return Err("nginx not installed".to_string());
    }

    // Create a backup of current state before restoring
    let _ = backup_file(&original_path).await;

    let out = Command::new("pkexec")
        .args(["cp", &backup_path, &original_path])
        .output()
        .await
        .map_err(|e| format!("Failed to restore backup: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("Failed to restore: {err}"));
    }

    let test = run_nginx_test().await;

    if test.passed {
        let _ = Command::new("pkexec")
            .args(["systemctl", "reload", "nginx"])
            .output()
            .await;
    }

    log_to_file("INFO", &format!("nginx_restore_backup {backup_path} -> {original_path}"));
    Ok(test)
}

// ─── Commands: WWW Files Browser ──────────────────────────────────────────────

fn build_www_entry_sync(path: &std::path::Path) -> Option<WwwEntry> {
    let meta = std::fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let is_dir = meta.is_dir();
    let size = if is_dir { 0 } else { meta.len() };
    let modified = meta
        .modified()
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_default();

    let children = if is_dir {
        std::fs::read_dir(path)
            .ok()
            .map(|entries| {
                let mut ch: Vec<WwwEntry> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| build_www_entry_sync(&e.path()))
                    .collect();
                ch.sort_by(|a, b| {
                    b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
                });
                ch
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    Some(WwwEntry {
        name,
        path: path.to_string_lossy().to_string(),
        is_dir,
        size,
        modified,
        children,
    })
}

#[tauri::command]
pub async fn nginx_list_www() -> Result<Vec<WwwEntry>, String> {
    let root = std::path::Path::new("/var/www");
    if !root.exists() {
        return Ok(vec![]);
    }

    let entries = std::fs::read_dir(root)
        .map_err(|e| format!("Failed to read /var/www: {e}"))?;

    let mut result: Vec<WwwEntry> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| build_www_entry_sync(&e.path()))
        .collect();

    result.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(result)
}

#[tauri::command]
pub async fn nginx_read_www_file(path: String) -> Result<String, String> {
    if !path.starts_with("/var/www/") {
        return Err("Path must be under /var/www/".to_string());
    }

    let content = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    String::from_utf8(content)
        .map_err(|_| "File is binary and cannot be displayed".to_string())
}

#[tauri::command]
pub async fn nginx_create_www_dir(path: String) -> Result<(), String> {
    if !path.starts_with("/var/www/") {
        return Err("Path must be under /var/www/".to_string());
    }

    let out = Command::new("pkexec")
        .args(["mkdir", "-p", &path])
        .output()
        .await
        .map_err(|e| format!("Failed to create dir: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("mkdir failed: {err}"));
    }

    log_to_file("INFO", &format!("nginx_create_www_dir {path}"));
    Ok(())
}

#[tauri::command]
pub async fn nginx_delete_www_entry(path: String) -> Result<(), String> {
    if !path.starts_with("/var/www/") {
        return Err("Path must be under /var/www/".to_string());
    }

    let out = Command::new("pkexec")
        .args(["rm", "-rf", &path])
        .output()
        .await
        .map_err(|e| format!("Failed to delete: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("rm failed: {err}"));
    }

    log_to_file("INFO", &format!("nginx_delete_www_entry {path}"));
    Ok(())
}

#[tauri::command]
pub async fn nginx_rename_www_entry(old_path: String, new_path: String) -> Result<(), String> {
    if !old_path.starts_with("/var/www/") || !new_path.starts_with("/var/www/") {
        return Err("Paths must be under /var/www/".to_string());
    }

    let out = Command::new("pkexec")
        .args(["mv", &old_path, &new_path])
        .output()
        .await
        .map_err(|e| format!("Failed to rename: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("mv failed: {err}"));
    }

    log_to_file("INFO", &format!("nginx_rename_www_entry {old_path} -> {new_path}"));
    Ok(())
}

#[tauri::command]
pub async fn nginx_upload_www_file(src_path: String, dest_dir: String) -> Result<(), String> {
    if !dest_dir.starts_with("/var/www/") {
        return Err("Destination must be under /var/www/".to_string());
    }

    let filename = Path::new(&src_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "Invalid source path".to_string())?;

    let dest_path = format!("{dest_dir}/{filename}");

    let out = Command::new("pkexec")
        .args(["cp", &src_path, &dest_path])
        .output()
        .await
        .map_err(|e| format!("Failed to copy file: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("cp failed: {err}"));
    }

    log_to_file("INFO", &format!("nginx_upload_www_file {src_path} -> {dest_path}"));
    Ok(())
}

// ─── Commands: Log Viewer ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_read_log(path: String, lines: u32, filter: Option<String>) -> Result<String, String> {
    let allowed_prefix = "/var/log/nginx/";
    if !path.starts_with(allowed_prefix) {
        return Err("Log path must be under /var/log/nginx/".to_string());
    }

    let lines_str = lines.to_string();
    let out = Command::new("pkexec")
        .args(["tail", "-n", &lines_str, &path])
        .output()
        .await
        .map_err(|e| format!("Failed to read log: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("tail failed: {err}"));
    }

    let content = String::from_utf8_lossy(&out.stdout).to_string();

    if let Some(f) = filter {
        if !f.is_empty() {
            let filtered: Vec<&str> = content
                .lines()
                .filter(|line| line.contains(f.as_str()))
                .collect();
            return Ok(filtered.join("\n"));
        }
    }

    Ok(content)
}

#[tauri::command]
pub async fn nginx_clear_log(path: String) -> Result<(), String> {
    let allowed_prefix = "/var/log/nginx/";
    if !path.starts_with(allowed_prefix) {
        return Err("Log path must be under /var/log/nginx/".to_string());
    }

    pkexec_write(&path, "").await?;

    log_to_file("INFO", &format!("nginx_clear_log {path}"));
    Ok(())
}

#[tauri::command]
pub async fn nginx_list_log_files() -> Result<Vec<String>, String> {
    let mut logs = Vec::new();
    let standard = ["/var/log/nginx/access.log", "/var/log/nginx/error.log"];

    for f in &standard {
        if tokio::fs::metadata(f).await.is_ok() {
            logs.push(f.to_string());
        }
    }

    Ok(logs)
}

// ─── Commands: SSL Manager ────────────────────────────────────────────────────

#[tauri::command]
pub async fn nginx_check_certbot() -> bool {
    crate::binary_exists("certbot").await
}

#[tauri::command]
pub async fn nginx_list_ssl_certs() -> Result<Vec<SslCert>, String> {
    if !crate::binary_exists("certbot").await {
        return Ok(vec![]);
    }

    let live_dir = "/etc/letsencrypt/live";
    let mut certs = Vec::new();

    let mut entries = match tokio::fs::read_dir(live_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry.path().is_dir() {
            continue;
        }

        let domain = entry.file_name().to_string_lossy().to_string();
        if domain == "README" {
            continue;
        }

        let cert_path = format!("{live_dir}/{domain}/cert.pem");

        // Get expiry via openssl
        let expiry_out = Command::new("openssl")
            .args(["x509", "-noout", "-enddate", "-in", &cert_path])
            .output()
            .await;

        let (expiry, days) = match expiry_out {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                let date_str = text
                    .trim()
                    .strip_prefix("notAfter=")
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let dt = chrono::DateTime::parse_from_str(
                    &format!("{date_str} +0000"),
                    "%b %e %H:%M:%S %Y GMT %z",
                )
                .ok()
                .map(|d| d.with_timezone(&chrono::Local));

                if let Some(d) = dt {
                    let now = chrono::Local::now();
                    let diff = d.signed_duration_since(now);
                    let days = diff.num_days();
                    (d.format("%Y-%m-%d").to_string(), days)
                } else {
                    (date_str, 0)
                }
            }
            _ => ("Unknown".to_string(), 0),
        };

        let status = if days < 0 {
            "expired".to_string()
        } else if days < 30 {
            "expiring".to_string()
        } else {
            "valid".to_string()
        };

        certs.push(SslCert {
            domain,
            cert_path,
            expiry,
            days_until_expiry: days,
            status,
        });
    }

    certs.sort_by(|a, b| a.domain.cmp(&b.domain));
    Ok(certs)
}

#[tauri::command]
pub async fn nginx_renew_cert(domain: String) -> Result<String, String> {
    if !crate::binary_exists("certbot").await {
        return Err("certbot is not installed".to_string());
    }

    let out = Command::new("pkexec")
        .args(["certbot", "renew", "--cert-name", &domain, "--non-interactive"])
        .output()
        .await
        .map_err(|e| format!("Failed to run certbot: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = format!("{stdout}{stderr}").trim().to_string();

    if !out.status.success() {
        log_to_file("ERROR", &format!("nginx_renew_cert {domain} failed: {stderr}"));
        return Err(format!("certbot renew failed:\n{combined}"));
    }

    log_to_file("INFO", &format!("nginx_renew_cert {domain}"));
    Ok(combined)
}
