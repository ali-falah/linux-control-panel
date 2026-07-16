use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::utils::privilege::tokio::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub id: String,
    pub name: String,
    pub baseurl: String,
    pub enabled: bool,
    pub file_path: String,
    pub metalink: Option<String>,
    pub mirrorlist: Option<String>,
    pub gpgcheck: bool,
    pub priority: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSpeedResult {
    pub url: String,
    pub speed_ms: Option<u32>,
}

/// Parse all .repo files from /etc/yum.repos.d/
#[tauri::command]
pub async fn list_repos() -> Result<Vec<RepoEntry>, String> {
    let repo_dir = PathBuf::from("/etc/yum.repos.d");
    if !repo_dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();

    let dir_entries =
        fs::read_dir(&repo_dir).map_err(|e| format!("Failed to read /etc/yum.repos.d: {e}"))?;

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("repo") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log_to_file("WARN", &format!("Could not read {:?}: {e}", path));
                continue;
            }
        };

        let file_path = path.to_string_lossy().to_string();
        let mut current_id = String::new();
        let mut current_name = String::new();
        let mut current_baseurl = String::new();
        let mut current_enabled = true;
        let mut current_metalink: Option<String> = None;
        let mut current_mirrorlist: Option<String> = None;
        let mut current_gpgcheck = true;
        let mut current_priority: Option<u32> = None;
        let mut in_section = false;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                // Save previous section
                if in_section && !current_id.is_empty() {
                    entries.push(RepoEntry {
                        id: current_id.clone(),
                        name: if current_name.is_empty() {
                            current_id.clone()
                        } else {
                            current_name.clone()
                        },
                        baseurl: current_baseurl.clone(),
                        enabled: current_enabled,
                        file_path: file_path.clone(),
                        metalink: current_metalink.clone(),
                        mirrorlist: current_mirrorlist.clone(),
                        gpgcheck: current_gpgcheck,
                        priority: current_priority,
                    });
                }
                current_id = line[1..line.len() - 1].to_string();
                current_name = String::new();
                current_baseurl = String::new();
                current_enabled = true;
                current_metalink = None;
                current_mirrorlist = None;
                current_gpgcheck = true;
                current_priority = None;
                in_section = true;
            } else if in_section {
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim().to_lowercase();
                    let val = val.trim().to_string();
                    match key.as_str() {
                        "name" => current_name = val,
                        "baseurl" => current_baseurl = val,
                        "enabled" => current_enabled = val == "1" || val == "true",
                        "metalink" => current_metalink = Some(val),
                        "mirrorlist" => current_mirrorlist = Some(val),
                        "gpgcheck" => current_gpgcheck = val == "1",
                        "priority" => current_priority = val.parse::<u32>().ok(),
                        _ => {}
                    }
                }
            }
        }

        // Save last section
        if in_section && !current_id.is_empty() {
            entries.push(RepoEntry {
                id: current_id.clone(),
                name: if current_name.is_empty() {
                    current_id.clone()
                } else {
                    current_name.clone()
                },
                baseurl: current_baseurl.clone(),
                enabled: current_enabled,
                file_path: file_path.clone(),
                metalink: current_metalink.clone(),
                mirrorlist: current_mirrorlist.clone(),
                gpgcheck: current_gpgcheck,
                priority: current_priority,
            });
        }
    }

    Ok(entries)
}

/// Enable or disable a specific repo section in its .repo file via pkexec + sed
#[tauri::command]
pub async fn toggle_repo(repo_id: String, enabled: bool, file_path: String) -> Result<(), String> {
    let enabled_str = if enabled { "1" } else { "0" };

    // Use sed to update enabled= under the specific [repo_id] section
    let script = format!(
        r#"/^\[{}\]/,/^\[/ s/^enabled\s*=.*/enabled={}/;"#,
        regex::escape(&repo_id),
        enabled_str
    );

    let output = Command::new("pkexec")
        .args(["sed", "-i", &script, &file_path])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        log_to_file("ERROR", &format!("toggle_repo failed: {err}"));
        return Err(format!("Failed to toggle repo: {err}"));
    }

    log_to_file("INFO", &format!("Toggled repo {repo_id} enabled={enabled}"));
    Ok(())
}

/// Add a repo by URL using dnf config-manager
#[tauri::command]
pub async fn add_repo(url: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "config-manager", "--add-repo", &url])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("add_repo failed: {stderr}"));
        return Err(format!("Failed to add repo: {stderr}"));
    }

    log_to_file("INFO", &format!("Added repo: {url}"));
    Ok(stdout)
}

/// Run dnf makecache to refresh metadata
#[tauri::command]
pub async fn run_makecache() -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "makecache"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("makecache failed: {stderr}"));
        return Err(format!("dnf makecache failed: {stderr}"));
    }

    log_to_file("INFO", "Ran dnf makecache");
    Ok(stdout)
}

#[tauri::command]
pub async fn save_repo_details(
    repo_id: String,
    file_path: String,
    name: String,
    baseurl: String,
    metalink: Option<String>,
    mirrorlist: Option<String>,
    gpgcheck: bool,
    priority: Option<u32>,
) -> Result<(), String> {
    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read repo file: {e}"))?;

    let mut new_lines = Vec::new();
    let mut in_target_section = false;
    let mut seen_keys = std::collections::HashSet::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section_id = &trimmed[1..trimmed.len() - 1];
            if in_target_section {
                append_missing_repo_keys(&mut new_lines, &seen_keys, &name, &baseurl, &metalink, &mirrorlist, gpgcheck, priority);
                in_target_section = false;
            }
            if section_id == repo_id {
                in_target_section = true;
                seen_keys.clear();
            }
            new_lines.push(line.to_string());
        } else if in_target_section {
            if let Some((key, _)) = trimmed.split_once('=') {
                let key = key.trim().to_lowercase();
                seen_keys.insert(key.clone());
                let new_line = match key.as_str() {
                    "name" => format!("name={}", name),
                    "baseurl" => {
                        if baseurl.is_empty() {
                            "".to_string()
                        } else {
                            format!("baseurl={}", baseurl)
                        }
                    }
                    "metalink" => {
                        if let Some(ref m) = metalink {
                            if m.is_empty() { "".to_string() } else { format!("metalink={}", m) }
                        } else {
                            "".to_string()
                        }
                    }
                    "mirrorlist" => {
                        if let Some(ref m) = mirrorlist {
                            if m.is_empty() { "".to_string() } else { format!("mirrorlist={}", m) }
                        } else {
                            "".to_string()
                        }
                    }
                    "gpgcheck" => format!("gpgcheck={}", if gpgcheck { "1" } else { "0" }),
                    "priority" => {
                        if let Some(p) = priority {
                            format!("priority={}", p)
                        } else {
                            "".to_string()
                        }
                    }
                    _ => line.to_string(),
                };
                if !new_line.is_empty() {
                    new_lines.push(new_line);
                }
            } else {
                new_lines.push(line.to_string());
            }
        } else {
            new_lines.push(line.to_string());
        }
    }

    if in_target_section {
        append_missing_repo_keys(&mut new_lines, &seen_keys, &name, &baseurl, &metalink, &mirrorlist, gpgcheck, priority);
    }

    let final_content = new_lines.join("\n") + "\n";

    let mut child = Command::new("pkexec")
        .args(["tee", &file_path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run pkexec tee: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(final_content.as_bytes()).await
            .map_err(|e| format!("Failed to write: {e}"))?;
    }

    let out = child.wait_with_output().await
        .map_err(|e| format!("Failed to wait: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("Failed to write repo file: {err}"));
    }

    Ok(())
}

fn append_missing_repo_keys(
    lines: &mut Vec<String>,
    seen: &std::collections::HashSet<String>,
    name: &str,
    baseurl: &str,
    metalink: &Option<String>,
    mirrorlist: &Option<String>,
    gpgcheck: bool,
    priority: Option<u32>,
) {
    if !seen.contains("name") {
        lines.push(format!("name={}", name));
    }
    if !seen.contains("baseurl") && !baseurl.is_empty() {
        lines.push(format!("baseurl={}", baseurl));
    }
    if !seen.contains("metalink") {
        if let Some(ref m) = metalink {
            if !m.is_empty() {
                lines.push(format!("metalink={}", m));
            }
        }
    }
    if !seen.contains("mirrorlist") {
        if let Some(ref m) = mirrorlist {
            if !m.is_empty() {
                lines.push(format!("mirrorlist={}", m));
            }
        }
    }
    if !seen.contains("gpgcheck") {
        lines.push(format!("gpgcheck={}", if gpgcheck { "1" } else { "0" }));
    }
    if !seen.contains("priority") {
        if let Some(p) = priority {
            lines.push(format!("priority={}", p));
        }
    }
}

async fn fetch_urls_from_mirrorlist(url: &str) -> Vec<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    
    if let Ok(res) = client.get(url).send().await {
        if let Ok(text) = res.text().await {
            return text.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .take(5)
                .collect();
        }
    }
    vec![]
}

async fn fetch_urls_from_metalink(url: &str) -> Vec<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let mut urls = Vec::new();
    if let Ok(res) = client.get(url).send().await {
        if let Ok(text) = res.text().await {
            let mut remaining = text.as_str();
            while let Some(start_idx) = remaining.find("<url") {
                let segment = &remaining[start_idx..];
                if let Some(close_tag_idx) = segment.find('>') {
                    let content_segment = &segment[close_tag_idx+1..];
                    if let Some(end_tag_idx) = content_segment.find("</url>") {
                        let mirror_url = content_segment[..end_tag_idx].trim().to_string();
                        if mirror_url.starts_with("http") {
                            urls.push(mirror_url);
                            if urls.len() >= 5 {
                                break;
                            }
                        }
                        remaining = &content_segment[end_tag_idx..];
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }
    urls
}

async fn measure_url_speed(url: &str) -> Option<u32> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;

    let start = std::time::Instant::now();
    let res = client.get(url).send().await;
    match res {
        Ok(_) => {
            let duration = start.elapsed().as_millis() as u32;
            Some(duration)
        }
        Err(_) => None,
    }
}

#[tauri::command]
pub async fn test_repo_mirror_speeds(
    baseurl: String,
    mirrorlist: Option<String>,
    metalink: Option<String>,
) -> Result<Vec<MirrorSpeedResult>, String> {
    let mut urls_to_test = Vec::new();

    if let Some(ref ml) = mirrorlist {
        if !ml.is_empty() {
            let list_urls = fetch_urls_from_mirrorlist(ml).await;
            urls_to_test.extend(list_urls);
        }
    }

    if urls_to_test.is_empty() {
        if let Some(ref mt) = metalink {
            if !mt.is_empty() {
                let meta_urls = fetch_urls_from_metalink(mt).await;
                urls_to_test.extend(meta_urls);
            }
        }
    }

    if urls_to_test.is_empty() && !baseurl.is_empty() {
        urls_to_test.push(baseurl.clone());
    }

    if urls_to_test.is_empty() {
        return Err("No URLs to test".to_string());
    }

    let mut results = Vec::new();
    let mut handles = Vec::new();

    for url in urls_to_test {
        let url_clone = url.clone();
        let handle = tokio::spawn(async move {
            let speed = measure_url_speed(&url_clone).await;
            MirrorSpeedResult {
                url: url_clone,
                speed_ms: speed,
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        if let Ok(res) = handle.await {
            results.push(res);
        }
    }

    results.sort_by(|a, b| match (a.speed_ms, b.speed_ms) {
        (Some(sa), Some(sb)) => sa.cmp(&sb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    Ok(results)
}
