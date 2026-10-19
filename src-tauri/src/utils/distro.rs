use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistroFamily {
    Debian,
    Rhel,
    Arch,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroInfo {
    pub family: DistroFamily,
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub pretty_name: String,
    pub pkg_manager: String, // "apt", "dnf", "pacman", "unknown"
    pub mac_type: String,    // "selinux", "apparmor", "none"
    pub firewall: String,    // "firewalld", "ufw", "none"
    pub sudo_group: String,  // "sudo" or "wheel"
}

impl Default for DistroInfo {
    fn default() -> Self {
        Self {
            family: DistroFamily::Rhel,
            id: "fedora".to_string(),
            name: "Fedora Linux".to_string(),
            version_id: "".to_string(),
            pretty_name: "Fedora Linux".to_string(),
            pkg_manager: "dnf".to_string(),
            mac_type: "selinux".to_string(),
            firewall: "firewalld".to_string(),
            sudo_group: "wheel".to_string(),
        }
    }
}

pub fn detect_distro() -> DistroInfo {
    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut id = String::new();
    let mut id_like = String::new();
    let mut name = String::new();
    let mut version_id = String::new();
    let mut pretty_name = String::new();

    for line in os_release.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            let val = v.trim_matches('"').trim_matches('\'').to_string();
            match k {
                "ID" => id = val.to_lowercase(),
                "ID_LIKE" => id_like = val.to_lowercase(),
                "NAME" => name = val,
                "VERSION_ID" => version_id = val,
                "PRETTY_NAME" => pretty_name = val,
                _ => {}
            }
        }
    }

    let family = if id == "debian"
        || id == "ubuntu"
        || id == "pop"
        || id == "linuxmint"
        || id_like.contains("debian")
        || id_like.contains("ubuntu")
    {
        DistroFamily::Debian
    } else if id == "fedora"
        || id == "rhel"
        || id == "centos"
        || id == "almalinux"
        || id == "rocky"
        || id_like.contains("rhel")
        || id_like.contains("fedora")
    {
        DistroFamily::Rhel
    } else if id == "arch" || id == "manjaro" || id_like.contains("arch") {
        DistroFamily::Arch
    } else if Path::new("/etc/debian_version").exists() {
        DistroFamily::Debian
    } else if Path::new("/etc/redhat-release").exists() || Path::new("/etc/fedora-release").exists() {
        DistroFamily::Rhel
    } else {
        DistroFamily::Unknown
    };

    let pkg_manager = match family {
        DistroFamily::Debian => "apt".to_string(),
        DistroFamily::Rhel => "dnf".to_string(),
        DistroFamily::Arch => "pacman".to_string(),
        DistroFamily::Unknown => {
            if Path::new("/usr/bin/apt-get").exists() {
                "apt".to_string()
            } else if Path::new("/usr/bin/dnf").exists() {
                "dnf".to_string()
            } else {
                "unknown".to_string()
            }
        }
    };

    let mac_type = if Path::new("/sys/fs/selinux").exists() || Path::new("/etc/selinux/config").exists() {
        "selinux".to_string()
    } else if Path::new("/sys/kernel/security/apparmor").exists() || Path::new("/etc/apparmor.d").exists() {
        "apparmor".to_string()
    } else {
        "none".to_string()
    };

    let firewall = if Path::new("/usr/bin/firewall-cmd").exists() {
        "firewalld".to_string()
    } else if Path::new("/usr/sbin/ufw").exists() || Path::new("/usr/bin/ufw").exists() {
        "ufw".to_string()
    } else {
        "none".to_string()
    };

    let sudo_group = match family {
        DistroFamily::Debian => "sudo".to_string(),
        _ => "wheel".to_string(),
    };

    DistroInfo {
        family,
        id,
        name,
        version_id,
        pretty_name,
        pkg_manager,
        mac_type,
        firewall,
        sudo_group,
    }
}

