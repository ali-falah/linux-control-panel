use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelinuxStatus {
    pub status: String,       // "enabled" or "disabled"
    pub current_mode: String, // "enforcing" or "permissive"
    pub config_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denial {
    pub raw: String,
    pub timestamp: String,
    pub scontext: String,
    pub tcontext: String,
    pub tclass: String,
}

#[tauri::command]
pub async fn get_selinux_status() -> Result<SelinuxStatus, String> {
    if !crate::binary_exists("sestatus").await {
        return Err("sestatus not found. Is SELinux installed?".to_string());
    }

    let output = Command::new("sestatus")
        .output()
        .await
        .map_err(|e| format!("Failed to run sestatus: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut status = String::new();
    let mut current_mode = String::new();
    let mut config_mode = String::new();

    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim().to_string();
            match key {
                "SELinux status" => status = val,
                "Current mode" => current_mode = val,
                "Mode from config file" => config_mode = val,
                _ => {}
            }
        }
    }

    Ok(SelinuxStatus {
        status,
        current_mode,
        config_mode,
    })
}

#[tauri::command]
pub async fn set_selinux_state(mode: String) -> Result<String, String> {
    // mode should be "enforcing", "permissive", or "disabled"

    // Set runtime state if possible
    if mode == "enforcing" || mode == "permissive" {
        let setenforce_val = if mode == "enforcing" { "1" } else { "0" };
        Command::new("pkexec")
            .args(["/usr/sbin/setenforce", setenforce_val])
            .output()
            .await
            .map_err(|e| format!("pkexec setenforce failed: {}", e))?;
    }

    // Set permanent state in /etc/selinux/config
    let script = format!(
        "sed -i 's/^SELINUX=.*/SELINUX={}/' /etc/selinux/config",
        mode
    );
    let output = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!("Successfully set SELinux to {}", mode))
}

#[tauri::command]
pub async fn get_selinux_denials() -> Result<Vec<Denial>, String> {
    // Use ausearch to get AVCs.
    let output = Command::new("pkexec")
        .args(["/usr/sbin/ausearch", "-m", "AVC", "-ts", "recent"])
        .output()
        .await
        .map_err(|e| format!("pkexec ausearch failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut denials = Vec::new();

    // Very naive parsing of ausearch output. Each event is separated by "----"
    for event in stdout.split("----") {
        let event = event.trim();
        if event.is_empty() || event == "<no matches>" {
            continue;
        }

        let mut scontext = String::new();
        let mut tcontext = String::new();
        let mut tclass = String::new();
        let mut timestamp = String::new();

        if let Some(msg_idx) = event.find("msg='audit(") {
            let ts_start = msg_idx + 11;
            if let Some(ts_end) = event[ts_start..].find(':') {
                timestamp = event[ts_start..ts_start + ts_end].to_string();
            }
        }

        if let Some(idx) = event.find("scontext=") {
            let rest = &event[idx + 9..];
            scontext = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        }
        if let Some(idx) = event.find("tcontext=") {
            let rest = &event[idx + 9..];
            tcontext = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
        }
        if let Some(idx) = event.find("tclass=") {
            let rest = &event[idx + 7..];
            tclass = rest.split_whitespace().next().unwrap_or("").to_string();
        }

        if !scontext.is_empty() {
            denials.push(Denial {
                raw: event.to_string(),
                timestamp,
                scontext,
                tcontext,
                tclass,
            });
        }
    }

    Ok(denials)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelinuxBoolean {
    pub name: String,
    pub value: bool,
}

#[tauri::command]
pub async fn selinux_get_booleans() -> Result<Vec<SelinuxBoolean>, String> {
    if !crate::binary_exists("getsebool").await {
        return Err("getsebool command is not available".to_string());
    }

    let output = Command::new("getsebool")
        .arg("-a")
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut booleans = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(" --> ") {
            let name = k.trim().to_string();
            let value = v.trim() == "on";
            booleans.push(SelinuxBoolean { name, value });
        }
    }
    Ok(booleans)
}

#[tauri::command]
pub async fn selinux_set_boolean(
    name: String,
    value: bool,
    permanent: bool,
) -> Result<String, String> {
    let value_str = if value { "1" } else { "0" };
    let mut args = Vec::new();
    if permanent {
        args.push("-P");
    }
    args.push(&name);
    args.push(value_str);

    let output = Command::new("pkexec")
        .args(["setsebool"])
        .args(args)
        .output()
        .await
        .map_err(|e| format!("pkexec setsebool failed: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!(
        "Successfully set boolean {} to {} ({})",
        name,
        if value { "on" } else { "off" },
        if permanent { "permanent" } else { "runtime" }
    ))
}

#[tauri::command]
pub async fn selinux_explain_denial(raw_log: String) -> Result<String, String> {
    if !crate::binary_exists("audit2allow").await {
        return Err("audit2allow command is not available. Please install policycoreutils-python-utils.".to_string());
    }

    use tokio::io::AsyncWriteExt;
    let mut child = Command::new("audit2allow")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn audit2allow: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(raw_log.as_bytes()).await.map_err(|e| e.to_string())?;
    }

    let output = child.wait_with_output().await.map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout_str)
}

#[tauri::command]
pub async fn selinux_apply_policy_override(name: String, raw_log: String) -> Result<String, String> {
    if !crate::binary_exists("audit2allow").await {
        return Err("audit2allow is not available".to_string());
    }
    if !crate::binary_exists("semodule").await {
        return Err("semodule is not available".to_string());
    }

    let module_name: String = name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if module_name.is_empty() {
        return Err("Invalid module name".to_string());
    }

    let temp_dir = std::env::temp_dir();
    let te_path = temp_dir.join(format!("{}.te", module_name));
    let pp_path = temp_dir.join(format!("{}.pp", module_name));

    use tokio::io::AsyncWriteExt;
    
    let mut child = Command::new("pkexec")
        .args(["audit2allow", "-M", &module_name])
        .current_dir(&temp_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn audit2allow: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(raw_log.as_bytes()).await.map_err(|e| e.to_string())?;
    }

    let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("audit2allow -M failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let pp_file_str = pp_path.to_string_lossy().to_string();
    let install_out = Command::new("pkexec")
        .args(["semodule", "-i", &pp_file_str])
        .output()
        .await
        .map_err(|e| format!("Failed to run semodule: {e}"))?;

    let _ = std::fs::remove_file(te_path);
    let _ = std::fs::remove_file(pp_path);

    if !install_out.status.success() {
        return Err(format!("semodule -i failed: {}", String::from_utf8_lossy(&install_out.stderr)));
    }

    Ok(format!("Successfully compiled and installed policy module '{}'", module_name))
}
