use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;

use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoprProject {
    pub full_name: String,
    pub description: String,
    pub chroot_repos: Vec<String>,
    pub contact: Option<String>,
    pub homepage: Option<String>,
    pub instructions: Option<String>,
    pub packages_count: u32,
}

#[derive(Debug, Deserialize)]
struct CoprApiResponse {
    items: Vec<CoprApiItem>,
}

#[derive(Debug, Deserialize)]
struct CoprApiItem {
    full_name: Option<String>,
    description: Option<String>,
    chroot_repos: Option<std::collections::HashMap<String, String>>,
    contact: Option<String>,
    homepage: Option<String>,
    instructions: Option<String>,
    packages_count: Option<u32>,
}

/// Search Copr projects via the official API
#[tauri::command]
pub async fn search_copr(query: String) -> Result<Vec<CoprProject>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let url = format!(
        "https://copr.fedorainfracloud.org/api_3/project/search?query={}",
        urlencoding(&query)
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Copr API request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Copr API returned status: {}", response.status()));
    }

    let api_resp: CoprApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Copr API response: {e}"))?;

    let projects = api_resp
        .items
        .into_iter()
        .map(|item| CoprProject {
            full_name: item.full_name.unwrap_or_default(),
            description: item.description.unwrap_or_default(),
            chroot_repos: item
                .chroot_repos
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
            contact: item.contact,
            homepage: item.homepage,
            instructions: item.instructions,
            packages_count: item.packages_count.unwrap_or(0),
        })
        .collect();

    Ok(projects)
}

/// Enable a Copr repo via dnf copr enable
#[tauri::command]
pub async fn enable_copr(repo: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "copr", "enable", &repo, "-y"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("enable_copr {repo} failed: {stderr}"));
        return Err(format!("Failed to enable Copr repo: {stderr}"));
    }

    log_to_file("INFO", &format!("Enabled Copr repo: {repo}"));
    Ok(stdout)
}

/// Disable a Copr repo via dnf copr disable
#[tauri::command]
pub async fn disable_copr(repo: String) -> Result<String, String> {
    if !binary_exists("dnf").await {
        return Err("dnf is not available on this system".to_string());
    }

    let output = Command::new("pkexec")
        .args(["/usr/bin/dnf", "copr", "disable", &repo, "-y"])
        .output()
        .await
        .map_err(|e| format!("Failed to run pkexec: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        log_to_file("ERROR", &format!("disable_copr {repo} failed: {stderr}"));
        return Err(format!("Failed to disable Copr repo: {stderr}"));
    }

    log_to_file("INFO", &format!("Disabled Copr repo: {repo}"));
    Ok(stdout)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCoprRepo {
    pub copr_name: String,
    pub repo_id: String,
    pub name: String,
    pub enabled: bool,
    pub file_path: String,
    pub baseurl: String,
}

/// List all installed COPR repositories from /etc/yum.repos.d/*.repo
#[tauri::command]
pub async fn list_system_coprs() -> Result<Vec<SystemCoprRepo>, String> {
    let repos = crate::commands::repo_manager::list_repos().await?;
    let mut coprs = Vec::new();

    for repo in repos {
        let is_copr = repo.id.to_lowercase().contains("copr")
            || repo.file_path.to_lowercase().contains("copr")
            || repo.baseurl.to_lowercase().contains("copr")
            || repo.baseurl.to_lowercase().contains("fedorainfracloud.org");

        if !is_copr {
            continue;
        }

        let copr_name = extract_copr_name(&repo.id, &repo.baseurl, &repo.file_path);

        coprs.push(SystemCoprRepo {
            copr_name,
            repo_id: repo.id,
            name: repo.name,
            enabled: repo.enabled,
            file_path: repo.file_path,
            baseurl: repo.baseurl,
        });
    }

    Ok(coprs)
}

fn extract_copr_name(id: &str, baseurl: &str, file_path: &str) -> String {
    // 1. Try baseurl pattern: .../results/owner/project/...
    if let Some(pos) = baseurl.find("/results/") {
        let rest = &baseurl[pos + 9..];
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            return format!("{}/{}", parts[0], parts[1]);
        }
    }

    // 2. Try ID pattern: copr:copr.fedorainfracloud.org:owner:project or _copr:owner:project
    let id_parts: Vec<&str> = id.split(':').collect();
    if id_parts.len() >= 4 {
        return format!("{}/{}", id_parts[2], id_parts[3]);
    } else if id_parts.len() == 3 && id_parts[0].contains("copr") {
        return format!("{}/{}", id_parts[1], id_parts[2]);
    }

    // 3. Try filename pattern: _copr:copr.fedorainfracloud.org:owner:project.repo
    let filename = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if filename.contains("copr") {
        let clean_name = filename.trim_end_matches(".repo");
        let parts: Vec<&str> = clean_name.split(':').collect();
        if parts.len() >= 4 {
            return format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        }
    }

    id.to_string()
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['+'],
            c => {
                let encoded = format!("%{:02X}", c as u32);
                encoded.chars().collect()
            }
        })
        .collect()
}
