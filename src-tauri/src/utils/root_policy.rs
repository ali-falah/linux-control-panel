//! The policy that decides what may run with administrator rights.
//!
//! This module is deliberately free of any other dependency so that it can be
//! shared between the app (which validates a call *before* it is made) and
//! the small privileged helper `lcp-admin` (which validates it again *while*
//! it runs as root).

use std::path::Path;

/// Root programs the app may execute (matched by their base name, the
/// absolute path is resolved at runtime and then verified).
pub const ALLOWED_ROOT_PROGRAMS: &[&str] = &[
    // package management
    "dnf", "dnf5", "rpm", "apt", "apt-get", "add-apt-repository", "flatpak", "dpkg",
    "update-alternatives", "update-rc.d", "chkconfig", "certbot", "augenrules",
    // init system and services
    "systemctl", "journalctl", "loginctl", "ss", "ip", "ping", "dig", "host", "resolvectl",
    // users and groups
    "useradd", "usermod", "userdel", "groupadd", "groupmod", "groupdel", "gpasswd",
    "passwd", "chpasswd", "visudo", "id", "getent", "renice",
    // firewall and network
    "firewall-cmd", "ufw", "nmcli", "route", "resurrect", "connection", "smartctl",
    // files and text
    "cat", "ls", "stat", "find", "grep", "tail", "head", "zcat", "gzip", "sed", "awk",
    "cut", "sort", "wc", "tr", "tee", "install", "cp", "mv", "rm", "ln", "link", "mkdir",
    "chmod", "chown", "touch", "mktemp", "date", "readlink", "dirname", "basename", "x509",
    // hardware and boot
    "modprobe", "rmmod", "grub2-mkconfig", "grub-mkconfig", "grub2-script-check",
    "grub-script-check", "fail2ban-client",
    // selinux
    "setsebool", "semodule", "semanage", "restorecon", "ausearch",
    // shells (only for the app's own fixed scripts, never with user data)
    "bash", "sh", "stdbuf", "crontab", "nginx", "sshd",
];

/// Refuse a program that is not on the allow list.
pub fn ensure_program_allowed(program: &str) -> Result<(), String> {
    let base = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if ALLOWED_ROOT_PROGRAMS.contains(&base.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "Refused to run {program} with administrator rights: the program is not on the allow list."
        ))
    }
}

/// Resolve a program name to an absolute path.
///
/// The standard directories are searched first; the PATH is only used as a
/// fallback — a program found there still has to pass the root ownership
/// check before it may run with administrator rights.
pub fn resolve_root_program(name: &str) -> Option<String> {
    if name.starts_with('/') {
        let path = Path::new(name);
        return path.is_file().then(|| name.to_string());
    }
    for dir in [
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ] {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = Path::new(dir).join(name);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Verify that `path` is a regular file owned by root that no other user can
/// modify. Executing a user writable binary as root would hand root rights to
/// whoever is able to replace that file.
pub fn verify_trusted_binary(path: &str) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot verify {path}: {e}"))?;
    if !metadata.is_file() {
        return Err(format!(
            "Refused to run {path} with administrator rights: not a regular file."
        ));
    }
    if metadata.uid() != 0 {
        return Err(format!(
            "Refused to run {path} with administrator rights: the file is not owned by root."
        ));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(format!(
            "Refused to run {path} with administrator rights: the file is writable by other users."
        ));
    }
    Ok(())
}

/// Refuse arguments that would hand a shell to the privileged program.
pub fn ensure_no_shell_escape(program: &str, args: &[String]) -> Result<(), String> {
    let is_shell =
        program.ends_with("/bash") || program.ends_with("/sh") || program.ends_with("/dash");
    if is_shell && args.iter().any(|arg| arg == "-c") {
        return Err(
            "Refused to run a shell command with administrator rights: use a validated argument list instead."
                .to_string(),
        );
    }
    Ok(())
}

/// Check the arguments of a privileged call: no argument may be empty and no
/// argument may contain control characters (a newline could smuggle a second
/// command into a program that reads a list from stdin).
pub fn ensure_safe_arguments(args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.is_empty() {
            return Err(
                "Refused an empty argument for an administrator operation.".to_string(),
            );
        }
        if arg.contains('\n') || arg.contains('\r') || arg.contains('\0') {
            return Err(
                "Refused an argument with control characters for an administrator operation."
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Validate a value that comes from the user and is used as an *operand* of a
/// privileged program.
///
/// A value that starts with a dash would be read as an option by the program
/// (for example `dnf config-manager --add-repo --setopt=...`), so it is
/// refused here.
pub fn ensure_safe_operand(value: &str, what: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("The {what} must not be empty."));
    }
    if value.starts_with('-') {
        return Err(format!("The {what} must not start with a dash."));
    }
    if value.contains('\n') || value.contains('\r') || value.contains('\0') {
        return Err(format!("The {what} must not contain control characters."));
    }
    Ok(())
}

/// Validate a repository configuration file path.
///
/// Only the directories the package manager itself uses are allowed, the file
/// must have a repository file extension and no path element may be a symlink
/// or a relative reference — otherwise a crafted path could make the app
/// modify or delete an arbitrary file with root rights.
pub fn ensure_repo_file_path(path: &str) -> Result<(), String> {
    use std::path::{Component, Path, PathBuf};

    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err("The repository file path must be absolute.".to_string());
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("The repository file path must not contain relative parts.".to_string());
    }

    let allowed_dirs = [
        "/etc/yum.repos.d",
        "/etc/dnf",
        "/etc/apt/sources.list.d",
    ];
    let parent = candidate.parent().map(|p| p.to_string_lossy().to_string());
    let in_allowed_dir = parent
        .as_deref()
        .is_some_and(|dir| allowed_dirs.contains(&dir));
    if !in_allowed_dir {
        return Err(format!(
            "The repository file must live in one of {}.",
            allowed_dirs.join(", ")
        ));
    }

    let name = candidate
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty()
        || name.starts_with('.')
        || !(name.ends_with(".repo")
            || name.ends_with(".list")
            || name.ends_with(".sources")
            || name.ends_with(".sources.list"))
    {
        return Err("The repository file must be a .repo, .list or .sources file.".to_string());
    }

    // no symlink may be used to escape the directory
    let mut walked = PathBuf::from("/");
    for component in candidate.components() {
        walked.push(component);
        if walked.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("The repository file path must not contain a symbolic link.".to_string());
        }
    }

    Ok(())
}
