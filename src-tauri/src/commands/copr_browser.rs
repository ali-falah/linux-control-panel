use serde::{Deserialize, Serialize};

use crate::log_to_file;

/// Forward a validated Copr/PPA operation to the installed helper. The
/// helper re-validates the repository and selects dnf or
/// add-apt-repository authoritatively.
async fn run_copr_named_action(action: &str, params: &[String]) -> Result<(), String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 1);
    args.push(action.to_string());
    args.extend_from_slice(params);
    let output = crate::utils::privilege::run_privileged_with_stdin(
        helper,
        &args,
        &crate::utils::privilege::RootAuth::Polkit,
        "",
    )
    .await?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            return Err(format!(
                "The administrator helper failed while managing the Copr repository (status {:?}).",
                output.status.code()
            ));
        }
        return Err(detail);
    }
    Ok(())
}

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

/// Enable a Copr or PPA repo
#[tauri::command]
pub async fn enable_copr(repo: String) -> Result<String, String> {
    crate::utils::root_policy::copr_repo_valid(&repo)?;
    run_copr_named_action(
        "copr_manage",
        &[String::from("enable"), repo.clone()],
    )
    .await?;
    log_to_file("INFO", &format!("Enabled Copr repo: {repo}"));
    Ok(format!("Copr repository '{repo}' enabled."))
}

/// Disable a Copr or PPA repo
#[tauri::command]
pub async fn disable_copr(repo: String) -> Result<String, String> {
    crate::utils::root_policy::copr_repo_valid(&repo)?;
    run_copr_named_action(
        "copr_manage",
        &[String::from("disable"), repo.clone()],
    )
    .await?;
    log_to_file("INFO", &format!("Disabled Copr repo: {repo}"));
    Ok(format!("Copr repository '{repo}' disabled."))
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

/// List all installed COPR or PPA repositories
#[tauri::command]
pub async fn list_system_coprs() -> Result<Vec<SystemCoprRepo>, String> {
    let repos = crate::commands::repo_manager::list_repos().await?;
    let mut coprs = Vec::new();

    for repo in repos {
        let is_copr = repo.id.to_lowercase().contains("copr")
            || repo.file_path.to_lowercase().contains("copr")
            || repo.baseurl.to_lowercase().contains("copr")
            || repo.baseurl.to_lowercase().contains("fedorainfracloud.org")
            || repo.id.to_lowercase().contains("ppa")
            || repo.file_path.to_lowercase().contains("ppa")
            || repo.baseurl.to_lowercase().contains("launchpad");

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
