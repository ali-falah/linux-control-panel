use crate::utils::privilege::{ensure_repo_file_path, ensure_safe_operand};
use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};
use crate::utils::root_policy::{count_enabled_repos_in_content, repo_url_https_only};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

/// Parse all .repo files from /etc/yum.repos.d/ and .list/.sources from /etc/apt/
#[tauri::command]
pub async fn list_repos() -> Result<Vec<RepoEntry>, String> {
    let mut entries = Vec::new();

    let repo_dir = PathBuf::from("/etc/yum.repos.d");
    if repo_dir.exists() {
        if let Ok(dir_entries) = fs::read_dir(&repo_dir) {
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
        }
    }

    parse_apt_repos(&mut entries);

    Ok(entries)
}

fn parse_apt_repos(entries: &mut Vec<RepoEntry>) {
    let sources_list = PathBuf::from("/etc/apt/sources.list");
    if sources_list.exists() {
        parse_apt_list_file(&sources_list, entries);
    }

    let list_d = PathBuf::from("/etc/apt/sources.list.d");
    if list_d.exists() {
        if let Ok(dir) = fs::read_dir(&list_d) {
            for entry in dir.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "list" {
                    parse_apt_list_file(&path, entries);
                } else if ext == "sources" {
                    parse_apt_sources_file(&path, entries);
                }
            }
        }
    }
}

fn parse_apt_list_file(path: &Path, entries: &mut Vec<RepoEntry>) {
    let file_path = path.to_string_lossy().to_string();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("apt").to_string();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_commented = trimmed.starts_with('#');
        let unhashed = trimmed.trim_start_matches('#').trim();

        if unhashed.starts_with("deb ") || unhashed.starts_with("deb-src ") {
            let parts: Vec<&str> = unhashed.split_whitespace().collect();
            if parts.len() >= 3 {
                let (url_idx, suite_idx) = if parts.len() > 1 && parts[1].starts_with('[') {
                    let mut i = 1;
                    while i < parts.len() && !parts[i].ends_with(']') {
                        i += 1;
                    }
                    (i + 1, i + 2)
                } else {
                    (1, 2)
                };

                let baseurl = parts.get(url_idx).unwrap_or(&"").to_string();
                let suite = parts.get(suite_idx).unwrap_or(&"").to_string();
                let components = if suite_idx + 1 < parts.len() {
                    parts[suite_idx + 1..].join(" ")
                } else {
                    String::new()
                };

                let id = format!("{}-{}-{}", filename, suite, idx);
                let name = if components.is_empty() {
                    format!("APT: {} {}", baseurl, suite)
                } else {
                    format!("APT: {} ({} {})", baseurl, suite, components)
                };

                entries.push(RepoEntry {
                    id,
                    name,
                    baseurl,
                    enabled: !is_commented,
                    file_path: file_path.clone(),
                    metalink: None,
                    mirrorlist: None,
                    gpgcheck: true,
                    priority: None,
                });
            }
        }
    }
}

fn parse_apt_sources_file(path: &Path, entries: &mut Vec<RepoEntry>) {
    let file_path = path.to_string_lossy().to_string();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("sources").to_string();

    let mut uris = String::new();
    let mut suites = String::new();
    let mut enabled = true;
    let mut stanza_idx = 0;

    let flush_stanza = |entries: &mut Vec<RepoEntry>, uris: &str, suites: &str, enabled: bool, idx: usize| {
        if !uris.is_empty() {
            let id = format!("{}-stanza-{}", filename, idx);
            let name = format!("APT deb822: {} ({})", uris, suites);
            entries.push(RepoEntry {
                id,
                name,
                baseurl: uris.to_string(),
                enabled,
                file_path: file_path.clone(),
                metalink: None,
                mirrorlist: None,
                gpgcheck: true,
                priority: None,
            });
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_stanza(entries, &uris, &suites, enabled, stanza_idx);
            stanza_idx += 1;
            uris.clear();
            suites.clear();
            enabled = true;
            continue;
        }

        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            match key.as_str() {
                "uris" => uris = val.to_string(),
                "suites" => suites = val.to_string(),
                "enabled" => enabled = val.to_lowercase() != "no",
                _ => {}
            }
        }
    }

    flush_stanza(entries, &uris, &suites, enabled, stanza_idx);
}

/// Pure check: given all repo file contents as (path, content) pairs,
/// determine if disabling `target_repo_id` in `target_file` would leave
/// zero enabled repos system-wide.
///
/// Checks the **resulting state after the toggle would apply**: if the
/// target repo is currently enabled=1, subtracting 1 from the total
/// enabled count across all files gives the post-toggle count. If that
/// is zero, confirmation is required.
///
/// Accounts for repos in ALL files, not just the one being toggled —
/// "zero enabled repos system-wide" means checking across every repo file.
pub fn would_disable_last_enabled_repo(
    all_contents: &[(&str, &str)],
    target_file: &str,
    target_repo_id: &str,
) -> bool {
    let mut total_enabled = 0usize;
    let mut target_enabled = false;

    for (path, content) in all_contents {
        total_enabled += count_enabled_repos_in_content(content);
        if *path == target_file && is_repo_enabled_in_content(content, target_repo_id) {
            target_enabled = true;
        }
    }

    target_enabled && total_enabled.saturating_sub(1) == 0
}

/// Check if a specific repo section in `.repo` content has `enabled=1`.
/// Tracks section boundaries so `enabled=1` in a different section
/// is not attributed to the target repo.
fn is_repo_enabled_in_content(content: &str, repo_id: &str) -> bool {
    let target_header = format!("[{}]", repo_id);
    let mut in_target = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_target = trimmed == target_header;
            continue;
        }
        if in_target {
            if let Some((key, value)) = trimmed.split_once('=') {
                if key.trim() == "enabled" && value.trim() == "1" {
                    return true;
                }
            }
        }
    }
    false
}

/// Dispatch a named `lcp-admin` repo action via PolicyKit privilege escalation.
/// The helper enforces path confinement and argument validation — the app
/// never passes raw pkexec commands.
async fn run_repo_named_action(action: &str, params: &[String]) -> Result<(), String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 1);
    args.push(action.to_string());
    args.extend_from_slice(params);
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, "").await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

/// Enable or disable a specific repo in its file (.repo, .list, or .sources).
///
/// Dispatches to `repo_toggle_enable` or `repo_toggle_disable` named actions
/// instead of raw pkexec + sed. For disable operations, checks the
/// zero-enabled-repos lockout: if disabling would leave zero enabled repos
/// system-wide, requires explicit confirmation.
#[tauri::command]
pub async fn toggle_repo(repo_id: String, enabled: bool, file_path: String, confirmed: bool) -> Result<String, String> {
    // The path comes from the frontend, so it is checked against the directories
    // the package manager uses before the file is touched with root rights.
    ensure_repo_file_path(&file_path)?;

    // Zero-enabled-repos lockout for disable operations: if disabling this repo
    // would leave zero enabled repos system-wide, require confirmation.
    if !enabled && !confirmed {
        let all_contents = gather_all_repo_contents();
        let all_contents_refs: Vec<(&str, &str)> = all_contents.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
        if would_disable_last_enabled_repo(&all_contents_refs, &file_path, &repo_id) {
            return Ok(
                "CONFIRMATION_REQUIRED: Disabling this repository would leave no enabled \
                 repositories system-wide. Future package updates would be impossible. \
                 Do you want to proceed?"
                    .to_string(),
            );
        }
    }

    let action = if enabled { "repo_toggle_enable" } else { "repo_toggle_disable" };
    run_repo_named_action(
        action,
        &[String::from(&file_path), String::from(&repo_id)],
    )
    .await
    .map_err(|e| format!("Failed to toggle repo: {e}"))?;

    log_to_file("INFO", &format!("Toggled repo {repo_id} enabled={enabled}"));
    Ok(String::new())
}

/// Gather the contents of all repository files for lockout checks.
/// Reads every .repo, .list, and .sources file under the package manager dirs.
fn gather_all_repo_contents() -> Vec<(String, String)> {
    let mut contents = Vec::new();
    // Same locations the repo scanner reads (list_repos): /etc/yum.repos.d/,
    // /etc/apt/sources.list.d/, plus the standalone classic
    // /etc/apt/sources.list — otherwise the lockout check could miss enabled
    // repos defined there.
    let dirs = ["/etc/yum.repos.d", "/etc/apt/sources.list.d"];
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "repo" | "list" | "sources") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        contents.push((path.to_string_lossy().to_string(), content));
                    }
                }
            }
        }
    }
    // The classic standalone sources.list is not inside .list.d.
    if let Ok(content) = std::fs::read_to_string("/etc/apt/sources.list") {
        contents.push(("/etc/apt/sources.list".to_string(), content));
    }
    contents
}

/// Add a repo by URL using dnf config-manager or add-apt-repository.
///
/// https-only hard refusal: any scheme other than `https://` is rejected.
/// `confirmed` gates the destructive trust decision: without it, a distinct
/// warning result is returned (not a hard error) so the frontend can show a
/// confirmation dialog. With `confirmed=true`, the repo is added and the
/// resulting file is inspected for unsigned indicators (`gpgcheck=0`,
/// `repo_gpgcheck=0`); an unsigned warning is surfaced but never blocks or
/// rewrites the file.
#[tauri::command]
pub async fn add_repo(url: String, confirmed: bool) -> Result<String, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    // https-only hard refusal: HTTP, FTP, file://, CDROM and any other scheme
    // are refused — an unsigned or non-TLS source lets its maintainer run code
    // as root on every future dnf/apt operation.
    ensure_safe_operand(&url, "repository URL")?;
    repo_url_https_only(&url)?;

    // Confirmation gate: without it, return a distinct warning result so the
    // frontend can show a blocking confirmation dialog.
    if !confirmed {
        return Ok(
            "CONFIRMATION_REQUIRED: Adding a repository lets its maintainer run code as root \
             on every future update — do you trust this source?"
                .to_string(),
        );
    }

    if binary_exists("dnf").await {
        run_repo_named_action(
            "repo_add",
            &[String::from(&url)],
        )
        .await
        .map_err(|e| format!("Failed to add repo: {e}"))?;

        log_to_file("INFO", &format!("Added repo: {url}"));

        // Post-add unsigned-repo detection: inspect repo files for disabled
        // signature verification. Surface a warning but never block or rewrite.
        let unsigned_warning = if detect_unsigned_repos() {
            " WARNING: This repository does not use GPG signature verification. \
             Future updates from this source may not be authenticated."
        } else {
            ""
        };

        return Ok(format!("{unsigned_warning}"));
    }

    if binary_exists("add-apt-repository").await {
        run_repo_named_action(
            "repo_add",
            &[String::from(&url)],
        )
        .await
        .map_err(|e| format!("Failed to add repo: {e}"))?;

        log_to_file("INFO", &format!("Added repo: {url}"));

        let unsigned_warning = if detect_unsigned_repos() {
            " WARNING: This repository does not use GPG signature verification. \
             Future updates from this source may not be authenticated."
        } else {
            ""
        };

        return Ok(format!("{unsigned_warning}"));
    }

    Err("Neither dnf nor add-apt-repository is available on this system".to_string())
}

/// Scan repository files for unsigned indicators (`gpgcheck=0`,
/// `repo_gpgcheck=0`). Returns `true` if any unsigned repo is found.
///
/// Checks /etc/yum.repos.d/*.repo files. Used after `add_repo` to surface a
/// persistent unsigned-repository warning in the repo list — warning only,
/// never blocking or rewriting.
fn detect_unsigned_repos() -> bool {
    if let Ok(dir) = std::fs::read_dir("/etc/yum.repos.d") {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("repo") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if repo_has_unsigned_indicators(&content) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check whether a single repo file's content contains unsigned indicators
/// (`gpgcheck=0` or `repo_gpgcheck=0`). Pure function — unit-testable without
/// touching the filesystem.
fn repo_has_unsigned_indicators(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once('=') {
            if (key.trim() == "gpgcheck" || key.trim() == "repo_gpgcheck")
                && value.trim() == "0"
            {
                return true;
            }
        }
    }
    false
}

/// Run dnf makecache or apt-get update to refresh metadata
#[tauri::command]
pub async fn run_makecache() -> Result<String, String> {
    run_repo_named_action("repo_makecache", &[])
        .await
        .map_err(|e| format!("makecache failed: {e}"))?;

    log_to_file("INFO", "Ran dnf makecache / apt-get update");
    Ok(String::new())
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
    // the path comes from the frontend, so it is confined to the package
    // managers' own directories before anything reads or writes it
    ensure_repo_file_path(&file_path)?;
    // repository ids and display names are active inside root-owned INI
    // files: pin their shapes before anything else runs
    if repo_id.is_empty()
        || repo_id.len() > 64
        || !repo_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(format!("'{repo_id}' is not a valid repository identifier"));
    }
    ensure_safe_operand(&name, "repository name")?;

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

    // The content policy (F-2): path validation alone lets anything reconfigure
    // the package system's sources with root authority — the *body* of the
    // repository file is therefore validated here, in full, before the
    // privileged write. Disabling signature verification, local-file sources
    // and directives the panel never writes are rejected with a line-local
    // message instead of replaced.
    crate::utils::privilege::ensure_repo_file_content(&file_path, &final_content)?;

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

/// Pure check: given all repo file contents as (path, content) pairs,
/// determine if deleting `target_file` would leave zero enabled repos
/// system-wide.
///
/// Counts all enabled repos across all files, subtracts any enabled repos
/// that exist only in `target_file`. If the result is zero, confirmation is
/// required before deleting.
pub fn would_delete_last_enabled_repo(
    all_contents: &[(&str, &str)],
    target_file: &str,
) -> bool {
    let mut total_enabled = 0usize;
    let mut target_enabled = 0usize;

    for (path, content) in all_contents {
        let file_enabled = count_enabled_repos_in_content(content);
        total_enabled += file_enabled;
        if *path == target_file {
            target_enabled = file_enabled;
        }
    }

    total_enabled.saturating_sub(target_enabled) == 0 && total_enabled > 0
}

#[tauri::command]
pub async fn delete_repo(repo_id: String, file_path: String, confirmed: bool) -> Result<String, String> {
    // The path comes from the frontend, so it is checked against the directories
    // the package manager uses before the file is deleted with root rights.
    ensure_repo_file_path(&file_path)?;
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Ok(String::new());
    }

    // Zero-enabled-repos lockout for delete: if deleting this file would leave
    // zero enabled repos system-wide, require confirmation.
    if !confirmed {
        let all_contents = gather_all_repo_contents();
        let all_contents_refs: Vec<(&str, &str)> = all_contents.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
        if would_delete_last_enabled_repo(&all_contents_refs, &file_path) {
            return Ok(
                "CONFIRMATION_REQUIRED: Deleting this repository would leave no enabled \
                 repositories system-wide. Future package updates would be impossible. \
                 Do you want to proceed?"
                    .to_string(),
            );
        }
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    let sections: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with('[') && l.ends_with(']'))
        .collect();

    if sections.is_empty() || sections.len() <= 1 {
        // Whole-file deletion: dispatch to repo_delete named action.
        run_repo_named_action(
            "repo_delete",
            &[String::from(&file_path)],
        )
        .await
        .map_err(|e| format!("Failed to delete repo file: {e}"))?;
        log_to_file("INFO", &format!("Deleted repo file: {}", file_path));
        return Ok(String::new());
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
    // The content policy (F-2) also guards the delete flow's rewrite: the
    // remaining sections pass the same body validation before the privileged
    // write sees them.
    crate::utils::privilege::ensure_repo_file_content(&file_path, &final_content)?;
    crate::utils::privilege::write_file_as_root(&file_path, &final_content).await?;
    log_to_file("INFO", &format!("Deleted repo section [{}] from {}", repo_id, file_path));
    Ok(String::new())
}

#[tauri::command]
pub async fn clean_repo_cache(repo_id: String) -> Result<String, String> {
    // The repo id is handed to dnf as an operand — a value that starts with a
    // dash would be read as an option by dnf.
    ensure_safe_operand(&repo_id, "repository id")?;

    run_repo_named_action(
        "repo_clean_cache",
        &[String::from(&repo_id)],
    )
    .await
    .map_err(|e| format!("dnf clean failed: {e}"))?;

    Ok(String::new())
}

#[tauri::command]
pub async fn bulk_disable_repos(repo_targets: Vec<(String, String)>) -> Result<usize, String> {
    let mut count = 0;
    for (repo_id, file_path) in repo_targets {
        if toggle_repo(repo_id, false, file_path, false).await.is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod repo_lockout_tests {
    use super::{
        add_repo, detect_unsigned_repos, is_repo_enabled_in_content,
        repo_has_unsigned_indicators, would_delete_last_enabled_repo,
        would_disable_last_enabled_repo,
    };

    #[test]
    fn would_disable_last_enabled_repo_returns_false_when_multiple_enabled() {
        let contents = [
            ("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=1\n"),
            ("/etc/yum.repos.d/updates.repo", "[updates]\nenabled=1\n"),
        ];
        assert!(
            !would_disable_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo", "fedora"),
            "disabling one of two enabled repos must not require confirmation"
        );
    }

    #[test]
    fn would_disable_last_enabled_repo_returns_true_when_single_enabled() {
        let contents = [("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=1\n")];
        assert!(
            would_disable_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo", "fedora"),
            "disabling the only enabled repo must require confirmation"
        );
    }

    #[test]
    fn would_disable_last_enabled_repo_returns_false_when_target_not_enabled() {
        let contents = [
            ("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=0\n"),
            ("/etc/yum.repos.d/updates.repo", "[updates]\nenabled=1\n"),
        ];
        assert!(
            !would_disable_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo", "fedora"),
            "disabling an already-disabled repo must not require confirmation"
        );
    }

    #[test]
    fn would_disable_last_enabled_repo_checks_across_files() {
        // Target file's repo is enabled, but another file also has an enabled repo
        let contents = [
            ("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=1\n"),
            ("/etc/yum.repos.d/updates.repo", "[updates]\nenabled=1\n"),
        ];
        assert!(
            !would_disable_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo", "fedora"),
            "system-wide check across all repo files must find the second enabled repo"
        );
    }

    #[test]
    fn would_disable_last_enabled_repo_empty_content() {
        let contents = [("/etc/yum.repos.d/empty.repo", "")];
        assert!(
            !would_disable_last_enabled_repo(&contents, "/etc/yum.repos.d/empty.repo", "fedora"),
            "empty file: no enabled repos, no confirmation needed"
        );
    }

    #[test]
    fn is_repo_enabled_in_content_detects_enabled() {
        assert!(
            is_repo_enabled_in_content("[fedora]\nenabled=1\n", "fedora"),
            "enabled=1 in target section must be detected"
        );
        assert!(
            !is_repo_enabled_in_content("[fedora]\nenabled=0\n", "fedora"),
            "enabled=0 must not be detected as enabled"
        );
        assert!(
            !is_repo_enabled_in_content("[fedora]\nname=Fedora\n", "fedora"),
            "no enabled key must not be detected as enabled"
        );
    }

    #[test]
    fn is_repo_enabled_in_content_respects_section_boundaries() {
        let contents = "[other]\nenabled=1\n[fedora]\nenabled=0\n";
        assert!(
            !is_repo_enabled_in_content(contents, "fedora"),
            "enabled=1 in a different section must not count for fedora"
        );
    }

    #[test]
    fn repo_has_unsigned_indicators_detects_gpgcheck_zero() {
        assert!(
            repo_has_unsigned_indicators("[repo]\nname=Test\nbaseurl=https://example.com\ngpgcheck=0\n"),
            "gpgcheck=0 must be detected"
        );
        assert!(
            repo_has_unsigned_indicators("[repo]\nname=Test\nbaseurl=https://example.com\nrepo_gpgcheck=0\n"),
            "repo_gpgcheck=0 must be detected"
        );
    }

    #[test]
    fn repo_has_unsigned_indicators_refuses_signed_repos() {
        assert!(
            !repo_has_unsigned_indicators("[repo]\nname=Test\nbaseurl=https://example.com\ngpgcheck=1\n"),
            "gpgcheck=1 must not be detected as unsigned"
        );
        assert!(
            !repo_has_unsigned_indicators("[repo]\nname=Test\nbaseurl=https://example.com\n"),
            "no gpgcheck key must not be detected as unsigned"
        );
        assert!(
            !repo_has_unsigned_indicators(""),
            "empty content must not be detected as unsigned"
        );
        assert!(
            !repo_has_unsigned_indicators("[repo]\nenabled=0\nname=Test\n"),
            "disabled repo without gpgcheck=0 must not be detected"
        );
    }

    #[test]
    fn detect_unsigned_repos_reads_system_dir() {
        // detect_unsigned_repos reads /etc/yum.repos.d; it may return true or
        // false depending on the system state. The important thing is that it
        // doesn't panic.
        let _ = detect_unsigned_repos();
    }

    #[tokio::test]
    async fn add_repo_returns_confirmation_required_when_not_confirmed() {
        let result = add_repo(
            "https://download.fedoraproject.org/pub/fedora/linux/releases/$releasever/Everything/$basearch/os/"
                .to_string(),
            false,
        )
        .await;
        assert!(result.is_ok(), "not confirmed must return Ok, not Err");
        let msg = result.unwrap();
        assert!(
            msg.starts_with("CONFIRMATION_REQUIRED"),
            "not confirmed must return CONFIRMATION_REQUIRED warning: {msg:?}"
        );
    }

    #[tokio::test]
    async fn add_repo_refuses_non_https_url() {
        let result = add_repo("http://download.fedoraproject.org/pub/".to_string(), true).await;
        assert!(
            result.is_err(),
            "http URL must be refused: {result:?}"
        );
    }

    #[tokio::test]
    async fn add_repo_refuses_empty_url() {
        let result = add_repo("".to_string(), true).await;
        assert!(
            result.is_err(),
            "empty URL must be refused: {result:?}"
        );
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_true_when_file_has_only_enabled_repo() {
        let contents = [("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=1\n")];
        assert!(
            would_delete_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo"),
            "deleting the only enabled repo must require confirmation"
        );
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_false_when_other_files_have_enabled_repos() {
        let contents = [
            ("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=1\n"),
            ("/etc/yum.repos.d/updates.repo", "[updates]\nenabled=1\n"),
        ];
        assert!(
            !would_delete_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo"),
            "deleting one of two files with enabled repos must not require confirmation"
        );
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_false_when_target_has_no_enabled_repos() {
        let contents = [
            ("/etc/yum.repos.d/fedora.repo", "[fedora]\nenabled=0\n"),
            ("/etc/yum.repos.d/updates.repo", "[updates]\nenabled=1\n"),
        ];
        assert!(
            !would_delete_last_enabled_repo(&contents, "/etc/yum.repos.d/fedora.repo"),
            "deleting a file with no enabled repos must not require confirmation"
        );
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_false_when_no_enabled_repos() {
        let contents = [("/etc/yum.repos.d/empty.repo", "")];
        assert!(
            !would_delete_last_enabled_repo(&contents, "/etc/yum.repos.d/empty.repo"),
            "no enabled repos at all must not require confirmation"
        );
    }

    #[test]
    fn would_delete_last_enabled_repo_subtracts_target_file_repos() {
        // Target file has 2 enabled repos, another file has 1 enabled repo.
        // Deleting target leaves 1 enabled repo → no confirmation needed.
        let contents = [
            ("/etc/yum.repos.d/multi.repo", "[r1]\nenabled=1\n[r2]\nenabled=1\n"),
            ("/etc/yum.repos.d/single.repo", "[s1]\nenabled=1\n"),
        ];
        assert!(
            !would_delete_last_enabled_repo(&contents, "/etc/yum.repos.d/multi.repo"),
            "deleting multi-enabled file leaves other enabled repos → no confirmation"
        );
    }
}
