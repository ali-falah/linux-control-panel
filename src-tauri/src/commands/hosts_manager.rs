use serde::{Deserialize, Serialize};
use crate::log_to_file;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostEntry {
    pub id: String,
    pub ip: String,
    pub hostnames: Vec<String>,
    pub comment: String,
    pub enabled: bool,
    pub category: String,
}

/// Parse /etc/hosts into structured entries
#[tauri::command]
pub async fn read_hosts() -> Result<Vec<HostEntry>, String> {
    let content = tokio::fs::read_to_string("/etc/hosts")
        .await
        .map_err(|e| format!("Failed to read /etc/hosts: {e}"))?;

    let entries = parse_hosts(&content);
    Ok(entries)
}

fn parse_hosts(content: &str) -> Vec<HostEntry> {
    let mut entries = Vec::new();
    let mut current_category = "Default".to_string();
    let mut id_counter = 0u32;

    for line in content.lines() {
        let trimmed = line.trim();

        // Pure comment line that acts as a category header
        if trimmed.starts_with('#') {
            let comment_text = trimmed.trim_start_matches('#').trim().to_string();
            // If comment is a section header (not empty), use it as category
            if !comment_text.is_empty() && !comment_text.starts_with(|c: char| c.is_lowercase()) {
                current_category = comment_text.clone();
            }
            // Add as a disabled comment entry
            id_counter += 1;
            entries.push(HostEntry {
                id: format!("entry-{id_counter}"),
                ip: String::new(),
                hostnames: vec![],
                comment: comment_text,
                enabled: false,
                category: current_category.clone(),
            });
            continue;
        }

        // Empty line
        if trimmed.is_empty() {
            continue;
        }

        // Could be a commented-out host entry: #127.0.0.1 hostname
        let (actual_line, is_enabled) = if trimmed.starts_with('#') {
            (trimmed.trim_start_matches('#').trim(), false)
        } else {
            (trimmed, true)
        };

        // Split on whitespace; first token is IP, rest are hostnames
        // Inline comments after #
        let (host_part, inline_comment) = if let Some(pos) = actual_line.find('#') {
            (
                &actual_line[..pos],
                actual_line[pos + 1..].trim().to_string(),
            )
        } else {
            (actual_line, String::new())
        };

        let tokens: Vec<&str> = host_part.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }

        let ip = tokens[0].to_string();
        let hostnames: Vec<String> = tokens[1..].iter().map(|s| s.to_string()).collect();

        // Basic IP validation
        if !looks_like_ip(&ip) {
            continue;
        }

        id_counter += 1;
        entries.push(HostEntry {
            id: format!("entry-{id_counter}"),
            ip,
            hostnames,
            comment: inline_comment,
            enabled: is_enabled,
            category: current_category.clone(),
        });
    }

    entries
}

fn looks_like_ip(s: &str) -> bool {
    // Simple check for IPv4 or IPv6
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 4 {
        return parts.iter().all(|p| p.parse::<u8>().is_ok());
    }
    // IPv6
    s.contains(':')
}

/// Serialize entries back to /etc/hosts format and write safely as root
#[tauri::command]
pub async fn write_hosts(entries: Vec<HostEntry>) -> Result<(), String> {
    let content = serialize_hosts(&entries);

    crate::utils::privilege::write_file_as_root("/etc/hosts", &content).await?;

    log_to_file("INFO", "Wrote /etc/hosts");
    Ok(())
}

fn serialize_hosts(entries: &[HostEntry]) -> String {
    let mut lines = Vec::new();
    let mut last_category = String::new();

    for entry in entries {
        // Insert category header when category changes
        if entry.category != last_category && !entry.ip.is_empty() {
            if !last_category.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("# {}", entry.category));
            last_category = entry.category.clone();
        }

        if entry.ip.is_empty() {
            // Pure comment line
            if entry.comment.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!("# {}", entry.comment));
            }
        } else {
            let hostnames = entry.hostnames.join(" ");
            let comment_part = if entry.comment.is_empty() {
                String::new()
            } else {
                format!(" # {}", entry.comment)
            };

            if entry.enabled {
                lines.push(format!("{}\t{}{}", entry.ip, hostnames, comment_part));
            } else {
                lines.push(format!("#{}\t{}{}", entry.ip, hostnames, comment_part));
            }
        }
    }

    lines.join("\n") + "\n"
}
