use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::utils::privilege::tokio::Command;

use crate::utils::root_policy;
use crate::{binary_exists, log_to_file};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatpakApp {
    pub name: String,
    pub app_id: String,
    pub version: String,
    pub origin: String,
    pub installation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpmPackage {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateEntry {
    pub common_name: String,
    pub flatpak: Option<FlatpakApp>,
    pub rpm: Option<RpmPackage>,
    pub recommendation: String,
}

/// List all installed Flatpak apps
#[tauri::command]
pub async fn list_flatpaks() -> Result<Vec<FlatpakApp>, String> {
    if !binary_exists("flatpak").await {
        return Ok(vec![]);
    }

    let output = Command::new("flatpak")
        .args([
            "list",
            "--app",
            "--columns=name,application,version,origin,installation",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run flatpak: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("flatpak list failed: {err}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let apps = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            FlatpakApp {
                name: parts.first().unwrap_or(&"").trim().to_string(),
                app_id: parts.get(1).unwrap_or(&"").trim().to_string(),
                version: parts.get(2).unwrap_or(&"").trim().to_string(),
                origin: parts.get(3).unwrap_or(&"").trim().to_string(),
                installation: parts.get(4).unwrap_or(&"system").trim().to_string(),
            }
        })
        .filter(|a| !a.app_id.is_empty())
        .collect();

    Ok(apps)
}

/// List all installed native packages (RPM on Fedora/RHEL, DEB on Debian/Ubuntu)
#[tauri::command]
pub async fn list_rpms() -> Result<Vec<RpmPackage>, String> {
    if binary_exists("rpm").await {
        let output = Command::new("rpm")
            .args([
                "-qa",
                "--queryformat",
                "%{NAME}\t%{VERSION}\t%{ARCH}\t%{SUMMARY}\n",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run rpm: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("rpm -qa failed: {err}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let packages = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.splitn(4, '\t').collect();
                RpmPackage {
                    name: parts.first().unwrap_or(&"").trim().to_string(),
                    version: parts.get(1).unwrap_or(&"").trim().to_string(),
                    arch: parts.get(2).unwrap_or(&"").trim().to_string(),
                    summary: parts.get(3).unwrap_or(&"").trim().to_string(),
                }
            })
            .filter(|p| !p.name.is_empty())
            .collect();

        return Ok(packages);
    }

    if binary_exists("dpkg-query").await {
        let output = Command::new("dpkg-query")
            .args([
                "-W",
                "-f=${Package}\t${Version}\t${Architecture}\t${binary:Summary}\n",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run dpkg-query: {e}"))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("dpkg-query failed: {err}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let packages = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.splitn(4, '\t').collect();
                RpmPackage {
                    name: parts.first().unwrap_or(&"").trim().to_string(),
                    version: parts.get(1).unwrap_or(&"").trim().to_string(),
                    arch: parts.get(2).unwrap_or(&"").trim().to_string(),
                    summary: parts.get(3).unwrap_or(&"").trim().to_string(),
                }
            })
            .filter(|p| !p.name.is_empty())
            .collect();

        return Ok(packages);
    }

    Err("Neither rpm nor dpkg-query is available on this system".to_string())
}

/// Detect packages available as both Flatpak and RPM
#[tauri::command]
pub async fn detect_duplicates() -> Result<Vec<DuplicateEntry>, String> {
    let flatpaks = list_flatpaks().await?;
    let rpms = list_rpms().await?;

    // Build a normalized name map for RPMs
    let rpm_map: HashMap<String, &RpmPackage> =
        rpms.iter().map(|r| (normalize_name(&r.name), r)).collect();

    let mut duplicates = Vec::new();

    for flatpak in &flatpaks {
        let flat_norm = normalize_name(&flatpak.name);

        // Try to find a matching RPM
        if let Some(rpm) = rpm_map.get(&flat_norm) {
            let recommendation = determine_recommendation(flatpak, rpm);
            duplicates.push(DuplicateEntry {
                common_name: flatpak.name.clone(),
                flatpak: Some(flatpak.clone()),
                rpm: Some((*rpm).clone()),
                recommendation,
            });
        } else {
            // Try matching on last part of app_id (e.g., org.gnome.Calculator -> calculator)
            let id_norm = flatpak
                .app_id
                .split('.')
                .last()
                .map(|s| normalize_name(s))
                .unwrap_or_default();

            if !id_norm.is_empty() {
                if let Some(rpm) = rpm_map.get(&id_norm) {
                    let recommendation = determine_recommendation(flatpak, rpm);
                    duplicates.push(DuplicateEntry {
                        common_name: flatpak.name.clone(),
                        flatpak: Some(flatpak.clone()),
                        rpm: Some((*rpm).clone()),
                        recommendation,
                    });
                }
            }
        }
    }

    Ok(duplicates)
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .replace(['-', '_', '.', ' '], "")
        .trim_start_matches("lib")
        .to_string()
}

fn determine_recommendation(flatpak: &FlatpakApp, _rpm: &RpmPackage) -> String {
    let system_tool_keywords = [
        "systemd", "dnf", "rpm", "apt", "dpkg", "gtk", "qt", "lib", "python", "perl", "kernel",
    ];

    let name_lower = flatpak.name.to_lowercase();
    let is_system_tool = system_tool_keywords.iter().any(|k| name_lower.contains(k));

    if is_system_tool {
        "Keep Native Package — better system integration for system tools".to_string()
    } else {
        "Keep Flatpak — sandboxed with automatic updates".to_string()
    }
}

/// Remove a Flatpak app
#[tauri::command]
pub async fn remove_flatpak(app_id: String, system_wide: bool) -> Result<String, String> {
    if !is_safe_package_id(&app_id) {
        return Err("Invalid Flatpak application ID".to_string());
    }
    if !binary_exists("flatpak").await {
        return Err("flatpak is not available".to_string());
    }

    let output = crate::commands::app_manager::remove_application(
        None,
        &app_id,
        "Flatpak",
        system_wide,
        None,
        None,
    )
    .await
    .map_err(|error| {
        log_to_file("ERROR", &format!("remove_flatpak {app_id} failed: {error}"));
        error
    })?;

    log_to_file("INFO", &format!("Removed Flatpak: {app_id}"));
    Ok(output)
}

/// Package names the panel refuses to uninstall, autoremove or roll back.
/// Shared with the privileged helper so the app preview and the helper's
/// own pre-flight check use exactly the same list.
pub fn is_protected_package(name: &str) -> bool {
    root_policy::is_protected_package(name)
}

pub fn is_safe_package_id(name: &str) -> bool {
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | ':' | '@' | '-'))
}

pub async fn rpm_remove_preview(name: &str) -> Result<String, String> {
    if !is_safe_package_id(name) {
        return Err("Invalid package name".to_string());
    }
    if is_protected_package(name) {
        return Err(format!("Action blocked: '{}' is an essential system package and cannot be removed.", name));
    }
    let is_dnf = binary_exists("dnf").await;
    let mut command = if is_dnf {
        let mut cmd = tokio::process::Command::new("dnf");
        cmd.args(["--color=never", "remove", "--assumeno", name]).env("LC_ALL", "C").env("LANG", "C");
        cmd
    } else if binary_exists("apt-get").await {
        let mut cmd = tokio::process::Command::new("apt-get");
        cmd.args(["-s", "remove", name]).env("LC_ALL", "C").env("LANG", "C");
        cmd
    } else {
        return Err("No supported package manager is available".to_string());
    };
    let output = command.output().await.map_err(|e| format!("Failed to preview package removal: {e}"))?;
    let preview = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if preview.trim().is_empty() {
        return Err("The package manager returned no removal preview".to_string());
    }

    let lower_preview = preview.to_lowercase();
    if ["error:", "problem:", "failed to", "unable to", "no match for argument", "could not"].iter().any(|marker| lower_preview.contains(marker)) {
        return Err("The package manager could not produce a safe removal preview. No changes were made.".to_string());
    }

    if is_dnf {
        let has_summary = lower_preview.contains("transaction summary");
        let expected_abort = lower_preview.contains("operation aborted");
        if !has_summary || (!output.status.success() && !expected_abort) {
            return Err("DNF did not return a complete removal preview. No changes were made.".to_string());
        }
        let mut in_remove_section = false;
        let mut saw_removal_row = false;
        for line in preview.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();
            if lower.starts_with("removing:") || lower.starts_with("removing ") {
                in_remove_section = true;
                continue;
            }
            if lower.starts_with("transaction summary") {
                break;
            }
            if !in_remove_section || trimmed.is_empty() || trimmed.starts_with('=') || lower.starts_with("package ") {
                continue;
            }
            let Some(candidate) = trimmed.split_whitespace().next() else { continue };
            if !candidate.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
                continue;
            }
            saw_removal_row = true;
            let package = candidate.rsplit_once('.').map(|(name, arch)| {
                if ["x86_64", "aarch64", "noarch", "i686", "i386", "ppc64le", "s390x"].contains(&arch) { name } else { candidate }
            }).unwrap_or(candidate);
            if is_protected_package(package) {
                return Err(format!("Removal blocked because the transaction includes protected package '{package}'."));
            }
        }
        if !saw_removal_row {
            return Err("DNF did not list the packages in its removal preview. No changes were made.".to_string());
        }
    } else {
        if !output.status.success() || !preview.lines().any(|line| line.starts_with("Remv ")) {
            return Err("APT did not return a complete removal preview. No changes were made.".to_string());
        }
        for line in preview.lines().filter(|line| line.starts_with("Remv ")) {
            if let Some(package) = line.split_whitespace().nth(1) {
                if is_protected_package(package) {
                    return Err(format!("Removal blocked because the transaction includes protected package '{package}'."));
                }
            }
        }
    }

    Ok(preview)
}

/// The packages a removal preview would remove, as `name` or `name.arch`
/// tokens, sorted and deduplicated.
///
/// The confirmation compares these sets rather than the raw text: DNF reprints
/// the table with different spacing and summary numbers between two runs, and
/// an exact-text check would reject a transaction that did not change.
pub fn preview_package_set(preview: &str) -> Option<Vec<String>> {
    let mut packages = Vec::new();
    let mut in_removal_table = false;
    let mut saw_dnf_table = false;

    for line in preview.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        if lower.starts_with("removing:") || lower.starts_with("removing ") {
            in_removal_table = true;
            saw_dnf_table = true;
            continue;
        }
        if lower.starts_with("transaction summary") {
            in_removal_table = false;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Remv ") {
            if let Some(package) = rest.split_whitespace().next() {
                packages.push(package.to_string());
            }
            continue;
        }
        if !in_removal_table {
            continue;
        }
        if trimmed.is_empty()
            || trimmed.starts_with('=')
            || lower.starts_with("package ")
            || lower.starts_with("removing ")
        {
            continue;
        }
        let Some(candidate) = trimmed.split_whitespace().next() else {
            continue;
        };
        if candidate
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        {
            packages.push(candidate.to_string());
        }
    }

    if !saw_dnf_table && packages.is_empty() {
        return None;
    }
    packages.sort();
    packages.dedup();
    Some(packages)
}

/// Whether the preview the user confirmed still describes the same removal.
pub fn same_removal_transaction(current: &str, expected: &str) -> bool {
    match (preview_package_set(current), preview_package_set(expected)) {
        (Some(current), Some(expected)) => current == expected,
        _ => current.trim() == expected.trim(),
    }
}

#[tauri::command]
pub async fn preview_rpm_removal(name: String) -> Result<String, String> {
    rpm_remove_preview(&name).await
}

/// Remove a native package (RPM on Fedora/RHEL, DEB on Debian/Ubuntu)
#[tauri::command]
pub async fn remove_rpm(name: String, expected_preview: String) -> Result<String, String> {
    if !is_safe_package_id(&name) {
        return Err("Invalid package name".to_string());
    }
    if is_protected_package(&name) {
        return Err(format!(
            "Action blocked: '{}' is a vital system package and cannot be removed.",
            name
        ));
    }

    let output = crate::commands::app_manager::remove_application(
        None,
        &name,
        "RPM",
        false,
        Some(&expected_preview),
        None,
    )
    .await
    .map_err(|error| {
        log_to_file("ERROR", &format!("remove_rpm {name} failed: {error}"));
        error
    })?;

    log_to_file("INFO", &format!("Removed native package: {name}"));
    Ok(output)
}

#[cfg(test)]
mod preview_tests {
    use super::{preview_package_set, same_removal_transaction};

    const DNF_A: &str = "Dependencies resolved.\n\
Removing:\n\
 Package        Arch    Version                Repository      Size\n\
 kernel-core    x86_64  6.5.6-300.fc39         @System         63 M\n\
 brave-browser  x86_64  1.60.0-1               @System        108 M\n\
\nTransaction Summary\n\
====================\n\
Remove  2 Packages\n\
\nFreed space: 171 M\n";

    // Same packages, different column widths and different summary numbers.
    const DNF_B: &str = "Dependencies resolved.\n\
Removing:\n\
Package              Arch    Version             Repository    Size\n\
kernel-core          x86_64  6.5.6-300.fc39      @System       64 M\n\
brave-browser        x86_64  1.60.0-1            @System       109 M\n\
\nTransaction Summary\n\
====================\n\
Remove  2 Packages\n\nFreed space: 173 M\n";

    const DNF_C: &str = "Removing:\n\
 Package        Arch    Version         Repository      Size\n\
 kernel-core    x86_64  6.5.6-300.fc39  @System         63 M\n\
\nTransaction Summary\n";

    const APT: &str = "NOTE: This is only a simulation!\n\
Remv brave-browser [1.60.0-1]\n\
Remv kernel-core [6.5.6-300.fc39]\n";

    #[test]
    fn preview_sets_ignore_spacing_and_summary_numbers() {
        assert_eq!(
            preview_package_set(DNF_A).unwrap(),
            vec!["brave-browser".to_string(), "kernel-core".to_string()]
        );
        assert!(same_removal_transaction(DNF_A, DNF_B));
    }

    #[test]
    fn a_different_package_set_is_a_different_transaction() {
        assert!(!same_removal_transaction(DNF_A, DNF_C));
    }

    #[test]
    fn apt_previews_are_read_too() {
        assert_eq!(
            preview_package_set(APT).unwrap(),
            vec!["brave-browser".to_string(), "kernel-core".to_string()]
        );
    }

    #[test]
    fn unknown_text_falls_back_to_exact_comparison() {
        assert!(preview_package_set("nothing useful").is_none());
        assert!(same_removal_transaction("same", "same"));
        assert!(!same_removal_transaction("same", "different"));
    }
}
