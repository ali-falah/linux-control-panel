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
    // users and groups (query + basic management; membership and passwords go
    // through named actions instead)
    "useradd", "userdel", "groupadd", "groupmod", "groupdel", "id", "getent", "renice",
    // firewall and network
    "firewall-cmd", "ufw", "nmcli", "route", "resurrect", "connection", "smartctl",
    // files and text (pure readers and filters only — anything that can create,
    // overwrite, replace or relabel a file belongs to a named action)
    "cat", "ls", "stat", "grep", "tail", "head", "zcat", "cut", "sort", "wc", "tr",
    "mkdir", "date", "readlink", "dirname", "basename",
    // the bootstrap's copy-template only (see check_install): fixed destinations
    "install",
    // hardware and boot
    "modprobe", "rmmod", "grub2-mkconfig", "grub-mkconfig", "grub2-script-check",
    "grub-script-check", "fail2ban-client",
    // the root helper itself — named-action dispatch only (check_helper_action)
    "lcp-admin",
    // selinux
    "setsebool", "semodule", "semanage", "restorecon", "ausearch",
    // web servers (test/config reads only)
    "nginx", "sshd",
];

/// Programs that are refused everywhere — in the app and, more importantly, in
/// the root helper itself.
///
/// A process running as the same user can call `pkexec …/lcp-admin <args>`
/// directly and skip every app-side check, so the helper must not execute
/// anything that could hand it root or arbitrary file access:
///
/// * shells and command wrappers are exec-capable (`bash` runs scripts,
///   `awk` runs `system()`, `stdbuf` launches a child binary),
/// * the file generalists can create, overwrite, relabel or destroy any file
///   up to `/etc/shadow` (`cp`, `mv`, `rm`, `sed -i`, `tee`, `chmod`, `chown`,
///   `ln`, `touch`, `mktemp`, `gzip`, and `x509` via `-out`) — `install` was
///   pulled back out of the list for the bootstrap's fixed-destination
///   template (`check_install`),
/// * `crontab` turns content into delayed root code execution,
/// * the authentication tools hand out root (`chpasswd`/`passwd` set any
///   password including root's, `gpasswd`/`usermod` can grant wheel, `visudo`
///   edits the sudoers policy),
/// * `find` carries `-exec`, `awk`/`gawk` are Turing-complete interpreters.
///
/// Call types that still need one of these get a dedicated, validated named
/// action in the helper (hardening phase 3) and then leave this list again.
pub const MIGRATING_ROOT_PROGRAMS: &[&str] = &[
    // exec-capable
    "bash", "sh", "stdbuf", "awk", "gawk", "find",
    // arbitrary file write/relabel/destroy
    "sed", "tee", "cp", "mv", "rm", "chmod", "chown", "visudo", "ln", "link",
    "touch", "mktemp", "gzip", "x509",
    // delayed root code execution
    "crontab",
    // root/grant-primitives
    "chpasswd", "passwd", "gpasswd", "usermod",
];

// ─── The helper's own fixed constants (shared with the app) ──────────────────

/// Where the PolicyKit rule is installed, by the bootstrap and by the
/// helper's `install_polkit_rule` action. The only rule file the helper will
/// ever write.
pub const POLKIT_RULE_FILE: &str = "/etc/polkit-1/rules.d/50-linux-control-panel.rules";

/// The PolicyKit rule: the helper may be run as root for an *active* local
/// session, and the authorisation is kept for the default five minutes
/// (`auth_admin_keep`) instead of asking for a password every time.
///
/// The `install_polkit_rule` helper action requires its stdin content to be
/// byte-identical to this constant — content the app controls *and* the
/// helper independently verifies, with no user-owned intermediate file that
/// could be swapped in between.
pub const POLKIT_RULE_TEXT: &str = r#"// Installed by the Linux Control Panel.
// Grants the root owned helper (and only the helper) the right to run with
// administrator rights. The authorisation is kept for the default
// timestamp_timeout (5 minutes), so a multi step action asks only once.
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.policykit.exec" &&
        subject.active && subject.local &&
        subject.isInGroup("wheel")) {
        var key = action.lookup("program");
        if (key == "/usr/local/libexec/lcp-admin") {
            return polkit.Result.AUTH_ADMIN_KEEP;
        }
    }
});
"#;

/// Refuse a program that is not on the allow list.
pub fn ensure_program_allowed(program: &str) -> Result<(), String> {
    let base = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if MIGRATING_ROOT_PROGRAMS.contains(&base.as_str()) {
        return Err(format!(
            "Refused to run {program} with administrator rights: this call type is temporarily disabled \
             while it is migrated to a validated named action of the root helper (hardening phase 3)."
        ));
    }
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

// ─── Per-program argument policy (hardening phase 2) ─────────────────────────
//
// An app-side check is only UX: any process of the same user can run
// `pkexec /usr/local/libexec/lcp-admin <args>` itself. The helper therefore
// re-checks every single argv element. This module is shared between the app
// and the helper, so both reject the same argv — the app early (better error
// message), the helper last (as the true gate).
//
// Generic rule for a program *without* a rule below: not a single argument may
// start with '-'. A flag is only ever accepted from that program's own allow
// list; each allowed flag is documented with why it is needed. Values without
// a dash are already covered by `ensure_safe_arguments` (emptiness, control
// characters) — here they are additionally matched against their concrete
// shape wherever the app actually sends data of that kind.

/// One value that must match a shape, with a readable refusal otherwise.
fn require(
    program: &str,
    value: &str,
    what: &str,
    ok: impl Fn(&str) -> bool,
) -> Result<(), String> {
    if ok(value) {
        Ok(())
    } else {
        Err(operand_not_allowed(program, value, what))
    }
}

fn operand_not_allowed(program: &str, value: &str, what: &str) -> String {
    format!("Refused to run {program} with administrator rights: {what} (got '{value}').")
}

fn flag_not_allowed(program: &str, arg: &str) -> String {
    format!(
        "Refused to run {program} with administrator rights: the option '{arg}' is not allowed for this program."
    )
}

/// Consume the value that follows a value-taking flag.
fn flag_value(
    program: &str,
    flag: &str,
    index: &mut usize,
    args: &[String],
) -> Result<String, String> {
    *index += 1;
    match args.get(*index) {
        Some(value) => Ok(value.clone()),
        None => Err(operand_not_allowed(
            program,
            flag,
            &format!("{flag} must be followed by a value"),
        )),
    }
}

fn is_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

/// Small signed number (renice priority: `-n 5` or `-n -5`).
fn is_small_priority(value: &str) -> bool {
    let magnitude_ok = is_digits(value.strip_prefix('-').unwrap_or(value));
    magnitude_ok && value.parse::<i32>().is_ok_and(|n| n.abs() <= 20)
}

/// Numeric user/group id at or above 1000: everything below belongs to system
/// accounts, and creating or moving accounts there is a privilege-granting
/// primitive.
fn is_non_system_id(value: &str) -> bool {
    is_digits(value) && value.parse::<u32>().is_ok_and(|id| id >= 1000)
}

/// Package/repository names as the app already validates them for the update
/// page: DNS-like words, never a path.
fn is_package_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | ':' | '@' | '-'))
}

/// Names, repo ids, zones — one "word", no separators beyond `_.:-` and no
/// leading dash.
fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

/// A POSIX login/group name: lowercase (or underscore) start, no whitespace,
/// bounded length.
fn is_account_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.'))
}

fn is_user_name(value: &str) -> bool {
    is_account_name(value)
}

fn is_group_name(value: &str) -> bool {
    is_account_name(value)
}

/// Kernel module (`modprobe 8021q`, `rmmod floppy`).
fn is_module_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Block device operand — the app only ever sends `/dev/<name>`.
fn is_device_path(value: &str) -> bool {
    match value.strip_prefix("/dev/") {
        Some(name) => !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

fn is_ipv4_or_ipv6_token(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':')
}

/// A systemd unit name: `nginx.service`, `ctrl-alt-del.target`. Never a unit
/// file path (see `check_systemctl`).
fn is_unit_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | ':'))
}

/// A GECOS comment field (full names): printable ASCII, no leading dash.
fn is_safe_comment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_graphic() || c == ' ')
}

/// Only the numeric timeout knob the update page needs — `--setopt` with an
/// arbitrary key could point dnf at a foreign repository root.
fn is_dnf_timeout_setopt(value: &str) -> bool {
    is_digits(value.strip_prefix("--setopt=timeout=").unwrap_or(""))
}

/// Absolute path, no relative components, no symbolic links on the way — the
/// same reasoning as in `ensure_repo_file_path`, applied to every path operand.
fn ensure_trusted_path(program: &str, path: &str, what: &str) -> Result<(), String> {
    use std::path::{Component, Path, PathBuf};

    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(operand_not_allowed(
            program,
            path,
            &format!("the {what} must be an absolute path"),
        ));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(operand_not_allowed(
            program,
            path,
            &format!("the {what} must not contain relative parts"),
        ));
    }
    let mut walked = PathBuf::from("/");
    for component in candidate.components() {
        walked.push(component);
        if walked
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(operand_not_allowed(
                program,
                path,
                &format!("the {what} must not contain a symbolic link"),
            ));
        }
    }
    Ok(())
}

// ─── systemctl / loginctl ────────────────────────────────────────────────────

/// `systemctl` verbs the app may run. Every entry changes or reports the state
/// of one installed unit. Verbs that load functionality of their own stay off
/// the list: `link` would load a unit from an arbitrary file, `edit` starts an
/// editor, `import-environment` injects session variables into the manager,
/// and any unit *file* path would make systemd load a foreign definition.
const SYSTEMCTL_VERBS: &[&str] = &[
    "start", "stop", "restart", "reload", "enable", "disable", "status",
    "is-active", "is-enabled", "daemon-reload",
];

fn check_systemctl(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "systemctl";
    for arg in args {
        if arg.contains('/') {
            return Err(operand_not_allowed(
                PROGRAM,
                arg,
                "a unit reference for systemctl must not contain '/' (no unit file paths)",
            ));
        }
    }
    let Some(verb) = args.first() else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a systemd verb (start, stop, restart, …) is required",
        ));
    };
    if verb.starts_with('-') {
        return Err(flag_not_allowed(PROGRAM, verb));
    }
    if !SYSTEMCTL_VERBS.contains(&verb.as_str()) {
        return Err(operand_not_allowed(
            PROGRAM,
            verb,
            "the verb must be one of start|stop|restart|reload|enable|disable|status|is-active|is-enabled|daemon-reload",
        ));
    }
    for arg in &args[1..] {
        match arg.as_str() {
            // the only flag any page needs: enable/disable immediately
            "--now" => {}
            _ if arg.starts_with('-') => return Err(flag_not_allowed(PROGRAM, arg)),
            _ if verb == "daemon-reload" => {
                return Err(operand_not_allowed(
                    PROGRAM,
                    arg,
                    "daemon-reload must not be combined with additional arguments",
                ));
            }
            _ => require(
                PROGRAM,
                arg,
                "a systemd unit name (e.g. nginx.service)",
                is_unit_name,
            )?,
        }
    }
    Ok(())
}

fn check_loginctl(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "loginctl";
    // exactly the one shape the session pages need
    let exact = args.len() == 2 && args[0] == "kill-session" && is_digits(&args[1]);
    if exact {
        Ok(())
    } else {
        Err(operand_not_allowed(
            PROGRAM,
            args.first().map(String::as_str).unwrap_or(""),
            "only 'kill-session <session-id>' with a numeric session id is allowed",
        ))
    }
}

// ─── package management ──────────────────────────────────────────────────────

/// Restrictions every package manager shares: no argument may name a local
/// file (any '/' or a package archive name — installing one would run the
/// attacker's maintainer scripts as root), and the flags that redirect the
/// manager to a foreign root, a fresh macro definition or a computed value are
/// refused outright.
fn check_package_manager_rules(
    program: &str,
    args: &[String],
    archive_suffix: &str,
) -> Result<(), String> {
    for arg in args {
        if arg.contains('/') || arg.to_ascii_lowercase().ends_with(archive_suffix) {
            return Err(operand_not_allowed(
                program,
                arg,
                &format!(
                    "an argument with '/' or ending in '{archive_suffix}' would install or alter packages from a local file, which is not allowed"
                ),
            ));
        }
        if arg == "--installroot"
            || arg.starts_with("--installroot=")
            || arg == "--define"
            || arg.starts_with("--define=")
            || arg == "--eval"
            || arg.starts_with("--eval=")
        {
            return Err(flag_not_allowed(program, arg));
        }
    }
    Ok(())
}

/// `dnf` verbs and flags exactly as the update/copr/history/clean pages send
/// them. Two used call shapes contain '/' (`copr enable owner/repo`,
/// `config-manager --add-repo <url>`) and are therefore deliberately refused
/// until their dedicated helper actions exist (breakage table, phase 2 report).
const DNF_VERBS: &[&str] = &[
    "check-update", "clean", "copr", "config-manager", "history",
    "autoremove", "makecache", "install", "upgrade", "remove", "updateinfo",
];

fn check_dnf(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "dnf";
    check_package_manager_rules(PROGRAM, args, ".rpm")?;

    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // non-interactive and best-package selection on the update page
                "-y" | "--best" | "--assumeno" | "--showduplicates" | "--only-upgrade" => {}
                // the only --setopt key: digits-only default repo timeout
                _ if is_dnf_timeout_setopt(arg) => {}
                // restrict one call to a single repository id
                "--repo" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a repository id after --repo", is_identifier)?;
                }
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }

    let Some(verb) = operands.first() else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a dnf verb (install, upgrade, clean, …) is required",
        ));
    };
    if !DNF_VERBS.contains(verb) {
        return Err(operand_not_allowed(
            PROGRAM,
            verb,
            "the verb must be one of check-update|clean|copr|config-manager|history|autoremove|makecache|install|upgrade|remove|updateinfo",
        ));
    }
    let rest = &operands[1..];
    match *verb {
        "clean" => {
            for arg in rest {
                require(PROGRAM, arg, "'clean' only accepts all|expire-cache", |v| {
                    matches!(v, "all" | "expire-cache")
                })?;
            }
        }
        "history" => {
            let exact = rest.len() == 2 && rest[0] == "undo" && is_digits(rest[1]);
            if !exact {
                return Err(operand_not_allowed(
                    PROGRAM,
                    rest.first().copied().unwrap_or(""),
                    "only 'history undo <transaction-id>' with a numeric id is allowed",
                ));
            }
        }
        "makecache" | "autoremove" | "check-update" | "updateinfo" => {
            if let Some(arg) = rest.first() {
                return Err(operand_not_allowed(
                    PROGRAM,
                    arg,
                    "this dnf verb must not be combined with additional arguments",
                ));
            }
        }
        // install / upgrade / remove: package names only
        _ => {
            for arg in rest {
                require(PROGRAM, arg, "a package name (no local files)", is_package_name)?;
            }
        }
    }
    Ok(())
}

/// Package queries only; operands are package names. The one file operand the
/// app needs (`rpm -qf <path>`) runs unprivileged and never reaches the
/// helper, so rpm sees no path arguments helper-side.
const RPM_FLAGS: &[&str] = &["-q", "-i", "-l", "-a", "-c", "-d", "-f", "--quiet"];

fn check_rpm(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "rpm";
    check_package_manager_rules(PROGRAM, args, ".rpm")?;
    let mut saw_package = false;
    for arg in args {
        if arg.starts_with('-') {
            if !RPM_FLAGS.contains(&arg.as_str()) {
                return Err(flag_not_allowed(PROGRAM, arg));
            }
        } else {
            require(PROGRAM, arg, "a package name", is_package_name)?;
            saw_package = true;
        }
    }
    if !saw_package {
        return Err(operand_not_allowed(PROGRAM, "", "a package name is required"));
    }
    Ok(())
}

const APT_VERBS: &[&str] = &["update", "install", "remove", "clean", "autoremove", "upgrade"];

fn check_apt(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "apt-get";
    check_package_manager_rules(PROGRAM, args, ".deb")?;
    let mut saw_verb = false;
    for arg in args {
        if arg.starts_with('-') {
            // -y: non-interactive confirmation
            if arg != "-y" {
                return Err(flag_not_allowed(PROGRAM, arg));
            }
        } else if !saw_verb {
            if !APT_VERBS.contains(&arg.as_str()) {
                return Err(operand_not_allowed(
                    PROGRAM,
                    arg,
                    "the verb must be one of update|install|remove|clean|autoremove|upgrade",
                ));
            }
            saw_verb = true;
        } else {
            require(PROGRAM, arg, "a package name", is_package_name)?;
        }
    }
    if !saw_verb {
        return Err(operand_not_allowed(PROGRAM, "", "an apt-get verb is required"));
    }
    Ok(())
}

/// `add-apt-repository` handles exactly two shapes: a `ppa:<owner>/<name>` id
/// or an http(s) repository URL — the forms the repository pages send.
fn check_add_apt_repository(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "add-apt-repository";
    let mut operands: Vec<&str> = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            match arg.as_str() {
                "-y" | "--remove" => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            operands.push(arg);
        }
    }
    let [value] = &operands[..] else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "exactly one PPA id or repository URL is required",
        ));
    };
    let ppa = value.starts_with("ppa:")
        && value
            .chars()
            .skip(4)
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | '+'));
    let url = value.starts_with("https://") || value.starts_with("http://");
    if !ppa && !url {
        return Err(operand_not_allowed(
            PROGRAM,
            value,
            "the repository must be a 'ppa:<owner>/<name>' id or an http(s) URL",
        ));
    }
    Ok(())
}

const FLATPAK_VERBS: &[&str] = &["install", "uninstall"];

fn check_flatpak(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "flatpak";
    let mut operands: Vec<&str> = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            match arg.as_str() {
                // --system/--user pin the installation target, -y confirms it
                "-y" | "--system" | "--user" => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            operands.push(arg);
        }
    }
    let Some(verb) = operands.first() else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a flatpak verb (install, uninstall) is required",
        ));
    };
    if !FLATPAK_VERBS.contains(verb) {
        return Err(operand_not_allowed(
            PROGRAM,
            verb,
            "the verb must be install or uninstall",
        ));
    }
    for arg in &operands[1..] {
        require(PROGRAM, arg, "an application id", |v| {
            !v.is_empty()
                && v.len() <= 256
                && v.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '+' | ':' | '-')
                })
        })?;
    }
    Ok(())
}

// ─── network ─────────────────────────────────────────────────────────────────

/// The subcommands in the list inspect or mutate the host's own network
/// configuration. `netns` (→ `ip netns exec <cmd>`, root code exec inside a
/// namespace) and `-batch` (reads a script file of commands) are refused.
fn check_ip(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "ip";
    let subcommands = ["link", "addr", "route", "neigh"];
    let mut saw_subcommand = false;
    for arg in args {
        if arg == "netns" || arg == "exec" {
            return Err(operand_not_allowed(
                PROGRAM,
                arg,
                "'netns' and 'exec' are not allowed for ip",
            ));
        }
        if arg == "-batch" || arg.starts_with("-batch") {
            return Err(flag_not_allowed(PROGRAM, arg));
        }
        if arg.starts_with('-') {
            // -o for one-line output, -4/-6 for the address family
            if !matches!(arg.as_str(), "-o" | "-4" | "-6") {
                return Err(flag_not_allowed(PROGRAM, arg));
            }
        } else if !saw_subcommand {
            if !subcommands.contains(&arg.as_str()) {
                return Err(operand_not_allowed(
                    PROGRAM,
                    arg,
                    "the subcommand must be one of link|addr|route|neigh",
                ));
            }
            saw_subcommand = true;
        } else {
            require(PROGRAM, arg, "an ip operand", |value| {
                value.len() <= 128
                    && value.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '/' | '@' | '_' | '-')
                    })
            })?;
        }
    }
    if !saw_subcommand {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "an ip subcommand (link, addr, route, neigh) is required",
        ));
    }
    Ok(())
}

/// nmcli property values (vpn data, private keys, addresses) are free-form by
/// design — a residual surface listed in the phase 2 report. Dashes are only
/// accepted for the machine-readable output flags.
fn check_nmcli(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "nmcli";
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                "-t" => {}
                "-f" | "-c" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a field list or output mode", |v| {
                        !v.is_empty()
                            && v.len() <= 64
                            && v.chars().all(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, ',' | '.' | '_' | '-')
                            })
                    })?;
                }
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        }
        index += 1;
    }
    Ok(())
}

/// firewalld options are `--word` or `--word=value`; the value charset covers
/// ports (`--add-port=8080/tcp`), services, sources and interface names. Zone
/// operands must follow `--zone`.
fn is_firewall_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix("--") else {
        return false;
    };
    let (word, value) = match rest.split_once('=') {
        Some((word, value)) => (word, Some(value)),
        None => (rest, None),
    };
    if word.is_empty()
        || word.ends_with('-')
        || !word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
    {
        return false;
    }
    if let Some(value) = value {
        if value.is_empty()
            || value.len() > 128
            || !value.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '-' | '.' | ',' | '_' | '@' | ':' | '/' | '=' | '+' | '*')
            })
        {
            return false;
        }
    }
    true
}

fn check_firewall_cmd(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "firewall-cmd";
    let mut has_zone_flag = false;
    let mut has_zone_operand = false;
    for arg in args {
        if arg.starts_with('-') {
            if !is_firewall_flag(arg) {
                return Err(flag_not_allowed(PROGRAM, arg));
            }
            if arg == "--zone" {
                has_zone_flag = true;
            }
        } else {
            require(PROGRAM, arg, "a zone name", |value| {
                value.len() <= 64
                    && value.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-')
                    })
            })?;
            has_zone_operand = true;
        }
    }
    if has_zone_operand && !has_zone_flag {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a zone operand must be preceded by --zone",
        ));
    }
    // something has to happen: an option (the flag may vary), not just a zone
    let has_option = args.iter().any(|a| a.starts_with('-') && a != "--zone");
    if !has_option {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "at least one firewall option (e.g. --add-port=…, --panic-on) is required",
        ));
    }
    Ok(())
}

// ─── users and groups ────────────────────────────────────────────────────────

/// User creation. `-o`/`--non-unique` would allow duplicate ids and any
/// `-u`/`-g` below 1000 would place the account in the reserved system range —
/// both are refused.
fn check_useradd(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "useradd";
    let mut saw_account = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // -m: create the home directory
                "-m" => {}
                // -c <fullname>: GECOS comment
                "-c" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a printable full name for -c", is_safe_comment)?;
                }
                "--non-unique" | "-o" => {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "duplicate/non-unique ids are not allowed for user tools",
                    ));
                }
                "-u" | "-g" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(
                        PROGRAM,
                        &value,
                        &format!("an id of 1000 or higher for {arg}"),
                        is_non_system_id,
                    )?;
                }
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            require(PROGRAM, arg, "an account name", is_user_name)?;
            saw_account = true;
        }
        index += 1;
    }
    if !saw_account {
        return Err(operand_not_allowed(PROGRAM, "", "an account name is required"));
    }
    Ok(())
}

fn check_userdel(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "userdel";
    let mut saw_account = false;
    for arg in args {
        if arg.starts_with('-') {
            match arg.as_str() {
                // --force: delete even if the account is still logged in
                "--force" => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            require(PROGRAM, arg, "an account name", is_user_name)?;
            saw_account = true;
        }
    }
    if !saw_account {
        return Err(operand_not_allowed(PROGRAM, "", "an account name is required"));
    }
    Ok(())
}

/// `-aG <group>` is the sudo toggle, `-L`/`-U` lock and unlock. Ids below
/// 1000 and the duplicate-id options are refused like in `check_useradd`.
fn check_usermod(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "usermod";
    let mut saw_account = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // append the account to group(s)
                "-aG" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a group name after -aG", is_group_name)?;
                }
                "-c" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a printable full name for -c", is_safe_comment)?;
                }
                "--non-unique" | "-o" => {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "duplicate/non-unique ids are not allowed for user tools",
                    ));
                }
                "-u" | "-g" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(
                        PROGRAM,
                        &value,
                        &format!("an id of 1000 or higher for {arg}"),
                        is_non_system_id,
                    )?;
                }
                "-L" | "-U" => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            require(PROGRAM, arg, "an account name", is_user_name)?;
            saw_account = true;
        }
        index += 1;
    }
    if !saw_account {
        return Err(operand_not_allowed(PROGRAM, "", "an account name is required"));
    }
    Ok(())
}

/// `groupadd`/`groupmod`/`groupdel`: group names, `-g` with a non-system id,
/// `-n` for the rename, `-f` to not fail on an existing group.
fn check_group_tools(program: &str, args: &[String]) -> Result<(), String> {
    let mut saw_group = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                "-g" => {
                    let value = flag_value(program, arg, &mut index, args)?;
                    require(program, &value, "an id of 1000 or higher for -g", is_non_system_id)?;
                }
                "-n" => {
                    let value = flag_value(program, arg, &mut index, args)?;
                    require(program, &value, "a group name after -n", is_group_name)?;
                }
                "-f" | "--force" => {}
                "--non-unique" | "-o" => {
                    return Err(operand_not_allowed(
                        program,
                        arg,
                        "duplicate/non-unique ids are not allowed for group tools",
                    ));
                }
                _ => return Err(flag_not_allowed(program, arg)),
            }
        } else {
            require(program, arg, "a group name", is_group_name)?;
            saw_group = true;
        }
        index += 1;
    }
    if !saw_group {
        return Err(operand_not_allowed(program, "", "a group name is required"));
    }
    Ok(())
}

// ─── hardware, services, security daemons ────────────────────────────────────

fn check_renice(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "renice";
    let mut saw_priority = false;
    let mut saw_process = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            // -n <priority>: a small signed number
            "-n" => {
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                require(
                    PROGRAM,
                    &value,
                    "a priority between -20 and 20 for -n",
                    is_small_priority,
                )?;
                saw_priority = true;
            }
            // -p <pid>: numeric process id
            "-p" => {
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                require(PROGRAM, &value, "a numeric pid for -p", is_digits)?;
                saw_process = true;
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(flag_not_allowed(PROGRAM, arg));
                }
                require(PROGRAM, arg, "a numeric pid", is_digits)?;
                saw_process = true;
            }
        }
        index += 1;
    }
    if !saw_priority || !saw_process {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "'-n <priority> -p <pid>' with a numeric priority and pid is required",
        ));
    }
    Ok(())
}

fn check_smartctl(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "smartctl";
    let test_types = ["short", "long", "offline", "conveyance"];
    let mut saw_device = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // read all attributes as JSON / identity / health / scan bus
                "-a" | "-j" | "-i" | "-H" | "--scan" => {}
                // -t <type>: trigger a self test
                "-t" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(
                        PROGRAM,
                        &value,
                        "a self test type for -t",
                        |v| test_types.contains(&v),
                    )?;
                }
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            require(PROGRAM, arg, "a device path like /dev/sda", is_device_path)?;
            saw_device = true;
        }
        index += 1;
    }
    if !saw_device {
        return Err(operand_not_allowed(PROGRAM, "", "a device path is required"));
    }
    Ok(())
}

fn check_module_tools(program: &str, args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.starts_with('-') {
            return Err(flag_not_allowed(program, arg));
        }
        require(
            program,
            arg,
            "a kernel module name (e.g. 8021q)",
            is_module_name,
        )?;
    }
    if args.is_empty() {
        return Err(operand_not_allowed(
            program,
            "",
            "a kernel module name is required",
        ));
    }
    Ok(())
}

fn check_daemon_flags(program: &str, args: &[String], allowed: &[&str]) -> Result<(), String> {
    for arg in args {
        if arg.starts_with('-') {
            if !allowed.contains(&arg.as_str()) {
                return Err(flag_not_allowed(program, arg));
            }
        } else {
            return Err(operand_not_allowed(
                program,
                arg,
                "these daemons take no operands from the app, only flags from their allow list",
            ));
        }
    }
    Ok(())
}

/// The exact three shapes the fail2ban page uses; `set` writes are bounded to
/// ban/unban of a single address on one jail.
fn check_fail2ban(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "fail2ban-client";
    match &args[..] {
        [verb] if verb.as_str() == "status" => Ok(()),
        [verb, jail] if verb.as_str() == "status" => {
            require(PROGRAM, jail, "a jail name", is_module_name)
        }
        [verb, jail, action, ip]
            if verb.as_str() == "set" && matches!(action.as_str(), "banip" | "unbanip") =>
        {
            require(PROGRAM, jail, "a jail name", is_module_name)?;
            require(PROGRAM, ip, "an IPv4 or IPv6 address", is_ipv4_or_ipv6_token)
        }
        _ => Err(operand_not_allowed(
            PROGRAM,
            args.first().map(String::as_str).unwrap_or(""),
            "only 'status [<jail>]' and 'set <jail> banip|unbanip <ip>' are allowed",
        )),
    }
}

// ─── SELinux and audit ───────────────────────────────────────────────────────

/// `-P` makes a change persistent; else a boolean name plus one of its four
/// values — exactly what the SELinux page sends.
fn check_setsebool(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "setsebool";
    let rest: &[String] = match args.first().map(String::as_str) {
        Some("-P") => &args[1..],
        _ => args,
    };
    let exact = rest.len() == 2
        && is_identifier(&rest[0])
        && matches!(rest[1].as_str(), "on" | "off" | "1" | "0");
    if exact {
        Ok(())
    } else {
        Err(operand_not_allowed(
            PROGRAM,
            args.first().map(String::as_str).unwrap_or(""),
            "only '<boolean> on|off|1|0' with an optional -P is allowed",
        ))
    }
}

/// `-i` loads a compiled policy package from a path. The path is checked like
/// every other path operand (absolute, no relative parts, no symlinks); the
/// policy *content* still originates from the app's own audit module — a
/// fixed, approved source directory is phase 3 work.
fn check_semodule(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "semodule";
    let mut saw_install = false;
    let mut saw_other_flag = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                "-i" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    ensure_trusted_path(PROGRAM, &value, "policy package file")?;
                    saw_install = true;
                }
                "-l" => saw_other_flag = true,
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            return Err(operand_not_allowed(
                PROGRAM,
                arg,
                "semodule takes no bare operands, only -i <file> or -l",
            ));
        }
        index += 1;
    }
    if !saw_install && !saw_other_flag {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a policy install (-i <file>) or list (-l) is required",
        ));
    }
    Ok(())
}

fn check_ausearch(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "ausearch";
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // -m <type>: only AVC records are requested today,
                // -ts/-te <date>: the recent window
                "-m" | "-ts" | "-te" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "an ausearch value", is_search_token)?;
                }
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            require(PROGRAM, arg, "an ausearch value", is_search_token)?;
        }
        index += 1;
    }
    Ok(())
}

fn is_search_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ',' | '.'))
}

// ─── files ───────────────────────────────────────────────────────────────────

fn check_mkdir(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "mkdir";
    let mut saw_path = false;
    for arg in args {
        if arg.starts_with('-') {
            // -p: create intermediate directories
            if arg != "-p" {
                return Err(flag_not_allowed(PROGRAM, arg));
            }
        } else {
            ensure_trusted_path(PROGRAM, arg, "directory")?;
            saw_path = true;
        }
    }
    if !saw_path {
        return Err(operand_not_allowed(PROGRAM, "", "a directory path is required"));
    }
    Ok(())
}

/// The path readers: `-n <count>` is the only value flag; every operand must
/// be an absolute, canonical path. Reading arbitrary absolute files as root
/// stays possible *for reads* — a confidentiality residual tracked in the
/// phase 2 report; integrity is not affected, these programs cannot write.
fn check_log_reader(program: &str, args: &[String], allowed_flags: &[&str]) -> Result<(), String> {
    let mut saw_path = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                "-n" => {
                    let value = flag_value(program, arg, &mut index, args)?;
                    require(program, &value, "a numeric line count for -n", is_digits)?;
                }
                _ if allowed_flags.contains(&arg) => {}
                _ => return Err(flag_not_allowed(program, arg)),
            }
        } else {
            ensure_trusted_path(program, arg, "file path")?;
            saw_path = true;
        }
        index += 1;
    }
    if !saw_path {
        return Err(operand_not_allowed(
            program,
            "",
            "an absolute file path is required",
        ));
    }
    Ok(())
}

/// grep: `[flags] pattern path…`. The pattern is read-only data for grep (no
/// control characters, never an option — a dash reads as a flag and is
/// refused), everything after the first word is a checked path.
fn check_grep(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "grep";
    let allowed_flags = ["-i", "-E", "-F", "-v", "-q"];
    let mut saw_pattern_or_path = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                "-m" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a numeric count for -m", is_digits)?;
                }
                _ if allowed_flags.contains(&arg) => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else if saw_pattern_or_path {
            ensure_trusted_path(PROGRAM, arg, "file path")?;
        } else {
            require(PROGRAM, arg, "a search pattern", |v| {
                v.len() <= 256 && v.chars().all(|c| !c.is_control() && c != '\0')
            })?;
            saw_pattern_or_path = true;
        }
        index += 1;
    }
    if !saw_pattern_or_path {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a search pattern and file path are required",
        ));
    }
    Ok(())
}

// ─── certificate renewal ─────────────────────────────────────────────────────

/// A certbot hook runs arbitrary shell code as root when a certificate is
/// renewed, so any argument containing the word is refused — even embedded
/// (`--pre-hook`, `--hook-script`).
fn check_certbot(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "certbot";
    for arg in args {
        if arg.to_ascii_lowercase().contains("hook") {
            return Err(operand_not_allowed(
                PROGRAM,
                arg,
                "an argument containing 'hook' would let arbitrary code run as root on renewal",
            ));
        }
    }
    let mut saw_renew = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            match arg {
                // the domain selection and non-interactive flags of the
                // domains page; --cert-name takes the certificate's name
                "--cert-name" => {
                    let value = flag_value(PROGRAM, arg, &mut index, args)?;
                    require(PROGRAM, &value, "a certificate name for --cert-name", |v| {
                        !v.is_empty()
                            && v.len() <= 128
                            && v.chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                    })?;
                }
                "--non-interactive" | "--keep-until-expiring" => {}
                _ => return Err(flag_not_allowed(PROGRAM, arg)),
            }
        } else {
            match arg {
                // the one verb the renewal page sends
                "renew" => saw_renew = true,
                _ => {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "only 'certbot renew --cert-name <name>' is allowed",
                    ));
                }
            }
        }
        index += 1;
    }
    if !saw_renew {
        return Err(operand_not_allowed(
            PROGRAM,
            args.first().map(String::as_str).unwrap_or(""),
            "only 'certbot renew --cert-name <name>' is allowed",
        ));
    }
    Ok(())
}

// ─── the general case ────────────────────────────────────────────────────────

/// Programs without a specific rule: no flag at all may be passed. They are
/// only ever called with argument shapes of pure words today (`journalctl`,
/// `ss`, `ping`, `dig`, `host`, `ufw`, `update-alternatives`, `grub2-mkconfig`,
/// `dpkg`, `date`, `wc`, `tr`, `cut`, `sort`, `getent`, `id`, `restorecon`,
/// `semanage`, `augenrules`, `dirname`, `basename`, `head`, `hostname` …).
fn check_no_flags_at_all(program: &str, args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.starts_with('-') {
            return Err(flag_not_allowed(program, arg));
        }
    }
    Ok(())
}

/// The bootstrap's copy template — the only shape `install` may ever get. The
/// helper binary must come from a root-owned, non-user-writable source
/// (`verify_trusted_binary`): a user-writable file installed as the root
/// helper would hand root rights to whoever replaces that file later, since
/// PolicyKit authorises exactly this path. Nothing works without `-o root
/// -g root` and the fixed destination.
fn check_install(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "install";
    const HELPER_DEST: &str = "/usr/local/libexec/lcp-admin";

    let mut mode_is_correct = false;
    let mut owner = false;
    let mut group = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            // the binary is installed 0755, owned by root:root — no other
            // mode, no other owner
            "-m" => {
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                if mode_is_correct {
                    return Err(operand_not_allowed(PROGRAM, "-m", "-m is given more than once"));
                }
                if value != "0755" {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        &value,
                        "the mode of the installed helper must be 0755",
                    ));
                }
                mode_is_correct = true;
            }
            "-o" => {
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                if owner {
                    return Err(operand_not_allowed(PROGRAM, "-o", "-o is given more than once"));
                }
                if value != "root" {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        &value,
                        "the installed helper must be owned by root",
                    ));
                }
                owner = true;
            }
            "-g" => {
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                if group {
                    return Err(operand_not_allowed(PROGRAM, "-g", "-g is given more than once"));
                }
                if value != "root" {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        &value,
                        "the installed helper must be owned by group root",
                    ));
                }
                group = true;
            }
            // no -d (directories go through mkdir), no other flag
            _ if arg.starts_with('-') => return Err(flag_not_allowed(PROGRAM, arg)),
            _ => {
                if operands.len() >= 2 {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "install takes only a source and a destination",
                    ));
                }
                operands.push(arg);
            }
        }
        index += 1;
    }

    let [source, destination] = &operands[..] else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "install needs exactly '<trusted source> /usr/local/libexec/lcp-admin'",
        ));
    };
    if *destination != HELPER_DEST {
        return Err(operand_not_allowed(
            PROGRAM,
            destination,
            &format!("the destination of install must be {HELPER_DEST}"),
        ));
    }
    if !mode_is_correct || !owner || !group {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "install requires -m 0755 -o root -g root",
        ));
    }
    ensure_trusted_path(PROGRAM, source, "helper source")?;
    // the copy must start out trustworthy: root owned, not user writable
    verify_trusted_binary(source)?;
    Ok(())
}

/// The helper's named-action calls (the first slice of the phase 3 CLI):
/// `lcp-admin <action> [params]`. Actions so far: `install_polkit_rule`
/// (content via stdin, see the action in helper/src/main.rs).
const HELPER_ACTIONS: &[&str] = &["install_polkit_rule"];

fn check_helper_action(args: &[String]) -> Result<(), String> {
    let [action] = args else {
        return Err(operand_not_allowed(
            "lcp-admin",
            "",
            "exactly one action name must be given",
        ));
    };
    if !HELPER_ACTIONS.contains(&action.as_str()) {
        return Err(operand_not_allowed(
            "lcp-admin",
            action,
            "this action does not exist",
        ));
    }
    Ok(())
}

/// The per-program argument policy. Enforced in both directions — by the app
/// (before the call is even built) and inside the helper (last gate, before
/// the program is executed).
pub fn ensure_arguments_for_program(program: &str, args: &[String]) -> Result<(), String> {
    let base = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    match base.as_str() {
        "systemctl" => check_systemctl(args),
        "loginctl" => check_loginctl(args),
        "dnf" | "dnf5" => check_dnf(args),
        "rpm" => check_rpm(args),
        "apt-get" | "apt" => check_apt(args),
        "add-apt-repository" => check_add_apt_repository(args),
        "flatpak" => check_flatpak(args),
        "ip" => check_ip(args),
        "certbot" => check_certbot(args),
        "useradd" => check_useradd(args),
        "userdel" => check_userdel(args),
        "usermod" => check_usermod(args),
        "groupadd" | "groupdel" | "groupmod" => check_group_tools(base.as_str(), args),
        "smartctl" => check_smartctl(args),
        "modprobe" | "rmmod" => check_module_tools(base.as_str(), args),
        "fail2ban-client" => check_fail2ban(args),
        "setsebool" => check_setsebool(args),
        "semodule" => check_semodule(args),
        "ausearch" => check_ausearch(args),
        "firewall-cmd" => check_firewall_cmd(args),
        "nmcli" => check_nmcli(args),
        "renice" => check_renice(args),
        "nginx" => check_daemon_flags("nginx", args, &["-t", "-v", "-q"]),
        "sshd" => check_daemon_flags("sshd", args, &["-T", "-t"]),
        "mkdir" => check_mkdir(args),
        "tail" | "head" => check_log_reader(base.as_str(), args, &[]),
        "cat" => check_log_reader("cat", args, &[]),
        "ls" => check_log_reader("ls", args, &["-l", "-a", "-la", "-lh", "-A"]),
        "stat" | "readlink" | "zcat" => check_log_reader(base.as_str(), args, &[]),
        "grep" => check_grep(args),
        "install" => check_install(args),
        "lcp-admin" => check_helper_action(args),
        _ => check_no_flags_at_all(base.as_str(), args),
    }
}

#[cfg(test)]
mod argument_policy_tests {
    use super::ensure_arguments_for_program as check;

    fn ok(program: &str, args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check(program, &argv).is_ok(), "must be allowed: {program} {args:?}");
    }

    fn refused(program: &str, args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check(program, &argv).is_err(), "must be refused: {program} {args:?}");
    }

    // ── requirement 1: no low ids or duplicate ids for user/group tools ──────

    #[test]
    fn user_tools_reject_non_unique_ids_and_system_range_ids() {
        // -o / --non-unique: duplicate ids open a privilege confusion
        refused("useradd", &["-o", "-u", "1500", "marvin"]);
        refused("usermod", &["--non-unique", "-u", "1500", "marvin"]);
        refused("groupadd", &["-o", "-g", "1500", "deployers"]);
        // ids below 1000 belong to system accounts
        refused("useradd", &["-u", "0", "marvin"]);
        refused("useradd", &["-u", "999", "marvin"]);
        refused("usermod", &["-u", "499", "marvin"]);
        refused("groupadd", &["-g", "10", "deployers"]);
        refused("groupmod", &["-g", "999", "deployers"]);
        // invalid ids are refused too
        refused("useradd", &["-u", "10abc", "marvin"]);
        refused("groupadd", &["-g", "-", "deployers"]);
    }

    #[test]
    fn user_tools_accept_the_real_shapes_and_reject_unlisted_flags() {
        ok("useradd", &["-m", "-c", "Some Name", "newuser"]);
        ok("useradd", &["-u", "1500", "-m", "newuser"]);
        ok("userdel", &["--force", "goneuser"]);
        ok("usermod", &["-aG", "wheel", "newuser"]);
        ok("usermod", &["-L", "newuser"]);
        ok("groupadd", &["deployers"]);
        ok("groupmod", &["-n", "renamed-group", "old-group"]);
        refused("useradd", &["-P", "newuser"]);
        refused("useradd", &["-c", "Ok Name", "newuser", "-P"]);
        refused("useradd", &["/slash-start"]);
        refused("usermod", &["--prefix", "/home", "newuser"]);
        refused("userdel", &["-r", "goneuser"]);
        refused("usermod", &["-aG"]);
        refused("useradd", &["-m"]);
        // comments are printable words only
        refused("useradd", &["-c", "-leading-dash", "newuser"]);
        refused("useradd", &["-c", "a\nb", "newuser"]);
        ok("useradd", &["-c", "Sorensen (test)", "newuser"]);
    }

    // ── requirement 2: systemctl ─────────────────────────────────────────────

    #[test]
    fn systemctl_only_allows_the_documented_verbs_and_unit_names() {
        for verb in [
            "start", "stop", "restart", "reload", "enable", "disable", "status",
            "is-active", "is-enabled",
        ] {
            ok("systemctl", &[verb, "nginx.service"]);
        }
        ok("systemctl", &["enable", "--now", "cron-daily.timer"]);
        ok("systemctl", &["daemon-reload"]);
        refused("systemctl", &["daemon-reload", "nginx"]);
        ok("systemctl", &["is-active", "auditd"]);
        // exec-adjacent / editor / unit-file verbs are banned
        for verb in ["link", "edit", "import-environment", "set-environment", "mask", "unmask"] {
            refused("systemctl", &[verb, "some.service"]);
        }
        // any argument containing '/' is banned (unit file paths)
        refused("systemctl", &["start", "/opt/evil.service"]);
        refused("systemctl", &["status", "dev/sda.service"]);
    }

    #[test]
    fn systemctl_rejects_other_flags_and_bad_unit_names() {
        refused("systemctl", &["--user", "status", "nginx"]);
        refused("systemctl", &["start", "nginx;id"]);
        refused("systemctl", &["daemon-reload", "nginx"]);
        refused("systemctl", &["status", "nginx\n.service"]);
    }

    // ── requirement 3: dnf/rpm ───────────────────────────────────────────────

    #[test]
    fn dnf_and_rpm_refuse_local_files_and_foreign_roots() {
        // any '/' or archive-suffix operand
        refused("dnf", &["install", "/tmp/payload.rpm"]);
        refused("dnf", &["install", "sub/name"]);
        refused("dnf", &["install", "payload.rpm"]);
        refused("rpm", &["-i", "/tmp/payload.rpm"]);
        refused("rpm", &["-i", "payload.rpm"]);
        refused("rpm", &["-i", "../shadow"]);
        // --installroot / --define / --eval
        for flag in [
            "--installroot",
            "--installroot=/srv/evil",
            "--define",
            "--define=_x=1",
            "--eval",
            "--eval=%(id)",
        ] {
            refused("dnf", &[flag]);
            refused("rpm", &[flag]);
        }
        // --setopt restricted to the timeout knob
        refused("dnf", &["--setopt=x", "install", "pack"]);
        refused("dnf", &["--setopt=reposdir=/tmp/evil", "install", "pack"]);
    }

    #[test]
    fn dnf_accepts_the_real_shapes_and_rejects_foreign_verbs_and_flags() {
        ok("dnf", &["install", "--only-upgrade", "mesa-libGL"]);
        ok("dnf", &[
            "upgrade",
            "--best",
            "--assumeno",
            "--setopt=timeout=120",
            "mesa-libGL",
        ]);
        ok("dnf", &["history", "undo", "42", "-y"]);
        ok("dnf", &["clean", "all"]);
        ok("dnf", &["clean", "expire-cache", "--repo", "fedora"]);
        ok("dnf", &["autoremove", "-y"]);
        ok("dnf", &["makecache"]);
        refused("dnf", &["reinstall", "pack"]);
        refused("dnf", &["clean", "all", "--allowerasing"]);
        refused("dnf", &["--repo=fedora", "clean", "all"]);
        refused("dnf", &["--repo"]);
        refused("rpm", &["--queryformat", "%{}"]);
        refused("rpm", &["-e", "pack"]);
        ok("rpm", &["-q", "systemd"]);
        ok("rpm", &["--quiet", "-q", "systemd"]);

        // copr and config-manager --add-repo are refused until phase 3
        refused("dnf", &["copr", "enable", "owner/repo", "-y"]);
        refused("dnf", &["copr", "disable", "owner/repo", "-y"]);
        refused("dnf", &["config-manager", "--add-repo", "https://example.com/x.repo"]);
    }

    // ── requirement 4: ip ────────────────────────────────────────────────────

    #[test]
    fn ip_only_allows_listed_subcommands_and_never_netns_or_batch() {
        for sub in ["link", "addr", "route", "neigh"] {
            ok("ip", &[sub, "show"]);
        }
        ok("ip", &["link", "set", "dev", "ens3", "up"]);
        ok("ip", &["-o", "link", "show"]);
        refused("ip", &["netns", "exec", "something"]);
        refused("ip", &["netns", "list"]);
        refused("ip", &["-batch", "/tmp/evil"]);
        refused("ip", &["tunnel", "show"]);
        refused("ip", &["xfrm", "policy"]);
        refused("ip", &["--args"]);
    }

    // ── requirement 5: certbot ───────────────────────────────────────────────

    #[test]
    fn certbot_refuses_hooks_in_any_argument() {
        refused("certbot", &["renew", "--pre-hook", "bad"]);
        refused("certbot", &["--pre-hook=bad"]);
        refused("certbot", &["renew", "--hook-script", "evil"]);
        refused("certbot", &["renew", "--cert-name", "subhooked.example.com"]);
        // the real renewal shape still passes
        ok("certbot", &["renew", "--cert-name", "example.com", "--non-interactive"]);
    }

    // ── path rules ──────────────────────────────────────────────────────────

    #[test]
    fn path_operands_must_be_absolute_without_relative_parts_or_symlinks() {
        refused("cat", &["../etc/shadow"]);
        refused("cat", &["etc/shadow"]);
        refused("cat", &["/etc/../../shadow"]);
        refused("semodule", &["-i", "somefile.pp"]);
        refused("mkdir", &["-p", "relative/dir"]);
        refused("tail", &["-n", "5", "untrusted.log"]);
        ok("cat", &["/etc/hostname"]);
        ok("tail", &["-n", "15000", "/var/log/nginx/access.log"]);
        ok("ls", &["/etc/audit/rules.d/99-control-panel.rules"]);
        ok("mkdir", &["-p", "/etc/nginx/sites-enabled"]);
    }

    // ── everything else: no dash arguments ───────────────────────────────────

    #[test]
    fn dash_arguments_are_refused_for_programs_without_a_flag_list() {
        refused("ufw", &["--force", "enable"]);
        refused("journalctl", &["--no-pager"]);
        refused("getent", &["--dash"]);
        refused("head", &["-o", "/etc/passwd"]);
        ok("ufw", &["default", "deny", "outgoing"]);
        ok("getent", &["passwd", "someuser"]);
        ok("hostname", &[]);
    }

    // ── the remaining real call shapes ───────────────────────────────────────

    #[test]
    fn the_remaining_real_call_shapes_are_reviewed() {
        ok("smartctl", &["-a", "-j", "/dev/sda"]);
        ok("smartctl", &["-t", "short", "/dev/nvme0n1"]);
        ok("smartctl", &["-H", "-i", "/dev/vdb"]);
        refused("smartctl", &["-a", "sda"]); // device without its /dev prefix
        refused("smartctl", &["-t", "dangerous", "/dev/sda"]);
        refused("smartctl", &["--all", "/dev/sda"]);

        ok("modprobe", &["8021q"]);
        ok("rmmod", &["floppy"]);
        refused("modprobe", &["--force"]);

        ok("fail2ban-client", &["status"]);
        ok("fail2ban-client", &["status", "sshd"]);
        ok("fail2ban-client", &["set", "sshd", "banip", "192.0.2.1"]);
        ok("fail2ban-client", &["set", "sshd", "unbanip", "::1"]);
        refused("fail2ban-client", &["set", "sshd", "addjail", "new"]);
        refused("fail2ban-client", &["-bogus"]);
        refused("fail2ban-client", &["status", "weird.name"]);

        ok("setsebool", &["-P", "ftpd_full_access", "on"]);
        ok("setsebool", &["httpd_can_network_connect", "0"]);
        refused("setsebool", &["-P", "ftpd_full_access", "maybe"]);
        refused("setsebool", &["-P", "ftpd_full_access"]);
        refused("setsebool", &["-P"]);
        refused("setsebool", &["ftpd_full_access", "on", "extra"]);

        ok("semodule", &["-i", "/tmp/audit2allow/mod.pp"]);
        refused("semodule", &["-B"]);
        refused("semodule", &["-i"]);
        // an existing symlinked prefix must not be usable to escape
        refused("semodule", &["-i", "/proc/self/root/evil.pp"]);

        ok("ausearch", &["-m", "AVC", "-ts", "recent"]);

        ok("firewall-cmd", &["--zone", "public", "--add-port=8080/tcp"]);
        ok("firewall-cmd", &["--panic-on"]);
        ok("firewall-cmd", &["--zone", "home", "--permanent"]);
        refused("firewall-cmd", &["--zone", "public", "--add-service=<b>"]);
        refused("firewall-cmd", &["--zone", "public", "--add-port=;id"]);
        refused("firewall-cmd", &["-X", "public"]);

        ok("nmcli", &["-t", "-f", "UUID,NAME,TYPE,DEVICE,STATE", "connection", "show"]);
        refused("nmcli", &["--bogus"]);
        refused("nmcli", &["-f", "this-field-list-is-deliberately-well-over-the-sixty-four-character-limit-for-test-purposes"]);

        ok("renice", &["-n", "5", "-p", "4421"]);
        ok("renice", &["-n", "-5", "-p", "4421"]);
        refused("renice", &["-n", "25", "-p", "4421"]);
        refused("renice", &["-n", "5", "-p", "4421$", "x"]);

        ok("loginctl", &["kill-session", "42"]);
        refused("loginctl", &["terminate", "42"]);
        refused("loginctl", &["kill-session", "not-a-number"]);

        ok("nginx", &["-t"]);
        ok("nginx", &["-v"]);
        ok("sshd", &["-T"]);
        refused("sshd", &["-h", "/tmp/evil_host_key"]);
        refused("sshd", &["-T", "footgun"]);

        ok("flatpak", &["uninstall", "--system", "-y", "app/com.example.App/x86_64/stable"]);
        refused("flatpak", &["install", "--from", "evil.flatpakref"]);

        ok("apt-get", &["update"]);
        ok("apt-get", &["remove", "-y", "pack"]);
        ok("apt-get", &["clean"]);
        refused("apt-get", &["install", "pack", "-y", "--autoremove"]);

        ok("add-apt-repository", &["-y", "ppa:some-owner/some-ppa"]);
        ok("add-apt-repository", &["--remove", "-y", "ppa:some-owner/some-ppa"]);
        refused("add-apt-repository", &["-y", "--x-www", "ppa:some-owner/some-ppa"]);
        refused("add-apt-repository", &["-y", "ppa:some owner"]);

        ok("grep", &["-i", "pattern", "/var/log/messages"]);
        ok("grep", &["-m", "3", "-q", "pattern", "/var/log/messages"]);
        refused("grep", &["--all-match", "pattern", "/var/log/messages"]);
        refused("grep", &["-i", "pattern", "untrusted.log"]);
        refused("grep", &["-i"]);
    }

    // ── the bootstrap copy template (install) and the helper action ──────────

    #[test]
    fn install_only_accepts_the_strict_helper_template() {
        // /usr/bin/cat is a real, root-owned, non-user-writable file — dnf
        // would be a symlink and rightly refused by ensure_trusted_path
        ok("install", &[
            "-m", "0755",
            "-o", "root",
            "-g", "root",
            "/usr/bin/cat",
            "/usr/local/libexec/lcp-admin",
        ]);
        // flags may come in any order — the app sends it in this order
        ok("install", &[
            "-o", "root", "-g", "root", "-m", "0755",
            "/usr/bin/cat", "/usr/local/libexec/lcp-admin",
        ]);
        // wrong mode / owner / group
        refused("install", &["-m", "0644", "-o", "root", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        refused("install", &["-m", "0755", "-o", "bin", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        refused("install", &["-m", "0755", "-o", "root", "-g", "bin", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        refused("install", &["-o", "root", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        refused("install", &["-m", "0755", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        refused("install", &["-m", "0755", "-o", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        // foreign destinations and extra operands are refused
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/bin/cat", "/etc/shadow"]);
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/other"]);
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin", "extra"]);
        // no -d, no recursive/dereference flags
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "-d", "/usr/local/libexec"]);
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "-D", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"]);
        // a single operand is not the template
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/local/libexec/lcp-admin"]);
        // missing operands entirely
        refused("install", &["-m", "0755", "-o", "root", "-g", "root"]);
        // the src must be an absolute path in the first place
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "relative/lcp-admin", "/usr/local/libexec/lcp-admin"]);
        // a symlinked source (dnf → dnf-3 on Fedora) is refused
        refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/bin/dnf", "/usr/local/libexec/lcp-admin"]);
    }

    #[test]
    fn helper_action_calls_accept_only_known_actions() {
        ok("lcp-admin", &["install_polkit_rule"]);
        refused("lcp-admin", &[]);
        refused("lcp-admin", &["install_polkit_rule", "extra"]);
        refused("lcp-admin", &["some_other_action"]);
        refused("lcp-admin", &["-rf", "/x"]);
        refused("lcp-admin", &["/usr/bin/dnf"]);
    }
}
