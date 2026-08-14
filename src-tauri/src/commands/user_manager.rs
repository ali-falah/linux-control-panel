use serde::{Deserialize, Serialize};
use std::fs;
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub fullname: String,
    pub home_dir: String,
    pub shell: String,
    pub groups: Vec<String>,
    pub is_sudo: bool,
    pub is_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub groupname: String,
    pub gid: u32,
    pub members: Vec<String>,
}

/// Lists all human users (UID >= 1000 and <= 60000)
#[tauri::command]
pub async fn list_users() -> Result<Vec<UserInfo>, String> {
    let passwd_content = fs::read_to_string("/etc/passwd")
        .map_err(|e| format!("Failed to read /etc/passwd: {e}"))?;
    let group_content =
        fs::read_to_string("/etc/group").map_err(|e| format!("Failed to read /etc/group: {e}"))?;

    // Parse groups
    let mut group_memberships: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut gid_to_name: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

    for line in group_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 4 {
            let group_name = parts[0].to_string();
            if let Ok(gid) = parts[2].parse::<u32>() {
                gid_to_name.insert(gid, group_name.clone());
            }
            let members: Vec<&str> = parts[3].split(',').filter(|s| !s.is_empty()).collect();
            for member in members {
                group_memberships
                    .entry(member.to_string())
                    .or_default()
                    .push(group_name.clone());
            }
        }
    }

    let mut users = Vec::new();

    for line in passwd_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 7 {
            let username = parts[0].to_string();
            let uid = parts[2].parse::<u32>().unwrap_or(0);
            let gid = parts[3].parse::<u32>().unwrap_or(0);
            let fullname = parts[4].split(',').next().unwrap_or("").to_string();
            let home_dir = parts[5].to_string();
            let shell = parts[6].to_string();

            if uid >= 1000 && uid <= 60000 {
                let mut user_groups = group_memberships
                    .get(&username)
                    .cloned()
                    .unwrap_or_default();

                // Add primary group if not already listed
                if let Some(primary_group) = gid_to_name.get(&gid) {
                    if !user_groups.contains(primary_group) {
                        user_groups.push(primary_group.clone());
                    }
                }

                let is_sudo = user_groups.contains(&"wheel".to_string())
                    || user_groups.contains(&"sudo".to_string());

                let is_locked = if let Ok(out) = Command::new("passwd").args(["-S", &username]).output().await {
                    let s = String::from_utf8_lossy(&out.stdout);
                    s.contains(" L ") || s.contains(" LK ")
                } else {
                    false
                };

                users.push(UserInfo {
                    username,
                    uid,
                    gid,
                    fullname,
                    home_dir,
                    shell,
                    groups: user_groups,
                    is_sudo,
                    is_locked,
                });
            }
        }
    }

    Ok(users)
}

#[tauri::command]
pub async fn add_user(username: String, fullname: String) -> Result<String, String> {
    let mut args = vec!["/usr/sbin/useradd".to_string(), "-m".to_string()]; // -m creates home dir
    if !fullname.is_empty() {
        args.push("-c".to_string());
        args.push(fullname);
    }
    args.push(username);

    let output = Command::new("pkexec")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec useradd: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("User added successfully".to_string())
}

const PROTECTED_SYSTEM_USERS: &[&str] = &[
    "root", "nobody", "daemon", "bin", "sys", "sync", "games", "man", "lp", "mail",
    "news", "uucp", "proxy", "www-data", "backup", "list", "irc", "gnats", "systemd-network",
    "systemd-resolve", "systemd-timesync", "systemd-coredump", "systemd-oom", "systemd-journal",
    "dbus", "polkitd", "sshd", "chrony", "rpc", "avahi", "colord", "geoclue", "flatpak"
];

fn is_protected_system_user(username: &str) -> bool {
    let lower = username.trim().to_lowercase();
    if lower.starts_with("systemd-") {
        return true;
    }
    PROTECTED_SYSTEM_USERS.contains(&lower.as_str())
}

#[tauri::command]
pub async fn delete_user(username: String, remove_home: bool) -> Result<String, String> {
    if is_protected_system_user(&username) {
        return Err(format!("Action blocked: '{}' is a vital system account and cannot be deleted.", username));
    }

    let mut args = vec!["/usr/sbin/userdel".to_string(), "--force".to_string()];
    if remove_home {
        args.push("--remove".to_string());
    }
    args.push(username);

    let output = Command::new("pkexec")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec userdel: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("User deleted successfully".to_string())
}

#[tauri::command]
pub async fn change_password(username: String, password: String) -> Result<String, String> {
    // We must pass the password to chpasswd via stdin
    let script = format!(
        "echo '{}:{}' | /usr/sbin/chpasswd",
        username,
        password.replace("'", "'\\''")
    );
    let output = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec chpasswd: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Password changed successfully".to_string())
}

#[tauri::command]
pub async fn toggle_sudo(username: String, grant: bool) -> Result<String, String> {
    // Fedora uses 'wheel' group for sudo privileges
    // Actually, `gpasswd -d user wheel` is better for removal.
    let (cmd, args) = if grant {
        ("/usr/sbin/usermod", vec!["-aG", "wheel", &username])
    } else {
        ("/usr/bin/gpasswd", vec!["-d", &username, "wheel"])
    };

    let output = Command::new("pkexec")
        .arg(cmd)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec {}: {}", cmd, e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!(
        "Sudo access {} for {}",
        if grant { "granted" } else { "revoked" },
        username
    ))
}

#[tauri::command]
pub async fn list_groups() -> Result<Vec<GroupInfo>, String> {
    let group_content =
        fs::read_to_string("/etc/group").map_err(|e| format!("Failed to read /etc/group: {e}"))?;
    let mut groups = Vec::new();
    for line in group_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 4 {
            let gid: u32 = parts[2].parse().unwrap_or(0);
            let members: Vec<String> = parts[3]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            groups.push(GroupInfo {
                groupname: parts[0].to_string(),
                gid,
                members,
            });
        }
    }
    Ok(groups)
}

#[tauri::command]
pub async fn add_group(groupname: String) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args(["/usr/sbin/groupadd", &groupname])
        .output()
        .await
        .map_err(|e| format!("pkexec groupadd failed: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(format!("Group {} added", groupname))
}

#[tauri::command]
pub async fn delete_group(groupname: String) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args(["/usr/sbin/groupdel", &groupname])
        .output()
        .await
        .map_err(|e| format!("pkexec groupdel failed: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(format!("Group {} deleted", groupname))
}

#[tauri::command]
pub async fn modify_user_group(
    username: String,
    groupname: String,
    add: bool,
) -> Result<String, String> {
    let output = if add {
        Command::new("pkexec")
            .args(["/usr/sbin/usermod", "-aG", &groupname, &username])
            .output()
            .await
    } else {
        Command::new("pkexec")
            .args(["/usr/bin/gpasswd", "-d", &username, &groupname])
            .output()
            .await
    };
    let output = output.map_err(|e| format!("pkexec failed: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok("Membership updated".to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActiveSession {
    pub session_id: String,
    pub uid: String,
    pub user: String,
    pub seat: String,
    pub tty: String,
    pub state: String,
    pub idle_since_hint: String,
    pub is_current: bool,
}

#[tauri::command]
pub async fn user_get_active_sessions() -> Result<Vec<ActiveSession>, String> {
    let output = Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()
        .await
        .map_err(|e| format!("loginctl failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();
    
    let mut current_session_id = String::new();
    if let Ok(status_out) = Command::new("loginctl").arg("session-status").output().await {
        let status_str = String::from_utf8_lossy(&status_out.stdout);
        if let Some(first_line) = status_str.lines().next() {
            if let Some(id) = first_line.split_whitespace().next() {
                current_session_id = id.to_string();
            }
        }
    }

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let session_id = parts[0].to_string();
            let uid = parts[1].to_string();
            let user = parts[2].to_string();
            let seat = if parts.len() > 3 { parts[3].to_string() } else { "".to_string() };
            
            let details_out = Command::new("loginctl")
                .args(["show-session", &session_id, "-p", "TTY", "-p", "State", "-p", "IdleSinceHint", "-p", "Type", "-p", "Class"])
                .output()
                .await;
            
            let mut tty = "".to_string();
            let mut state = "".to_string();
            let mut idle = "".to_string();
            let mut session_type = "".to_string();
            let mut session_class = "".to_string();

            if let Ok(details) = details_out {
                let det_str = String::from_utf8_lossy(&details.stdout);
                for d_line in det_str.lines() {
                    if let Some((k, v)) = d_line.split_once('=') {
                        match k {
                            "TTY" => tty = v.to_string(),
                            "State" => state = v.to_string(),
                            "IdleSinceHint" => idle = v.to_string(),
                            "Type" => session_type = v.to_string(),
                            "Class" => session_class = v.to_string(),
                            _ => {}
                        }
                    }
                }
            }

            // Skip internal user-manager sessions
            if session_class == "manager" || session_class == "user-manager" {
                continue;
            }

            let mut is_current = session_id == current_session_id;
            
            // Fallback heuristic if loginctl session-status failed to identify a caller session
            if current_session_id.is_empty() || !current_session_id.chars().all(char::is_numeric) {
                if state == "active" && (session_type == "wayland" || session_type == "x11") {
                    is_current = true;
                }
            }

            sessions.push(ActiveSession {
                session_id,
                uid,
                user,
                seat,
                tty,
                state,
                idle_since_hint: idle,
                is_current,
            });
        }
    }

    Ok(sessions)
}

#[tauri::command]
pub async fn user_kill_session(session_id: String) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args(["loginctl", "kill-session", &session_id])
        .output()
        .await
        .map_err(|e| format!("loginctl kill-session failed: {}", e))?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    
    Ok(format!("Session {} terminated.", session_id))
}

#[tauri::command]
pub async fn user_get_ssh_keys(username: String) -> Result<String, String> {
    let path = if username == "root" {
        "/root/.ssh/authorized_keys".to_string()
    } else {
        format!("/home/{}/.ssh/authorized_keys", username)
    };

    let output = Command::new("pkexec")
        .args(["cat", &path])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok("".to_string())
    }
}

#[tauri::command]
pub async fn user_save_ssh_keys(username: String, keys: String) -> Result<String, String> {
    let (home_dir, owner) = if username == "root" {
        ("/root".to_string(), "root:root".to_string())
    } else {
        (format!("/home/{}", username), format!("{}:{}", username, username))
    };

    let ssh_dir = format!("{}/.ssh", home_dir);
    let keys_file = format!("{}/authorized_keys", ssh_dir);

    let _ = Command::new("pkexec").args(["mkdir", "-p", &ssh_dir]).output().await;
    let _ = Command::new("pkexec").args(["chmod", "700", &ssh_dir]).output().await;
    let _ = Command::new("pkexec").args(["chown", &owner, &ssh_dir]).output().await;

    let tmp_path = format!("/tmp/keys_{}", username);
    let _ = tokio::fs::write(&tmp_path, keys).await.map_err(|e| e.to_string())?;

    let mv_out = Command::new("pkexec")
        .args(["mv", &tmp_path, &keys_file])
        .output()
        .await
        .map_err(|e| format!("Failed to move keys file: {}", e))?;

    if !mv_out.status.success() {
        return Err(String::from_utf8_lossy(&mv_out.stderr).to_string());
    }

    let _ = Command::new("pkexec").args(["chmod", "600", &keys_file]).output().await;
    let _ = Command::new("pkexec").args(["chown", &owner, &keys_file]).output().await;

    Ok("SSH keys saved successfully.".to_string())
}

#[tauri::command]
pub async fn toggle_lock_user(username: String, lock: bool) -> Result<String, String> {
    if is_protected_system_user(&username) {
        return Err(format!("Action blocked: '{}' is a vital system account and cannot be locked/unlocked.", username));
    }

    let flag = if lock { "-l" } else { "-u" };
    let output = Command::new("pkexec")
        .args(["/usr/bin/passwd", flag, &username])
        .output()
        .await
        .map_err(|e| format!("pkexec passwd failed: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(format!(
        "User '{}' account successfully {}.",
        username,
        if lock { "locked" } else { "unlocked" }
    ))
}
