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

/// Reject a cron entry that could not be represented safely.
fn validate_cron_entry(entry: &str) -> Result<(), String> {
    if entry.trim().is_empty() {
        return Err("The cron job must not be empty.".to_string());
    }
    if entry.contains('\n') || entry.contains('\r') || entry.contains('\0') {
        return Err("The cron job must not contain line breaks.".to_string());
    }
    Ok(())
}

/// Read the current crontab (with or without root rights).
async fn read_crontab(is_root: bool) -> Result<String, String> {
    let args = vec![String::from("-l")];
    let output = if is_root {
        crate::utils::privilege::run_privileged_output(
            "/usr/bin/crontab",
            &args,
            &crate::utils::privilege::RootAuth::Polkit,
        )
        .await?
    } else {
        let mut cmd = tokio::process::Command::new("crontab");
        cmd.args(&args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.output()
            .await
            .map_err(|e| format!("Failed to read the crontab: {e}"))?
    };

    // "no crontab for user" is not an error — it just means the file is empty
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    if !output.status.success() && !stderr.contains("no crontab") {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Write the crontab (with or without root rights).
async fn write_crontab(is_root: bool, content: &str) -> Result<(), String> {
    let args = vec![String::from("-")];
    let output = if is_root {
        crate::utils::privilege::run_privileged_with_stdin(
            "/usr/bin/crontab",
            &args,
            &crate::utils::privilege::RootAuth::Polkit,
            content,
        )
        .await?
    } else {
        use ::tokio::io::AsyncWriteExt;
        let mut cmd = tokio::process::Command::new("crontab");
        cmd.args(&args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to write the crontab: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(content.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        child
            .wait_with_output()
            .await
            .map_err(|e| format!("Failed to write the crontab: {e}"))?
    };

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn add_cron_job(
    schedule: String,
    command: String,
    is_root: bool,
) -> Result<String, String> {
    let new_job = format!("{} {}", schedule, command);
    validate_cron_entry(&new_job)?;

    // The crontab is read, extended in the app and written back through
    // `crontab -`. No shell is involved, so a cron command can never be
    // interpreted as a shell command while it is added with root rights.
    let current = read_crontab(is_root).await?;
    let mut content = current;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&new_job);
    content.push('\n');

    write_crontab(is_root, &content).await?;

    Ok("Cron job added successfully".to_string())
}

#[tauri::command]
pub async fn delete_cron_job(raw: String, is_root: bool) -> Result<String, String> {
    // The entry is removed in the app and the result is written back with
    // `crontab -` — again without a shell.
    let current = read_crontab(is_root).await?;
    let remaining: Vec<&str> = current
        .lines()
        .filter(|line| line.trim() != raw.trim())
        .collect();
    let mut content = remaining.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }

    write_crontab(is_root, &content).await?;

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
