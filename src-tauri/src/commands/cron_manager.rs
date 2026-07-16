use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub raw: String,
    pub schedule: String,
    pub command: String,
    pub is_root: bool,
}

#[tauri::command]
pub async fn list_cron_jobs() -> Result<Vec<CronJob>, String> {
    let mut jobs = Vec::new();

    // User cron
    let user_out = Command::new("crontab").args(["-l"]).output().await;
    if let Ok(out) = user_out {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Parse "m h d mon dow command"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let schedule = parts[0..5].join(" ");
                    let command = parts[5..].join(" ");
                    jobs.push(CronJob {
                        raw: line.to_string(),
                        schedule,
                        command,
                        is_root: false,
                    });
                }
            }
        }
    }

    // Root cron (requires pkexec)
    let root_out = Command::new("pkexec")
        .args(["/usr/bin/crontab", "-l"])
        .output()
        .await;
    if let Ok(out) = root_out {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let schedule = parts[0..5].join(" ");
                    let command = parts[5..].join(" ");
                    jobs.push(CronJob {
                        raw: line.to_string(),
                        schedule,
                        command,
                        is_root: true,
                    });
                }
            }
        }
    }

    Ok(jobs)
}

#[tauri::command]
pub async fn add_cron_job(
    schedule: String,
    command: String,
    is_root: bool,
) -> Result<String, String> {
    let new_job = format!("{} {}", schedule, command);

    // Create a temporary script to append the cron job
    let script = format!(
        "(crontab -l 2>/dev/null; echo \"{}\") | crontab -",
        new_job.replace("\"", "\\\"")
    );

    let output = if is_root {
        Command::new("pkexec")
            .args(["bash", "-c", &script])
            .output()
            .await
    } else {
        Command::new("bash").args(["-c", &script]).output().await
    };

    let output = output.map_err(|e| format!("Failed to spawn process: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Cron job added successfully".to_string())
}

#[tauri::command]
pub async fn delete_cron_job(raw: String, is_root: bool) -> Result<String, String> {
    // Escape the raw string for grep -v
    let script = format!(
        "(crontab -l 2>/dev/null | grep -F -v \"{}\") | crontab -",
        raw.replace("\"", "\\\"").replace("$", "\\$")
    );

    let output = if is_root {
        Command::new("pkexec")
            .args(["bash", "-c", &script])
            .output()
            .await
    } else {
        Command::new("bash").args(["-c", &script]).output().await
    };

    let output = output.map_err(|e| format!("Failed to spawn process: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Cron job deleted successfully".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdTimer {
    pub unit: String,
    pub activates: String,
    pub status: String,
    pub description: String,
}

#[tauri::command]
pub async fn cron_list_timers() -> Result<Vec<SystemdTimer>, String> {
    let output = Command::new("systemctl")
        .args(["list-units", "--type=timer", "--all", "--no-legend", "--no-pager"])
        .output()
        .await
        .map_err(|e| format!("systemctl list-units failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut timers = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let unit = parts[0].to_string();
            let _load = parts[1].to_string();
            let active = parts[2].to_string();
            let sub = parts[3].to_string();
            let description = if parts.len() > 4 {
                parts[4..].join(" ")
            } else {
                "".to_string()
            };
            
            let status = format!("{} ({})", active, sub);
            let activates = unit.replace(".timer", ".service");

            timers.push(SystemdTimer {
                unit,
                activates,
                status,
                description,
            });
        }
    }

    Ok(timers)
}

#[tauri::command]
pub async fn cron_toggle_timer(unit: String, enable: bool) -> Result<String, String> {
    let action = if enable { "enable" } else { "disable" };
    let output = Command::new("pkexec")
        .args(["systemctl", action, "--now", &unit])
        .output()
        .await
        .map_err(|e| format!("systemctl failed: {}", e))?;
        
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    
    Ok(format!("Timer {} successfully", if enable { "enabled and started" } else { "disabled and stopped" }))
}
