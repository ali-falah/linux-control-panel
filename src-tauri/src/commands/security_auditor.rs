use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: String, // "Critical", "Warning", "Good"
    pub countermeasure: String,
    pub has_auto_fix: bool,
    pub is_resolved: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityReport {
    pub score: u32,
    pub findings: Vec<SecurityFinding>,
}

#[tauri::command]
pub async fn security_run_audit() -> Result<SecurityReport, String> {
    use std::panic::AssertUnwindSafe;
    use futures::FutureExt;

    let result = AssertUnwindSafe(async {
        crate::log_to_file("INFO", "security_run_audit: started");
        let mut findings = Vec::new();
        let mut max_score = 0;
        let mut current_score = 0;

    // 1. SSH Root Login
    let ssh_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("pkexec")
            .args(["cat", "/etc/ssh/sshd_config"])
            .stdout(std::process::Stdio::piped())
            .output()
    ).await;
        
    let mut root_login_disabled = false;
    let mut permit_val = "unknown/default".to_string();
    
    if let Ok(Ok(o)) = ssh_out {
        let ssh_cfg = String::from_utf8_lossy(&o.stdout);
        for line in ssh_cfg.lines() {
            let l = line.trim();
            if l.starts_with("PermitRootLogin") {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() > 1 {
                    permit_val = parts[1].to_string();
                    if permit_val == "no" || permit_val == "prohibit-password" {
                        root_login_disabled = true;
                    }
                }
            }
        }
    }

    max_score += 25;
    if root_login_disabled {
        current_score += 25;
        findings.push(SecurityFinding {
            id: "ssh_root".to_string(),
            title: "SSH Root Login".to_string(),
            description: format!("Root login over SSH is secured (current setting: {}).", permit_val),
            severity: "Good".to_string(),
            countermeasure: "None required. Maintain current configuration to prevent brute force root attacks.".to_string(),
            has_auto_fix: true,
            is_resolved: true,
        });
    } else {
        findings.push(SecurityFinding {
            id: "ssh_root".to_string(),
            title: "SSH Root Login Enabled".to_string(),
            description: "Root login over SSH is currently permitted. This exposes the most privileged account to brute force attacks over the network.".to_string(),
            severity: "Critical".to_string(),
            countermeasure: "Set PermitRootLogin to 'prohibit-password' or 'no' in /etc/ssh/sshd_config.".to_string(),
            has_auto_fix: true,
            is_resolved: false,
        });
    }

    crate::log_to_file("INFO", "security_run_audit: SSH check completed");

    // 2. SELinux Status
    let mut selinux_enforcing = false;
    if crate::binary_exists("sestatus").await {
        let se_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("sestatus").output()
        ).await;
        if let Ok(Ok(o)) = se_out {
            let text = String::from_utf8_lossy(&o.stdout);
            if text.contains("SELinux status:                 enabled") && text.contains("Current mode:                   enforcing") {
                selinux_enforcing = true;
            }
        }
    }

    max_score += 25;
    if selinux_enforcing {
        current_score += 25;
        findings.push(SecurityFinding {
            id: "selinux".to_string(),
            title: "SELinux Status".to_string(),
            description: "SELinux is enabled and enforcing.".to_string(),
            severity: "Good".to_string(),
            countermeasure: "Maintain enforcing mode to enforce mandatory access controls (MAC).".to_string(),
            has_auto_fix: true,
            is_resolved: true,
        });
    } else {
        findings.push(SecurityFinding {
            id: "selinux".to_string(),
            title: "SELinux Disabled or Permissive".to_string(),
            description: "SELinux is not actively enforcing policies. This removes a critical layer of defense against privilege escalation and zero-day vulnerabilities.".to_string(),
            severity: "Critical".to_string(),
            countermeasure: "Enable SELinux. Note: System must be rebooted after enabling to apply file contexts (autorelabel).".to_string(),
            has_auto_fix: true,
            is_resolved: false,
        });
    }

    crate::log_to_file("INFO", "security_run_audit: SELinux check completed");

    // 3. Firewall
    let mut fw_active = false;
    if crate::binary_exists("firewall-cmd").await {
        let fw_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd").args(["--state"]).output()
        ).await;
        if let Ok(Ok(o)) = fw_out {
            if o.status.success() { fw_active = true; }
        }
    }
    if !fw_active && crate::binary_exists("ufw").await {
        let ufw_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("ufw").args(["status"]).output()
        ).await;
        if let Ok(Ok(o)) = ufw_out {
            if String::from_utf8_lossy(&o.stdout).contains("Status: active") {
                fw_active = true;
            }
        }
    }

    max_score += 25;
    if fw_active {
        current_score += 25;
        findings.push(SecurityFinding {
            id: "firewall".to_string(),
            title: "System Firewall Active".to_string(),
            description: "A system firewall (firewalld or ufw) is currently running.".to_string(),
            severity: "Good".to_string(),
            countermeasure: "Ensure default incoming policy is set to drop/deny and only open necessary ports.".to_string(),
            has_auto_fix: true,
            is_resolved: true,
        });
    } else {
        findings.push(SecurityFinding {
            id: "firewall".to_string(),
            title: "Firewall Inactive".to_string(),
            description: "No active firewall detected. The system is exposing all open network ports unconditionally to the network.".to_string(),
            severity: "Critical".to_string(),
            countermeasure: "Enable firewalld or ufw with a default deny incoming policy.".to_string(),
            has_auto_fix: true,
            is_resolved: false,
        });
    }

    crate::log_to_file("INFO", "security_run_audit: Firewall check completed");

    // 4. Password Policy (PASS_MAX_DAYS)
    let mut good_pass_policy = false;
    let mut cur_max = "unknown".to_string();
    let login_out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("cat").arg("/etc/login.defs").output()
    ).await;
    if let Ok(Ok(o)) = login_out {
        let text = String::from_utf8_lossy(&o.stdout);
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with("PASS_MAX_DAYS") {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() > 1 {
                    cur_max = parts[1].to_string();
                    if let Ok(days) = parts[1].parse::<u32>() {
                        if days <= 90 {
                            good_pass_policy = true;
                        }
                    }
                }
            }
        }
    }

    max_score += 25;
    if good_pass_policy {
        current_score += 25;
        findings.push(SecurityFinding {
            id: "pass_policy".to_string(),
            title: "Password Aging Policy".to_string(),
            description: format!("PASS_MAX_DAYS is restricted (current: {}).", cur_max),
            severity: "Good".to_string(),
            countermeasure: "Maintain regular password rotations.".to_string(),
            has_auto_fix: true,
            is_resolved: true,
        });
    } else {
        findings.push(SecurityFinding {
            id: "pass_policy".to_string(),
            title: "Weak Password Policy".to_string(),
            description: format!("Passwords are allowed to be valid for too long (PASS_MAX_DAYS={}).", cur_max),
            severity: "Warning".to_string(),
            countermeasure: "Update /etc/login.defs to restrict PASS_MAX_DAYS to 90 and increase PASS_MIN_LEN.".to_string(),
            has_auto_fix: true,
            is_resolved: false,
        });
    }

        let score = if max_score > 0 { ((current_score as f32 / max_score as f32) * 100.0) as u32 } else { 0 };

        crate::log_to_file("INFO", "security_run_audit: completed successfully");
        Ok(SecurityReport { score, findings })
    }).catch_unwind().await;

    match result {
        Ok(res) => res,
        Err(err) => {
            let msg = if let Some(s) = err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            Err(format!("Rust panic: {}", msg))
        }
    }
}

#[tauri::command]
pub async fn security_fix_root_ssh(enable: bool) -> Result<String, String> {
    let script = if enable {
        "sed -i 's/^#\\?PermitRootLogin.*/PermitRootLogin prohibit-password/g' /etc/ssh/sshd_config"
    } else {
        "sed -i 's/^#\\?PermitRootLogin.*/PermitRootLogin yes/g' /etc/ssh/sshd_config"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", &format!("{} && (systemctl is-active --quiet sshd && systemctl reload sshd || systemctl is-active --quiet ssh && systemctl reload ssh || true)", script)])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(if enable { "Root SSH set to prohibit-password." } else { "Root SSH enabled." }.to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[tauri::command]
pub async fn security_fix_password_policy() -> Result<String, String> {
    let script = "sed -i 's/^PASS_MAX_DAYS.*/PASS_MAX_DAYS   90/g' /etc/login.defs && sed -i 's/^PASS_MIN_LEN.*/PASS_MIN_LEN    12/g' /etc/login.defs";
    
    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok("Password policy updated successfully.".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[tauri::command]
pub async fn security_fix_firewall() -> Result<String, String> {
    let has_firewalld = crate::binary_exists("firewall-cmd").await;
    if has_firewalld {
        let _ = Command::new("pkexec").args(["systemctl", "enable", "--now", "firewalld"]).output().await;
        let out = Command::new("pkexec").args(["firewall-cmd", "--set-default-zone=drop"]).output().await.map_err(|e| e.to_string())?;
        if out.status.success() { return Ok("Firewalld enabled with default drop.".to_string()); }
    }

    let has_ufw = crate::binary_exists("ufw").await;
    if has_ufw {
        let _ = Command::new("pkexec").args(["ufw", "--force", "enable"]).output().await;
        let out = Command::new("pkexec").args(["ufw", "default", "deny", "incoming"]).output().await.map_err(|e| e.to_string())?;
        if out.status.success() { return Ok("UFW enabled with default deny.".to_string()); }
    }

    Err("Neither firewalld nor ufw could be configured.".to_string())
}

#[tauri::command]
pub async fn security_fix_selinux(enable: bool) -> Result<String, String> {
    let script = if enable {
        "sed -i 's/^SELINUX=.*/SELINUX=enforcing/g' /etc/selinux/config && touch /.autorelabel"
    } else {
        "sed -i 's/^SELINUX=.*/SELINUX=disabled/g' /etc/selinux/config"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        if enable {
            Ok("SELinux set to enforcing. A reboot is required to relabel the filesystem.".to_string())
        } else {
            Ok("SELinux disabled. A reboot is required.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}
