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
            // Remove surrounding quotes from value
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
                    if config.hidden_timeout {
                        "hidden"
                    } else {
                        "menu"
                    }
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
            if config.hidden_timeout {
                "hidden"
            } else {
                "menu"
            }
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

    Ok("Successfully wrote /etc/default/grub".to_string())
}

#[tauri::command]
pub async fn rebuild_grub() -> Result<String, String> {
    // Determine grub output path. Fedora uses /boot/grub2/grub.cfg
    // We will just run `grub2-mkconfig -o /boot/grub2/grub.cfg`
    let output = Command::new("pkexec")
        .args(["/usr/sbin/grub2-mkconfig", "-o", "/boot/grub2/grub.cfg"])
        .output()
        .await
        .map_err(|e| format!("pkexec failed: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Successfully rebuilt GRUB configuration".to_string())
}
