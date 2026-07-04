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

                users.push(UserInfo {
                    username,
                    uid,
                    gid,
                    fullname,
                    home_dir,
                    shell,
                    groups: user_groups,
                    is_sudo,
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

#[tauri::command]
pub async fn delete_user(username: String, remove_home: bool) -> Result<String, String> {
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
    let action = if grant { "-aG" } else { "-d" };
    let output = Command::new("pkexec")
        .args(["/usr/sbin/usermod", action, "wheel", &username])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec usermod: {e}"))?;

    // usermod -d doesn't work that way. Actually, `gpasswd -d user wheel` is better for removal.
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
