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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelinuxBoolean {
    pub name: String,
    pub value: bool,
    pub is_critical: bool,
    pub risk_description: Option<String>,
}

fn get_boolean_criticality(name: &str) -> (bool, Option<String>) {
    match name {
        "ssh_sysadm_login" | "allow_ssh_keysign" => (
            true,
            Some("Restricts OpenSSH administrative or key authentication. Disabling may lock out remote SSH access.".to_string())
        ),
        "login_user_exec" | "authlogin_nsswitch_use_ldap" => (
            true,
            Some("Controls core user login and authentication execution. Changing may block PAM user logins.".to_string())
        ),
        "dbus_system_bus" | "systemd_homed" => (
            true,
            Some("Controls core systemd and D-Bus IPC transitions. Changing may disrupt desktop and system services.".to_string())
        ),
        "domain_can_mmap_files" => (
            true,
            Some("Controls memory file mapping across domains. Disabling may cause widespread application crashes.".to_string())
        ),
        "cron_userdomain_transition" => (
            true,
            Some("Controls scheduled cron execution domains. Disabling may silently break system cron jobs.".to_string())
        ),
        "selinuxuser_ping" => (
            false,
            Some("Controls whether standard unprivileged users can execute the ping binary.".to_string())
        ),
        _ => (false, None),
    }
}

#[tauri::command]
pub async fn get_selinux_status() -> Result<SelinuxStatus, String> {
    if crate::binary_exists("sestatus").await {
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

        return Ok(SelinuxStatus {
            status,
            current_mode,
            config_mode,
        });
    }

    // AppArmor detection for Debian / Ubuntu
    if crate::binary_exists("aa-status").await || std::path::Path::new("/sys/kernel/security/apparmor").exists() {
        let output = Command::new("aa-status")
            .output()
            .await;

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let is_loaded = stdout.contains("apparmor module is loaded");
            let has_enforce = stdout.contains("in enforce mode");

            return Ok(SelinuxStatus {
                status: if is_loaded { "enabled (AppArmor)".to_string() } else { "disabled".to_string() },
                current_mode: if has_enforce { "enforcing".to_string() } else { "complain".to_string() },
                config_mode: "apparmor".to_string(),
            });
        }
    }

    Ok(SelinuxStatus {
        status: "disabled".to_string(),
        current_mode: "not installed".to_string(),
        config_mode: "none".to_string(),
    })
}

#[tauri::command]
pub async fn set_selinux_state(mode: String) -> Result<String, String> {
    let mode_lower = mode.trim().to_lowercase();
    if mode_lower == "disabled" {
        return Err("Disabling SELinux is blocked to protect system security. Resolve the policy denial or use a temporary permissive mode only for troubleshooting.".to_string());
    }
    if mode_lower != "enforcing" && mode_lower != "permissive" {
        return Err(format!("Invalid SELinux mode '{}'. Only 'enforcing' or 'permissive' are permitted.", mode));
    }

    let setenforce_value = if mode_lower == "enforcing" { "1" } else { "0" };
    let script = format!(
        r#"set -eu
config=/etc/selinux/config
backup=/etc/selinux/config.bak
test -f "$config" && test ! -L "$config" || {{ echo 'SELinux configuration is missing or unsafe to edit.' >&2; exit 1; }}
test ! -L "$backup" || {{ echo 'SELinux backup path is a symbolic link; refusing to continue.' >&2; exit 1; }}
tmp=$(mktemp /etc/selinux/config.XXXXXX)
trap 'rm -f "$tmp"' EXIT
cp -p "$config" "$tmp"
sed -i 's/^SELINUX=.*/SELINUX={mode_lower}/' "$tmp"
count=$(grep -c '^SELINUX=' "$tmp" || true)
test "$count" -eq 1 && grep -q '^SELINUX={mode_lower}$' "$tmp" || {{ echo 'SELINUX setting could not be safely updated.' >&2; exit 1; }}
cp -p "$config" "$backup"
mv -f "$tmp" "$config"
if ! /usr/sbin/setenforce {setenforce_value}; then
    cp -p "$backup" "$config"
    echo 'Runtime mode change failed; persistent configuration was restored.' >&2
    exit 1
fi
trap - EXIT"#
    );
    let output = Command::new("pkexec")
        .args(["/bin/bash", "-c", &script])
        .output()
        .await
        .map_err(|e| format!("Could not request authorization to change SELinux mode: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(if stderr.trim().is_empty() {
            "SELinux change was not applied. The system authorization was canceled or the operation failed.".to_string()
        } else {
            format!("SELinux change was not applied: {stderr}")
        });
    }

    Ok(format!("Successfully set SELinux to {}", mode_lower))
}

#[tauri::command]
pub async fn set_selinux_mode(mode: String) -> Result<String, String> {
    set_selinux_state(mode).await
}

#[tauri::command]
pub async fn get_selinux_denials() -> Result<Vec<Denial>, String> {
    if crate::binary_exists("ausearch").await {
        let output = Command::new("pkexec")
            .args(["/usr/sbin/ausearch", "-m", "AVC", "-ts", "recent"])
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut denials = Vec::new();

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

                return Ok(denials);
            }
        }
    }

    // AppArmor denials from journalctl
    if crate::binary_exists("journalctl").await {
        let output = Command::new("journalctl")
            .args(["-k", "--grep=apparmor=\"DENIED\"", "-n", "30", "--no-pager"])
            .output()
            .await;

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut denials = Vec::new();
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.contains("apparmor=\"DENIED\"") {
                    continue;
                }

                let mut scontext = String::new();
                let mut tcontext = String::new();
                let mut tclass = String::new();

                if let Some(idx) = trimmed.find("profile=\"") {
                    let rest = &trimmed[idx + 9..];
                    if let Some(end) = rest.find('"') {
                        scontext = rest[..end].to_string();
                    }
                }
                if let Some(idx) = trimmed.find("name=\"") {
                    let rest = &trimmed[idx + 6..];
                    if let Some(end) = rest.find('"') {
                        tcontext = rest[..end].to_string();
                    }
                }
                if let Some(idx) = trimmed.find("operation=\"") {
                    let rest = &trimmed[idx + 11..];
                    if let Some(end) = rest.find('"') {
                        tclass = rest[..end].to_string();
                    }
                }

                let timestamp = trimmed.split_whitespace().take(3).collect::<Vec<_>>().join(" ");

                denials.push(Denial {
                    raw: trimmed.to_string(),
                    timestamp,
                    scontext: if scontext.is_empty() { "AppArmor".to_string() } else { scontext },
                    tcontext,
                    tclass,
                });
            }
            return Ok(denials);
        }
    }

    Ok(Vec::new())
}

#[tauri::command]
pub async fn selinux_get_booleans() -> Result<Vec<SelinuxBoolean>, String> {
    if !crate::binary_exists("getsebool").await {
        return Ok(Vec::new());
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
            let (is_critical, risk_description) = get_boolean_criticality(&name);
            booleans.push(SelinuxBoolean { name, value, is_critical, risk_description });
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
    let name_clean = name.trim();
    if !name_clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name_clean.is_empty() {
        return Err("Invalid boolean name. Only alphanumeric characters and underscores are permitted.".to_string());
    }

    let value_str = if value { "1" } else { "0" };
    let mut args = Vec::new();
    if permanent {
        args.push("-P");
    }
    args.push(name_clean);
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
        name_clean,
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

    if module_name.len() < 2 || module_name.len() > 64 || !module_name.chars().next().unwrap_or('0').is_alphabetic() {
        return Err("Invalid module name. Name must start with a letter, contain only alphanumeric and underscores, and be between 2-64 characters.".to_string());
    }

    if raw_log.trim().is_empty() {
        return Err("Cannot generate policy override: raw AVC log is empty.".to_string());
    }

    let temp_dir = std::env::temp_dir();
    let te_path = temp_dir.join(format!("{}.te", module_name));
    let pp_path = temp_dir.join(format!("{}.pp", module_name));

    use tokio::io::AsyncWriteExt;
    
    let mut child = Command::new("audit2allow")
        .args(["-M", &module_name])
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

#[tauri::command]
pub async fn selinux_apply_audit2allow(module_name: String, raw_log: String) -> Result<String, String> {
    selinux_apply_policy_override(module_name, raw_log).await
}

