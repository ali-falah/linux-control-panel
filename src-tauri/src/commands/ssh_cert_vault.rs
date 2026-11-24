use crate::utils::privilege::{
    ensure_arguments_for_program, resolve_root_program, run_privileged_output, RootAuth,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyItem {
    pub name: String,
    pub key_type: String,
    pub path: String,
    pub pub_key_path: Option<String>,
    pub fingerprint: String,
    pub public_key: String,
    pub comment: String,
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
    pub marker: String,
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
    /// `daemon` for `sshd -T`, `config` for a parsed file, `none` otherwise.
    pub source: String,
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
    pub is_self_signed: bool,
    pub date_parse_error: bool,
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
    pub needs_elevation: bool,
    pub total_banned_ips: usize,
    pub jails: Vec<Fail2banJailInfo>,
    pub error: Option<String>,
}

// ─── Shared path and file helpers ─────────────────────────────────────────────

const MAX_KEY_NAME: usize = 64;
const MAX_AUTHORIZED_KEY_BYTES: usize = 16 * 1024;
const RESERVED_SSH_NAMES: &[&str] = &[
    "config",
    "known_hosts",
    "authorized_keys",
    "authorized_keys2",
    "environment",
    "rc",
];

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| "The current user's home directory could not be determined.".to_string())
}

fn ssh_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".ssh"))
}

fn ensure_ssh_dir() -> Result<PathBuf, String> {
    let dir = ssh_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    // OpenSSH refuses overly permissive directories; set the documented mode.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Could not secure {}: {e}", dir.display()))?;
    Ok(dir)
}

fn validate_key_name(raw: &str) -> Result<String, String> {
    let name = raw.trim().strip_suffix(".pub").unwrap_or(raw.trim()).trim();
    if name.is_empty() {
        return Err("A key name is required.".to_string());
    }
    if name.len() > MAX_KEY_NAME {
        return Err(format!(
            "A key name must be at most {MAX_KEY_NAME} characters."
        ));
    }
    if name == "." || name == ".." || name.starts_with('.') {
        return Err("A key name must not start with a dot or be a path component.".to_string());
    }
    if name.starts_with('-') {
        return Err("A key name must not start with a dash.".to_string());
    }
    if RESERVED_SSH_NAMES
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
    {
        return Err(format!(
            "'{name}' is an SSH configuration file, not a key pair."
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | '+'))
    {
        return Err(format!(
            "The key name '{name}' contains characters that are not allowed in a filename."
        ));
    }
    Ok(name.to_string())
}

fn key_paths(name: &str) -> Result<(PathBuf, PathBuf), String> {
    let dir = ssh_dir()?;
    let name = validate_key_name(name)?;
    Ok((dir.join(&name), dir.join(format!("{name}.pub"))))
}

fn atomic_write(path: &Path, content: &str, mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{} is a symlink; refusing to replace it. Resolve the symlink first.",
                path.display()
            ));
        }
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let temp = parent.join(format!(".{name}.lcp-{}-{nonce}.tmp", std::process::id()));

    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .open(&temp)
            .map_err(|e| format!("Could not create a temporary file in {}: {e}", parent.display()))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Could not write {}: {e}", temp.display()))?;
        file.sync_all()
            .map_err(|e| format!("Could not sync {}: {e}", temp.display()))?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("Could not set permissions on {}: {e}", temp.display()))?;
        fs::rename(&temp, path)
            .map_err(|e| format!("Could not replace {}: {e}", path.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn single_line(value: &str, what: &str, max: usize) -> Result<String, String> {
    let value = value.trim();
    if value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err(format!("The {what} must be a single line."));
    }
    if value.len() > max {
        return Err(format!("The {what} must be at most {max} characters."));
    }
    Ok(value.to_string())
}

// ─── 1. SSH Keys ───────────────────────────────────────────────────────────────

fn fingerprint_for_key(path: &Path) -> String {
    let output = Command::new("ssh-keygen")
        .args(["-lf", &path.to_string_lossy()])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let fields: Vec<&str> = text.split_whitespace().collect();
            if fields.len() >= 2 {
                format!("{} {}", fields[0], fields[1])
            } else {
                "Fingerprint unavailable".to_string()
            }
        }
        _ => "Fingerprint unavailable".to_string(),
    }
}

/// Read a user-owned SSH file with a size cap. These files can be edited by
/// other local processes, so an unbounded read would let one of them stall or
/// exhaust the app.
fn read_ssh_file(path: &Path, limit: u64) -> Result<String, String> {
    let meta = fs::metadata(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if meta.len() > limit {
        return Err(format!(
            "{} is larger than the supported limit ({} bytes).",
            path.display(),
            limit
        ));
    }
    fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))
}

fn list_ssh_keys_blocking() -> Result<Vec<SshKeyItem>, String> {
    let mut items = Vec::new();
    let ssh_dir = ssh_dir()?;
    if !ssh_dir.exists() {
        return Ok(items);
    }

    let entries = fs::read_dir(&ssh_dir)
        .map_err(|e| format!("Could not read {}: {e}", ssh_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let Some(private_name) = name.strip_suffix(".pub") else {
            continue;
        };
        if private_name.is_empty() {
            continue;
        }
        let private_path = ssh_dir.join(private_name);
        let public_key = read_ssh_file(&path, 1024 * 1024)
            .unwrap_or_default()
            .trim()
            .to_string();
        let parts: Vec<&str> = public_key.split_whitespace().collect();
        let key_type = parts.first().copied().unwrap_or("Unknown").to_string();
        let comment = if parts.len() > 2 {
            parts[2..].join(" ")
        } else {
            String::new()
        };
        let fingerprint = if public_key.is_empty() {
            "Empty public key".to_string()
        } else {
            fingerprint_for_key(&path)
        };

        items.push(SshKeyItem {
            name: private_name.to_string(),
            key_type,
            path: private_path.to_string_lossy().to_string(),
            pub_key_path: Some(path.to_string_lossy().to_string()),
            fingerprint,
            public_key,
            comment,
            has_private: private_path.is_file(),
        });
    }
    items.sort_by_key(|item| item.name.to_lowercase());
    Ok(items)
}

#[tauri::command]
pub async fn vault_list_ssh_keys() -> Result<Vec<SshKeyItem>, String> {
    tokio::task::spawn_blocking(list_ssh_keys_blocking)
        .await
        .map_err(|e| format!("SSH key scan failed: {e}"))?
}

fn write_pty(fd: libc::c_int, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let written = unsafe {
            libc::write(
                fd,
                bytes[offset..].as_ptr().cast(),
                bytes.len() - offset,
            )
        };
        if written <= 0 {
            break;
        }
        offset += written as usize;
    }
}

/// Run `ssh-keygen` with a passphrase without putting the passphrase in argv.
/// A pseudo-terminal is used only for the two interactive prompts; the
/// passphrase never leaves this process's memory.
fn run_keygen_with_passphrase(args: &[String], passphrase: &str) -> Result<(), String> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if opened != 0 {
        return Err(format!(
            "Could not create a pseudo-terminal for ssh-keygen: {}",
            std::io::Error::last_os_error()
        ));
    }

    let slave_file = unsafe { fs::File::from_raw_fd(slave) };
    let stdin_file = slave_file
        .try_clone()
        .map_err(|e| format!("Could not duplicate the terminal descriptor: {e}"))?;
    let stderr_file = slave_file
        .try_clone()
        .map_err(|e| format!("Could not duplicate the terminal descriptor: {e}"))?;

    let mut child = Command::new("ssh-keygen")
        .args(args)
        .stdin(Stdio::from(stdin_file))
        .stderr(Stdio::from(stderr_file))
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to execute ssh-keygen: {e}"))?;
    // The parent must not keep the slave open, or reads never see EOF.
    drop(slave_file);

    let master_file = unsafe { fs::File::from_raw_fd(master) };
    let master_fd = master_file.as_raw_fd();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut responses = 0u8;
    let status = loop {
        if let Some(current) = child
            .try_wait()
            .map_err(|e| format!("Could not wait for ssh-keygen: {e}"))?
        {
            break current;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("ssh-keygen timed out while waiting for the passphrase prompt.".to_string());
        }

        let mut poll_fd = libc::pollfd {
            fd: master_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll_fd, 1, 100) };
        if ready <= 0 || (poll_fd.revents & libc::POLLIN) == 0 {
            continue;
        }

        let mut buffer = [0u8; 1024];
        let read = unsafe { libc::read(master_fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read <= 0 {
            continue;
        }
        let chunk = &buffer[..read as usize];
        let text = String::from_utf8_lossy(chunk).to_ascii_lowercase();
        if text.contains("passphrase") && responses < 2 {
            write_pty(master_fd, passphrase.as_bytes());
            write_pty(master_fd, b"\n");
            responses += 1;
        }
    };

    drop(master_file);

    if status.success() {
        Ok(())
    } else {
        Err(
            "ssh-keygen failed while creating the encrypted key. Check the key name and permissions."
                .to_string(),
        )
    }
}

fn generate_ssh_key_blocking(
    key_type: String,
    bits: Option<u32>,
    filename: String,
    comment: Option<String>,
    passphrase: Option<String>,
) -> Result<String, String> {
    let key_type = key_type.trim().to_lowercase();
    if !matches!(key_type.as_str(), "ed25519" | "rsa" | "ecdsa") {
        return Err("The key type must be ed25519, rsa, or ecdsa.".to_string());
    }
    let name = validate_key_name(&filename)?;
    let comment = comment
        .map(|value| single_line(&value, "comment", 256))
        .transpose()?
        .unwrap_or_default();
    let passphrase = passphrase.unwrap_or_default();
    if passphrase.len() > 1024 {
        return Err("The passphrase must be at most 1024 characters.".to_string());
    }
    if passphrase.contains('\0') || passphrase.contains('\n') || passphrase.contains('\r') {
        return Err("The passphrase must not contain control characters.".to_string());
    }

    let dir = ensure_ssh_dir()?;
    let target = dir.join(&name);
    let pub_path = dir.join(format!("{name}.pub"));
    if fs::symlink_metadata(&target).is_ok() || fs::symlink_metadata(&pub_path).is_ok() {
        return Err(format!(
            "A key or public key already exists at {}.",
            target.display()
        ));
    }

    let mut args = vec![
        "-q".to_string(),
        "-t".to_string(),
        key_type.clone(),
        "-f".to_string(),
        target.to_string_lossy().to_string(),
    ];
    if !comment.is_empty() {
        args.push("-C".to_string());
        args.push(comment);
    }
    if key_type == "rsa" {
        let bits = bits.unwrap_or(4096);
        if !matches!(bits, 2048 | 3072 | 4096) {
            return Err("RSA keys must use 2048, 3072, or 4096 bits.".to_string());
        }
        args.push("-b".to_string());
        args.push(bits.to_string());
    } else if key_type == "ecdsa" {
        let bits = bits.unwrap_or(256);
        if !matches!(bits, 256 | 384 | 521) {
            return Err("ECDSA keys must use 256, 384, or 521 bits.".to_string());
        }
        args.push("-b".to_string());
        args.push(bits.to_string());
    }

    let result = if passphrase.is_empty() {
        args.push("-N".to_string());
        args.push(String::new());
        Command::new("ssh-keygen")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to execute ssh-keygen: {e}"))
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    Err(if stderr.is_empty() {
                        "ssh-keygen failed.".to_string()
                    } else {
                        format!("ssh-keygen failed: {stderr}")
                    })
                }
            })
    } else {
        run_keygen_with_passphrase(&args, &passphrase)
    };

    if let Err(error) = result {
        let _ = fs::remove_file(&target);
        let _ = fs::remove_file(&pub_path);
        return Err(error);
    }

    let public_key = fs::read_to_string(&pub_path)
        .map_err(|e| format!("ssh-keygen created no public key at {}: {e}", pub_path.display()))?
        .trim()
        .to_string();
    if public_key.is_empty() {
        return Err("ssh-keygen created an empty public key.".to_string());
    }
    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o600));
    let _ = fs::set_permissions(&pub_path, fs::Permissions::from_mode(0o644));
    Ok(public_key)
}

#[tauri::command]
pub async fn vault_generate_ssh_key(
    key_type: String,
    bits: Option<u32>,
    filename: String,
    comment: Option<String>,
    passphrase: Option<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        generate_ssh_key_blocking(key_type, bits, filename, comment, passphrase)
    })
    .await
    .map_err(|e| format!("SSH key generation failed: {e}"))?
}

#[tauri::command]
pub fn vault_delete_ssh_key(name: String) -> Result<String, String> {
    let (private_path, public_path) = key_paths(&name)?;
    // Check every path before deleting anything, so a symlink or directory
    // cannot leave the pair half-removed.
    let mut existing = Vec::new();
    for path in [&private_path, &public_path] {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_file() => existing.push(path.clone()),
            Ok(_) => {
                return Err(format!(
                    "{} is not a regular file; refusing to delete it.",
                    path.display()
                ));
            }
            Err(_) => {}
        }
    }
    for path in &existing {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
    }
    let deleted = !existing.is_empty();
    if deleted {
        Ok(format!(
            "Deleted SSH key pair '{}'.",
            private_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ))
    } else {
        Err(format!("No key files found for '{}'.", name.trim()))
    }
}

// ─── 2. SSH Client Config (~/.ssh/config) ─────────────────────────────────────

fn host_directive(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    if lower == "host" {
        return Some(String::new());
    }
    if lower.starts_with("host ") || lower.starts_with("host\t") {
        return Some(trimmed[4..].trim().to_string());
    }
    None
}

fn is_block_boundary(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    lower.starts_with("host ") || lower.starts_with("host\t") || lower.starts_with("match ")
}

fn find_host_block(lines: &[String], alias: &str) -> Option<(usize, usize)> {
    let alias = alias.split_whitespace().collect::<Vec<_>>().join(" ");
    for (index, line) in lines.iter().enumerate() {
        let Some(patterns) = host_directive(line) else {
            continue;
        };
        let normalized = patterns.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized != alias {
            continue;
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, line)| is_block_boundary(line))
            .map(|(index, _)| index)
            .unwrap_or(lines.len());
        return Some((index, end));
    }
    None
}

fn render_host_block(host: &SshClientHost) -> Result<Vec<String>, String> {
    let alias = single_line(&host.host, "host alias", 256)?;
    if alias.is_empty() {
        return Err("A host alias is required.".to_string());
    }
    if alias.starts_with('-') || alias.starts_with('#') {
        return Err("The host alias must not start with '-' or '#'.".to_string());
    }
    if alias == "*" {
        return Err("The global 'Host *' block is managed by OpenSSH, not as a host profile.".to_string());
    }
    let hostname = single_line(&host.hostname, "target hostname", 255)?;
    if hostname.is_empty() {
        return Err("A target hostname or IP is required.".to_string());
    }
    if hostname
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '\\')
    {
        return Err("The target hostname must not contain whitespace or path separators.".to_string());
    }
    let user = single_line(&host.user, "remote user", 64)?;
    if !user.is_empty()
        && !user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err("The remote user contains unsupported characters.".to_string());
    }
    let port = single_line(&host.port, "port", 5)?;
    let port = if port.is_empty() {
        "22".to_string()
    } else {
        let parsed = port
            .parse::<u16>()
            .map_err(|_| "The SSH port must be a number between 1 and 65535.".to_string())?;
        if parsed == 0 {
            return Err("The SSH port must be between 1 and 65535.".to_string());
        }
        parsed.to_string()
    };
    let identity = single_line(&host.identity_file, "identity file", 512)?;
    if identity.chars().any(char::is_whitespace) {
        return Err("The identity file path must not contain whitespace.".to_string());
    }
    let proxy = single_line(&host.proxy_jump, "ProxyJump", 512)?;
    if !proxy.is_empty()
        && !proxy
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | ':' | ',' | '[' | ']' | '-' | '%'))
    {
        return Err("The ProxyJump value contains unsupported characters.".to_string());
    }

    let mut lines = vec![format!("Host {alias}"), format!("    HostName {hostname}")];
    if !user.is_empty() {
        lines.push(format!("    User {user}"));
    }
    lines.push(format!("    Port {port}"));
    if !identity.is_empty() {
        lines.push(format!("    IdentityFile {identity}"));
    }
    if !proxy.is_empty() {
        lines.push(format!("    ProxyJump {proxy}"));
    }
    let extra = host.extra_config.trim();
    if !extra.is_empty() {
        if extra.len() > 16 * 1024 {
            return Err("The extra SSH options are too long.".to_string());
        }
        for (index, line) in extra.lines().enumerate() {
            if index >= 100 {
                return Err("At most 100 extra SSH option lines are allowed.".to_string());
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.chars().any(|c| c == '\r' || c == '\0') {
                return Err("Extra SSH options must not contain control characters.".to_string());
            }
            let first = line.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            if first == "host" || first == "match" {
                return Err("Extra options must not start a new Host or Match block.".to_string());
            }
            lines.push(format!("    {line}"));
        }
    }
    Ok(lines)
}

fn upsert_host_block(lines: &[String], host: &SshClientHost) -> Result<Vec<String>, String> {
    let alias = host.host.split_whitespace().collect::<Vec<_>>().join(" ");
    let block = render_host_block(host)?;
    if let Some((start, end)) = find_host_block(lines, &alias) {
        let mut prefix: Vec<String> = lines[..start].to_vec();
        while prefix.last().is_some_and(|line| line.trim().is_empty()) {
            prefix.pop();
        }
        if !prefix.is_empty() {
            prefix.push(String::new());
        }
        let mut suffix: Vec<String> = lines[end..].to_vec();
        while suffix.first().is_some_and(|line| line.trim().is_empty()) {
            suffix.remove(0);
        }
        prefix.extend(block);
        if !suffix.is_empty() {
            prefix.push(String::new());
            prefix.extend(suffix);
        }
        Ok(prefix)
    } else {
        let mut output: Vec<String> = lines.to_vec();
        while output.last().is_some_and(|line| line.trim().is_empty()) {
            output.pop();
        }
        if !output.is_empty() {
            output.push(String::new());
        }
        output.extend(block);
        Ok(output)
    }
}

fn delete_host_block(lines: &[String], alias: &str) -> Result<Vec<String>, String> {
    let alias = alias.split_whitespace().collect::<Vec<_>>().join(" ");
    if alias == "*" {
        return Err("The global 'Host *' block is not a removable host profile.".to_string());
    }
    let Some((start, end)) = find_host_block(lines, &alias) else {
        return Err(format!("No SSH host block named '{alias}' was found."));
    };
    let mut output: Vec<String> = lines[..start].to_vec();
    let mut suffix: Vec<String> = lines[end..].to_vec();
    while suffix.first().is_some_and(|line| line.trim().is_empty()) {
        suffix.remove(0);
    }
    while output.last().is_some_and(|line| line.trim().is_empty()) {
        output.pop();
    }
    if !output.is_empty() && !suffix.is_empty() {
        output.push(String::new());
    }
    output.extend(suffix);
    Ok(output)
}

fn parse_ssh_config(content: &str) -> Vec<SshClientHost> {
    let mut hosts = Vec::new();
    let mut current: Option<SshClientHost> = None;
    let mut extra: Vec<String> = Vec::new();
    let mut in_host = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("match ") {
            if let Some(mut host) = current.take() {
                host.extra_config = extra.join("\n");
                hosts.push(host);
            }
            extra.clear();
            in_host = false;
            continue;
        }
        if let Some(patterns) = host_directive(line) {
            if let Some(mut host) = current.take() {
                host.extra_config = extra.join("\n");
                hosts.push(host);
            }
            extra.clear();
            if patterns.trim() == "*" {
                // Global defaults are not a server profile; leave them in the
                // file and keep them out of the editable list.
                current = None;
                in_host = true;
                continue;
            }
            current = Some(SshClientHost {
                host: patterns,
                hostname: String::new(),
                user: String::new(),
                port: String::new(),
                identity_file: String::new(),
                proxy_jump: String::new(),
                extra_config: String::new(),
            });
            in_host = true;
            continue;
        }
        if !in_host {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            extra.push(trimmed.to_string());
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let key = parts[0].to_ascii_lowercase();
        let value = parts[1..].join(" ");
        if let Some(host) = current.as_mut() {
            match key.as_str() {
                "hostname" => host.hostname = value,
                "user" => host.user = value,
                "port" => host.port = value,
                "identityfile" => host.identity_file = value,
                "proxyjump" => host.proxy_jump = value,
                _ => extra.push(trimmed.to_string()),
            }
        }
    }
    if let Some(mut host) = current {
        host.extra_config = extra.join("\n");
        hosts.push(host);
    }
    hosts
}

#[tauri::command]
pub fn vault_list_ssh_client_config() -> Result<Vec<SshClientHost>, String> {
    let config = ssh_dir()?.join("config");
    if !config.exists() {
        return Ok(Vec::new());
    }
    let content = read_ssh_file(&config, 2 * 1024 * 1024)?;
    Ok(parse_ssh_config(&content))
}

#[tauri::command]
pub fn vault_upsert_ssh_client_host(host: SshClientHost) -> Result<String, String> {
    let dir = ensure_ssh_dir()?;
    let config = dir.join("config");
    let content = if config.exists() {
        read_ssh_file(&config, 2 * 1024 * 1024)?
    } else {
        String::new()
    };
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let updated = upsert_host_block(&lines, &host)?;
    let mut output = updated.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    atomic_write(&config, &output, 0o600)?;
    Ok(format!("Host profile '{}' saved to ~/.ssh/config.", host.host.trim()))
}

#[tauri::command]
pub fn vault_delete_ssh_client_host(host_name: String) -> Result<String, String> {
    let config = ssh_dir()?.join("config");
    if !config.exists() {
        return Err("No ~/.ssh/config file exists.".to_string());
    }
    let content = read_ssh_file(&config, 2 * 1024 * 1024)?;
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let updated = delete_host_block(&lines, &host_name)?;
    let mut output = updated.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    atomic_write(&config, &output, 0o600)?;
    Ok(format!("Host profile '{}' removed from ~/.ssh/config.", host_name.trim()))
}

// ─── 3. Known Hosts (~/.ssh/known_hosts) ───────────────────────────────────────

fn parse_known_host_line(line: &str, index: usize) -> Option<KnownHostItem> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let (marker, offset) = if parts.first().is_some_and(|part| part.starts_with('@')) {
        (
            parts.first().copied().unwrap_or("").to_string(),
            1usize,
        )
    } else {
        (String::new(), 0usize)
    };
    if parts.len() < offset + 3 {
        return None;
    }
    let host_raw = parts[offset];
    let key_type = parts[offset + 1].to_string();
    let key_data = parts[offset + 2];

    let display_host = if host_raw.starts_with("|1|") {
        format!("[Hashed host, line {}]", index + 1)
    } else {
        host_raw.to_string()
    };

    let temp_key = format!("{key_type} {key_data}\n");
    let fingerprint = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(temp_key.as_bytes());
            }
            child.wait_with_output().ok()
        })
        .map(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            let fields: Vec<&str> = text.split_whitespace().collect();
            if fields.len() >= 2 {
                format!("{} {}", fields[0], fields[1])
            } else {
                "Available".to_string()
            }
        })
        .unwrap_or_else(|| "Available".to_string());

    Some(KnownHostItem {
        line_number: index + 1,
        host: display_host,
        key_type,
        fingerprint,
        marker,
        raw: trimmed.to_string(),
    })
}

fn list_known_hosts_blocking() -> Result<Vec<KnownHostItem>, String> {
    let file = ssh_dir()?.join("known_hosts");
    if !file.exists() {
        return Ok(Vec::new());
    }
    let content = read_ssh_file(&file, 4 * 1024 * 1024)?;
    Ok(content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_known_host_line(line, index))
        .collect())
}

#[tauri::command]
pub async fn vault_list_known_hosts() -> Result<Vec<KnownHostItem>, String> {
    tokio::task::spawn_blocking(list_known_hosts_blocking)
        .await
        .map_err(|e| format!("Known hosts scan failed: {e}"))?
}

fn update_ssh_file_line(
    path: &Path,
    line_number: usize,
    raw: Option<&str>,
    not_found: &str,
) -> Result<(), String> {
    let content = read_ssh_file(path, 4 * 1024 * 1024)?;
    let mut lines: Vec<&str> = content.lines().collect();
    if line_number == 0 || line_number > lines.len() {
        return Err("The selected line no longer exists. Refresh the list.".to_string());
    }
    if let Some(expected) = raw {
        if lines[line_number - 1].trim() != expected.trim() {
            return Err("The file changed since it was loaded. Refresh the list and try again.".to_string());
        }
    }
    lines.remove(line_number - 1);
    let mut output = lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    atomic_write(path, &output, 0o600).map_err(|error| format!("{not_found}: {error}"))
}

#[tauri::command]
pub fn vault_remove_known_host(line_number: usize, raw: Option<String>) -> Result<String, String> {
    let file = ssh_dir()?.join("known_hosts");
    if !file.exists() {
        return Err("The known_hosts file does not exist.".to_string());
    }
    update_ssh_file_line(
        &file,
        line_number,
        raw.as_deref(),
        "Could not remove the known host entry.",
    )?;
    Ok("Known host entry removed.".to_string())
}

#[tauri::command]
pub fn vault_clear_known_hosts() -> Result<String, String> {
    let file = ssh_dir()?.join("known_hosts");
    if file.exists() {
        atomic_write(&file, "", 0o600)?;
    }
    Ok("All known host records were cleared.".to_string())
}

// ─── 4. Authorized Keys ────────────────────────────────────────────────────────

fn key_algorithm(token: &str) -> bool {
    token.starts_with("ssh-")
        || token.starts_with("ecdsa-")
        || token.starts_with("sk-ssh-")
        || token.starts_with("sk-ecdsa-")
        || token.starts_with("rsa-sha2-")
}

/// Split an authorized_keys line into type, data, comment, and options.
/// Options are everything before the key algorithm; this keeps `command="..."`
/// prefixes visible even though the UI does not edit them.
fn parse_authorized_key(line: &str) -> Option<(String, String, String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let mut key_index = None;
    for (index, part) in parts.iter().enumerate() {
        if key_algorithm(part) && index + 1 < parts.len() {
            let data = parts[index + 1];
            if !data.is_empty()
                && data
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
            {
                key_index = Some(index);
                break;
            }
        }
    }
    let index = key_index?;
    let options = if index > 0 {
        parts[..index].join(" ")
    } else {
        String::new()
    };
    let key_type = parts[index].to_string();
    let key_data = parts[index + 1].to_string();
    let comment = if parts.len() > index + 2 {
        parts[index + 2..].join(" ")
    } else {
        String::new()
    };
    Some((key_type, key_data, comment, options))
}

fn validate_authorized_key(raw: &str) -> Result<String, String> {
    if raw.len() > MAX_AUTHORIZED_KEY_BYTES {
        return Err("The public key is too long.".to_string());
    }
    if raw.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err("Paste exactly one public key on a single line.".to_string());
    }
    let normalized = raw.trim().to_string();
    let Some((_, data, _, _)) = parse_authorized_key(&normalized) else {
        return Err(
            "That does not look like an OpenSSH public key (for example ssh-ed25519 AAAA... comment)."
                .to_string(),
        );
    };
    let padded = match data.len() % 4 {
        0 => data.clone(),
        2 => format!("{data}=="),
        3 => format!("{data}="),
        _ => return Err("The public key data is not valid base64.".to_string()),
    };
    if BASE64.decode(padded).is_err() {
        return Err("The public key data is not valid base64.".to_string());
    }
    Ok(normalized)
}

fn list_authorized_keys_blocking() -> Result<Vec<AuthorizedKeyItem>, String> {
    let file = ssh_dir()?.join("authorized_keys");
    if !file.exists() {
        return Ok(Vec::new());
    }
    let content = read_ssh_file(&file, 4 * 1024 * 1024)?;
    Ok(content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_authorized_key(line).map(|(key_type, key_data, comment, options)| {
                AuthorizedKeyItem {
                    line_number: index + 1,
                    key_type,
                    key_data,
                    comment,
                    options,
                    raw: line.trim().to_string(),
                }
            })
        })
        .collect())
}

#[tauri::command]
pub fn vault_list_authorized_keys() -> Result<Vec<AuthorizedKeyItem>, String> {
    list_authorized_keys_blocking()
}

#[tauri::command]
pub fn vault_add_authorized_key(pub_key: String) -> Result<String, String> {
    let key = validate_authorized_key(&pub_key)?;
    let dir = ensure_ssh_dir()?;
    let file = dir.join("authorized_keys");
    let mut existing = if file.exists() {
        read_ssh_file(&file, 4 * 1024 * 1024)?
    } else {
        String::new()
    };
    if existing
        .lines()
        .any(|line| line.trim() == key.trim())
    {
        return Err("That public key is already authorized.".to_string());
    }
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(&key);
    existing.push('\n');
    atomic_write(&file, &existing, 0o600)?;
    Ok("Public key added to ~/.ssh/authorized_keys.".to_string())
}

#[tauri::command]
pub fn vault_remove_authorized_key(line_number: usize, raw: Option<String>) -> Result<String, String> {
    let file = ssh_dir()?.join("authorized_keys");
    if !file.exists() {
        return Err("The authorized_keys file does not exist.".to_string());
    }
    update_ssh_file_line(
        &file,
        line_number,
        raw.as_deref(),
        "Could not remove the authorized key.",
    )?;
    Ok("Authorized key removed.".to_string())
}

// ─── 5. SSHD Hardening Audit ───────────────────────────────────────────────────

fn apply_sshd_setting(
    status: &mut SshdHardeningStatus,
    key: &str,
    value: &str,
    first_only: bool,
) {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return;
    }
    let target = match key {
        "permitrootlogin" => &mut status.permit_root_login,
        "passwordauthentication" => &mut status.password_authentication,
        "pubkeyauthentication" => &mut status.pubkey_authentication,
        "x11forwarding" => &mut status.x11_forwarding,
        "port" => &mut status.port,
        _ => return,
    };
    if first_only && *target != "unset" {
        return;
    }
    *target = value;
}

fn parse_sshd_effective(status: &mut SshdHardeningStatus, content: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        apply_sshd_setting(status, &key.to_ascii_lowercase(), value, true);
    }
}

fn parse_sshd_config(content: &str, status: &mut SshdHardeningStatus) {
    let mut in_match = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("match ") || lower == "match" {
            // Values after Match are conditional; using them as global values
            // would report a setting that does not apply to the whole daemon.
            in_match = true;
            continue;
        }
        if in_match {
            continue;
        }
        if lower.starts_with("include ") {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        // sshd uses the first obtained value for most keywords.
        apply_sshd_setting(status, &key.to_ascii_lowercase(), value, true);
    }
}

fn expand_include(pattern: &str) -> Vec<PathBuf> {
    let path = Path::new(pattern);
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    if !name.contains('*') && !name.contains('?') {
        return vec![path.to_path_buf()];
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("/etc/ssh"));
    let suffix = name.rsplit('*').next().unwrap_or("");
    let mut paths = Vec::new();
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let candidate = entry.path();
            let candidate_name = candidate
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if candidate_name.ends_with(suffix) && candidate.is_file() {
                paths.push(candidate);
            }
        }
    }
    paths.sort();
    paths
}

async fn read_sshd_file(path: &Path, elevated: bool) -> Result<String, String> {
    let text = path.to_string_lossy().to_string();
    if elevated {
        let args = [text.clone()];
        ensure_arguments_for_program("/usr/bin/cat", &args)?;
        let program = resolve_root_program("cat")
            .ok_or_else(|| "cat is not available for a validated privileged read.".to_string())?;
        let output = run_privileged_output(&program, &args, &RootAuth::Polkit).await?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))
}

async fn expand_sshd_lines(
    lines: Vec<String>,
    elevated: bool,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
) -> Vec<String> {
    let mut output = Vec::new();
    for line in lines {
        let lower = line.trim().to_ascii_lowercase();
        let include = lower
            .strip_prefix("include ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(pattern) = include else {
            output.push(line);
            continue;
        };
        if depth == 0 {
            continue;
        }
        for path in expand_include(&pattern) {
            if !visited.insert(path.clone()) {
                continue;
            }
            if let Ok(fragment) = read_sshd_file(&path, elevated).await {
                let fragment_lines: Vec<String> =
                    fragment.lines().map(str::to_string).collect();
                let expanded = Box::pin(expand_sshd_lines(
                    fragment_lines,
                    elevated,
                    depth - 1,
                    visited,
                ))
                .await;
                output.extend(expanded);
            }
        }
    }
    output
}

async fn read_sshd_tree(elevated: bool) -> Result<String, String> {
    let main = Path::new("/etc/ssh/sshd_config");
    let content = read_sshd_file(main, elevated).await?;
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let mut visited = HashSet::new();
    visited.insert(main.to_path_buf());
    let expanded = expand_sshd_lines(lines, elevated, 4, &mut visited).await;
    Ok(expanded.join("\n"))
}

#[tauri::command]
pub async fn vault_get_sshd_hardening(elevated: bool) -> Result<SshdHardeningStatus, String> {
    let mut status = SshdHardeningStatus {
        permit_root_login: "unset".to_string(),
        password_authentication: "unset".to_string(),
        pubkey_authentication: "unset".to_string(),
        x11_forwarding: "unset".to_string(),
        port: "unset".to_string(),
        config_path: "/etc/ssh/sshd_config".to_string(),
        is_evaluated: false,
        source: "none".to_string(),
        error: None,
    };

    let sshd_bin = ["/usr/sbin/sshd", "/usr/bin/sshd"]
        .iter()
        .find(|path| Path::new(path).exists())
        .copied()
        .unwrap_or("sshd");

    let sshd_output = if elevated {
        let args = ["-T".to_string()];
        match ensure_arguments_for_program(sshd_bin, &args) {
            Ok(()) => match resolve_root_program("sshd") {
                Some(program) => {
                    run_privileged_output(&program, &args, &RootAuth::Polkit).await
                }
                None => Err("sshd is not available for a validated privileged read.".to_string()),
            },
            Err(error) => Err(error),
        }
    } else {
        tokio::process::Command::new(sshd_bin)
            .arg("-T")
            .output()
            .await
            .map_err(|e| e.to_string())
    };

    if let Ok(output) = sshd_output {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.trim().is_empty() {
                parse_sshd_effective(&mut status, &text);
                status.is_evaluated = true;
                status.source = "daemon".to_string();
                return Ok(status);
            }
        }
    }

    match read_sshd_tree(elevated).await {
        Ok(content) => {
            parse_sshd_config(&content, &mut status);
            status.is_evaluated = true;
            status.source = "config".to_string();
        }
        Err(error) => {
            status.error = Some(format!(
                "Root access is needed to evaluate /etc/ssh/sshd_config ({error})."
            ));
        }
    }
    Ok(status)
}

// ─── 6. SSL Certificates & Remote Live Tester ──────────────────────────────────

fn parse_openssl_date(date_str: &str) -> (i64, bool, bool, bool) {
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        return (0, false, false, false);
    }
    let clean = trimmed.replace(" GMT", " +0000").replace(" UTC", " +0000");
    let parsed = DateTime::parse_from_str(&clean, "%b %e %H:%M:%S %Y %z")
        .or_else(|_| DateTime::parse_from_str(&clean, "%b %d %H:%M:%S %Y %z"))
        .or_else(|_| DateTime::parse_from_rfc3339(trimmed))
        .map(|date| date.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(trimmed, "%b %e %H:%M:%S %Y")
                .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%b %d %H:%M:%S %Y"))
                .map(|date| DateTime::<Utc>::from_naive_utc_and_offset(date, Utc))
        });
    let Ok(expiry) = parsed else {
        return (0, false, false, false);
    };
    let diff_secs = (expiry - Utc::now()).num_seconds();
    let days = diff_secs.div_euclid(86400);
    let is_expired = diff_secs <= 0;
    let is_expiring_soon = !is_expired && days <= 30;
    (days, is_expired, is_expiring_soon, true)
}

fn openssl_cert_info(file_path: &Path) -> Option<SslCertItem> {
    let path_str = file_path.to_str()?;
    let out = Command::new("openssl")
        .args([
            "x509",
            "-in",
            path_str,
            "-noout",
            "-subject",
            "-issuer",
            "-dates",
            "-ext",
            "subjectAltName",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    cert_from_openssl_text(&text, path_str, path_str)
}

fn cert_from_openssl_text(text: &str, name: &str, path: &str) -> Option<SslCertItem> {
    let mut subject = String::new();
    let mut issuer = String::new();
    let mut not_after = String::new();
    let mut not_before = String::new();
    let mut san_domains = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("subject=") {
            subject = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("issuer=") {
            issuer = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("notAfter=") {
            not_after = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("notBefore=") {
            not_before = value.trim().to_string();
        }
        if trimmed.contains("DNS:") {
            for part in trimmed.split(',') {
                if let Some(dns) = part.trim().strip_prefix("DNS:") {
                    let dns = dns.trim();
                    if !dns.is_empty() {
                        san_domains.push(dns.to_string());
                    }
                }
            }
        }
    }

    if subject.is_empty() && not_after.is_empty() {
        return None;
    }
    let (days_valid, is_expired, is_expiring_soon, parsed) =
        parse_openssl_date(&not_after);
    Some(SslCertItem {
        name: name.to_string(),
        subject: subject.clone(),
        is_self_signed: !subject.is_empty() && subject == issuer,
        issuer,
        not_before,
        not_after,
        days_valid,
        path: path.to_string(),
        san_domains,
        is_expired,
        is_expiring_soon: is_expiring_soon || !parsed,
        date_parse_error: !parsed,
    })
}

fn cert_file_allowed(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !(lower.ends_with(".crt") || lower.ends_with(".pem") || lower.ends_with(".cer")) {
        return false;
    }
    !(lower.contains("ca-bundle")
        || lower.contains("ca-certificates")
        || lower.contains("tls-ca-bundle")
        || lower == "chain.pem"
        || lower == "privkey.pem")
}

fn collect_cert_files(dir: &Path, depth: usize, seen: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() >= 300 || !dir.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let trust_store = dir.starts_with("/etc/ssl/certs") || dir.starts_with("/etc/pki/tls/certs");
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(link_meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if link_meta.file_type().is_symlink() {
            // Trust stores are full of hash symlinks to CA certificates. They
            // are not server certificates, and following them floods the page.
            if trust_store {
                continue;
            }
            let Ok(target_meta) = fs::metadata(&path) else {
                continue;
            };
            if target_meta.is_dir() {
                continue;
            }
        } else if link_meta.is_dir() {
            collect_cert_files(&path, depth - 1, seen, out);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !cert_file_allowed(name) {
            continue;
        }
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(canonical) {
            out.push(path);
            if out.len() >= 300 {
                return;
            }
        }
    }
}

fn scan_ssl_certs_blocking() -> Result<Vec<SslCertItem>, String> {
    if Command::new("openssl").arg("version").output().is_err() {
        return Err("OpenSSL is not installed; certificate inspection is unavailable.".to_string());
    }
    let home = home_dir().unwrap_or_default();
    let mut roots = vec![
        PathBuf::from("/etc/letsencrypt/live"),
        PathBuf::from("/etc/nginx"),
        PathBuf::from("/etc/httpd"),
        PathBuf::from("/etc/apache2"),
        PathBuf::from("/etc/lighttpd"),
        PathBuf::from("/etc/pki/tls/certs"),
        PathBuf::from("/etc/ssl/certs"),
    ];
    if !home.as_os_str().is_empty() {
        roots.push(home.join(".local/share/mkcert"));
    }

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for root in &roots {
        collect_cert_files(root, 4, &mut seen, &mut files);
    }

    let mut certs = Vec::new();
    let mut unique = HashSet::new();
    for path in files {
        if let Some(cert) = openssl_cert_info(&path) {
            let key = format!("{}|{}|{}", cert.subject, cert.issuer, cert.not_after);
            if unique.insert(key) {
                certs.push(cert);
            }
        }
    }
    certs.sort_by(|a, b| {
        a.days_valid
            .cmp(&b.days_valid)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(certs)
}

#[tauri::command]
pub async fn vault_list_ssl_certs() -> Result<Vec<SslCertItem>, String> {
    tokio::task::spawn_blocking(scan_ssl_certs_blocking)
        .await
        .map_err(|e| format!("Certificate scan failed: {e}"))?
}

fn valid_dns_name(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let host = host.trim_end_matches('.');
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

fn tls_target(host: &str, port: u16) -> Result<(String, Option<String>), String> {
    let host = host.trim().trim_end_matches('.');
    if host.is_empty() {
        return Err("Host cannot be empty.".to_string());
    }
    if port == 0 {
        return Err("The port must be between 1 and 65535.".to_string());
    }
    if host.starts_with('-') || host.contains('/') || host.contains('\\') || host.contains('@') {
        return Err("Enter a hostname or IP address without a scheme or user.".to_string());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let target = match ip {
            std::net::IpAddr::V4(_) => format!("{ip}:{port}"),
            std::net::IpAddr::V6(_) => format!("[{ip}]:{port}"),
        };
        return Ok((target, None));
    }
    if !valid_dns_name(host) {
        return Err("Enter a valid DNS hostname or IP address.".to_string());
    }
    Ok((format!("{host}:{port}"), Some(host.to_string())))
}

#[tauri::command]
pub async fn vault_test_remote_ssl(host: String, port: u16) -> Result<SslCertItem, String> {
    let (target, sni) = tls_target(&host, port)?;
    let mut command = tokio::process::Command::new("openssl");
    command
        .args(["s_client", "-connect", &target, "-showcerts"])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(sni) = &sni {
        command.args(["-servername", sni]);
    }
    let output = tokio::time::timeout(Duration::from_secs(12), command.output())
        .await
        .map_err(|_| format!("The TLS handshake with {target} timed out."))?
        .map_err(|e| format!("Failed to run openssl s_client: {e}"))?;

    let full_text = String::from_utf8_lossy(&output.stdout);
    if !full_text.contains("BEGIN CERTIFICATE") {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("Could not retrieve a TLS certificate from {target}.")
        } else {
            format!("Could not retrieve a TLS certificate from {target}: {error}")
        });
    }

    let mut child = Command::new("openssl")
        .args([
            "x509",
            "-noout",
            "-subject",
            "-issuer",
            "-dates",
            "-ext",
            "subjectAltName",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn openssl x509: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(full_text.as_bytes());
    }
    let parsed = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&parsed.stdout);
    let name = format!("{}:{}", host.trim(), port);
    let path = format!("tls://{target}");
    cert_from_openssl_text(&text, &name, &path)
        .ok_or_else(|| format!("OpenSSL returned no certificate details for {target}."))
}

// ─── 7. Fail2ban Threat Defense ────────────────────────────────────────────────

fn valid_jail_name(jail: &str) -> bool {
    !jail.is_empty()
        && jail.len() <= 64
        && jail
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn valid_ip(value: &str) -> bool {
    value.trim().parse::<std::net::IpAddr>().is_ok()
}

async fn run_fail2ban_client(args: &[String], elevated: bool) -> Result<std::process::Output, String> {
    if elevated {
        ensure_arguments_for_program("fail2ban-client", args)?;
        let program = resolve_root_program("fail2ban-client").ok_or_else(|| {
            "fail2ban-client is not installed or is not available for privileged use.".to_string()
        })?;
        run_privileged_output(&program, args, &RootAuth::Polkit).await
    } else {
        tokio::process::Command::new("fail2ban-client")
            .args(args)
            .output()
            .await
            .map_err(|e| format!("Failed to run fail2ban-client: {e}"))
    }
}

fn fail2ban_needs_auth(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("access denied")
        || lower.contains("polkit")
        || lower.contains("operation not permitted")
}

fn fail2ban_not_installed() -> Fail2banStatus {
    Fail2banStatus {
        is_installed: false,
        is_active: false,
        needs_elevation: false,
        total_banned_ips: 0,
        jails: Vec::new(),
        error: None,
    }
}

#[tauri::command]
pub async fn vault_get_fail2ban_status(elevated: bool) -> Result<Fail2banStatus, String> {
    if resolve_root_program("fail2ban-client").is_none() {
        return Ok(fail2ban_not_installed());
    }

    let status_args = vec!["status".to_string()];
    let status_output = run_fail2ban_client(&status_args, elevated).await;
    let output = match status_output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let needs_elevation = !elevated && fail2ban_needs_auth(&error);
            return Ok(Fail2banStatus {
                is_installed: true,
                is_active: false,
                needs_elevation,
                total_banned_ips: 0,
                jails: Vec::new(),
                error: if error.is_empty() { None } else { Some(error) },
            });
        }
        Err(error) => {
            let needs_elevation = !elevated && fail2ban_needs_auth(&error);
            return Ok(Fail2banStatus {
                is_installed: true,
                is_active: false,
                needs_elevation,
                total_banned_ips: 0,
                jails: Vec::new(),
                error: Some(error),
            });
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut jail_names = Vec::new();
    for line in text.lines() {
        if let Some(position) = line.find("Jail list:") {
            jail_names = line[position + "Jail list:".len()..]
                .split(',')
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
        }
    }

    let mut jails = Vec::new();
    let mut total_banned = 0;
    let mut error = None;
    for jail in jail_names {
        if !valid_jail_name(&jail) {
            continue;
        }
        let args = vec!["status".to_string(), jail.clone()];
        let jail_output = run_fail2ban_client(&args, elevated).await;
        let mut currently_banned = 0;
        let mut jail_total = 0;
        let mut banned_ips = Vec::new();
        match jail_output {
            Ok(output) if output.status.success() => {
                let jail_text = String::from_utf8_lossy(&output.stdout);
                for line in jail_text.lines() {
                    if let Some(value) = line.split(':').nth(1) {
                        if line.contains("Currently banned:") {
                            currently_banned = value.trim().parse().unwrap_or(0);
                        } else if line.contains("Total banned:") {
                            jail_total = value.trim().parse().unwrap_or(0);
                        }
                    }
                    if let Some(position) = line.find("Banned IP list:") {
                        banned_ips = line[position + "Banned IP list:".len()..]
                            .split_whitespace()
                            .map(str::to_string)
                            .collect();
                    }
                }
            }
            Ok(output) => {
                let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if !message.is_empty() && error.is_none() {
                    error = Some(message);
                }
            }
            Err(message) => {
                if error.is_none() {
                    error = Some(message);
                }
            }
        }
        total_banned += currently_banned;
        jails.push(Fail2banJailInfo {
            jail_name: jail,
            currently_banned,
            total_banned: jail_total.max(currently_banned),
            banned_ips,
        });
    }
    jails.sort_by(|a, b| a.jail_name.cmp(&b.jail_name));

    Ok(Fail2banStatus {
        is_installed: true,
        is_active: true,
        needs_elevation: !elevated && error.is_some(),
        total_banned_ips: total_banned,
        jails,
        error,
    })
}

async fn fail2ban_action(args: &[String]) -> Result<String, String> {
    let output = run_fail2ban_client(args, true).await?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            "fail2ban-client failed.".to_string()
        } else {
            error
        })
    }
}

#[tauri::command]
pub async fn vault_unban_ip(jail: String, ip: String) -> Result<String, String> {
    if !valid_jail_name(&jail) {
        return Err("The jail name is not valid.".to_string());
    }
    if !valid_ip(&ip) {
        return Err("Enter a valid IPv4 or IPv6 address.".to_string());
    }
    fail2ban_action(&[
        "set".to_string(),
        jail.clone(),
        "unbanip".to_string(),
        ip.clone(),
    ])
    .await?;
    Ok(format!("IP {ip} was unbanned from jail {jail}."))
}

#[tauri::command]
pub async fn vault_ban_ip(jail: String, ip: String) -> Result<String, String> {
    if !valid_jail_name(&jail) {
        return Err("The jail name is not valid.".to_string());
    }
    if !valid_ip(&ip) {
        return Err("Enter a valid IPv4 or IPv6 address.".to_string());
    }
    fail2ban_action(&[
        "set".to_string(),
        jail.clone(),
        "banip".to_string(),
        ip.clone(),
    ])
    .await?;
    Ok(format!("IP {ip} was banned in jail {jail}."))
}

#[tauri::command]
pub async fn vault_manage_fail2ban_service(action: String) -> Result<String, String> {
    let valid_actions = ["start", "restart", "stop", "enable", "disable"];
    if !valid_actions.contains(&action.as_str()) {
        return Err(format!("Invalid fail2ban action: {action}"));
    }
    let args = vec![action.clone(), "fail2ban".to_string()];
    ensure_arguments_for_program("systemctl", &args)?;
    let program = resolve_root_program("systemctl")
        .ok_or_else(|| "systemctl is not available for privileged use.".to_string())?;
    let output = run_privileged_output(&program, &args, &RootAuth::Polkit).await?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("Could not {action} the fail2ban service.")
        } else {
            format!("Could not {action} the fail2ban service: {error}")
        });
    }
    Ok(format!("Fail2ban service {action} completed."))
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn host(alias: &str, hostname: &str) -> SshClientHost {
        SshClientHost {
            host: alias.to_string(),
            hostname: hostname.to_string(),
            user: String::new(),
            port: String::new(),
            identity_file: String::new(),
            proxy_jump: String::new(),
            extra_config: String::new(),
        }
    }

    #[test]
    fn key_names_reject_traversal_and_reserved_files() {
        assert_eq!(validate_key_name("id_ed25519_work").unwrap(), "id_ed25519_work");
        assert_eq!(validate_key_name("work.pub").unwrap(), "work");
        for bad in ["../id", "a/b", "..", ".hidden", "-oops", "config", "known_hosts", "authorized_keys"] {
            assert!(validate_key_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn host_blocks_are_upserted_without_touching_global_config() {
        let original = vec![
            "# global settings".to_string(),
            "Host *".to_string(),
            "    ServerAliveInterval 60".to_string(),
            "".to_string(),
            "Host dev".to_string(),
            "    HostName old.example.com".to_string(),
            "".to_string(),
            "Match user deploy".to_string(),
            "    X11Forwarding no".to_string(),
        ];
        let updated = upsert_host_block(&original, &host("dev", "new.example.com")).unwrap();
        let text = updated.join("\n");
        assert!(text.contains("ServerAliveInterval 60"), "{text}");
        assert!(text.contains("HostName new.example.com"), "{text}");
        assert!(!text.contains("old.example.com"), "{text}");
        assert!(text.contains("Match user deploy"), "{text}");
        assert_eq!(text.matches("Host dev").count(), 1, "{text}");
    }

    #[test]
    fn adding_a_host_appends_and_deleting_it_preserves_the_rest() {
        let original = vec!["# keep me".to_string(), "Host old".to_string(), "    HostName old".to_string()];
        let added = upsert_host_block(&original, &host("new", "new.example.com")).unwrap();
        assert_eq!(added[0], "# keep me");
        let removed = delete_host_block(&added, "new").unwrap();
        assert!(!removed.join("\n").contains("Host new"));
        assert!(removed.join("\n").contains("Host old"));
        assert!(removed.join("\n").contains("# keep me"));
    }

    #[test]
    fn authorized_keys_with_options_and_comments_are_parsed() {
        let line = r#"command="echo hi",no-pty ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample deploy@laptop"#;
        let (kind, data, comment, options) = parse_authorized_key(line).unwrap();
        assert_eq!(kind, "ssh-ed25519");
        assert_eq!(data, "AAAAC3NzaC1lZDI1NTE5AAAAIExample");
        assert_eq!(comment, "deploy@laptop");
        assert!(options.contains("no-pty"));
    }

    #[test]
    fn authorized_key_validation_rejects_multiple_lines_and_bad_base64() {
        assert!(validate_authorized_key("ssh-ed25519 AAAA test").is_ok());
        assert!(validate_authorized_key("ssh-ed25519 AAAA one\nssh-ed25519 BBBB two").is_err());
        assert!(validate_authorized_key("ssh-ed25519 not_base64!").is_err());
    }

    #[test]
    fn known_hosts_markers_do_not_shift_the_key_fields() {
        let item = parse_known_host_line(
            "@revoked example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample",
            3,
        )
        .unwrap();
        assert_eq!(item.marker, "@revoked");
        assert_eq!(item.host, "example.com");
        assert_eq!(item.key_type, "ssh-ed25519");
    }

    #[test]
    fn openssl_dates_are_classified() {
        let (days, expired, soon, parsed) = parse_openssl_date("Jan  1 00:00:00 2030 GMT");
        assert!(parsed);
        assert!(!expired);
        assert!(days > 0);
        assert!(!soon || days <= 30);
        let (_, expired, _, parsed) = parse_openssl_date("Jan  1 00:00:00 2000 GMT");
        assert!(parsed);
        assert!(expired);
        assert!(!parse_openssl_date("not a date").3);
    }

    #[test]
    fn basic_sshd_settings_use_the_first_value() {
        let mut status = SshdHardeningStatus {
            permit_root_login: "unset".to_string(),
            password_authentication: "unset".to_string(),
            pubkey_authentication: "unset".to_string(),
            x11_forwarding: "unset".to_string(),
            port: "unset".to_string(),
            config_path: String::new(),
            is_evaluated: false,
            source: String::new(),
            error: None,
        };
        parse_sshd_config(
            "PasswordAuthentication no\nPasswordAuthentication yes\nMatch user x\nX11Forwarding yes\n",
            &mut status,
        );
        assert_eq!(status.password_authentication, "no");
        assert_eq!(status.x11_forwarding, "unset");
    }

    #[test]
    fn remote_tls_targets_are_normalized_and_validated() {
        assert_eq!(
            tls_target("example.com", 443).unwrap(),
            ("example.com:443".to_string(), Some("example.com".to_string()))
        );
        assert_eq!(
            tls_target("2001:db8::1", 443).unwrap(),
            ("[2001:db8::1]:443".to_string(), None)
        );
        assert!(tls_target("https://example.com", 443).is_err());
        assert!(tls_target("example.com", 0).is_err());
        assert!(tls_target("bad host", 443).is_err());
    }

    #[test]
    fn fail2ban_inputs_are_validated() {
        assert!(valid_jail_name("sshd"));
        assert!(valid_jail_name("nginx-http-auth"));
        assert!(!valid_jail_name("../sshd"));
        assert!(!valid_jail_name(""));
        assert!(valid_ip("192.0.2.1"));
        assert!(valid_ip("2001:db8::1"));
        assert!(!valid_ip("999.1.1.1"));
        assert!(!valid_ip("hostname"));
    }

    #[test]
    fn passphrase_keygen_uses_a_pty_and_produces_an_unlockable_key() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-ssh-keygen-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("key");
        let args = vec![
            "-q".to_string(),
            "-t".to_string(),
            "ed25519".to_string(),
            "-f".to_string(),
            target.to_string_lossy().to_string(),
        ];
        run_keygen_with_passphrase(&args, "correct horse").unwrap();
        let unlocked = Command::new("ssh-keygen")
            .args(["-y", "-P", "correct horse", "-f"])
            .arg(&target)
            .output()
            .unwrap();
        assert!(unlocked.status.success(), "{}", String::from_utf8_lossy(&unlocked.stderr));
        let locked = Command::new("ssh-keygen")
            .args(["-y", "-P", "", "-f"])
            .arg(&target)
            .output()
            .unwrap();
        assert!(!locked.status.success(), "an empty passphrase must not unlock the key");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_rendering_rejects_newline_injection_and_large_extras() {
        let mut injected = host("dev", "example.com");
        injected.user = "root\n    ProxyCommand evil".to_string();
        assert!(render_host_block(&injected).is_err());

        let mut extra = host("dev", "example.com");
        extra.extra_config = "Host other".to_string();
        assert!(render_host_block(&extra).is_err());

        let mut too_many = host("dev", "example.com");
        too_many.extra_config = (0..101).map(|i| format!("SendEnv VAR{i}")).collect::<Vec<_>>().join("\n");
        assert!(render_host_block(&too_many).is_err());
    }

    #[test]
    fn global_host_star_block_is_not_listed_or_deletable() {
        let hosts = parse_ssh_config("Host *\n  ServerAliveInterval 60\n\nHost dev\n  HostName dev.example.com\n");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host, "dev");
        let lines: Vec<String> = "Host *\n  ServerAliveInterval 60\n".lines().map(str::to_string).collect();
        assert!(delete_host_block(&lines, "*").is_err());
        assert!(render_host_block(&host("*", "example.com")).is_err());
    }

    #[test]
    fn ssh_config_parser_keeps_unknown_directives_in_extra_config() {
        let hosts = parse_ssh_config(
            "Host dev\n  HostName dev.example.com\n  User deploy\n  LocalForward 8080 localhost:80\n",
        );
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host, "dev");
        assert_eq!(hosts[0].hostname, "dev.example.com");
        assert!(hosts[0].extra_config.contains("LocalForward 8080"));
    }

    #[test]
    fn fail2ban_auth_errors_are_distinguished_from_a_stopped_service() {
        assert!(fail2ban_needs_auth("ERROR Permission denied opening socket"));
        assert!(!fail2ban_needs_auth("Failed to access socket path: Is fail2ban running?"));
    }

    #[test]
    fn certificate_text_parses_sans_and_self_signed_state() {
        let text = "subject=CN=probe.local\nissuer=CN=probe.local\nnotBefore=Nov 22 11:04:12 2026 GMT\nnotAfter=Dec 22 11:04:12 2026 GMT\nX509v3 Subject Alternative Name:\n    DNS:probe.local, DNS:www.probe.local\n";
        let cert = cert_from_openssl_text(text, "cert.pem", "/tmp/cert.pem").unwrap();
        assert!(cert.is_self_signed);
        assert_eq!(cert.san_domains, vec!["probe.local", "www.probe.local"]);
        assert!(!cert.date_parse_error);
    }

    #[test]
    fn atomic_writes_are_private_and_refuse_symlinks() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-ssh-atomic-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config");
        atomic_write(&path, "Host dev\n    HostName example.com\n", 0o600).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(fs::read_to_string(&path).unwrap().contains("Host dev"));

        let link = dir.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(atomic_write(&link, "Host evil\n", 0o600).is_err());
        assert!(fs::read_to_string(&path).unwrap().contains("Host dev"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn certificate_file_names_exclude_bundles_and_private_keys() {
        assert!(cert_file_allowed("fullchain.pem"));
        assert!(cert_file_allowed("site.crt"));
        assert!(!cert_file_allowed("ca-bundle.crt"));
        assert!(!cert_file_allowed("privkey.pem"));
        assert!(!cert_file_allowed("chain.pem"));
    }
}
