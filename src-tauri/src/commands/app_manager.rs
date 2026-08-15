use serde::Serialize;
use std::fs;
use std::path::Path;
use crate::utils::privilege::tokio::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct DesktopApp {
    pub name: String,
    pub exec: String,
    pub source: String,
    pub package_id: Option<String>,
    pub file_path: String,
}

#[tauri::command]
pub async fn list_desktop_apps() -> Result<Vec<DesktopApp>, String> {
    let mut apps = Vec::new();

    let paths_to_scan = vec![
        "/usr/share/applications",
        "/var/lib/flatpak/exports/share/applications",
    ];

    let home_dir = dirs::home_dir();
    let local_share;
    let local_flatpak;
    
    if let Some(home) = home_dir {
        local_share = home.join(".local/share/applications");
        if local_share.exists() {
            // we could push it
        }
        local_flatpak = home.join(".local/share/flatpak/exports/share/applications");
        if local_flatpak.exists() {
            // we could push it
        }
    }

    let mut desktop_files = Vec::new();
    for p in &paths_to_scan {
        let dir = Path::new(p);
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "desktop") {
                        desktop_files.push(path);
                    }
                }
            }
        }
    }
    
    if let Some(home) = dirs::home_dir() {
        let ls = home.join(".local/share/applications");
        if ls.exists() {
            if let Ok(entries) = fs::read_dir(ls) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "desktop") {
                        desktop_files.push(path);
                    }
                }
            }
        }
    }

    for path in desktop_files {
        if let Ok(content) = fs::read_to_string(&path) {
            let mut name = None;
            let mut exec = None;
            let mut no_display = false;

            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("Name=") && name.is_none() {
                    name = Some(line.trim_start_matches("Name=").to_string());
                } else if line.starts_with("Exec=") && exec.is_none() {
                    exec = Some(line.trim_start_matches("Exec=").to_string());
                } else if line.starts_with("NoDisplay=true") || line.starts_with("NoDisplay=1") {
                    no_display = true;
                }
            }

            if no_display {
                continue;
            }

            if let (Some(n), Some(e)) = (name, exec) {
                let path_str = path.to_string_lossy().to_string();
                let source;
                let mut package_id = None;
                if path_str.contains("flatpak") {
                    source = "Flatpak".to_string();
                    if let Some(stem) = path.file_stem() {
                        package_id = Some(stem.to_string_lossy().to_string());
                    }
                } else if e.starts_with("waydroid ") || path_str.contains("waydroid") || content.contains("X-WayDroid") || content.contains("waydroid") {
                    source = "Waydroid".to_string();
                    if let Some(stem) = path.file_stem() {
                        package_id = Some(stem.to_string_lossy().to_string());
                    }
                } else {
                    if let Ok(output) = Command::new("rpm")
                        .arg("-qf")
                        .arg(&path)
                        .output().await
                    {
                        if output.status.success() {
                            let pkg = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            if !pkg.contains("is not owned") && !pkg.is_empty() {
                                package_id = Some(pkg);
                            }
                        }
                    }
                    if package_id.is_some() {
                        source = "RPM".to_string();
                    } else {
                        source = "Local".to_string();
                        if let Some(stem) = path.file_stem() {
                            package_id = Some(stem.to_string_lossy().to_string());
                        }
                    }
                }

                apps.push(DesktopApp {
                    name: n,
                    exec: e,
                    source,
                    package_id,
                    file_path: path_str,
                });
            }
        }
    }

    // Sort alphabetically by name
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    
    // Deduplicate by package_id / name
    apps.dedup_by(|a, b| {
        if let (Some(pid_a), Some(pid_b)) = (&a.package_id, &b.package_id) {
            pid_a == pid_b && a.source == b.source
        } else {
            a.name == b.name && a.source == b.source
        }
    });

    Ok(apps)
}

#[derive(Serialize)]
pub struct AppMeta {
    pub size_bytes: u64,
    pub install_date: u64,
}

#[derive(Serialize)]
pub struct AppDetails {
    pub version: String,
    pub description: String,
    pub files: Vec<String>,
}

#[tauri::command]
pub async fn get_app_meta(package_id: String, source: String) -> Result<AppMeta, String> {
    if source == "Flatpak" {
        let mut size_bytes = 0;
        if let Ok(output) = Command::new("flatpak").arg("info").arg("--show-size").arg(&package_id).output().await {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(bytes) = s.parse::<u64>() {
                    size_bytes = bytes;
                }
            }
        }
        Ok(AppMeta {
            size_bytes,
            install_date: 0,
        })
    } else if source == "RPM" {
        // RPM
        if let Ok(output) = Command::new("rpm").arg("-q").arg("--queryformat").arg("%{SIZE}|%{INSTALLTIME}").arg(&package_id).output().await {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let parts: Vec<&str> = s.split('|').collect();
                if parts.len() == 2 {
                    let size_bytes = parts[0].parse::<u64>().unwrap_or(0);
                    let install_date = parts[1].parse::<u64>().unwrap_or(0);
                    return Ok(AppMeta { size_bytes, install_date });
                }
            }
        }
        Ok(AppMeta { size_bytes: 0, install_date: 0 })
    } else {
        Ok(AppMeta { size_bytes: 0, install_date: 0 })
    }
}

#[tauri::command]
pub async fn get_app_details(package_id: String, source: String) -> Result<AppDetails, String> {
    if source == "Flatpak" {
        let mut version = "Unknown".to_string();
        let mut description = "No description available.".to_string();
        
        if let Ok(output) = Command::new("flatpak").arg("info").arg(&package_id).output().await {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout).to_string();
                for line in info.lines() {
                    if line.trim().starts_with("Version:") {
                        version = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
                    } else if line.trim().starts_with("Description:") {
                        // flatpak info description might be empty or missing. 
                        // But usually Name/Description are at the top.
                    }
                }
            }
        }
        
        Ok(AppDetails {
            version,
            description,
            files: Vec::new(),
        })
    } else if source == "Waydroid" {
        let pkg = package_id.trim_start_matches("waydroid.");
        Ok(AppDetails {
            version: "Android App".to_string(),
            description: format!("Waydroid Android package ({})", pkg),
            files: Vec::new(),
        })
    } else if source == "Local" {
        Ok(AppDetails {
            version: "Local Application".to_string(),
            description: "Desktop launcher / user-installed local program.".to_string(),
            files: Vec::new(),
        })
    } else {
        // RPM
        let mut version = "Unknown".to_string();
        let mut description = String::new();
        let mut files = Vec::new();
        
        if let Ok(output) = Command::new("rpm").arg("-qi").arg(&package_id).output().await {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout).to_string();
                let mut in_desc = false;
                for line in info.lines() {
                    if in_desc {
                        description.push_str(line);
                        description.push('\n');
                    } else if line.starts_with("Version") {
                        if let Some(idx) = line.find(':') {
                            version = line[idx+1..].trim().to_string();
                        }
                    } else if line.starts_with("Description") {
                        in_desc = true;
                    }
                }
            }
        }
        
        if let Ok(output) = Command::new("rpm").arg("-ql").arg(&package_id).output().await {
            if output.status.success() {
                let list = String::from_utf8_lossy(&output.stdout).to_string();
                for line in list.lines() {
                    files.push(line.to_string());
                }
            }
        }
        
        
        Ok(AppDetails {
            version,
            description: description.trim().to_string(),
            files,
        })
    }
}

#[tauri::command]
pub async fn uninstall_app(app_handle: AppHandle, package_id: String, source: String) -> Result<(), String> {
    if source != "Flatpak" && source != "Waydroid" && source != "Local" && crate::commands::flatpak_rpm::is_protected_package(&package_id) {
        return Err(format!("Action blocked: '{}' is a vital system package and cannot be uninstalled.", package_id));
    }

    if source == "Local" {
        let home = dirs::home_dir().unwrap_or_default();
        let local_path = home.join(".local/share/applications").join(format!("{}.desktop", package_id));
        if local_path.exists() {
            let _ = fs::remove_file(local_path);
        }
        let _ = app_handle.emit("uninstall-log", "Removed local application desktop entry.");
        return Ok(());
    }

    let mut cmd = if source == "Flatpak" {
        let mut c = Command::new("pkexec");
        c.args(&["flatpak", "uninstall", "-y", &package_id]);
        c
    } else if source == "Waydroid" {
        let mut c = Command::new("waydroid");
        let pkg = package_id.trim_start_matches("waydroid.");
        c.args(&["app", "remove", pkg]);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.args(&["dnf", "remove", "-y", &package_id]);
        c
    };

    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone.emit("uninstall-log", line);
        }
    });

    let app_handle_clone2 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone2.emit("uninstall-log", line);
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    
    if !status.success() {
        return Err(format!("Uninstallation failed with status: {}", status));
    }

    let _ = app_handle.emit("uninstall-log", "
Uninstallation completed. Running cleanup...");

    let mut cleanup_cmd = if source == "Flatpak" {
        let mut c = Command::new("pkexec");
        c.args(&["flatpak", "uninstall", "--unused", "-y"]);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.args(&["dnf", "autoremove", "-y"]);
        c
    };

    cleanup_cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut cleanup_child = cleanup_cmd.spawn().map_err(|e| e.to_string())?;

    let cleanup_stdout = cleanup_child.stdout.take().unwrap();
    let cleanup_stderr = cleanup_child.stderr.take().unwrap();

    let app_handle_clone3 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(cleanup_stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone3.emit("uninstall-log", line);
        }
    });

    let app_handle_clone4 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(cleanup_stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone4.emit("uninstall-log", line);
        }
    });

    let cleanup_status = cleanup_child.wait().await.map_err(|e| e.to_string())?;
    let _ = app_handle.emit("uninstall-log", format!("
Cleanup finished with code {}.", cleanup_status.code().unwrap_or(1)));

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlatpakPermissions {
    pub network: bool,
    pub ipc: bool,
    pub fallback_x11: bool,
    pub x11: bool,
    pub wayland: bool,
    pub pulseaudio: bool,
    pub gpu: bool,
    pub host_files: bool,
    pub home_files: bool,
}

fn parse_flatpak_ini(content: &str, permissions: &mut FlatpakPermissions, _is_override: bool) {
    let mut in_context = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_context = line.to_lowercase() == "[context]";
        } else if in_context {
            if let Some((key, val)) = line.split_once('=') {
                let val = val.trim();
                let items: Vec<&str> = val.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                
                match key.trim().to_lowercase().as_str() {
                    "shared" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "network" {
                                permissions.network = !negated;
                            } else if name == "ipc" {
                                permissions.ipc = !negated;
                            }
                        }
                    }
                    "sockets" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            match name {
                                "x11" => permissions.x11 = !negated,
                                "wayland" => permissions.wayland = !negated,
                                "fallback-x11" => permissions.fallback_x11 = !negated,
                                "pulseaudio" => permissions.pulseaudio = !negated,
                                _ => {}
                            }
                        }
                    }
                    "devices" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "dri" {
                                permissions.gpu = !negated;
                            }
                        }
                    }
                    "filesystems" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "host" {
                                permissions.host_files = !negated;
                            } else if name == "home" {
                                permissions.home_files = !negated;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tauri::command]
pub async fn get_flatpak_permissions(app_id: String) -> Result<FlatpakPermissions, String> {
    let mut perms = FlatpakPermissions {
        network: false,
        ipc: false,
        fallback_x11: false,
        x11: false,
        wayland: false,
        pulseaudio: false,
        gpu: false,
        host_files: false,
        home_files: false,
    };

    let mut base_metadata = None;
    let paths = vec![
        format!("/var/lib/flatpak/app/{}/current/active/metadata", app_id),
        format!("/var/lib/flatpak/app/{}/x86_64/stable/active/metadata", app_id),
    ];

    let mut found_path = None;
    for p in paths {
        let path = Path::new(&p);
        if path.exists() {
            found_path = Some(path.to_path_buf());
            break;
        }
    }

    if found_path.is_none() {
        if let Some(home) = dirs::home_dir() {
            let user_paths = vec![
                home.join(format!(".local/share/flatpak/app/{}/current/active/metadata", app_id)),
                home.join(format!(".local/share/flatpak/app/{}/x86_64/stable/active/metadata", app_id)),
            ];
            for p in user_paths {
                if p.exists() {
                    found_path = Some(p);
                    break;
                }
            }
        }
    }

    if let Some(path) = found_path {
        if let Ok(content) = fs::read_to_string(path) {
            base_metadata = Some(content);
        }
    }

    if base_metadata.is_none() {
        if let Ok(output) = Command::new("flatpak")
            .args(["info", "--show-metadata", &app_id])
            .output()
            .await
        {
            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout).to_string();
                base_metadata = Some(content);
            }
        }
    }

    if let Some(content) = base_metadata {
        parse_flatpak_ini(&content, &mut perms, false);
    }

    if let Some(home) = dirs::home_dir() {
        let override_path = home.join(format!(".local/share/flatpak/overrides/{}", app_id));
        if override_path.exists() {
            if let Ok(content) = fs::read_to_string(override_path) {
                parse_flatpak_ini(&content, &mut perms, true);
            }
        }
    }

    Ok(perms)
}

#[tauri::command]
pub async fn set_flatpak_permission(
    app_id: String,
    permission: String,
    enable: bool,
) -> Result<(), String> {
    let mut args = vec!["override", "--user"];
    
    let opt = match permission.as_str() {
        "network" => if enable { "--share=network" } else { "--unshare=network" },
        "ipc" => if enable { "--share=ipc" } else { "--unshare=ipc" },
        "x11" => if enable { "--socket=x11" } else { "--nosocket=x11" },
        "wayland" => if enable { "--socket=wayland" } else { "--nosocket=wayland" },
        "fallback_x11" => if enable { "--socket=fallback-x11" } else { "--nosocket=fallback-x11" },
        "pulseaudio" => if enable { "--socket=pulseaudio" } else { "--nosocket=pulseaudio" },
        "gpu" => if enable { "--device=dri" } else { "--nodevice=dri" },
        "host_files" => if enable { "--filesystem=host" } else { "--nofilesystem=host" },
        "home_files" => if enable { "--filesystem=home" } else { "--nofilesystem=home" },
        _ => return Err(format!("Unknown permission type: {}", permission)),
    };
    
    args.push(opt);
    args.push(&app_id);

    let output = Command::new("flatpak")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run flatpak override: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("flatpak override failed: {err}"));
    }

    Ok(())
}

#[tauri::command]
pub async fn get_app_dependencies(package_id: String, source: String) -> Result<Vec<String>, String> {
    let mut deps = Vec::new();
    if source == "Flatpak" {
        if let Ok(output) = Command::new("flatpak")
            .arg("info")
            .arg(&package_id)
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.starts_with("Runtime:") || line.starts_with("Sdk:") || line.starts_with("Ref:") {
                        deps.push(line.to_string());
                    }
                }
            }
        }
    } else {
        if let Ok(output) = Command::new("rpm")
            .arg("-q")
            .arg("--requires")
            .arg(&package_id)
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if !line.starts_with("rpmlib(") && !line.is_empty() {
                        deps.push(line.to_string());
                    }
                }
            }
        }
    }
    
    Ok(deps)
}

#[tauri::command]
pub async fn scan_local_appimages() -> Result<Vec<DesktopApp>, String> {
    let mut apps = Vec::new();
    let mut scan_dirs = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let apps_dir = home.join("Applications");
        if apps_dir.exists() {
            scan_dirs.push(apps_dir);
        }
        let bin_dir = home.join(".local/bin");
        if bin_dir.exists() {
            scan_dirs.push(bin_dir);
        }
    }

    for dir in scan_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let lower = filename.to_lowercase();
                    if lower.ends_with(".appimage") {
                        let path_str = path.to_string_lossy().to_string();
                        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        let desktop_filename = format!("appimage-{}.desktop", stem.replace(' ', "-").to_lowercase());
                        
                        let has_shortcut = dirs::home_dir()
                            .map(|h| h.join(".local/share/applications").join(&desktop_filename).exists())
                            .unwrap_or(false);

                        apps.push(DesktopApp {
                            name: stem,
                            exec: path_str.clone(),
                            source: "AppImage".to_string(),
                            package_id: Some(if has_shortcut { desktop_filename } else { "".to_string() }),
                            file_path: path_str,
                        });
                    }
                }
            }
        }
    }

    Ok(apps)
}

#[tauri::command]
pub async fn register_appimage(
    name: String,
    exec_path: String,
    icon: Option<String>,
    create_shortcut: bool,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let desktop_dir = home.join(".local/share/applications");
    let _ = fs::create_dir_all(&desktop_dir);

    let stem = Path::new(&exec_path).file_stem().unwrap_or_default().to_string_lossy().to_string();
    let desktop_filename = format!("appimage-{}.desktop", stem.replace(' ', "-").to_lowercase());
    let desktop_filepath = desktop_dir.join(&desktop_filename);

    if create_shortcut {
        let content = format!(
            "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nIcon={}\nCategories=Utility;Application;\nTerminal=false\nComment=AppImage Application\n",
            name,
            exec_path,
            icon.unwrap_or_else(|| "system-run".to_string())
        );
        fs::write(&desktop_filepath, content)
            .map_err(|e| format!("Failed to write desktop shortcut file: {e}"))?;
    } else {
        if desktop_filepath.exists() {
            let _ = fs::remove_file(desktop_filepath);
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn launch_desktop_app(exec: String) -> Result<(), String> {
    let clean_exec = exec
        .split_whitespace()
        .filter(|s| !s.starts_with('%'))
        .collect::<Vec<&str>>()
        .join(" ");

    let cmd = if clean_exec.trim().is_empty() { exec } else { clean_exec };

    tokio::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} &", cmd))
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let target = if p.is_file() {
        p.parent().unwrap_or(p).to_string_lossy().to_string()
    } else {
        path
    };

    tokio::process::Command::new("xdg-open")
        .arg(&target)
        .spawn()
        .map_err(|e| format!("Failed to open file manager: {e}"))?;

    Ok(())
}

