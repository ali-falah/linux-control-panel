use crate::utils::privilege::{ensure_repo_file_path, ensure_safe_operand};
use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};
use crate::utils::root_policy::{
    is_system_owned_repo_path, repo_id_is_protected, repo_override_option_valid,
    repo_url_https_only, REPO_READ_ONLY_DIRS, USER_REPO_DIR,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{binary_exists, log_to_file};

/// Shown when the host has no dnf5. Plain language only: the user is not
/// expected to read stderr or exit codes.
const DNF4_UNSUPPORTED_MESSAGE: &str =
    "This system uses an older package manager (dnf4) that isn't supported — repository management is unavailable here.";

/// dnf5's config-manager override file. Only this file is touched by
/// `config-manager setopt/unsetopt`; other override files are distribution
/// owned and are never removed here.
const DNF5_CONFIG_MANAGER_OVERRIDE: &str = "/etc/dnf/repos.override.d/99-config_manager.repo";

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
    /// True when dnf5 reports the config file under a package-owned directory
    /// (`/usr/share/dnf5/repos.d`, `/etc/distro.repos.d`). Such a repo is
    /// toggle-only: the panel never writes its file.
    pub system_owned: bool,
}

/// One entry of `dnf5 repo info --all --json`. Fields are `serde(default)`
/// so a missing optional key cannot abort the whole listing.
#[derive(Debug, Clone, Deserialize)]
struct Dnf5RepoInfo {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    priority: Option<u32>,
    #[serde(default)]
    base_url: Vec<String>,
    #[serde(default)]
    metalink: String,
    #[serde(default)]
    mirrorlist: String,
    #[serde(default)]
    pkg_gpgcheck: bool,
    #[serde(default)]
    repo_gpgcheck: bool,
    #[serde(default)]
    repo_file_path: String,
}

/// One entry of `dnf5 repo list --all --json` — the metadata-free view.
#[derive(Debug, Clone, Deserialize)]
struct Dnf5RepoListEntry {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_enabled: bool,
}

impl Dnf5RepoInfo {
    fn to_entry(&self) -> RepoEntry {
        RepoEntry {
            id: self.id.clone(),
            name: if self.name.is_empty() { self.id.clone() } else { self.name.clone() },
            baseurl: self.base_url.join(" "),
            enabled: self.is_enabled,
            file_path: self.repo_file_path.clone(),
            metalink: empty_to_none(&self.metalink),
            mirrorlist: empty_to_none(&self.mirrorlist),
            gpgcheck: self.pkg_gpgcheck,
            priority: self.priority,
            system_owned: is_system_owned_repo_path(&self.repo_file_path),
        }
    }
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

/// Parse dnf5's JSON output. Pure, so both layouts can be tested from
/// fixtures without running dnf5.
fn parse_dnf5_repos(json: &str) -> Result<Vec<Dnf5RepoInfo>, String> {
    serde_json::from_str(json)
        .map_err(|e| format!("dnf5 returned unreadable repository data: {e}"))
}

/// Run one read-only `dnf5 ... --json` query. `-C` keeps it offline and
/// fast: it never refreshes the metadata cache.
async fn dnf5_json(args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("dnf5")
        .arg("-C")
        .args(args)
        .arg("--json")
        .output()
        .await
        .map_err(|e| format!("dnf5 could not be started: {e}"))?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "dnf5 could not report repositories".to_string()
        } else {
            format!("dnf5 could not report repositories: {detail}")
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn dnf5_repo_info_json(query: &[&str]) -> Result<String, String> {
    let mut args = vec!["repo", "info"];
    args.extend_from_slice(query);
    dnf5_json(&args).await
}

/// `dnf5 repo list --all --json` — ids, names and the *effective* enabled
/// state (overrides applied), without loading any metadata. Unlike
/// `repo info`, this works for a repo that has never been cached.
async fn dnf5_repo_list_json() -> Result<String, String> {
    dnf5_json(&["repo", "list", "--all"]).await
}

fn parse_dnf5_repo_list(json: &str) -> Result<Vec<Dnf5RepoListEntry>, String> {
    serde_json::from_str(json)
        .map_err(|e| format!("dnf5 returned unreadable repository data: {e}"))
}

/// Every repo dnf5 knows about, including disabled ones. `repo info` is the
/// preferred source: it also reports the config file dnf5 really used.
///
/// `repo info` needs cached metadata for enabled repositories; a repository
/// that was just added has none yet, which would fail the whole listing. In
/// that case the list falls back to `repo list --all` for the authoritative
/// ids/state and reads the remaining display fields from dnf's repo files.
async fn load_dnf5_repos() -> Result<Vec<Dnf5RepoInfo>, String> {
    if !binary_exists("dnf5").await {
        return Err(DNF4_UNSUPPORTED_MESSAGE.to_string());
    }

    match dnf5_repo_info_json(&["--all"])
        .await
        .and_then(|json| parse_dnf5_repos(&json))
    {
        Ok(repos) => Ok(repos),
        Err(info_error) => {
            log_to_file(
                "WARN",
                &format!(
                    "dnf5 repo info unavailable ({info_error}); \
                     falling back to repo list plus config files"
                ),
            );
            load_dnf5_repos_from_list(&info_error).await
        }
    }
}

/// Fallback for `load_dnf5_repos`: dnf5 still decides every id and effective
/// enabled state; the repo files supply the display/edit fields `repo list`
/// does not report.
async fn load_dnf5_repos_from_list(info_error: &str) -> Result<Vec<Dnf5RepoInfo>, String> {
    let json = dnf5_repo_list_json()
        .await
        .map_err(|list_error| format!("{info_error}; dnf5 repo list also failed: {list_error}"))?;
    let listed = parse_dnf5_repo_list(&json)?;

    Ok(listed
        .into_iter()
        .map(|item| {
            let details = repo_details_from_config_files(&item.id);
            Dnf5RepoInfo {
                id: item.id,
                name: details
                    .as_ref()
                    .and_then(|d| d.name.clone())
                    .unwrap_or(item.name),
                is_enabled: item.is_enabled,
                priority: details.as_ref().and_then(|d| d.priority),
                base_url: details
                    .as_ref()
                    .map(|d| d.baseurls.clone())
                    .unwrap_or_default(),
                metalink: details
                    .as_ref()
                    .and_then(|d| d.metalink.clone())
                    .unwrap_or_default(),
                mirrorlist: details
                    .as_ref()
                    .and_then(|d| d.mirrorlist.clone())
                    .unwrap_or_default(),
                pkg_gpgcheck: details.as_ref().map_or(true, |d| d.gpgcheck),
                repo_gpgcheck: false,
                repo_file_path: details
                    .as_ref()
                    .map(|d| d.file_path.clone())
                    .unwrap_or_default(),
            }
        })
        .collect())
}

/// Display fields of one repo section found on disk.
#[derive(Debug, Default)]
struct RepoFileDetails {
    file_path: String,
    name: Option<String>,
    baseurls: Vec<String>,
    metalink: Option<String>,
    mirrorlist: Option<String>,
    gpgcheck: bool,
    priority: Option<u32>,
}

/// Find the `.repo` file that defines `repo_id` and read its section. Used
/// only by the `repo info`-unavailable fallback; ids and state still come
/// from dnf5.
fn repo_details_from_config_files(repo_id: &str) -> Option<RepoFileDetails> {
    for dir in REPO_READ_ONLY_DIRS
        .iter()
        .copied()
        .chain(std::iter::once(USER_REPO_DIR))
    {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("repo") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(mut details) = repo_details_from_content(&content, repo_id) {
                details.file_path = path.to_string_lossy().to_string();
                return Some(details);
            }
        }
    }
    None
}

/// Parse one repo file and return `repo_id`'s section fields.
fn repo_details_from_content(content: &str, repo_id: &str) -> Option<RepoFileDetails> {
    let mut in_target = false;
    let mut found = false;
    let mut details = RepoFileDetails {
        gpgcheck: true,
        ..RepoFileDetails::default()
    };
    let mut baseurls = Vec::new();
    let mut metalink = None;
    let mut mirrorlist = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_target {
                break;
            }
            in_target = &trimmed[1..trimmed.len() - 1] == repo_id;
            found |= in_target;
            continue;
        }
        if !in_target {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim().to_lowercase().as_str() {
            "name" => details.name = Some(value),
            "baseurl" => baseurls.extend(value.split_whitespace().map(str::to_string)),
            "metalink" => metalink = Some(value).filter(|v| !v.is_empty()),
            "mirrorlist" => mirrorlist = Some(value).filter(|v| !v.is_empty()),
            "gpgcheck" => details.gpgcheck = value == "1" || value.eq_ignore_ascii_case("true"),
            "priority" => details.priority = value.parse::<u32>().ok(),
            _ => {}
        }
    }

    if !found {
        return None;
    }
    details.baseurls = baseurls;
    details.metalink = metalink;
    details.mirrorlist = mirrorlist;
    Some(details)
}

async fn load_repo_entries() -> Result<Vec<RepoEntry>, String> {
    Ok(load_dnf5_repos().await?.iter().map(Dnf5RepoInfo::to_entry).collect())
}

/// Effective state of one repo, taken from `repo list`'s report. An empty
/// result means the repo is unknown (dnf5 exits 0 in that case), so it is an
/// error, never a success.
fn effective_state_from_list_json(json: &str, repo_id: &str) -> Result<bool, String> {
    let repos = parse_dnf5_repo_list(json)?;
    let mut matches = repos.iter().filter(|repo| repo.id == repo_id);
    match (matches.next(), matches.next()) {
        (Some(repo), None) => Ok(repo.is_enabled),
        (None, _) => Err(format!("dnf5 does not report a repository with id '{repo_id}'")),
        (Some(_), Some(_)) => Err(format!("dnf5 reports the repository id '{repo_id}' more than once")),
    }
}

/// Verification never depends on metadata: `repo list` reports the
/// authoritative effective state and works for an uncached repository.
async fn repo_effective_enabled(repo_id: &str) -> Result<bool, String> {
    let json = dnf5_repo_list_json().await?;
    effective_state_from_list_json(&json, repo_id)
}

fn ensure_effective_state(repo_id: &str, expected: bool, actual: bool) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    let state = |enabled: bool| if enabled { "enabled" } else { "disabled" };
    Err(format!(
        "dnf5 reports repository '{repo_id}' as {}, but {} was requested",
        state(actual),
        state(expected)
    ))
}

async fn verify_repo_enabled(repo_id: &str, expected: bool) -> Result<(), String> {
    let actual = repo_effective_enabled(repo_id).await?;
    ensure_effective_state(repo_id, expected, actual)
}

/// After a delete, dnf5 must no longer report the id at all.
///
/// `repo list` is used instead of `repo info`: it needs no metadata, so the
/// check works even when the deleted repo had never been cached.
async fn verify_repo_absent(repo_id: &str) -> Result<(), String> {
    let json = dnf5_repo_list_json().await?;
    let repos = parse_dnf5_repo_list(&json)?;
    if repos.iter().any(|repo| repo.id == repo_id) {
        return Err(format!(
            "dnf5 still reports repository '{repo_id}' after deletion"
        ));
    }
    Ok(())
}

/// Parse `dnf5 --dump-repo-config=<id>` output. The tool prints a header line
/// per repository (`======== "id" repository configuration: ========`, which
/// starts with `=` and is skipped), then `key = value` lines. A list or string
/// option that is empty prints as a bare key with no `=`, so it maps to an
/// empty string.
fn parse_repo_config_dump(output: &str) -> std::collections::HashMap<String, String> {
    let mut values = std::collections::HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('=') {
            continue;
        }
        match trimmed.split_once('=') {
            Some((key, value)) => {
                values.insert(key.trim().to_string(), value.trim().to_string());
            }
            None => {
                values.insert(trimmed.to_string(), String::new());
            }
        }
    }
    values
}

/// dnf5's effective configuration for one repository, as key/value pairs.
///
/// `--dump-repo-config` reads configuration only — no metadata download — so
/// it works for an uncached repo and on an offline host. An empty map means
/// dnf5 does not know the id.
async fn dnf5_repo_config_dump(
    repo_id: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let selector = format!("--dump-repo-config={repo_id}");
    let output = tokio::process::Command::new("dnf5")
        .arg(&selector)
        .output()
        .await
        .map_err(|e| format!("dnf5 could not be started: {e}"))?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("dnf5 could not report the configuration of '{repo_id}'")
        } else {
            format!("dnf5 could not report the configuration of '{repo_id}': {detail}")
        });
    }

    Ok(parse_repo_config_dump(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Collapse whitespace so a value dnf5 normalises (multiple base URLs, line
/// wrapping, trailing spaces) compares equal to what the panel wrote.
fn normalize_repo_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compare one saved field with dnf5's fresh report. Returns whether it
/// matches; a mismatch logs both the raw and the normalised forms at DEBUG so
/// a tolerant comparison never hides what dnf5 actually stored.
fn saved_field_matches(
    repo_id: &str,
    field: &str,
    raw_actual: &str,
    raw_expected: &str,
) -> bool {
    let actual = normalize_repo_value(raw_actual);
    let expected = normalize_repo_value(raw_expected);
    if actual == expected {
        return true;
    }
    log_to_file(
        "DEBUG",
        &format!(
            "repo save verification: '{repo_id}' {field} differs \
             (raw actual={raw_actual:?}, raw expected={raw_expected:?}, \
             normalized actual={actual:?}, normalized expected={expected:?})"
        ),
    );
    false
}

/// Split a dnf list value on whitespace and on the commas
/// `--dump-repo-config` puts between base URLs.
fn split_repo_list(value: &str) -> Vec<String> {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Compare a list-valued field (baseurl). dnf5 preserves the order, so a
/// reordering is a real difference; only the separator (spaces in the file,
/// commas in the dump) is normalised.
fn saved_list_field_matches(
    repo_id: &str,
    field: &str,
    raw_actual: &str,
    raw_expected: &str,
) -> bool {
    let actual = split_repo_list(raw_actual);
    let expected = split_repo_list(raw_expected);
    if actual == expected {
        return true;
    }
    log_to_file(
        "DEBUG",
        &format!(
            "repo save verification: '{repo_id}' {field} differs \
             (raw actual={raw_actual:?}, raw expected={raw_expected:?}, \
             parsed actual={actual:?}, parsed expected={expected:?})"
        ),
    );
    false
}

/// Verify the values the panel just wrote against dnf5's own effective
/// configuration.
///
/// `--dump-repo-config` reads configuration only (no metadata download), so
/// the check works for a repo that has never been cached and on an offline
/// host. dnf5 normalises some values — baseurl is printed comma-separated and
/// unset options print as bare keys — so comparison is tolerant while every
/// mismatch logs the raw and normalised forms at DEBUG.
async fn verify_saved_repo_details(
    repo_id: &str,
    name: &str,
    baseurl: &str,
    metalink: &Option<String>,
    mirrorlist: &Option<String>,
    gpgcheck: bool,
    priority: Option<u32>,
) -> Result<(), String> {
    let values = dnf5_repo_config_dump(repo_id).await?;
    if values.is_empty() {
        return Err(format!(
            "dnf5 no longer reports repository '{repo_id}' after saving"
        ));
    }
    let actual = |key: &str| values.get(key).map(String::as_str).unwrap_or("");

    let mut mismatches = Vec::new();
    if !saved_field_matches(repo_id, "name", actual("name"), name) {
        mismatches.push("name");
    }
    if !saved_list_field_matches(repo_id, "baseurl", actual("baseurl"), baseurl) {
        mismatches.push("baseurl");
    }
    if !saved_field_matches(
        repo_id,
        "metalink",
        actual("metalink"),
        metalink.as_deref().unwrap_or(""),
    ) {
        mismatches.push("metalink");
    }
    if !saved_field_matches(
        repo_id,
        "mirrorlist",
        actual("mirrorlist"),
        mirrorlist.as_deref().unwrap_or(""),
    ) {
        mismatches.push("mirrorlist");
    }

    // dnf5 exposes the package signature check under `pkg_gpgcheck`;
    // `gpgcheck` is the deprecated alias the panel still writes.
    let actual_gpgcheck = if values.contains_key("pkg_gpgcheck") {
        actual("pkg_gpgcheck")
    } else {
        actual("gpgcheck")
    };
    let actual_gpgcheck = actual_gpgcheck == "1" || actual_gpgcheck.eq_ignore_ascii_case("true");
    if actual_gpgcheck != gpgcheck {
        log_to_file(
            "DEBUG",
            &format!(
                "repo save verification: '{repo_id}' gpgcheck differs \
                 (actual={actual_gpgcheck}, expected={gpgcheck})"
            ),
        );
        mismatches.push("gpgcheck");
    }

    // `repo info` reports no priority (99 default); the dump always prints
    // the effective value.
    let actual_priority = actual("priority").parse::<u32>().unwrap_or(99);
    let expected_priority = priority.unwrap_or(99);
    if actual_priority != expected_priority {
        log_to_file(
            "DEBUG",
            &format!(
                "repo save verification: '{repo_id}' priority differs \
                 (actual={actual_priority}, expected={expected_priority})"
            ),
        );
        mismatches.push("priority");
    }

    if mismatches.is_empty() {
        return Ok(());
    }
    Err(format!(
        "dnf5 did not apply the saved {} for repository '{repo_id}'",
        mismatches.join(", ")
    ))
}

/// The option names dnf5's config-manager override file sets for one repo id.
///
/// Only this repo's `[id]` section is read, and only option names that pass
/// `repo_override_option_valid` are returned — they become arguments of the
/// privileged `unsetopt` action. A missing file is an empty list.
fn repo_override_options(repo_id: &str, override_path: &Path) -> Vec<String> {
    let content = match fs::read_to_string(override_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut in_section = false;
    let mut options = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = &trimmed[1..trimmed.len() - 1] == repo_id;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim().to_ascii_lowercase();
            if repo_override_option_valid(&key) {
                options.push(key);
            }
        }
    }
    options.sort();
    options.dedup();
    options
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSpeedResult {
    pub url: String,
    pub speed_ms: Option<u32>,
}

/// List dnf repositories from dnf5's own report and apt sources from
/// `/etc/apt`. dnf5 is authoritative: file contents are never parsed for
/// enabled state, because overrides can disagree with the base file.
#[tauri::command]
pub async fn list_repos() -> Result<Vec<RepoEntry>, String> {
    if !binary_exists("dnf5").await {
        // Keep apt-only systems working (scope: apt paths are unchanged); a
        // dnf4-only host gets one plain-language message instead of an empty
        // table or a raw error.
        if binary_exists("apt-get").await {
            let mut apt_entries = Vec::new();
            parse_apt_repos(&mut apt_entries);
            return Ok(apt_entries);
        }
        return Err(DNF4_UNSUPPORTED_MESSAGE.to_string());
    }

    let mut entries = load_repo_entries().await?;
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
                    system_owned: false,
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
                system_owned: false,
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

/// Pure check over dnf5's effective repo list: would disabling
/// `target_repo_id` leave zero enabled repos system-wide?
///
/// Uses effective `is_enabled`, so a repo disabled by an override is not
/// counted as enabled and a repo enabled by an override is.
pub fn would_disable_last_enabled_repo(repos: &[RepoEntry], target_repo_id: &str) -> bool {
    let total_enabled = repos.iter().filter(|repo| repo.enabled).count();
    let target_enabled = repos
        .iter()
        .any(|repo| repo.id == target_repo_id && repo.enabled);

    target_enabled && total_enabled == 1
}

/// Pure check over dnf5's effective repo list: would deleting the target
/// section (`whole_file = false`) or its whole file (`whole_file = true`)
/// leave zero enabled repos system-wide?
pub fn would_delete_last_enabled_repo(
    repos: &[RepoEntry],
    target_file: &str,
    target_repo_id: &str,
    whole_file: bool,
) -> bool {
    let total_enabled = repos.iter().filter(|repo| repo.enabled).count();
    if total_enabled == 0 {
        return false;
    }

    let removed_enabled = if whole_file {
        repos
            .iter()
            .filter(|repo| repo.enabled && repo.file_path == target_file)
            .count()
    } else {
        usize::from(repos.iter().any(|repo| {
            repo.enabled && repo.id == target_repo_id && repo.file_path == target_file
        }))
    };

    total_enabled.saturating_sub(removed_enabled) == 0
}

/// Apt repositories keep the existing file-editing helper; dnf repos are
/// toggled through dnf5's override file.
fn is_apt_repo_path(path: &str) -> bool {
    path.ends_with(".list") || path.ends_with(".sources") || path.ends_with(".sources.list")
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

/// Enable or disable a repository.
///
/// dnf repos go through `dnf5 config-manager setopt <id>.enabled=1|0`
/// (override file), never a file write — including repos whose config lives
/// under the read-only `/usr/share/dnf5/repos.d`. Apt repos keep their
/// existing `.list`/`.sources` helper path.
///
/// Every dnf toggle is verified against dnf5's effective state afterwards.
/// For disable operations, the zero-enabled-repos lockout uses dnf5's
/// effective enabled count and requires explicit confirmation.
#[tauri::command]
pub async fn toggle_repo(repo_id: String, enabled: bool, file_path: String, confirmed: bool) -> Result<String, String> {
    if is_apt_repo_path(&file_path) {
        // The path comes from the frontend, so it is checked against the
        // directories the package manager uses before the file is touched.
        ensure_repo_file_path(&file_path)?;
        let action = if enabled { "repo_toggle_enable" } else { "repo_toggle_disable" };
        run_repo_named_action(
            action,
            &[String::from(&file_path), String::from(&repo_id)],
        )
        .await
        .map_err(|e| format!("Failed to toggle repo: {e}"))?;

        log_to_file("INFO", &format!("Toggled apt repo {repo_id} enabled={enabled}"));
        return Ok(String::new());
    }

    // Only a repo dnf5 itself reports may be passed to a privileged command.
    let repos = load_repo_entries().await?;
    let target = repos
        .iter()
        .find(|repo| repo.id == repo_id)
        .ok_or_else(|| format!("dnf5 does not report a repository with id '{repo_id}'"))?;
    if target.file_path != file_path {
        return Err(format!(
            "Repository '{repo_id}' is defined in '{}', not '{file_path}'",
            target.file_path
        ));
    }

    // Zero-enabled-repos lockout for disable operations.
    if !enabled && !confirmed && would_disable_last_enabled_repo(&repos, &repo_id) {
        return Ok(
            "CONFIRMATION_REQUIRED: Disabling this repository would leave no enabled \
             repositories system-wide. Future package updates would be impossible. \
             Do you want to proceed?"
                .to_string(),
        );
    }

    run_repo_named_action(
        "repo_config_manager_set",
        &[String::from(&repo_id), if enabled { "1" } else { "0" }.to_string()],
    )
    .await
    .map_err(|e| format!("Failed to toggle repo: {e}"))?;

    verify_repo_enabled(&repo_id, enabled)
        .await
        .map_err(|e| format!("Failed to toggle repo: {e}"))?;

    log_to_file("INFO", &format!("Toggled repo {repo_id} enabled={enabled}"));
    Ok(String::new())
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
        let unsigned_warning = if detect_unsigned_repos().await {
            " WARNING: This repository does not use GPG signature verification. \
             Future updates from this source may not be authenticated."
        } else {
            ""
        };

        return Ok(unsigned_warning.to_string());
    }

    if binary_exists("add-apt-repository").await {
        run_repo_named_action(
            "repo_add",
            &[String::from(&url)],
        )
        .await
        .map_err(|e| format!("Failed to add repo: {e}"))?;

        log_to_file("INFO", &format!("Added repo: {url}"));

        // apt sources keep their previous behaviour: the dnf5 signature scan
        // does not apply to them.
        let unsigned_warning = "";

        return Ok(unsigned_warning.to_string());
    }

    Err("Neither dnf nor add-apt-repository is available on this system".to_string())
}

/// Warn when any repo dnf5 reports has signature verification off. Uses
/// dnf5's own `pkg_gpgcheck`/`repo_gpgcheck`, so package-owned repos in
/// `/usr/share/dnf5/repos.d` are covered too.
async fn detect_unsigned_repos() -> bool {
    match load_dnf5_repos().await {
        Ok(repos) => repos
            .iter()
            .any(|repo| !repo.pkg_gpgcheck || !repo.repo_gpgcheck),
        Err(e) => {
            log_to_file("WARN", &format!("Unsigned-repository scan skipped: {e}"));
            false
        }
    }
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
    // dnf5 reports some repos from package-owned directories. They are
    // toggle-only: writing their file would modify distribution content.
    if is_system_owned_repo_path(&file_path) {
        return Err(format!(
            "The repository file '{file_path}' belongs to the distribution and cannot be edited here. Its enabled state can still be changed."
        ));
    }

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

    // dnf5 is authoritative: re-read the repo and compare every saved field.
    verify_saved_repo_details(
        &repo_id, &name, &baseurl, &metalink, &mirrorlist, gpgcheck, priority,
    )
    .await
    .map_err(|e| format!("Failed to save repository details: {e}"))?;

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
    if !binary_exists("dnf5").await {
        if binary_exists("apt-get").await {
            // apt hosts have no repomd repositories for this dnf-oriented probe.
            return Ok(vec![]);
        }
        return Err(DNF4_UNSUPPORTED_MESSAGE.to_string());
    }

    let (releasever, basearch) = get_system_repo_vars();
    let mut diagnostics = Vec::new();

    struct PendingProbe {
        repo_id: String,
        name: String,
        file_path: String,
        enabled: bool,
        candidate_url: Option<String>,
    }

    let mut pending_probes = Vec::new();

    // dnf5 is authoritative for ids, effective enabled state and config file
    // paths. A repo disabled by an override is reported disabled even when
    // its base file says enabled=1.
    let repos = load_repo_entries().await?;
    let loaded_files: std::collections::HashSet<&str> = repos
        .iter()
        .map(|repo| repo.file_path.as_str())
        .collect();

    for repo in &repos {
        // apt entries have no repomd.xml to probe.
        if is_apt_repo_path(&repo.file_path) {
            continue;
        }
        let raw_url = if !repo.baseurl.is_empty() {
            repo.baseurl.split_whitespace().next().unwrap_or("")
        } else if let Some(mirrorlist) = &repo.mirrorlist {
            mirrorlist.as_str()
        } else if let Some(metalink) = &repo.metalink {
            metalink.as_str()
        } else {
            ""
        };
        let candidate_url = if raw_url.is_empty() {
            None
        } else {
            Some(substitute_repo_vars(raw_url, &releasever, &basearch))
        };

        pending_probes.push(PendingProbe {
            repo_id: repo.id.clone(),
            name: repo.name.clone(),
            file_path: repo.file_path.clone(),
            enabled: repo.enabled,
            candidate_url,
        });
    }

    // Diagnostic pass: repo files on disk that dnf5 did not load. An empty,
    // unreadable or syntactically invalid file is skipped by dnf5 silently,
    // so it would otherwise be invisible in this list.
    for dir in REPO_READ_ONLY_DIRS
        .iter()
        .copied()
        .chain(std::iter::once(USER_REPO_DIR))
    {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("repo") {
                continue;
            }
            let file_path = path.to_string_lossy().to_string();
            if loaded_files.contains(file_path.as_str()) {
                continue;
            }
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            let (empty, message) = match fs::metadata(&path) {
                Err(e) => (false, format!("Cannot read metadata: {e}")),
                Ok(metadata) if metadata.len() == 0 => (
                    true,
                    "Repository file is 0 bytes (empty and corrupted)".to_string(),
                ),
                Ok(_) => (
                    false,
                    "dnf5 did not load any repository from this file (unreadable or invalid syntax)"
                        .to_string(),
                ),
            };
            diagnostics.push(RepoDiagnostic {
                repo_id: filename.clone(),
                name: filename,
                file_path: file_path.clone(),
                enabled: false,
                status: if empty { "empty" } else { "corrupted" }.to_string(),
                latency_ms: None,
                http_status: None,
                repomd_valid: false,
                error_message: Some(message),
                tested_url: None,
                is_empty_file: empty,
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

/// Name of the package that owns `path`, when the distribution's package
/// manager says the file belongs to one.
///
/// A package-owned repository file is not the panel's to delete: removing it
/// leaves the package with a missing file and the next update puts it back.
/// Both probes are unprivileged and take one absolute path operand.
async fn repo_file_package_owner(path: &str, apt_target: bool) -> Option<String> {
    let output = if apt_target {
        // `dpkg-query -S` prints "<package>: <path>" for an owned file.
        tokio::process::Command::new("dpkg-query")
            .args(["-S", path])
            .output()
            .await
    } else {
        tokio::process::Command::new("rpm")
            .args(["-q", "-f", path, "--queryformat", "%{NAME}\n"])
            .output()
            .await
    };

    let Ok(output) = output else { return None };
    if !output.status.success() {
        // "not owned by any package" is a normal, non-fatal answer.
        return None;
    }
    let report = String::from_utf8_lossy(&output.stdout);
    let first_line = report.lines().next().unwrap_or("").trim();
    let owner = if apt_target {
        first_line.split(':').next().unwrap_or("").trim()
    } else {
        first_line
    };
    if owner.is_empty() {
        None
    } else {
        Some(owner.to_string())
    }
}

/// Refuse a deletion that could damage the host, whatever the caller asked for.
///
/// The frontend disables Delete for system-owned entries, but the command is
/// the trust boundary: a stale or hand-crafted call must hit the same wall.
///
/// * the id is a distribution family (`fedora`, `updates`, `rpmfusion-…`) —
///   never deletable, wherever the file lives;
/// * the file is owned by an installed package — the panel may switch such a
///   repository on or off, but deleting the file breaks the package and the
///   next update restores it;
/// * a whole-file delete would also remove a protected section that shares the
///   file (multi-repo files are only edited section by section).
fn deletion_refusal(
    repo_id: &str,
    whole_file: bool,
    section_ids: &[&str],
    package_owner: Option<&str>,
) -> Result<(), String> {
    if repo_id_is_protected(repo_id) {
        return Err(format!(
            "'{repo_id}' is a distribution repository. The panel will not delete it: removing \
             it can leave this host without working package updates."
        ));
    }

    if let Some(owner) = package_owner {
        return Err(format!(
            "'{repo_id}' belongs to the installed package '{owner}'. Deleting the file would \
             leave that package broken and the next update would restore it. Remove the package \
             instead — its file goes with it."
        ));
    }

    if whole_file {
        if let Some(protected) = section_ids
            .iter()
            .copied()
            .find(|id| repo_id_is_protected(id))
        {
            return Err(format!(
                "'{repo_id}' shares its file with the distribution repository '{protected}'. \
                 Deleting the file would remove that repository too."
            ));
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_repo(repo_id: String, file_path: String, confirmed: bool) -> Result<String, String> {
    // Package-owned dnf repo files are read-only for the panel.
    if is_system_owned_repo_path(&file_path) {
        return Err(format!(
            "The repository file '{file_path}' belongs to the distribution and cannot be deleted here."
        ));
    }

    // The path comes from the frontend, so it is checked against the directories
    // the package manager uses before the file is deleted with root rights.
    ensure_repo_file_path(&file_path)?;
    let path = Path::new(&file_path);
    if !path.exists() {
        return Ok(String::new());
    }

    let apt_target = is_apt_repo_path(&file_path);
    let content = fs::read_to_string(path).unwrap_or_default();
    let sections: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with('[') && l.ends_with(']'))
        .collect();
    let whole_file = sections.len() <= 1;

    // Distribution families, package-owned files and multi-repo files that
    // carry a protected section are refused here, before the confirmation
    // dialog, so the call fails with a reason instead of offering a delete
    // that must not happen.
    let section_ids: Vec<&str> = sections
        .iter()
        .map(|section| section.trim_matches(['[', ']']))
        .collect();
    let package_owner = repo_file_package_owner(&file_path, apt_target).await;
    if let Err(refusal) = deletion_refusal(
        &repo_id,
        whole_file,
        &section_ids,
        package_owner.as_deref(),
    ) {
        log_to_file(
            "WARN",
            &format!("Refused to delete {repo_id} at {file_path}: {refusal}"),
        );
        return Err(refusal);
    }

    // Authoritative view for the lockout and the target check: dnf5's
    // effective state for dnf repos, the parsed apt sources for apt ones.
    let repos = if apt_target {
        let mut entries = Vec::new();
        parse_apt_repos(&mut entries);
        entries
    } else {
        load_repo_entries().await?
    };

    if !apt_target
        && !repos
            .iter()
            .any(|repo| repo.id == repo_id && repo.file_path == file_path)
    {
        return Err(format!(
            "dnf5 does not report repository '{repo_id}' in '{file_path}'"
        ));
    }

    // Always preview first. The first call (`confirmed = false`) never
    // touches the system; the second call carries the user's decision.
    if !confirmed {
        let lockout = would_delete_last_enabled_repo(&repos, &file_path, &repo_id, whole_file);
        let detail = if lockout {
            "Deleting this repository would leave no enabled repositories \
             system-wide. Future package updates would be impossible. \
             Do you want to proceed?"
        } else {
            "This removes the repository configuration from the system. \
             Do you want to proceed?"
        };
        return Ok(format!("CONFIRMATION_REQUIRED: {detail}"));
    }

    // dnf5's override for this id is stopped first, through the verified
    // config-manager mechanism: a leftover `[id]` section would otherwise
    // outlive the repository file.
    if !apt_target {
        for option in repo_override_options(&repo_id, Path::new(DNF5_CONFIG_MANAGER_OVERRIDE)) {
            run_repo_named_action(
                "repo_config_manager_unset",
                &[repo_id.clone(), option.clone()],
            )
            .await
            .map_err(|e| format!("Failed to remove repository override '{option}': {e}"))?;
        }
    }

    if whole_file {
        // Whole-file deletion: dispatch to repo_delete named action.
        run_repo_named_action(
            "repo_delete",
            &[String::from(&file_path)],
        )
        .await
        .map_err(|e| format!("Failed to delete repo file: {e}"))?;
        log_to_file("INFO", &format!("Deleted repo file: {}", file_path));
    } else {
        let mut new_lines = Vec::new();
        let mut skipping = false;
        let target_header = format!("[{}]", repo_id);

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                skipping = trimmed == target_header;
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
    }

    // dnf5 must no longer report the id at all.
    if !apt_target {
        verify_repo_absent(&repo_id)
            .await
            .map_err(|e| format!("Failed to delete repo: {e}"))?;
    }

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
        // confirmed: false so the zero-enabled-repos lockout still applies.
        // toggle_repo returns Ok("CONFIRMATION_REQUIRED: …") without disabling
        // when the lockout triggers — only count a real disable (empty Ok).
        if matches!(
            toggle_repo(repo_id, false, file_path, false).await,
            Ok(ref r) if r.is_empty()
        ) {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod repo_lockout_tests {
    use super::{
        add_repo, deletion_refusal, effective_state_from_list_json, normalize_repo_value,
        parse_dnf5_repos, parse_repo_config_dump, repo_override_options, saved_field_matches,
        saved_list_field_matches, split_repo_list, would_delete_last_enabled_repo,
        would_disable_last_enabled_repo, Dnf5RepoInfo, RepoEntry,
    };

    const FEDORA45: &str = include_str!("fixtures/fedora45.json");
    const DUMP_FEDORA45: &str = include_str!("fixtures/dump-fedora45.txt");
    const DUMP_MULTIBASEURL: &str = include_str!("fixtures/dump-multibaseurl.txt");
    const FEDORA44: &str = include_str!("fixtures/fedora44.json");
    const OVERRIDE_MISMATCH: &str = include_str!("fixtures/override-mismatch.json");

    fn repo(id: &str, file: &str, enabled: bool) -> RepoEntry {
        RepoEntry {
            id: id.to_string(),
            name: id.to_string(),
            baseurl: String::new(),
            enabled,
            file_path: file.to_string(),
            metalink: None,
            mirrorlist: None,
            gpgcheck: true,
            priority: None,
            system_owned: file.starts_with("/usr/share/dnf5/repos.d")
                || file.starts_with("/etc/distro.repos.d"),
        }
    }

    // ── dnf5 JSON parsing ─────────────────────────────────────────────────

    #[test]
    fn fedora45_fixture_maps_effective_state_and_system_owned() {
        let repos = parse_dnf5_repos(FEDORA45).expect("fedora45 fixture must parse");
        assert_eq!(repos.len(), 5);

        let fedora = repos.iter().find(|r| r.id == "fedora").unwrap();
        assert!(fedora.is_enabled);
        assert_eq!(fedora.repo_file_path, "/etc/yum.repos.d/fedora.repo");
        assert_eq!(fedora.base_url.len(), 1);

        let testing = repos.iter().find(|r| r.id == "updates-testing").unwrap();
        assert!(!testing.is_enabled, "disabled repo must report is_enabled=false");
        assert_eq!(testing.metalink, "https://mirrors.fedoraproject.org/metalink?repo=updates-testing-f45&arch=x86_64");

        let entry = repos
            .iter()
            .find(|r| r.id == "fedora-cisco-openh264")
            .unwrap()
            .to_entry();
        assert!(entry.system_owned, "a /usr/share/dnf5/repos.d file is system-owned");
        assert_eq!(entry.file_path, "/usr/share/dnf5/repos.d/fedora-cisco-openh264.repo");

        let copr = repos
            .iter()
            .find(|r| r.id == "copr:copr.fedorainfracloud.org:group_user:project")
            .unwrap()
            .to_entry();
        assert!(!copr.system_owned, "a /etc/yum.repos.d COPR file is user-editable");
    }

    #[test]
    fn fedora44_fixture_tolerates_missing_optional_fields() {
        let repos = parse_dnf5_repos(FEDORA44).expect("fedora44 fixture must parse");
        assert_eq!(repos.len(), 2);
        let fedora = repos.iter().find(|r| r.id == "fedora").unwrap();
        assert!(fedora.is_enabled);
        assert_eq!(fedora.priority, None);
        assert!(!fedora.pkg_gpgcheck, "missing pkg_gpgcheck defaults to false");
        assert_eq!(fedora.repo_file_path, "/etc/yum.repos.d/fedora.repo");
    }

    #[test]
    fn override_fixture_wins_over_the_file_content() {
        let repos = parse_dnf5_repos(OVERRIDE_MISMATCH).expect("fixture must parse");
        let onlyoffice = repos.iter().find(|r| r.id == "onlyoffice").unwrap();
        assert!(
            !onlyoffice.is_enabled,
            "the override (enabled=0) must decide the effective state even when the base file says enabled=1"
        );
    }

    #[test]
    fn effective_state_accepts_one_matching_repo() {
        let listed = r#"[
            {"id":"fedora","name":"Fedora","is_enabled":true},
            {"id":"updates-testing","name":"Testing","is_enabled":false}
        ]"#;
        assert_eq!(effective_state_from_list_json(listed, "fedora").unwrap(), true);
        assert_eq!(
            effective_state_from_list_json(listed, "updates-testing").unwrap(),
            false
        );
    }

    #[test]
    fn effective_state_rejects_unknown_id() {
        let listed = r#"[{"id":"fedora","is_enabled":true}]"#;
        let error = effective_state_from_list_json(listed, "does-not-exist").unwrap_err();
        assert!(error.contains("does-not-exist"), "error must name the id: {error}");
    }

    #[test]
    fn effective_state_rejects_duplicate_ids() {
        let duplicate = r#"[
            {"id":"dup","is_enabled":true},
            {"id":"dup","is_enabled":false}
        ]"#;
        assert!(
            effective_state_from_list_json(duplicate, "dup").is_err(),
            "two entries with the same id must not be treated as a valid state"
        );
    }

    #[test]
    fn unreadable_json_is_an_error_not_an_empty_list() {
        assert!(parse_dnf5_repos("not json").is_err());
        assert!(parse_dnf5_repos("").is_err());
    }

    // ── lockouts over dnf5's effective list ───────────────────────────────

    #[test]
    fn would_disable_last_enabled_repo_returns_false_when_multiple_enabled() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", true),
            repo("updates", "/etc/yum.repos.d/fedora.repo", true),
        ];
        assert!(!would_disable_last_enabled_repo(&repos, "fedora"));
    }

    #[test]
    fn would_disable_last_enabled_repo_returns_true_when_single_enabled() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", true),
            repo("updates", "/etc/yum.repos.d/fedora.repo", false),
        ];
        assert!(would_disable_last_enabled_repo(&repos, "fedora"));
    }

    #[test]
    fn would_disable_last_enabled_repo_returns_false_when_target_not_enabled() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", false),
            repo("updates", "/etc/yum.repos.d/fedora.repo", true),
        ];
        assert!(!would_disable_last_enabled_repo(&repos, "fedora"));
    }

    #[test]
    fn would_disable_last_enabled_repo_checks_across_files() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", true),
            repo("copr", "/etc/yum.repos.d/copr.repo", true),
        ];
        assert!(!would_disable_last_enabled_repo(&repos, "fedora"));
    }

    #[test]
    fn would_disable_last_enabled_repo_with_no_repos() {
        assert!(!would_disable_last_enabled_repo(&[], "fedora"));
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_true_when_file_has_only_enabled_repo() {
        let repos = [repo("fedora", "/etc/yum.repos.d/fedora.repo", true)];
        assert!(would_delete_last_enabled_repo(
            &repos,
            "/etc/yum.repos.d/fedora.repo",
            "fedora",
            true
        ));
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_false_when_other_files_have_enabled_repos() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", true),
            repo("updates", "/etc/yum.repos.d/updates.repo", true),
        ];
        assert!(!would_delete_last_enabled_repo(
            &repos,
            "/etc/yum.repos.d/fedora.repo",
            "fedora",
            true
        ));
    }

    #[test]
    fn would_delete_last_enabled_repo_returns_false_when_target_has_no_enabled_repos() {
        let repos = [
            repo("fedora", "/etc/yum.repos.d/fedora.repo", false),
            repo("updates", "/etc/yum.repos.d/updates.repo", true),
        ];
        assert!(!would_delete_last_enabled_repo(
            &repos,
            "/etc/yum.repos.d/fedora.repo",
            "fedora",
            true
        ));
    }

    #[test]
    fn would_delete_last_enabled_repo_subtracts_only_the_target_section() {
        // The file has two enabled sections; deleting one leaves the other.
        let repos = [
            repo("first", "/etc/yum.repos.d/multi.repo", true),
            repo("second", "/etc/yum.repos.d/multi.repo", true),
        ];
        assert!(!would_delete_last_enabled_repo(
            &repos,
            "/etc/yum.repos.d/multi.repo",
            "first",
            false
        ));
        assert!(would_delete_last_enabled_repo(
            &repos,
            "/etc/yum.repos.d/multi.repo",
            "first",
            true
        ));
    }

    // ── save verification ─────────────────────────────────────────────────

    #[test]
    fn normalize_repo_value_collapses_whitespace() {
        assert_eq!(
            normalize_repo_value("  https://a/   https://b/  "),
            "https://a/ https://b/"
        );
        assert_eq!(normalize_repo_value("line\nwrapped"), "line wrapped");
        assert_eq!(normalize_repo_value(""), "");
    }

    #[test]
    fn saved_field_matches_is_whitespace_tolerant() {
        assert!(saved_field_matches(
            "fedora",
            "baseurl",
            "https://a/ https://b/",
            "https://a/\nhttps://b/"
        ));
        assert!(saved_field_matches("fedora", "name", "Fedora  45", "Fedora 45"));
    }

    #[test]
    fn saved_field_matches_rejects_a_real_difference() {
        assert!(!saved_field_matches(
            "fedora",
            "baseurl",
            "https://actual/",
            "https://different/"
        ));
        assert!(!saved_field_matches("fedora", "name", "", "Fedora"));
    }

    // ── config-manager dump parsing (save verification source) ─────────────

    #[test]
    fn dump_parser_reads_real_fedora_dump() {
        let values = parse_repo_config_dump(DUMP_FEDORA45);
        assert_eq!(values["name"], "Fedora 45 - x86_64");
        assert_eq!(values["enabled"], "1");
        assert_eq!(values["pkg_gpgcheck"], "1");
        assert_eq!(values["priority"], "99");
        assert_eq!(
            values["metalink"],
            "https://mirrors.fedoraproject.org/metalink?repo=fedora-45&arch=x86_64"
        );
        assert_eq!(
            values["mirrorlist"], "",
            "an unset string option prints as a bare key"
        );
        assert_eq!(values["baseurl"], "");
        assert!(
            !values.contains_key("======== \"fedora\" repository configuration: ========"),
            "the header line must not become a key"
        );
    }

    #[test]
    fn dump_parser_keeps_comma_separated_baseurls() {
        let values = parse_repo_config_dump(DUMP_MULTIBASEURL);
        assert_eq!(
            values["baseurl"],
            "https://example.com/repo,https://example.com/repo2"
        );
        assert_eq!(values["priority"], "42");
    }

    #[test]
    fn split_repo_list_accepts_the_dumps_commas_and_the_files_spaces() {
        assert_eq!(
            split_repo_list("https://a/,https://b/"),
            vec!["https://a/".to_string(), "https://b/".to_string()]
        );
        assert_eq!(
            split_repo_list("https://a/\nhttps://b/"),
            vec!["https://a/".to_string(), "https://b/".to_string()]
        );
        assert_eq!(
            split_repo_list("  https://a/ ,  https://b/ "),
            split_repo_list("https://a/ https://b/")
        );
        assert!(split_repo_list("").is_empty());
    }

    #[test]
    fn saved_list_field_matches_is_separator_tolerant_but_keeps_order() {
        // dnf5 prints base_url comma-separated; the panel writes spaces.
        assert!(saved_list_field_matches(
            "fedora",
            "baseurl",
            "https://a/,https://b/",
            "https://a/ https://b/"
        ));
        assert!(
            !saved_list_field_matches("fedora", "baseurl", "https://a/,https://b/", "https://b/ https://a/"),
            "a reordered list is a real difference"
        );
        assert!(!saved_list_field_matches("fedora", "baseurl", "https://a/", "https://a/ https://b/"));
    }

    #[test]
    fn system_owned_entries_are_flagged_from_the_repo_file_path() {
        let repos = parse_dnf5_repos(FEDORA45).unwrap();
        let system_owned: Vec<Dnf5RepoInfo> = repos
            .into_iter()
            .filter(|repo| repo.to_entry().system_owned)
            .collect();
        assert_eq!(system_owned.len(), 1);
        assert_eq!(system_owned[0].id, "fedora-cisco-openh264");
    }

    // ── override cleanup ──────────────────────────────────────────────────

    #[test]
    fn repo_override_options_reads_only_the_target_section() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-repo-override-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("99-config_manager.repo");
        std::fs::write(
            &path,
            "[onlyoffice]\nenabled=0\n\n[fedora]\nenabled=1\ngpgcheck=0\n",
        )
        .unwrap();

        assert_eq!(
            repo_override_options("onlyoffice", &path),
            vec!["enabled".to_string()]
        );
        assert_eq!(
            repo_override_options("fedora", &path),
            vec!["enabled".to_string(), "gpgcheck".to_string()]
        );
        assert!(repo_override_options("missing", &path).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repo_override_options_skips_names_unfit_for_an_operand() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-repo-override-bad-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("99-config_manager.repo");
        std::fs::write(
            &path,
            "[fedora]\nenabled=1\nBad Name=1\n--evil=1\nenabled=0\n",
        )
        .unwrap();

        assert_eq!(
            repo_override_options("fedora", &path),
            vec!["enabled".to_string()],
            "invalid option names are skipped and duplicates collapse"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repo_override_options_missing_file_is_empty() {
        assert!(repo_override_options("fedora", std::path::Path::new(
            "/nonexistent/99-config_manager.repo"
        ))
        .is_empty());
    }

    // ── add_repo guards (unchanged behaviour) ─────────────────────────────

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
        assert!(result.is_err(), "http URL must be refused: {result:?}");
    }

    #[tokio::test]
    async fn add_repo_refuses_empty_url() {
        let result = add_repo("".to_string(), true).await;
        assert!(result.is_err(), "empty URL must be refused: {result:?}");
    }

    // ── delete guards: critical repositories are never deletable ──────────

    #[test]
    fn delete_refuses_distribution_repository_families() {
        for id in [
            "fedora",
            "fedora-source",
            "updates",
            "updates-testing-debuginfo",
            "rpmfusion-free-updates",
        ] {
            let error = deletion_refusal(id, true, &[id], None).unwrap_err();
            assert!(
                error.contains("distribution repository"),
                "{id} must be refused with the distribution reason: {error}"
            );
        }
    }

    #[test]
    fn delete_refuses_package_owned_files() {
        let error = deletion_refusal(
            "onlyoffice",
            true,
            &["onlyoffice"],
            Some("onlyoffice-repo"),
        )
        .unwrap_err();
        assert!(
            error.contains("onlyoffice-repo") && error.contains("Remove the package"),
            "the refusal must name the owning package and the way out: {error}"
        );
    }

    #[test]
    fn whole_file_delete_refuses_a_shared_protected_section() {
        let error = deletion_refusal(
            "docker-ce-stable",
            true,
            &["docker-ce-stable", "fedora-cisco-openh264"],
            None,
        )
        .unwrap_err();
        assert!(
            error.contains("fedora-cisco-openh264"),
            "the refusal must name the protected section it would remove: {error}"
        );
    }

    #[test]
    fn section_delete_leaves_a_protected_sibling_alone() {
        // Deleting one section rewrites the file; the protected sibling stays.
        assert!(deletion_refusal(
            "docker-ce-test",
            false,
            &["docker-ce-test", "fedora-cisco-openh264"],
            None,
        )
        .is_ok());
    }

    #[test]
    fn ordinary_user_repositories_remain_deletable() {
        assert!(deletion_refusal("beekeeper-studio", true, &["beekeeper-studio"], None).is_ok());
        assert!(deletion_refusal(
            "copr:copr.fedorainfracloud.org:group_user:project",
            true,
            &["copr:copr.fedorainfracloud.org:group_user:project"],
            None,
        )
        .is_ok());
    }
}
