use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;

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
                let mut source = "Unknown".to_string();
                let mut package_id = None;
                if path_str.contains("flatpak") {
                    source = "Flatpak".to_string();
                    if let Some(stem) = path.file_stem() {
                        package_id = Some(stem.to_string_lossy().to_string());
                    }
                } else {
                    // Try to resolve RPM
                    source = "RPM".to_string();
                    if let Ok(output) = Command::new("rpm")
                        .arg("-qf")
                        .arg(&path)
                        .output()
                    {
                        if output.status.success() {
                            let pkg = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            if !pkg.contains("is not owned") && !pkg.is_empty() {
                                package_id = Some(pkg);
                            }
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
    
    // Deduplicate by package_id if we have multiple desktop files for the same package
    apps.dedup_by(|a, b| {
        if let (Some(pid_a), Some(pid_b)) = (&a.package_id, &b.package_id) {
            pid_a == pid_b
        } else {
            false
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
        if let Ok(output) = Command::new("flatpak").arg("info").arg("--show-size").arg(&package_id).output() {
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
    } else {
        // RPM
        if let Ok(output) = Command::new("rpm").arg("-q").arg("--queryformat").arg("%{SIZE}|%{INSTALLTIME}").arg(&package_id).output() {
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
    }
}

#[tauri::command]
pub async fn get_app_details(package_id: String, source: String) -> Result<AppDetails, String> {
    if source == "Flatpak" {
        let mut version = "Unknown".to_string();
        let mut description = "No description available.".to_string();
        
        if let Ok(output) = Command::new("flatpak").arg("info").arg(&package_id).output() {
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
    } else {
        // RPM
        let mut version = "Unknown".to_string();
        let mut description = String::new();
        let mut files = Vec::new();
        
        if let Ok(output) = Command::new("rpm").arg("-qi").arg(&package_id).output() {
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
        
        if let Ok(output) = Command::new("rpm").arg("-ql").arg(&package_id).output() {
            if output.status.success() {
                let list = String::from_utf8_lossy(&output.stdout).to_string();
                for line in list.lines() {
                    files.push(line.to_string());
                }
            }
        }
        
        if description.is_empty() {
            description = "No description available.".to_string();
        }
        
        Ok(AppDetails {
            version,
            description: description.trim().to_string(),
            files,
        })
    }
}
