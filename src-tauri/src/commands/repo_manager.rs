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

        let file_path = path.to_string_lossy().to_string();
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log_to_file("WARN", &format!("Could not read {:?}: {e}", path));
                let filename = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                entries.push(RepoEntry {
                    id: filename.clone(),
                    name: filename,
                    baseurl: String::new(),
                    enabled: false,
                    file_path: file_path.clone(),
                    metalink: None,
                    mirrorlist: None,
                    gpgcheck: false,
                    priority: None,
                });
                continue;
            }
        };

        let mut current_id = String::new();
        let mut current_name = String::new();
        let mut current_baseurl = String::new();
        let mut current_enabled = true;
        let mut current_metalink: Option<String> = None;
        let mut current_mirrorlist: Option<String> = None;
        let mut current_gpgcheck = true;
        let mut current_priority: Option<u32> = None;
        let mut in_section = false;
        let mut sections_found = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                sections_found += 1;
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
            sections_found += 1;
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

        if sections_found == 0 {
            let filename = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "unknown.repo".to_string());
            entries.push(RepoEntry {
                id: filename.clone(),
                name: filename,
                baseurl: String::new(),
                enabled: false,
                file_path: file_path.clone(),
                metalink: None,
                mirrorlist: None,
                gpgcheck: false,
                priority: None,
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

    crate::utils::privilege::write_file_as_root(&file_path, &final_content).await?;

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
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .ok()?;

    let start = std::time::Instant::now();
    let res = client.get(url)
        .header("User-Agent", "dnf/5")
        .send()
        .await;

    match res {
        Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
            let duration = start.elapsed().as_millis() as u32;
            Some(duration)
        }
        _ => None,
    }
}

#[tauri::command]
pub async fn test_repo_mirror_speeds(
    baseurl: String,
    mirrorlist: Option<String>,
    metalink: Option<String>,
) -> Result<Vec<MirrorSpeedResult>, String> {
    let (releasever, basearch) = get_system_repo_vars();
    let mut urls_to_test = Vec::new();

    if let Some(ref ml) = mirrorlist {
        if !ml.is_empty() {
            let sub_ml = substitute_repo_vars(ml, &releasever, &basearch);
            let list_urls = fetch_urls_from_mirrorlist(&sub_ml).await;
            urls_to_test.extend(list_urls);
        }
    }

    if urls_to_test.is_empty() {
        if let Some(ref mt) = metalink {
            if !mt.is_empty() {
                let sub_mt = substitute_repo_vars(mt, &releasever, &basearch);
                let meta_urls = fetch_urls_from_metalink(&sub_mt).await;
                urls_to_test.extend(meta_urls);
            }
        }
    }

    if urls_to_test.is_empty() && !baseurl.is_empty() {
        let sub_base = substitute_repo_vars(&baseurl, &releasever, &basearch);
        let probe_url = get_repo_probe_url(&sub_base);
        urls_to_test.push(probe_url);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoDiagnostic {
    pub repo_id: String,
    pub name: String,
    pub file_path: String,
    pub enabled: bool,
    pub status: String, // "healthy" | "slow" | "unreachable" | "corrupted" | "empty" | "disabled"
    pub latency_ms: Option<u32>,
    pub http_status: Option<u16>,
    pub repomd_valid: bool,
    pub error_message: Option<String>,
    pub tested_url: Option<String>,
    pub is_empty_file: bool,
    pub is_corrupted_syntax: bool,
}

fn get_system_repo_vars() -> (String, String) {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64".to_string(),
        "aarch64" => "aarch64".to_string(),
        other => other.to_string(),
    };

    let mut releasever = "44".to_string();
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VERSION_ID=") {
                let clean = val.trim_matches('"').trim();
                if !clean.is_empty() {
                    releasever = clean.to_string();
                    break;
                }
            }
        }
    }
    (releasever, arch)
}

fn substitute_repo_vars(url: &str, releasever: &str, basearch: &str) -> String {
    url.replace("$releasever", releasever)
       .replace("$basearch", basearch)
       .replace("$infra", "stock")
       .replace("$arch", basearch)
}

fn get_repo_probe_url(raw_url: &str) -> String {
    let clean = raw_url.trim().trim_end_matches('/');
    if clean.contains("metalink?") || clean.contains("mirrorlist?") 
        || clean.ends_with(".xml") || clean.ends_with(".repo") 
        || clean.contains("/metalink") || clean.contains("/mirrorlist") {
        clean.to_string()
    } else {
        format!("{clean}/repodata/repomd.xml")
    }
}

#[tauri::command]
pub async fn validate_all_repos() -> Result<Vec<RepoDiagnostic>, String> {
    let repo_dir = PathBuf::from("/etc/yum.repos.d");
    if !repo_dir.exists() {
        return Ok(vec![]);
    }

    let (releasever, basearch) = get_system_repo_vars();
    let mut diagnostics = Vec::new();
    let dir_entries = fs::read_dir(&repo_dir).map_err(|e| format!("Failed to read /etc/yum.repos.d: {e}"))?;

    struct PendingProbe {
        repo_id: String,
        name: String,
        file_path: String,
        enabled: bool,
        candidate_url: Option<String>,
    }

    let mut pending_probes = Vec::new();

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("repo") {
            continue;
        }

        let file_path = path.to_string_lossy().to_string();
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                diagnostics.push(RepoDiagnostic {
                    repo_id: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                    name: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                    file_path: file_path.clone(),
                    enabled: false,
                    status: "corrupted".to_string(),
                    latency_ms: None,
                    http_status: None,
                    repomd_valid: false,
                    error_message: Some(format!("Cannot read metadata: {e}")),
                    tested_url: None,
                    is_empty_file: false,
                    is_corrupted_syntax: true,
                });
                continue;
            }
        };

        if metadata.len() == 0 {
            let filename = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            diagnostics.push(RepoDiagnostic {
                repo_id: filename.clone(),
                name: filename,
                file_path: file_path.clone(),
                enabled: false,
                status: "empty".to_string(),
                latency_ms: None,
                http_status: None,
                repomd_valid: false,
                error_message: Some("Repository file is 0 bytes (empty and corrupted)".to_string()),
                tested_url: None,
                is_empty_file: true,
                is_corrupted_syntax: true,
            });
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                diagnostics.push(RepoDiagnostic {
                    repo_id: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                    name: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                    file_path: file_path.clone(),
                    enabled: false,
                    status: "corrupted".to_string(),
                    latency_ms: None,
                    http_status: None,
                    repomd_valid: false,
                    error_message: Some(format!("Could not read file content: {e}")),
                    tested_url: None,
                    is_empty_file: false,
                    is_corrupted_syntax: true,
                });
                continue;
            }
        };

        let mut current_id = String::new();
        let mut current_name = String::new();
        let mut current_baseurl = String::new();
        let mut current_enabled = true;
        let mut current_metalink: Option<String> = None;
        let mut current_mirrorlist: Option<String> = None;
        let mut in_section = false;
        let mut sections_found = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                if in_section && !current_id.is_empty() {
                    sections_found += 1;
                    let cand = if !current_baseurl.is_empty() {
                        Some(substitute_repo_vars(&current_baseurl, &releasever, &basearch))
                    } else if let Some(ref ml) = current_mirrorlist {
                        Some(substitute_repo_vars(ml, &releasever, &basearch))
                    } else if let Some(ref mt) = current_metalink {
                        Some(substitute_repo_vars(mt, &releasever, &basearch))
                    } else {
                        None
                    };

                    pending_probes.push(PendingProbe {
                        repo_id: current_id.clone(),
                        name: if current_name.is_empty() { current_id.clone() } else { current_name.clone() },
                        file_path: file_path.clone(),
                        enabled: current_enabled,
                        candidate_url: cand,
                    });
                }
                current_id = line[1..line.len() - 1].to_string();
                current_name = String::new();
                current_baseurl = String::new();
                current_enabled = true;
                current_metalink = None;
                current_mirrorlist = None;
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
                        _ => {}
                    }
                }
            }
        }

        if in_section && !current_id.is_empty() {
            sections_found += 1;
            let cand = if !current_baseurl.is_empty() {
                Some(substitute_repo_vars(&current_baseurl, &releasever, &basearch))
            } else if let Some(ref ml) = current_mirrorlist {
                Some(substitute_repo_vars(ml, &releasever, &basearch))
            } else if let Some(ref mt) = current_metalink {
                Some(substitute_repo_vars(mt, &releasever, &basearch))
            } else {
                None
            };

            pending_probes.push(PendingProbe {
                repo_id: current_id.clone(),
                name: if current_name.is_empty() { current_id.clone() } else { current_name.clone() },
                file_path: file_path.clone(),
                enabled: current_enabled,
                candidate_url: cand,
            });
        }

        if sections_found == 0 {
            let filename = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            diagnostics.push(RepoDiagnostic {
                repo_id: filename.clone(),
                name: filename,
                file_path: file_path.clone(),
                enabled: false,
                status: "corrupted".to_string(),
                latency_ms: None,
                http_status: None,
                repomd_valid: false,
                error_message: Some("No valid [repo] section headers found in file".to_string()),
                tested_url: None,
                is_empty_file: false,
                is_corrupted_syntax: true,
            });
        }
    }

    // Concurrently test reachability and latency for enabled repos
    let mut probe_handles = Vec::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(4500))
        .build()
        .unwrap_or_default();

    for probe in pending_probes {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            if !probe.enabled {
                return RepoDiagnostic {
                    repo_id: probe.repo_id,
                    name: probe.name,
                    file_path: probe.file_path,
                    enabled: false,
                    status: "disabled".to_string(),
                    latency_ms: None,
                    http_status: None,
                    repomd_valid: false,
                    error_message: None,
                    tested_url: probe.candidate_url,
                    is_empty_file: false,
                    is_corrupted_syntax: false,
                };
            }

            let Some(raw_url) = probe.candidate_url else {
                return RepoDiagnostic {
                    repo_id: probe.repo_id,
                    name: probe.name,
                    file_path: probe.file_path,
                    enabled: true,
                    status: "corrupted".to_string(),
                    latency_ms: None,
                    http_status: None,
                    repomd_valid: false,
                    error_message: Some("No baseurl, metalink, or mirrorlist provided".to_string()),
                    tested_url: None,
                    is_empty_file: false,
                    is_corrupted_syntax: true,
                };
            };

            let probe_url = get_repo_probe_url(&raw_url);
            let start = std::time::Instant::now();
            let mut res = client_clone.get(&probe_url)
                .header("User-Agent", "dnf/5")
                .send()
                .await;

            // If repomd.xml returned 404, fallback to checking raw_url in case server serves root
            if let Ok(ref resp) = res {
                if resp.status().as_u16() == 404 && probe_url != raw_url {
                    if let Ok(fallback_resp) = client_clone.get(&raw_url)
                        .header("User-Agent", "dnf/5")
                        .send()
                        .await 
                    {
                        if fallback_resp.status().is_success() || fallback_resp.status().is_redirection() {
                            res = Ok(fallback_resp);
                        }
                    }
                }
            }

            let elapsed = start.elapsed().as_millis() as u32;

            match res {
                Ok(resp) => {
                    let status_code = resp.status().as_u16();
                    if resp.status().is_success() || resp.status().is_redirection() {
                        let status_str = if elapsed > 1500 { "slow" } else { "healthy" };
                        let err_msg = if elapsed > 1500 {
                            Some(format!("Slow response time ({}ms > 1500ms)", elapsed))
                        } else {
                            None
                        };

                        RepoDiagnostic {
                            repo_id: probe.repo_id,
                            name: probe.name,
                            file_path: probe.file_path,
                            enabled: true,
                            status: status_str.to_string(),
                            latency_ms: Some(elapsed),
                            http_status: Some(status_code),
                            repomd_valid: true,
                            error_message: err_msg,
                            tested_url: Some(probe_url),
                            is_empty_file: false,
                            is_corrupted_syntax: false,
                        }
                    } else if status_code == 404 {
                        RepoDiagnostic {
                            repo_id: probe.repo_id,
                            name: probe.name,
                            file_path: probe.file_path,
                            enabled: true,
                            status: "unreachable".to_string(),
                            latency_ms: Some(elapsed),
                            http_status: Some(status_code),
                            repomd_valid: false,
                            error_message: Some(format!("HTTP 404 Not Found — repository metadata is defunct, removed, or has invalid version path")),
                            tested_url: Some(probe_url),
                            is_empty_file: false,
                            is_corrupted_syntax: false,
                        }
                    } else {
                        RepoDiagnostic {
                            repo_id: probe.repo_id,
                            name: probe.name,
                            file_path: probe.file_path,
                            enabled: true,
                            status: "unreachable".to_string(),
                            latency_ms: Some(elapsed),
                            http_status: Some(status_code),
                            repomd_valid: false,
                            error_message: Some(format!("HTTP {} error from server", status_code)),
                            tested_url: Some(probe_url),
                            is_empty_file: false,
                            is_corrupted_syntax: false,
                        }
                    }
                }
                Err(err) => {
                    let err_str = if err.is_timeout() {
                        "Connection timed out (> 4.5s) — mirror server is unreachable or offline".to_string()
                    } else {
                        format!("Network connection failed: {err}")
                    };

                    RepoDiagnostic {
                        repo_id: probe.repo_id,
                        name: probe.name,
                        file_path: probe.file_path,
                        enabled: true,
                        status: "unreachable".to_string(),
                        latency_ms: None,
                        http_status: None,
                        repomd_valid: false,
                        error_message: Some(err_str),
                        tested_url: Some(probe_url),
                        is_empty_file: false,
                        is_corrupted_syntax: false,
                    }
                }
            }
        });
        probe_handles.push(handle);
    }

    for handle in probe_handles {
        if let Ok(diag) = handle.await {
            diagnostics.push(diag);
        }
    }

    diagnostics.sort_by(|a, b| {
        let order = |s: &str| match s {
            "unreachable" => 0,
            "corrupted" => 1,
            "empty" => 2,
            "slow" => 3,
            "healthy" => 4,
            _ => 5,
        };
        order(&a.status).cmp(&order(&b.status)).then(a.repo_id.cmp(&b.repo_id))
    });

    Ok(diagnostics)
}

#[tauri::command]
pub async fn delete_repo(repo_id: String, file_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    let sections: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with('[') && l.ends_with(']'))
        .collect();

    if sections.is_empty() || sections.len() <= 1 {
        let output = Command::new("pkexec")
            .args(["rm", "-f", &file_path])
            .output()
            .await
            .map_err(|e| format!("Failed to delete repo file: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("Failed to delete repo file: {err}"));
        }
        log_to_file("INFO", &format!("Deleted repo file: {}", file_path));
        return Ok(());
    }

    let mut new_lines = Vec::new();
    let mut skipping = false;
    let target_header = format!("[{}]", repo_id);

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == target_header {
                skipping = true;
            } else {
                skipping = false;
            }
        }

        if !skipping {
            new_lines.push(line);
        }
    }

    let final_content = new_lines.join("\n") + "\n";
    crate::utils::privilege::write_file_as_root(&file_path, &final_content).await?;
    log_to_file("INFO", &format!("Deleted repo section [{}] from {}", repo_id, file_path));
    Ok(())
}

#[tauri::command]
pub async fn clean_repo_cache(repo_id: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "clean", "expire-cache", "--repo", &repo_id])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("dnf clean failed: {stderr}"));
    }

    Ok(stdout)
}

#[tauri::command]
pub async fn bulk_disable_repos(repo_targets: Vec<(String, String)>) -> Result<usize, String> {
    let mut count = 0;
    for (repo_id, file_path) in repo_targets {
        if toggle_repo(repo_id, false, file_path).await.is_ok() {
            count += 1;
        }
    }
    Ok(count)
}
