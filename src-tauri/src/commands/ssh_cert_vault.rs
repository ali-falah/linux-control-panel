use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::utils::privilege::tokio::Command as PrivCommand;
use chrono::{DateTime, Utc, NaiveDateTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyItem {
    pub name: String,
    pub key_type: String,
    pub path: String,
    pub pub_key_path: Option<String>,
    pub fingerprint: String,
    pub public_key: String,
    pub has_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshClientHost {
    pub host: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity_file: String,
    pub proxy_jump: String,
    pub extra_config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHostItem {
    pub line_number: usize,
    pub host: String,
    pub key_type: String,
    pub fingerprint: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedKeyItem {
    pub line_number: usize,
    pub key_type: String,
    pub key_data: String,
    pub comment: String,
    pub options: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshdHardeningStatus {
    pub permit_root_login: String,
    pub password_authentication: String,
    pub pubkey_authentication: String,
    pub x11_forwarding: String,
    pub port: String,
    pub config_path: String,
    pub is_evaluated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslCertItem {
    pub name: String,
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub days_valid: i64,
    pub path: String,
    pub san_domains: Vec<String>,
    pub is_expired: bool,
    pub is_expiring_soon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fail2banJailInfo {
    pub jail_name: String,
    pub currently_banned: usize,
    pub total_banned: usize,
    pub banned_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                            Ok(out) => {
                                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                if s.is_empty() { "Fingerprint unavailable".to_string() } else { s }
                            }
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

// ─── 2. SSH Client Config (~/.ssh/config) ──────────────────────────────────────

#[tauri::command]
pub fn vault_list_ssh_client_config() -> Result<Vec<SshClientHost>, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let config_file = Path::new(&home).join(".ssh").join("config");

    if !config_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&config_file).unwrap_or_default();
    let mut hosts = Vec::new();
    let mut current_host: Option<SshClientHost> = None;
    let mut extra_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let key = parts[0].to_lowercase();
        if key == "host" && parts.len() >= 2 {
            if let Some(mut prev) = current_host.take() {
                prev.extra_config = extra_lines.join("\n");
                hosts.push(prev);
                extra_lines.clear();
            }
            current_host = Some(SshClientHost {
                host: parts[1..].join(" "),
                hostname: String::new(),
                user: String::new(),
                port: String::new(),
                identity_file: String::new(),
                proxy_jump: String::new(),
                extra_config: String::new(),
            });
        } else if let Some(ref mut host) = current_host {
            let val = parts[1..].join(" ");
            match key.as_str() {
                "hostname" => host.hostname = val,
                "user" => host.user = val,
                "port" => host.port = val,
                "identityfile" => host.identity_file = val,
                "proxyjump" => host.proxy_jump = val,
                _ => extra_lines.push(trimmed.to_string()),
            }
        }
    }

    if let Some(mut last) = current_host {
        last.extra_config = extra_lines.join("\n");
        hosts.push(last);
    }

    Ok(hosts)
}

#[tauri::command]
pub fn vault_save_ssh_client_config(hosts: Vec<SshClientHost>) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let ssh_dir = Path::new(&home).join(".ssh");
    if !ssh_dir.exists() {
        let _ = fs::create_dir_all(&ssh_dir);
    }
    let config_file = ssh_dir.join("config");

    let mut output = String::new();
    for h in hosts {
        if h.host.trim().is_empty() {
            continue;
        }
        output.push_str(&format!("Host {}\n", h.host.trim()));
        if !h.hostname.trim().is_empty() {
            output.push_str(&format!("    HostName {}\n", h.hostname.trim()));
        }
        if !h.user.trim().is_empty() {
            output.push_str(&format!("    User {}\n", h.user.trim()));
        }
        if !h.port.trim().is_empty() {
            output.push_str(&format!("    Port {}\n", h.port.trim()));
        }
        if !h.identity_file.trim().is_empty() {
            output.push_str(&format!("    IdentityFile {}\n", h.identity_file.trim()));
        }
        if !h.proxy_jump.trim().is_empty() {
            output.push_str(&format!("    ProxyJump {}\n", h.proxy_jump.trim()));
        }
        for extra in h.extra_config.lines() {
            let etrim = extra.trim();
            if !etrim.is_empty() {
                output.push_str(&format!("    {}\n", etrim));
            }
        }
        output.push('\n');
    }

    fs::write(&config_file, output).map_err(|e| format!("Failed to write ~/.ssh/config: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn vault_delete_ssh_client_host(host_name: String) -> Result<(), String> {
    let mut current_hosts = vault_list_ssh_client_config()?;
    current_hosts.retain(|h| h.host != host_name);
    vault_save_ssh_client_config(current_hosts)
}

// ─── 3. Known Hosts (~/.ssh/known_hosts) ───────────────────────────────────────

#[tauri::command]
pub fn vault_list_known_hosts() -> Result<Vec<KnownHostItem>, String> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let kh_file = Path::new(&home).join(".ssh").join("known_hosts");

    if !kh_file.exists() {
        return Ok(items);
    }

    let content = fs::read_to_string(&kh_file).unwrap_or_default();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            let host_raw = parts[0];
            let key_type = parts.get(1).copied().unwrap_or("Unknown").to_string();
            let key_data = parts.get(2).copied().unwrap_or("").to_string();

            let display_host = if host_raw.starts_with("|1|") {
                format!("[Hashed Host #{}]", idx + 1)
            } else {
                host_raw.to_string()
            };

            // Calculate fingerprint if key_data available
            let fingerprint = if !key_data.is_empty() {
                let temp_key = format!("{} {}\n", key_type, key_data);
                let child = Command::new("ssh-keygen")
                    .args(["-lf", "-"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                match child {
                    Ok(mut c) => {
                        if let Some(mut sin) = c.stdin.take() {
                            use std::io::Write;
                            let _ = sin.write_all(temp_key.as_bytes());
                        }
                        match c.wait_with_output() {
                            Ok(o) => {
                                let res = String::from_utf8_lossy(&o.stdout).trim().to_string();
                                if res.is_empty() { "Available".to_string() } else { res }
                            }
                            Err(_) => "Available".to_string(),
                        }
                    }
                    Err(_) => "Available".to_string(),
                }
            } else {
                "N/A".to_string()
            };

            items.push(KnownHostItem {
                line_number: idx + 1,
                host: display_host,
                key_type,
                fingerprint,
                raw: trimmed.to_string(),
            });
        }
    }

    Ok(items)
}

#[tauri::command]
pub fn vault_remove_known_host(line_number: usize) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let kh_file = Path::new(&home).join(".ssh").join("known_hosts");

    if !kh_file.exists() {
        return Err("known_hosts file does not exist".to_string());
    }

    let content = fs::read_to_string(&kh_file).map_err(|e| e.to_string())?;
    let new_lines: Vec<&str> = content
        .lines()
        .enumerate()
        .filter(|(idx, _)| idx + 1 != line_number)
        .map(|(_, l)| l)
        .collect();

    fs::write(&kh_file, new_lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn vault_clear_known_hosts() -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let kh_file = Path::new(&home).join(".ssh").join("known_hosts");

    if kh_file.exists() {
        fs::write(&kh_file, "").map_err(|e| e.to_string())?;
    }
    Ok("All known host records cleared successfully".to_string())
}

// ─── 4. Authorized Keys ────────────────────────────────────────────────────────

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
            let is_key_algo = |s: &str| -> bool {
                s.starts_with("ssh-") || s.starts_with("ecdsa-") || s.starts_with("sk-ssh-") || s.starts_with("sk-ecdsa-")
            };

            let (key_type, key_data, comment, options) = if is_key_algo(parts[0]) {
                (parts[0].to_string(), parts[1].to_string(), parts.get(2..).map(|c| c.join(" ")).unwrap_or_default(), String::new())
            } else if parts.len() >= 3 && is_key_algo(parts[1]) {
                (parts[1].to_string(), parts[2].to_string(), parts.get(3..).map(|c| c.join(" ")).unwrap_or_default(), parts[0].to_string())
            } else {
                (parts[0].to_string(), parts.get(1).unwrap_or(&"").to_string(), String::new(), String::new())
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

// ─── 5. SSHD Hardening Audit ───────────────────────────────────────────────────

#[tauri::command]
pub async fn vault_get_sshd_hardening() -> Result<SshdHardeningStatus, String> {
    let config_path = "/etc/ssh/sshd_config";
    let mut status = SshdHardeningStatus {
        permit_root_login: "prohibit-password".to_string(),
        password_authentication: "yes".to_string(),
        pubkey_authentication: "yes".to_string(),
        x11_forwarding: "no".to_string(),
        port: "22".to_string(),
        config_path: config_path.to_string(),
        is_evaluated: false,
        error: None,
    };

    let sshd_bin = if Path::new("/usr/sbin/sshd").exists() {
        "/usr/sbin/sshd"
    } else if Path::new("/usr/bin/sshd").exists() {
        "/usr/bin/sshd"
    } else {
        "sshd"
    };

    // 1. Try evaluating active sshd daemon config directly (sshd -T)
    let sshd_t = PrivCommand::new("pkexec")
        .args([sshd_bin, "-T"])
        .output()
        .await;

    if let Ok(out) = sshd_t {
        if out.status.success() {
            let txt = String::from_utf8_lossy(&out.stdout);
            for line in txt.lines() {
                let trimmed = line.trim();
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
            status.is_evaluated = true;
            status.error = None;
            return Ok(status);
        }
    }

    // 2. Fallback: Cat config file with root privileges if sshd -T had an issue
    let cat_res = PrivCommand::new("pkexec")
        .args(["cat", config_path])
        .output()
        .await;

    if let Ok(out) = cat_res {
        if out.status.success() {
            let content = String::from_utf8_lossy(&out.stdout);
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
            status.is_evaluated = true;
            status.error = None;
            return Ok(status);
        }
    }

    // 3. Fallback: Direct read without root
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
        status.is_evaluated = true;
        status.error = None;
    } else {
        status.error = Some("Root access needed to evaluate /etc/ssh/sshd_config (0600 root:root)".to_string());
    }

    Ok(status)
}

// ─── 6. SSL Certificates & Remote Live Tester ──────────────────────────────────

fn parse_openssl_date(date_str: &str) -> (i64, bool, bool) {
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        return (0, false, false);
    }

    // Parse formats like "Oct 12 18:05:55 2026 GMT"
    let clean = trimmed.replace(" GMT", " +0000").replace(" UTC", " +0000");
    if let Ok(dt) = DateTime::parse_from_str(&clean, "%b %e %H:%M:%S %Y %z")
        .or_else(|_| DateTime::parse_from_str(&clean, "%b %d %H:%M:%S %Y %z"))
        .or_else(|_| DateTime::parse_from_rfc3339(trimmed))
    {
        let now = Utc::now();
        let expiry = dt.with_timezone(&Utc);
        let diff_secs = (expiry - now).num_seconds();
        let days = diff_secs / 86400;
        let is_expired = diff_secs <= 0;
        let is_expiring_soon = !is_expired && days <= 30;
        return (days, is_expired, is_expiring_soon);
    }

    if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%b %e %H:%M:%S %Y")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%b %d %H:%M:%S %Y"))
    {
        let now = Utc::now().naive_utc();
        let diff_secs = (ndt - now).num_seconds();
        let days = diff_secs / 86400;
        let is_expired = diff_secs <= 0;
        let is_expiring_soon = !is_expired && days <= 30;
        return (days, is_expired, is_expiring_soon);
    }

    (0, false, false)
}

fn parse_ssl_cert_file(file_path: &Path) -> Option<SslCertItem> {
    let path_str = file_path.to_str()?;
    let out = Command::new("openssl")
        .args(["x509", "-in", path_str, "-noout", "-subject", "-issuer", "-dates", "-ext", "subjectAltName"])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let txt = String::from_utf8_lossy(&out.stdout);
    let mut subject = String::new();
    let mut issuer = String::new();
    let mut not_after = String::new();
    let mut not_before = String::new();
    let mut san_domains = Vec::new();

    for l in txt.lines() {
        let trimmed = l.trim();
        if trimmed.starts_with("subject=") {
            subject = trimmed.trim_start_matches("subject=").trim().to_string();
        } else if trimmed.starts_with("issuer=") {
            issuer = trimmed.trim_start_matches("issuer=").trim().to_string();
        } else if trimmed.starts_with("notAfter=") {
            not_after = trimmed.trim_start_matches("notAfter=").trim().to_string();
        } else if trimmed.starts_with("notBefore=") {
            not_before = trimmed.trim_start_matches("notBefore=").trim().to_string();
        } else if trimmed.starts_with("DNS:") || trimmed.contains("DNS:") {
            for part in trimmed.split(',') {
                let p = part.trim();
                if let Some(dns) = p.strip_prefix("DNS:") {
                    san_domains.push(dns.trim().to_string());
                }
            }
        }
    }

    if subject.is_empty() && not_after.is_empty() {
        return None;
    }

    let (days_valid, is_expired, is_expiring_soon) = parse_openssl_date(&not_after);
    let name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    Some(SslCertItem {
        name,
        subject,
        issuer,
        not_before,
        not_after,
        days_valid,
        path: path_str.to_string(),
        san_domains,
        is_expired,
        is_expiring_soon,
    })
}

fn collect_ssl_files_recursive(dir: &Path, max_depth: usize, results: &mut Vec<PathBuf>) {
    if max_depth == 0 || !dir.exists() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_ssl_files_recursive(&p, max_depth - 1, results);
            } else if p.is_file() {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if name.ends_with(".crt") || name.ends_with(".pem") || name.ends_with(".cer") {
                    // Ignore CA symlinks/hash files e.g. 002c0b4f.0
                    results.push(p);
                }
            }
        }
    }
}

#[tauri::command]
pub fn vault_list_ssl_certs() -> Result<Vec<SslCertItem>, String> {
    let mut certs = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();
    let mut search_paths = vec![
        PathBuf::from("/etc/letsencrypt/live"),
        PathBuf::from("/etc/nginx/ssl"),
        PathBuf::from("/etc/ssl/certs"),
        PathBuf::from("/etc/pki/tls/certs"),
    ];

    if !home.is_empty() {
        search_paths.push(Path::new(&home).join(".local/share/mkcert"));
    }

    let mut found_files = Vec::new();
    for sp in &search_paths {
        collect_ssl_files_recursive(sp, 3, &mut found_files);
    }

    for file_path in found_files {
        // Skip ca-certificates bundles
        let fname = file_path.to_string_lossy();
        if fname.contains("ca-bundle") || fname.contains("ca-certificates") {
            continue;
        }
        if let Some(cert) = parse_ssl_cert_file(&file_path) {
            certs.push(cert);
        }
    }

    Ok(certs)
}

#[tauri::command]
pub fn vault_test_remote_ssl(host: String, port: u16) -> Result<SslCertItem, String> {
    let clean_host = host.trim().to_string();
    if clean_host.is_empty() {
        return Err("Host cannot be empty".to_string());
    }

    let target = format!("{}:{}", clean_host, port);
    let s_client_out = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &target,
            "-servername",
            &clean_host,
            "-showcerts",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run openssl s_client: {e}"))?;

    let full_txt = String::from_utf8_lossy(&s_client_out.stdout);
    if !full_txt.contains("BEGIN CERTIFICATE") {
        let err_txt = String::from_utf8_lossy(&s_client_out.stderr);
        return Err(format!("Could not retrieve TLS certificate from {target}: {err_txt}"));
    }

    // Pipe certificates to openssl x509
    let mut x509_cmd = Command::new("openssl")
        .args(["x509", "-noout", "-subject", "-issuer", "-dates", "-ext", "subjectAltName"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn openssl x509: {e}"))?;

    if let Some(mut stdin) = x509_cmd.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(full_txt.as_bytes());
    }

    let x509_out = x509_cmd.wait_with_output().map_err(|e| e.to_string())?;
    let parsed_txt = String::from_utf8_lossy(&x509_out.stdout);

    let mut subject = String::new();
    let mut issuer = String::new();
    let mut not_after = String::new();
    let mut not_before = String::new();
    let mut san_domains = Vec::new();

    for l in parsed_txt.lines() {
        let trimmed = l.trim();
        if trimmed.starts_with("subject=") {
            subject = trimmed.trim_start_matches("subject=").trim().to_string();
        } else if trimmed.starts_with("issuer=") {
            issuer = trimmed.trim_start_matches("issuer=").trim().to_string();
        } else if trimmed.starts_with("notAfter=") {
            not_after = trimmed.trim_start_matches("notAfter=").trim().to_string();
        } else if trimmed.starts_with("notBefore=") {
            not_before = trimmed.trim_start_matches("notBefore=").trim().to_string();
        } else if trimmed.starts_with("DNS:") || trimmed.contains("DNS:") {
            for part in trimmed.split(',') {
                let p = part.trim();
                if let Some(dns) = p.strip_prefix("DNS:") {
                    san_domains.push(dns.trim().to_string());
                }
            }
        }
    }

    let (days_valid, is_expired, is_expiring_soon) = parse_openssl_date(&not_after);

    Ok(SslCertItem {
        name: format!("{}:{}", clean_host, port),
        subject,
        issuer,
        not_before,
        not_after,
        days_valid,
        path: format!("tls://{}:{}", clean_host, port),
        san_domains,
        is_expired,
        is_expiring_soon,
    })
}

// ─── 7. Fail2ban Threat Defense ────────────────────────────────────────────────

#[tauri::command]
pub async fn vault_get_fail2ban_status() -> Result<Fail2banStatus, String> {
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

    let status_out = PrivCommand::new("pkexec")
        .args(["fail2ban-client", "status"])
        .output()
        .await;

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
                let j_out = PrivCommand::new("pkexec")
                    .args(["fail2ban-client", "status", &j])
                    .output()
                    .await;
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
pub async fn vault_unban_ip(jail: String, ip: String) -> Result<String, String> {
    let output = PrivCommand::new("pkexec")
        .args(["fail2ban-client", "set", &jail, "unbanip", &ip])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("IP {} successfully unbanned from jail {}", ip, jail))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub async fn vault_ban_ip(jail: String, ip: String) -> Result<String, String> {
    let output = PrivCommand::new("pkexec")
        .args(["fail2ban-client", "set", &jail, "banip", &ip])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("IP {} banned in jail {}", ip, jail))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub async fn vault_manage_fail2ban_service(action: String) -> Result<(), String> {
    let valid_actions = ["start", "restart", "stop", "enable", "disable"];
    if !valid_actions.contains(&action.as_str()) {
        return Err(format!("Invalid systemd action: {action}"));
    }

    let out = PrivCommand::new("pkexec")
        .args(["systemctl", &action, "fail2ban"])
        .output()
        .await
        .map_err(|e| format!("Failed to run systemctl {action} fail2ban: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("Failed to {action} fail2ban service: {err}"));
    }

    Ok(())
}
