use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::log_to_file;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostEntry {
    pub id: String,
    pub ip: String,
    pub hostnames: Vec<String>,
    pub comment: String,
    pub enabled: bool,
    pub category: String,
    /// A line the parser could not classify (for example a malformed record).
    /// It is carried through unchanged so saving the file never drops it.
    #[serde(default)]
    pub raw: Option<String>,
}

/// Read /etc/hosts as structured entries.
#[tauri::command]
pub async fn read_hosts() -> Result<Vec<HostEntry>, String> {
    let content = read_hosts_text().await?;
    Ok(parse_hosts(&content))
}

/// The exact file text, used for an accurate diff before saving.
#[tauri::command]
pub async fn read_hosts_raw() -> Result<String, String> {
    read_hosts_text().await
}

async fn read_hosts_text() -> Result<String, String> {
    tokio::fs::read_to_string("/etc/hosts")
        .await
        .map_err(|e| format!("Failed to read /etc/hosts: {e}"))
}

/// Serialize the edited entries without writing anything, so the UI can show
/// the exact content a save would produce.
#[tauri::command]
pub fn preview_hosts(entries: Vec<HostEntry>) -> Result<String, String> {
    validate_entries(&entries)?;
    Ok(serialize_hosts(&entries))
}

fn parse_hosts(content: &str) -> Vec<HostEntry> {
    let mut entries = Vec::new();
    let mut current_category = String::new();
    let mut id_counter = 0u32;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(comment_line) = trimmed.strip_prefix('#') {
            let text = comment_line.trim();

            // A commented-out host line is a disabled entry, not a comment.
            if let Some(entry) = parse_host_line(text, false, &current_category, &mut id_counter) {
                entries.push(entry);
                continue;
            }

            // Section headings are comments with an uppercase first letter.
            if is_category_header(text) {
                current_category = text.to_string();
                continue;
            }

            id_counter += 1;
            entries.push(HostEntry {
                id: format!("entry-{id_counter}"),
                ip: String::new(),
                hostnames: Vec::new(),
                comment: text.to_string(),
                enabled: false,
                category: current_category.clone(),
                raw: None,
            });
            continue;
        }

        if let Some(entry) = parse_host_line(trimmed, true, &current_category, &mut id_counter) {
            entries.push(entry);
        } else {
            // Preserve an unrecognised line verbatim instead of deleting it.
            id_counter += 1;
            entries.push(HostEntry {
                id: format!("entry-{id_counter}"),
                ip: String::new(),
                hostnames: Vec::new(),
                comment: String::new(),
                enabled: false,
                category: current_category.clone(),
                raw: Some(trimmed.to_string()),
            });
        }
    }

    entries
}

fn is_category_header(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    // Headings in hosts files are title/uppercase labels, not sentences and
    // not numeric comments such as dates.
    first.is_uppercase() && text.chars().any(|c| c.is_alphabetic())
}

fn parse_host_line(
    line: &str,
    enabled: bool,
    category: &str,
    id_counter: &mut u32,
) -> Option<HostEntry> {
    let (host_part, inline_comment) = match line.find('#') {
        Some(pos) => (&line[..pos], line[pos + 1..].trim().to_string()),
        None => (line, String::new()),
    };

    let tokens: Vec<&str> = host_part.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }

    let ip = tokens[0].trim();
    if ip.parse::<IpAddr>().is_err() {
        return None;
    }

    let hostnames: Vec<String> = tokens[1..].iter().map(|part| part.to_string()).collect();
    if hostnames.is_empty() {
        return None;
    }

    *id_counter += 1;
    Some(HostEntry {
        id: format!("entry-{id_counter}"),
        ip: ip.to_string(),
        hostnames,
        comment: inline_comment,
        enabled,
        category: category.to_string(),
        raw: None,
    })
}

fn validate_entries(entries: &[HostEntry]) -> Result<(), String> {
    for (index, entry) in entries.iter().enumerate() {
        let position = index + 1;

        if has_control_chars(&entry.comment) {
            return Err(format!(
                "Entry {position}: the comment contains a control character"
            ));
        }
        if has_control_chars(&entry.category) {
            return Err(format!(
                "Entry {position}: the category contains a control character"
            ));
        }
        if !entry.category.is_empty() {
            if entry.category != entry.category.trim() {
                return Err(format!(
                    "Entry {position}: the category has leading or trailing spaces"
                ));
            }
            if !entry
                .category
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase())
            {
                return Err(format!(
                    "Entry {position}: a category must start with a capital letter so it survives a reload"
                ));
            }
        }

        if let Some(raw) = &entry.raw {
            if raw.contains('\n') || raw.contains('\r') || raw.contains('\0') {
                return Err(format!(
                    "Entry {position}: an unrecognised line must be a single line"
                ));
            }
            continue;
        }

        if entry.ip.is_empty() {
            continue;
        }

        if entry.ip != entry.ip.trim() || entry.ip.parse::<IpAddr>().is_err() {
            return Err(format!(
                "Entry {position}: '{}' is not a valid IP address",
                entry.ip
            ));
        }
        if entry.hostnames.is_empty() {
            return Err(format!(
                "Entry {position}: at least one hostname is required"
            ));
        }
        for hostname in &entry.hostnames {
            if hostname.is_empty()
                || hostname
                    .chars()
                    .any(|c| c.is_whitespace() || c == '#' || c.is_control())
            {
                return Err(format!(
                    "Entry {position}: hostname '{hostname}' is empty or contains whitespace/#"
                ));
            }
        }
    }
    Ok(())
}

fn has_control_chars(value: &str) -> bool {
    value.chars().any(|c| c.is_control())
}

fn serialize_hosts(entries: &[HostEntry]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut last_category = String::new();

    for entry in entries {
        if let Some(raw) = &entry.raw {
            emit_category(&mut lines, &mut last_category, &entry.category);
            lines.push(raw.clone());
            continue;
        }

        if entry.ip.is_empty() {
            emit_category(&mut lines, &mut last_category, &entry.category);
            lines.push(if entry.comment.is_empty() {
                String::new()
            } else {
                format!("# {}", entry.comment)
            });
            continue;
        }

        emit_category(&mut lines, &mut last_category, &entry.category);
        let hostnames = entry.hostnames.join(" ");
        let comment = if entry.comment.is_empty() {
            String::new()
        } else {
            format!(" # {}", entry.comment)
        };
        if entry.enabled {
            lines.push(format!("{}\t{}{}", entry.ip, hostnames, comment));
        } else {
            lines.push(format!("#{}\t{}{}", entry.ip, hostnames, comment));
        }
    }

    let mut content = lines.join("\n");
    content.push('\n');
    content
}

fn emit_category(lines: &mut Vec<String>, last_category: &mut String, category: &str) {
    if category.is_empty() || category == last_category {
        return;
    }
    if !last_category.is_empty() && !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(format!("# {category}"));
    *last_category = category.to_string();
}

/// Validate, serialize and write /etc/hosts with root privileges.
#[tauri::command]
pub async fn write_hosts(
    entries: Vec<HostEntry>,
    expected_content: Option<String>,
) -> Result<(), String> {
    validate_entries(&entries)?;
    if let Some(expected) = expected_content {
        let current = read_hosts_text().await?;
        if current != expected {
            return Err(
                "/etc/hosts changed on disk since it was loaded. Reload before saving so the \
                 external change is not overwritten."
                    .to_string(),
            );
        }
    }
    let content = serialize_hosts(&entries);

    crate::utils::privilege::write_file_as_root("/etc/hosts", &content).await?;

    log_to_file("INFO", "Wrote /etc/hosts");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(input: &str) -> String {
        serialize_hosts(&parse_hosts(input))
    }

    #[test]
    fn category_headers_are_not_duplicated_on_save() {
        let input = "\
# Local hosts
127.0.0.1 localhost
::1 localhost
";
        let output = roundtrip(input);
        assert_eq!(output.matches("# Local hosts").count(), 1, "{output}");
        assert!(!output.contains("# Default"), "{output}");
        assert_eq!(
            output,
            "# Local hosts\n127.0.0.1\tlocalhost\n::1\tlocalhost\n"
        );
    }

    #[test]
    fn commented_out_hosts_survive_as_disabled_entries() {
        let input = "\
127.0.0.1 localhost
#10.0.0.5 old-host # retired
";
        let entries = parse_hosts(input);
        let disabled = entries
            .iter()
            .find(|entry| entry.ip == "10.0.0.5")
            .expect("disabled host entry");
        assert!(!disabled.enabled);
        assert_eq!(disabled.hostnames, vec!["old-host"]);
        assert_eq!(disabled.comment, "retired");
        assert!(roundtrip(input).contains("#10.0.0.5\told-host # retired"));
    }

    #[test]
    fn plain_comments_and_blank_entry_lines_are_preserved() {
        let input = "\
# Local hosts
127.0.0.1 localhost
# added by hand
#
10.0.0.1 app.local
";
        let output = roundtrip(input);
        assert!(output.contains("# added by hand"), "{output}");
        assert!(output.contains("\n\n10.0.0.1\tapp.local"), "{output}");
    }

    #[test]
    fn unparsed_lines_are_preserved_verbatim() {
        let input = "not-a-real-host-line\n127.0.0.1 localhost\n";
        let entries = parse_hosts(input);
        assert_eq!(entries[0].raw.as_deref(), Some("not-a-real-host-line"));
        let output = roundtrip(input);
        assert!(output.starts_with("not-a-real-host-line\n"), "{output}");
        assert!(output.contains("127.0.0.1\tlocalhost"), "{output}");
    }

    #[test]
    fn ipv6_and_inline_comments_are_parsed() {
        let entries = parse_hosts("fe80::1\trouter.local # uplink\n");
        assert_eq!(entries[0].ip, "fe80::1");
        assert_eq!(entries[0].hostnames, vec!["router.local"]);
        assert_eq!(entries[0].comment, "uplink");
        assert!(entries[0].enabled);
    }

    #[test]
    fn validation_rejects_bad_ips_hostnames_and_controls() {
        let mut entry = HostEntry {
            id: "x".into(),
            ip: "999.1.1.1".into(),
            hostnames: vec!["host".into()],
            comment: String::new(),
            enabled: true,
            category: "Custom".into(),
            raw: None,
        };
        assert!(validate_entries(&[entry.clone()]).is_err());

        entry.ip = "10.0.0.1".into();
        entry.hostnames = vec![];
        assert!(validate_entries(&[entry.clone()]).is_err());

        entry.hostnames = vec!["two words".into()];
        assert!(validate_entries(&[entry.clone()]).is_err());

        entry.hostnames = vec!["host".into()];
        entry.comment = "broken\ncomment".into();
        assert!(validate_entries(&[entry.clone()]).is_err());

        entry.comment = String::new();
        entry.category = "custom".into();
        assert!(validate_entries(&[entry]).is_err());
    }

    #[test]
    fn a_new_category_gets_one_header() {
        let entries = vec![
            HostEntry {
                id: "1".into(),
                ip: "10.0.0.1".into(),
                hostnames: vec!["a".into()],
                comment: String::new(),
                enabled: true,
                category: "Custom".into(),
                raw: None,
            },
            HostEntry {
                id: "2".into(),
                ip: "10.0.0.2".into(),
                hostnames: vec!["b".into()],
                comment: String::new(),
                enabled: true,
                category: "Custom".into(),
                raw: None,
            },
        ];
        let output = serialize_hosts(&entries);
        assert_eq!(output.matches("# Custom").count(), 1, "{output}");
    }
}
