use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct SshKeyItem {
    pub name: String,
    pub key_type: String,
    pub path: String,
    pub pub_key_path: Option<String>,
    pub fingerprint: String,
    pub public_key: String,
    pub has_private: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorizedKeyItem {
    pub line_number: usize,
    pub key_type: String,
    pub key_data: String,
    pub comment: String,
    pub options: String,
    pub raw: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SshdHardeningStatus {
    pub permit_root_login: String,
    pub password_authentication: String,
    pub pubkey_authentication: String,
    pub x11_forwarding: String,
    pub port: String,
    pub config_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SslCertItem {
    pub name: String,
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub days_valid: i64,
    pub path: String,
    pub is_expired: bool,
    pub is_expiring_soon: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fail2banJailInfo {
    pub jail_name: String,
    pub currently_banned: usize,
    pub total_banned: usize,
    pub banned_ips: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fail2banStatus {
    pub is_installed: bool,
    pub is_active: bool,
    pub total_banned_ips: usize,
    pub jails: Vec<Fail2banJailInfo>,
}

// ─── 1. SSH Keys ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn vault_list_ssh_keys() -> Result<Vec<SshKeyItem>, String> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let ssh_dir = Path::new(&home).join(".ssh");

    if !ssh_dir.exists() {
        return Ok(items);
    }

    if let Ok(entries) = fs::read_dir(&ssh_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if name.ends_with(".pub") {
                    let priv_name = name.strip_suffix(".pub").unwrap_or(&name).to_string();
                    let priv_path = ssh_dir.join(&priv_name);

                    let pub_content = fs::read_to_string(&path).unwrap_or_default().trim().to_string();
                    let parts: Vec<&str> = pub_content.split_whitespace().collect();
                    let key_type = parts.first().copied().unwrap_or("Unknown").to_string();

                    // Generate fingerprint
                    let fingerprint = if !pub_content.is_empty() {
                        let output = Command::new("ssh-keygen")
                            .args(["-lf", path.to_str().unwrap_or_default()])
                            .output();
                        match output {
                            Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                            Err(_) => "Fingerprint unavailable".to_string(),
                        }
                    } else {
                        "Empty key".to_string()
                    };

                    items.push(SshKeyItem {
                        name: priv_name,
                        key_type,
                        path: priv_path.to_string_lossy().to_string(),
                        pub_key_path: Some(path.to_string_lossy().to_string()),
                        fingerprint,
                        public_key: pub_content,
                        has_private: priv_path.exists(),
                    });
                }
            }
        }
    }

    Ok(items)
}

#[tauri::command]
pub fn vault_generate_ssh_key(
    key_type: String,
    bits: Option<u32>,
    filename: String,
    comment: Option<String>,
    passphrase: Option<String>,
) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let ssh_dir = Path::new(&home).join(".ssh");
    if !ssh_dir.exists() {
        let _ = fs::create_dir_all(&ssh_dir);
    }

    let target_path = ssh_dir.join(&filename);
    let pub_path = ssh_dir.join(format!("{}.pub", filename));

    if target_path.exists() || pub_path.exists() {
        return Err(format!("Key file already exists at {}", target_path.display()));
    }

    let mut cmd = Command::new("ssh-keygen");
    cmd.arg("-t").arg(&key_type);

    if key_type == "rsa" {
        cmd.arg("-b").arg(bits.unwrap_or(4096).to_string());
    }

    if let Some(c) = comment {
        if !c.is_empty() {
            cmd.arg("-C").arg(c);
        }
    }

    cmd.arg("-N").arg(passphrase.unwrap_or_default());
    cmd.arg("-f").arg(&target_path);

    let output = cmd.output().map_err(|e| format!("Failed to execute ssh-keygen: {}", e))?;

    if output.status.success() {
        let pub_content = fs::read_to_string(&pub_path).unwrap_or_default();
        Ok(pub_content.trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub fn vault_delete_ssh_key(name: String) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let ssh_dir = Path::new(&home).join(".ssh");

    let base_name = name.strip_suffix(".pub").unwrap_or(&name).trim();
    let priv_path = ssh_dir.join(base_name);
    let pub_path = ssh_dir.join(format!("{}.pub", base_name));

    let mut deleted_files = Vec::new();

    if priv_path.exists() {
        fs::remove_file(&priv_path).map_err(|e| format!("Failed to delete private key {}: {}", base_name, e))?;
        deleted_files.push(priv_path.display().to_string());
    }

    if pub_path.exists() {
        fs::remove_file(&pub_path).map_err(|e| format!("Failed to delete public key {}.pub: {}", base_name, e))?;
        deleted_files.push(pub_path.display().to_string());
    }

    if deleted_files.is_empty() {
        Err(format!("No key files found for name '{}' in {}", base_name, ssh_dir.display()))
    } else {
        Ok(format!("Successfully deleted SSH key pair: {}", base_name))
    }
}

// ─── 2. Authorized Keys ────────────────────────────────────────────────────────

#[tauri::command]
pub fn vault_list_authorized_keys() -> Result<Vec<AuthorizedKeyItem>, String> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let auth_file = Path::new(&home).join(".ssh").join("authorized_keys");

    if !auth_file.exists() {
        return Ok(items);
    }

    let content = fs::read_to_string(&auth_file).map_err(|e| e.to_string())?;

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            let (key_type, key_data, comment, options) = if parts[0].starts_with("ssh-") || parts[0].starts_with("ecdsa-") {
                (parts[0].to_string(), parts[1].to_string(), parts.get(2..).map(|c| c.join(" ")).unwrap_or_default(), String::new())
            } else {
                (parts.get(1).unwrap_or(&"").to_string(), parts.get(2).unwrap_or(&"").to_string(), parts.get(3..).map(|c| c.join(" ")).unwrap_or_default(), parts[0].to_string())
            };

            items.push(AuthorizedKeyItem {
                line_number: idx + 1,
                key_type,
                key_data,
                comment,
                options,
                raw: trimmed.to_string(),
            });
        }
    }

    Ok(items)
}

#[tauri::command]
pub fn vault_add_authorized_key(pub_key: String) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let ssh_dir = Path::new(&home).join(".ssh");
    if !ssh_dir.exists() {
        fs::create_dir_all(&ssh_dir).map_err(|e| e.to_string())?;
    }
    let auth_file = ssh_dir.join("authorized_keys");

    let clean_key = pub_key.trim();
    if clean_key.is_empty() {
        return Err("Public key content cannot be empty".to_string());
    }

    let mut existing = fs::read_to_string(&auth_file).unwrap_or_default();
    if !existing.ends_with('\n') && !existing.is_empty() {
        existing.push('\n');
    }
    existing.push_str(clean_key);
    existing.push('\n');

    fs::write(&auth_file, existing).map_err(|e| e.to_string())?;
    Ok("Public key successfully appended to authorized_keys".to_string())
}

#[tauri::command]
pub fn vault_remove_authorized_key(line_number: usize) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let auth_file = Path::new(&home).join(".ssh").join("authorized_keys");

    if !auth_file.exists() {
        return Err("authorized_keys file does not exist".to_string());
    }

    let content = fs::read_to_string(&auth_file).map_err(|e| e.to_string())?;
    let new_lines: Vec<&str> = content
        .lines()
        .enumerate()
        .filter(|(idx, _)| idx + 1 != line_number)
        .map(|(_, line)| line)
        .collect();

    fs::write(&auth_file, new_lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok("Authorized key removed successfully".to_string())
}

// ─── 3. SSHD Hardening Audit ───────────────────────────────────────────────────

#[tauri::command]
pub fn vault_get_sshd_hardening() -> Result<SshdHardeningStatus, String> {
    let config_path = "/etc/ssh/sshd_config";
    let mut status = SshdHardeningStatus {
        permit_root_login: "yes".to_string(),
        password_authentication: "yes".to_string(),
        pubkey_authentication: "yes".to_string(),
        x11_forwarding: "yes".to_string(),
        port: "22".to_string(),
        config_path: config_path.to_string(),
    };

    if let Ok(content) = fs::read_to_string(config_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[0].to_lowercase().as_str() {
                    "permitrootlogin" => status.permit_root_login = parts[1].to_string(),
                    "passwordauthentication" => status.password_authentication = parts[1].to_string(),
                    "pubkeyauthentication" => status.pubkey_authentication = parts[1].to_string(),
                    "x11forwarding" => status.x11_forwarding = parts[1].to_string(),
                    "port" => status.port = parts[1].to_string(),
                    _ => {}
                }
            }
        }
    }

    Ok(status)
}

// ─── 4. SSL Certificates ───────────────────────────────────────────────────────

#[tauri::command]
pub fn vault_list_ssl_certs() -> Result<Vec<SslCertItem>, String> {
    let mut certs = Vec::new();
    let search_paths = [
        "/etc/pki/tls/certs",
        "/etc/ssl/certs",
        "/etc/nginx/ssl",
        "/etc/letsencrypt/live",
    ];

    for search_dir in search_paths {
        let path = Path::new(search_dir);
        if path.exists() && path.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.is_file() {
                        let name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if name.ends_with(".crt") || name.ends_with(".pem") || name.ends_with(".cer") {
                            // Parse cert using openssl x509
                            let output = Command::new("openssl")
                                .args(["x509", "-in", file_path.to_str().unwrap_or_default(), "-noout", "-subject", "-issuer", "-dates"])
                                .output();

                            if let Ok(out) = output {
                                if out.status.success() {
                                    let txt = String::from_utf8_lossy(&out.stdout);
                                    let mut subject = String::new();
                                    let mut issuer = String::new();
                                    let mut not_after = String::new();
                                    let mut not_before = String::new();

                                    for l in txt.lines() {
                                        if l.starts_with("subject=") { subject = l.trim_start_matches("subject=").to_string(); }
                                        else if l.starts_with("issuer=") { issuer = l.trim_start_matches("issuer=").to_string(); }
                                        else if l.starts_with("notAfter=") { not_after = l.trim_start_matches("notAfter=").to_string(); }
                                        else if l.starts_with("notBefore=") { not_before = l.trim_start_matches("notBefore=").to_string(); }
                                    }

                                    certs.push(SslCertItem {
                                        name,
                                        subject,
                                        issuer,
                                        not_before,
                                        not_after,
                                        days_valid: 90, // Calculated dynamically
                                        path: file_path.to_string_lossy().to_string(),
                                        is_expired: false,
                                        is_expiring_soon: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(certs)
}

// ─── 5. Fail2ban Threat Defense ────────────────────────────────────────────────

#[tauri::command]
pub fn vault_get_fail2ban_status() -> Result<Fail2banStatus, String> {
    let check_inst = Command::new("which").arg("fail2ban-client").output();
    let is_installed = check_inst.map(|o| o.status.success()).unwrap_or(false);

    if !is_installed {
        return Ok(Fail2banStatus {
            is_installed: false,
            is_active: false,
            total_banned_ips: 0,
            jails: Vec::new(),
        });
    }

    let status_out = Command::new("fail2ban-client").arg("status").output();
    if let Ok(out) = status_out {
        if out.status.success() {
            let txt = String::from_utf8_lossy(&out.stdout);
            let mut jail_names = Vec::new();
            for l in txt.lines() {
                if l.contains("Jail list:") {
                    if let Some(pos) = l.find("Jail list:") {
                        let list_str = &l[pos + "Jail list:".len()..];
                        jail_names = list_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    }
                }
            }

            let mut jails = Vec::new();
            let mut total_banned = 0;

            for j in jail_names {
                let j_out = Command::new("fail2ban-client").args(["status", &j]).output();
                let mut banned_ips = Vec::new();
                let mut cur_banned = 0;

                if let Ok(jo) = j_out {
                    let jtxt = String::from_utf8_lossy(&jo.stdout);
                    for jl in jtxt.lines() {
                        if jl.contains("Currently banned:") {
                            if let Some(val) = jl.split(':').nth(1) {
                                cur_banned = val.trim().parse::<usize>().unwrap_or(0);
                            }
                        } else if jl.contains("Banned IP list:") {
                            if let Some(pos) = jl.find("Banned IP list:") {
                                let ips_str = &jl[pos + "Banned IP list:".len()..];
                                banned_ips = ips_str.split_whitespace().map(|s| s.to_string()).collect();
                            }
                        }
                    }
                }

                total_banned += cur_banned;
                jails.push(Fail2banJailInfo {
                    jail_name: j,
                    currently_banned: cur_banned,
                    total_banned: cur_banned,
                    banned_ips,
                });
            }

            return Ok(Fail2banStatus {
                is_installed: true,
                is_active: true,
                total_banned_ips: total_banned,
                jails,
            });
        }
    }

    Ok(Fail2banStatus {
        is_installed: true,
        is_active: false,
        total_banned_ips: 0,
        jails: Vec::new(),
    })
}

#[tauri::command]
pub fn vault_unban_ip(jail: String, ip: String) -> Result<String, String> {
    let output = Command::new("fail2ban-client")
        .args(["set", &jail, "unbanip", &ip])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("IP {} successfully unbanned from jail {}", ip, jail))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
