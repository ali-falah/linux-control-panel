use serde::{Deserialize, Serialize};
use std::fs;
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrubConfig {
    pub timeout: i32,
    pub hidden_timeout: bool,
    pub cmdline_linux: String,
    pub default_entry: String,
    pub raw_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrubValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Validates GRUB kernel arguments and configuration parameters against known system-breaking patterns.
pub fn validate_grub_parameters(config: &GrubConfig) -> GrubValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Timeout range check
    if config.timeout < -1 || config.timeout > 300 {
        errors.push(format!("Timeout '{}' is out of safe range (-1 to 300 seconds).", config.timeout));
    }

    // 2. Timeout 0 + hidden combination (High Risk of Locking User Out of Recovery Menu)
    if config.timeout == 0 && config.hidden_timeout {
        warnings.push("Both 'Timeout = 0s' and 'Hidden Menu' are active. If a kernel panic occurs, accessing the GRUB recovery menu will be difficult.".to_string());
    }

    // 3. Shell Injection & Malformed Characters in CMDLINE
    let cmdline = &config.cmdline_linux;
    if cmdline.contains('\n') || cmdline.contains('\r') || cmdline.contains('\0') {
        errors.push("Kernel parameters must not contain newline or null characters.".to_string());
    }
    if cmdline.contains(';') || cmdline.contains('&') || cmdline.contains('|') || cmdline.contains('`') || cmdline.contains("$(") {
        errors.push("Shell metacharacters (;, &, |, `, $()) are forbidden in kernel parameters to prevent corruption.".to_string());
    }

    // 4. Quote balance check
    let single_quotes = cmdline.chars().filter(|&c| c == '\'').count();
    let double_quotes = cmdline.chars().filter(|&c| c == '"').count();
    if single_quotes % 2 != 0 || double_quotes % 2 != 0 {
        errors.push("Unbalanced quotes detected in kernel parameters. Unclosed quotes will corrupt /etc/default/grub.".to_string());
    }

    // 5. Detect Hazardous / Broken Kernel Arguments
    for arg in cmdline.split_whitespace() {
        let arg_lower = arg.to_lowercase();
        if arg_lower == "init=/bin/false" || arg_lower == "init=/dev/null" {
            errors.push(format!("Fatal parameter '{}' will prevent system initialization and boot.", arg));
        } else if arg_lower.starts_with("init=/bin/sh") || arg_lower.starts_with("init=/bin/bash") {
            warnings.push(format!("Parameter '{}' bypasses systemd and boots directly to a single root shell.", arg));
        } else if arg_lower == "emergency" || arg_lower == "rd.break" || arg_lower == "single" {
            warnings.push(format!("Parameter '{}' forces boot into emergency/recovery mode.", arg));
        } else if arg_lower == "mem=0" || arg_lower == "maxcpus=0" {
            errors.push(format!("Fatal parameter '{}' disables memory/CPU allocation.", arg));
        } else if arg_lower == "selinux=0" || arg_lower == "enforcing=0" {
            warnings.push(format!("Parameter '{}' disables SELinux kernel security enforcement.", arg));
        }
    }

    // 6. Default Entry validation
    if config.default_entry.contains('\n') || config.default_entry.contains('"') || config.default_entry.contains(';') {
        errors.push("Default boot entry contains invalid characters or newlines.".to_string());
    }

    GrubValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
    }
}

#[tauri::command]
pub async fn validate_grub_config(config: GrubConfig) -> Result<GrubValidationResult, String> {
    Ok(validate_grub_parameters(&config))
}

#[tauri::command]
pub async fn read_grub_config() -> Result<GrubConfig, String> {
    let content = fs::read_to_string("/etc/default/grub")
        .map_err(|e| format!("Failed to read /etc/default/grub: {}", e))?;

    let mut timeout = 5;
    let mut hidden_timeout = false;
    let mut cmdline_linux = String::new();
    let mut default_entry = "saved".to_string();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();

            match key {
                "GRUB_TIMEOUT" => timeout = value.parse().unwrap_or(5),
                "GRUB_TIMEOUT_STYLE" => hidden_timeout = value == "hidden",
                "GRUB_CMDLINE_LINUX" => cmdline_linux = value,
                "GRUB_DEFAULT" => default_entry = value,
                _ => {}
            }
        }
    }

    Ok(GrubConfig {
        timeout,
        hidden_timeout,
        cmdline_linux,
        default_entry,
        raw_content: content,
    })
}

#[tauri::command]
pub async fn write_grub_config(config: GrubConfig) -> Result<String, String> {
    // 1. Strict Validation Check before writing
    let validation = validate_grub_parameters(&config);
    if !validation.is_valid {
        return Err(format!("Cannot save invalid GRUB configuration:\n• {}", validation.errors.join("\n• ")));
    }

    // 2. Automatic Backup of existing /etc/default/grub
    if let Ok(existing_content) = fs::read_to_string("/etc/default/grub") {
        if !existing_content.trim().is_empty() {
            let _ = crate::utils::privilege::write_file_as_root("/etc/default/grub.bak", &existing_content).await;
        }
    }

    let mut new_lines = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    for line in config.raw_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim();
            seen_keys.insert(key.to_string());

            let new_line = match key {
                "GRUB_TIMEOUT" => format!("GRUB_TIMEOUT={}", config.timeout),
                "GRUB_TIMEOUT_STYLE" => format!(
                    "GRUB_TIMEOUT_STYLE={}",
                    if config.hidden_timeout { "hidden" } else { "menu" }
                ),
                "GRUB_CMDLINE_LINUX" => format!("GRUB_CMDLINE_LINUX=\"{}\"", config.cmdline_linux),
                "GRUB_DEFAULT" => format!("GRUB_DEFAULT=\"{}\"", config.default_entry),
                _ => line.to_string(), // Keep untouched
            };
            new_lines.push(new_line);
        } else {
            new_lines.push(line.to_string());
        }
    }

    // Append missing keys if they weren't in the original file
    if !seen_keys.contains("GRUB_TIMEOUT") {
        new_lines.push(format!("GRUB_TIMEOUT={}", config.timeout));
    }
    if !seen_keys.contains("GRUB_TIMEOUT_STYLE") {
        new_lines.push(format!(
            "GRUB_TIMEOUT_STYLE={}",
            if config.hidden_timeout { "hidden" } else { "menu" }
        ));
    }
    if !seen_keys.contains("GRUB_CMDLINE_LINUX") {
        new_lines.push(format!("GRUB_CMDLINE_LINUX=\"{}\"", config.cmdline_linux));
    }
    if !seen_keys.contains("GRUB_DEFAULT") {
        new_lines.push(format!("GRUB_DEFAULT=\"{}\"", config.default_entry));
    }

    let final_content = new_lines.join("\n") + "\n";

    // Write safely via base64 decoding as root
    crate::utils::privilege::write_file_as_root("/etc/default/grub", &final_content).await?;

    Ok("Successfully validated and wrote /etc/default/grub (safety backup created at /etc/default/grub.bak)".to_string())
}

#[tauri::command]
pub async fn grub_has_backup() -> Result<bool, String> {
    Ok(std::path::Path::new("/etc/default/grub.bak").exists())
}

#[tauri::command]
pub async fn grub_restore_backup() -> Result<String, String> {
    if !std::path::Path::new("/etc/default/grub.bak").exists() {
        return Err("No backup file found at /etc/default/grub.bak".to_string());
    }

    let backup_content = fs::read_to_string("/etc/default/grub.bak")
        .map_err(|e| format!("Failed to read backup: {}", e))?;

    crate::utils::privilege::write_file_as_root("/etc/default/grub", &backup_content).await?;
    Ok("Successfully restored /etc/default/grub from previous safety backup".to_string())
}

#[tauri::command]
pub async fn rebuild_grub() -> Result<String, String> {
    // 1. Verify /etc/default/grub exists and is non-empty
    let content = fs::read_to_string("/etc/default/grub")
        .map_err(|e| format!("Cannot rebuild GRUB: /etc/default/grub is missing or unreadable ({})", e))?;

    if content.trim().is_empty() {
        return Err("Cannot rebuild GRUB: /etc/default/grub is empty. Please restore backup first.".to_string());
    }

    // Determine grub output path. Fedora/RHEL uses /boot/grub2/grub.cfg
    let output = Command::new("pkexec")
        .args(["/usr/sbin/grub2-mkconfig", "-o", "/boot/grub2/grub.cfg"])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Successfully rebuilt GRUB bootloader configuration".to_string())
}

