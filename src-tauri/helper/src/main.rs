//! `lcp-admin` — the small, root owned helper that performs privileged
//! actions for the Control Panel.
//!
//! The app never runs a program with root rights itself. It asks the desktop's
//! PolicyKit agent to authorise *this* helper, and the agent runs it as root.
//! Because the helper is the only thing that is ever authorised, a cached
//! authorisation can not be used to run `dnf`, `systemctl` or a shell as root
//! by some other program of the same user.
//!
//! The helper re-validates everything it is asked to do — the program has to
//! be on the allow list and the arguments have to be free of control
//! characters. A shell is never started with `-c`.
//!
//! Usage: `lcp-admin <program> [args…]`; named actions:
//! `lcp-admin <action> [params]` (see `ROOT_ACTIONS`).

#[path = "../../src/utils/root_policy.rs"]
#[allow(dead_code)]
mod root_policy;

use root_policy::{
    ensure_arguments_for_program, ensure_no_shell_escape, ensure_program_allowed,
    ensure_safe_arguments, ensure_repo_file_content, ensure_repo_file_path, resolve_root_program,
    restore_profile_sh_backup_shape, revision_sibling_shape, verify_trusted_binary,
    is_valid_account_name, nginx_config_remove_path_valid, nginx_config_path_valid,
    nginx_link_as_valid, nginx_site_unlink_path_valid, validate_nginx_config_content,
    validate_profile_sh_content, validate_profile_sh_path, var_www_is_protected,
    firewall_rich_rule_valid, firewall_port_value_valid, firewall_service_name_valid,
    firewall_zone_name_valid, firewall_interface_name_valid, FirewallRichRule,
    FirewallRichRuleAction, FirewallRichRuleElement, FirewallRichRuleFamily,
    var_www_path_valid, NGINX_BACKUPS_DIR, NGINX_CONFIG_CONTENT_MAX, NGINX_SITES_AVAILABLE,
    NGINX_SITES_ENABLED, NGINX_WWW_INSTALL_MAX, PROFILE_CONTENT_MAX, POLKIT_RULE_FILE,
    POLKIT_RULE_TEXT, PROFILE_D_DIR, REPO_CONTENT_MAX, VAR_WWW_DIR,
};

use rustix::fs::{AtFlags, CWD, Gid, Mode, OFlags, Stat, Uid};
use rustix::fd::OwnedFd;
use rustix::fd::AsFd;

/// Where the helper reads local account records from. A
/// `--features test-fixture` build reads the fixture twin instead — the
/// production build has no fixture path compiled in.
#[cfg(not(feature = "test-fixture"))]
const PASSWD_FILE: &str = "/etc/passwd";
#[cfg(feature = "test-fixture")]
const PASSWD_FILE: &str = root_policy::FIXTURE_PASSWD_FILE;

/// The uid/gid the helper treats as the privileged owner of root-owned
/// destination directories (profile.d, repos.d). In production the helper
/// always runs as root (uid/gid 0). A `--features test-fixture` build runs as
/// the invoking test user — whose uid/gid stand in for root there — so the
/// very same ownership checks stay exerciseable against fixture directories.
#[cfg(not(feature = "test-fixture"))]
fn privileged_uid() -> u32 {
    0
}
#[cfg(not(feature = "test-fixture"))]
fn privileged_gid() -> u32 {
    0
}
#[cfg(feature = "test-fixture")]
fn privileged_uid() -> u32 {
    rustix::process::getuid().as_raw()
}
#[cfg(feature = "test-fixture")]
fn privileged_gid() -> u32 {
    rustix::process::getgid().as_raw()
}

/// The firewall tooling is addressed by absolute path, never by PATH. The
/// fixture build swaps in a stub recording the argv it receives and exits
/// without touching any real firewall.
#[cfg(not(feature = "test-fixture"))]
const FIREWALL_CMD_BIN: &str = "/usr/bin/firewall-cmd";
#[cfg(feature = "test-fixture")]
const FIREWALL_CMD_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/firewall-cmd";
#[cfg(not(feature = "test-fixture"))]
const UFW_CANDIDATES: [&str; 2] = ["/usr/sbin/ufw", "/usr/bin/ufw"];
#[cfg(feature = "test-fixture")]
const UFW_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/ufw"];

/// The service manager for enabling the firewalld unit. Addressed by
/// absolute path; the fixture build swaps in a stub so tests never touch the
/// host's init system.
#[cfg(not(feature = "test-fixture"))]
const SYSTEMCTL_BIN: &str = "/usr/bin/systemctl";
#[cfg(feature = "test-fixture")]
const SYSTEMCTL_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/systemctl";

/// `ss` is used for best-effort SSH session detection only. The fixture
/// build swaps in a stub so tests can pin the guard behavior without ever
/// touching the real socket table.
#[cfg(not(feature = "test-fixture"))]
const SS_CANDIDATES: [&str; 2] = ["/usr/bin/ss", "/bin/ss"];
#[cfg(feature = "test-fixture")]
const SS_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/ss"];

fn ss_bin() -> Option<&'static str> {
    SS_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

/// Package-manager binaries and lock locations are addressed by absolute
/// candidate paths. The fixture build swaps in stubs so tests never touch the
/// host package databases or run a real transaction.
#[cfg(not(feature = "test-fixture"))]
const DNF_CANDIDATES: [&str; 2] = ["/usr/bin/dnf", "/bin/dnf"];
#[cfg(feature = "test-fixture")]
const DNF_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/dnf"];
#[cfg(not(feature = "test-fixture"))]
const APT_GET_CANDIDATES: [&str; 2] = ["/usr/bin/apt-get", "/bin/apt-get"];
#[cfg(feature = "test-fixture")]
const APT_GET_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/apt-get"];
#[cfg(not(feature = "test-fixture"))]
const DF_CANDIDATES: [&str; 2] = ["/usr/bin/df", "/bin/df"];
#[cfg(feature = "test-fixture")]
const DF_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/df"];
#[cfg(not(feature = "test-fixture"))]
const UNAME_CANDIDATES: [&str; 2] = ["/usr/bin/uname", "/bin/uname"];
#[cfg(feature = "test-fixture")]
const UNAME_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/uname"];

/// Security-auditor files all live under this root; the fixture build swaps it
/// below /tmp so tests never touch /etc or /tmp permissions.
#[cfg(not(feature = "test-fixture"))]
const SYSTEMD_UNIT_ROOT: &str = "/etc/systemd/system";
#[cfg(feature = "test-fixture")]
const SYSTEMD_UNIT_ROOT: &str = "/tmp/lcp-admin-test-fixture/systemd/system";
#[cfg(not(feature = "test-fixture"))]
const SECURITY_ETC_ROOT: &str = "/etc";
#[cfg(feature = "test-fixture")]
const SECURITY_ETC_ROOT: &str = "/tmp/lcp-admin-test-fixture/etc";
#[cfg(not(feature = "test-fixture"))]
const SECURITY_TMP_DIR: &str = "/tmp";
#[cfg(feature = "test-fixture")]
const SECURITY_TMP_DIR: &str = "/tmp/lcp-admin-test-fixture/tmp";
#[cfg(not(feature = "test-fixture"))]
const SSHD_CANDIDATES: [&str; 2] = ["/usr/sbin/sshd", "/usr/bin/sshd"];
#[cfg(feature = "test-fixture")]
const SSHD_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/sshd"];
#[cfg(not(feature = "test-fixture"))]
const SYSCTL_CANDIDATES: [&str; 2] = ["/usr/sbin/sysctl", "/usr/bin/sysctl", ];
#[cfg(feature = "test-fixture")]
const SYSCTL_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/sysctl"];
#[cfg(not(feature = "test-fixture"))]
const AUTHSELECT_CANDIDATES: [&str; 2] = ["/usr/bin/authselect", "/bin/authselect"];
#[cfg(feature = "test-fixture")]
const AUTHSELECT_CANDIDATES: [&str; 1] = ["/tmp/lcp-admin-test-fixture/bin/authselect"];

fn security_path(rel: &str) -> String {
    format!("{SECURITY_ETC_ROOT}/{rel}")
}

fn sshd_bin() -> Option<&'static str> {
    SSHD_CANDIDATES.iter().find(|p| std::path::Path::new(p).is_file()).copied()
}
fn sysctl_bin() -> Option<&'static str> {
    SYSCTL_CANDIDATES.iter().find(|p| std::path::Path::new(p).is_file()).copied()
}
fn authselect_bin() -> Option<&'static str> {
    AUTHSELECT_CANDIDATES.iter().find(|p| std::path::Path::new(p).is_file()).copied()
}

/// Write one fixed security file atomically: reject symlinks, back up an
/// existing file, write a sibling temp file and rename it into place.
/// Every file the security actions may read, write or remove. No operation
/// accepts a path from the caller; this list is the second line of defence
/// against an accidental future path construction bug.
const SECURITY_MANAGED_FILES: &[&str] = &[
    "ssh/sshd_config",
    "ssh/sshd_config.d/50-idle-timeout.conf",
    "ssh/sshd_config.d/50-banner.conf",
    "sysctl.d/99-hardening.conf",
    "login.defs",
    "tmpfiles.d/tmp-sticky.conf",
    "security/faillock.conf.d/50-hardening.conf",
    "modprobe.d/disable-unused-fs.conf",
    "issue",
    "issue.net",
    "passwd",
    "shadow",
];

/// Resolve one managed relative file, refusing any path outside the list or
/// whose base/parent components are symbolic links (so /etc/ssh symlinked
/// elsewhere cannot redirect a root write).
fn security_checked_path(rel: &str) -> Result<std::path::PathBuf, String> {
    if !SECURITY_MANAGED_FILES.contains(&rel) {
        return Err(format!("lcp-admin: unmanaged security file: {rel}"));
    }
    let base = std::path::Path::new(SECURITY_ETC_ROOT);
    if base
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(format!("lcp-admin: refusing a symlinked managed root: {SECURITY_ETC_ROOT}"));
    }
    let mut current = base.to_path_buf();
    for component in rel.split('/') {
        current.push(component);
        if let Ok(metadata) = current.symlink_metadata() {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "lcp-admin: refusing a managed path containing a symbolic link: {}",
                    current.display()
                ));
            }
        }
    }
    Ok(current)
}

fn security_write_file_impl(rel: &str, content: &str, backup: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let checked = security_checked_path(rel)?;
    let path = checked.to_string_lossy().to_string();
    let existing_mode = std::fs::metadata(&path)
        .ok()
        .map(|metadata| metadata.permissions().mode());
    if std::path::Path::new(&path).symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("lcp-admin: refusing to write through a symbolic link: {path}"));
    }
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("lcp-admin: could not create {} ({e})", parent.display()))?;
    }
    if backup && std::path::Path::new(&path).is_file() {
        let backup = format!("{path}.control-panel.bak");
        let _ = std::fs::copy(&path, &backup);
    }
    let tmp = format!("{path}.tmp.{}", std::process::id());
    std::fs::write(&tmp, content)
        .map_err(|e| format!("lcp-admin: could not write {tmp} ({e})"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = existing_mode.unwrap_or(0o644) & 0o7777;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode));
    }
    security_ensure_root_owner(&tmp)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("lcp-admin: could not replace {path} ({e})"));
    }
    Ok(())
}

fn security_write_file(rel: &str, content: &str) -> Result<(), String> {
    security_write_file_impl(rel, content, true)
}

fn security_read_file(rel: &str) -> Result<String, String> {
    let path = security_checked_path(rel)?;
    std::fs::read_to_string(&path)
        .map_err(|e| format!("lcp-admin: could not read {} ({e})", path.display()))
}

fn security_remove_file(rel: &str) -> Result<(), String> {
    let path_buf = security_checked_path(rel)?;
    let path = path_buf.to_string_lossy().to_string();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("lcp-admin: could not remove {path} ({e})")),
    }
}

/// Restore the pre-edit content without overwriting the preserved backup.
fn security_restore_file(rel: &str, content: &str) -> Result<(), String> {
    security_write_file_impl(rel, content, false)
}

fn security_ensure_root_owner(path: &str) -> Result<(), String> {
    #[cfg(feature = "test-fixture")]
    {
        let _ = path;
        Ok(())
    }
    #[cfg(not(feature = "test-fixture"))]
    {
        std::os::unix::fs::chown(path, Some(0), Some(0))
            .map_err(|e| format!("lcp-admin: could not set root ownership on {path} ({e})"))
    }
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

fn reload_ssh(required: bool) -> Result<(), String> {
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return if required {
            Err("lcp-admin: systemctl is not available".to_string())
        } else {
            Ok(())
        };
    }
    for unit in ["sshd", "ssh"] {
        if command_succeeds(SYSTEMCTL_BIN, &["is-active", "--quiet", unit]) {
            return run_firewall_program(SYSTEMCTL_BIN, &["reload", unit], "systemctl");
        }
    }
    Ok(())
}

/// Rewrite a line-oriented config: drop lines whose first token (ignoring a
/// leading '#') matches one of `keys`, then append the supplied settings.
fn rewrite_settings(
    rel: &str,
    keys: &[&str],
    append: &[(&str, &str)],
) -> Result<(), String> {
    let checked = security_checked_path(rel)?;
    let original = match std::fs::read_to_string(&checked) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("lcp-admin: could not read {} ({e})", checked.display())),
    };
    let mut kept: Vec<String> = Vec::new();
    for line in original.lines() {
        let trimmed = line.trim_start();
        let without_comment = trimmed.trim_start_matches('#').trim_start();
        let first = without_comment.split_whitespace().next().unwrap_or("");
        if keys.iter().any(|key| first.eq_ignore_ascii_case(key)) {
            continue;
        }
        kept.push(line.to_string());
    }
    for (key, value) in append {
        kept.push(format!("{key} {value}"));
    }
    let mut content = kept.join("\n");
    content.push('\n');
    security_write_file(rel, &content)
}

/// User/group tools, addressed by absolute candidate paths. The fixture build
/// swaps every one for a logging stub so no account or session is touched.
#[cfg(not(feature = "test-fixture"))]
const USER_TOOL_CANDIDATES: &[&[&str]] = &[
    &["/usr/sbin/useradd", "/usr/bin/useradd"],
    &["/usr/sbin/userdel", "/usr/bin/userdel"],
    &["/usr/sbin/usermod", "/usr/bin/usermod"],
    &["/usr/bin/gpasswd", "/usr/bin/gpasswd"],
    &["/usr/bin/passwd", "/usr/bin/passwd"],
    &["/usr/sbin/chpasswd", "/usr/bin/chpasswd"],
    &["/usr/sbin/groupadd", "/usr/bin/groupadd"],
    &["/usr/sbin/groupdel", "/usr/bin/groupdel"],
    &["/usr/bin/loginctl", "/bin/loginctl"],
];
#[cfg(feature = "test-fixture")]
const USER_TOOL_CANDIDATES: &[&[&str]] = &[
    &["/tmp/lcp-admin-test-fixture/bin/useradd"],
    &["/tmp/lcp-admin-test-fixture/bin/userdel"],
    &["/tmp/lcp-admin-test-fixture/bin/usermod"],
    &["/tmp/lcp-admin-test-fixture/bin/gpasswd"],
    &["/tmp/lcp-admin-test-fixture/bin/passwd"],
    &["/tmp/lcp-admin-test-fixture/bin/chpasswd"],
    &["/tmp/lcp-admin-test-fixture/bin/groupadd"],
    &["/tmp/lcp-admin-test-fixture/bin/groupdel"],
    &["/tmp/lcp-admin-test-fixture/bin/loginctl"],
];

fn user_tool(index: usize) -> Option<&'static str> {
    USER_TOOL_CANDIDATES
        .get(index)?
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

fn require_user_tool(index: usize, label: &str) -> Result<&'static str, String> {
    user_tool(index)
        .ok_or_else(|| format!("lcp-admin: {label} is not available on this system"))
}
#[cfg(not(feature = "test-fixture"))]
const PACKAGE_LOCK_PATHS: &[&str] = &[
    "/run/dnf.pid",
    "/run/dnf.lock",
    "/var/cache/dnf/dnf.pid",
    "/var/lib/dpkg/lock-frontend",
    "/var/lib/dpkg/lock",
    "/var/lib/apt/lists/lock",
];
#[cfg(feature = "test-fixture")]
const PACKAGE_LOCK_PATHS: &[&str] = &["/tmp/lcp-admin-test-fixture/ctl/pkg-lock"];
const PACKAGE_MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

fn dnf_bin() -> Option<&'static str> {
    DNF_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

fn apt_get_bin() -> Option<&'static str> {
    APT_GET_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

fn df_bin() -> Option<&'static str> {
    DF_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

fn uname_bin() -> Option<&'static str> {
    UNAME_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

/// One change that can interrupt the administrator's SSH session. The
/// variants map to the cases approved for GROUP 2 of the firewall
/// migration. The guard runs in the helper, before any firewall command —
/// the app-side guard alone cannot protect against a direct helper call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirewallSshGuard {
    RemoveServiceSsh,
    RemovePort22,
    RichDropSsh,
    RichRejectSsh,
    MoveSshInterface,
}

impl FirewallSshGuard {
    fn message(self) -> &'static str {
        match self {
            Self::RemoveServiceSsh => "removing the ssh firewall service",
            Self::RemovePort22 => "removing port 22 from the firewall",
            Self::RichDropSsh => "adding a firewall drop rule for ssh",
            Self::RichRejectSsh => "adding a firewall reject rule for ssh",
            Self::MoveSshInterface => "moving the interface carrying the active SSH session \
                 into a zone that does not permit ssh",
        }
    }
}

/// The confirmation trailing argument: only this exact literal authorizes a
/// protected operation. Its absence is never confirmation.
const FIREWALL_CONFIRM_ARG: &str = "confirm";

/// True when a firewalld port string (`22/tcp`) covers port 22. Unparseable
/// input cannot reach this point (the validators run first), but a malformed
/// value is still treated as covering 22: the guard must fail closed.
fn firewall_port_covers_22(value: &str) -> bool {
    let Some((ports, _protocol)) = value.split_once('/') else {
        return true;
    };
    const SSH_PORT: u16 = 22;
    let (start, end) = ports.split_once('-').map_or((ports, ports), |(s, e)| (s, e));
    let start: u16 = start.parse().unwrap_or(SSH_PORT);
    let end: u16 = end.parse().unwrap_or(SSH_PORT);
    start <= SSH_PORT && SSH_PORT <= end
}

/// The GROUP 2 SSH lockout guard for a zone rule. Removes only; adds never
/// lock the administrator out.
fn firewall_zone_rule_guard(kind: &str, value: &str, add: bool) -> Option<FirewallSshGuard> {
    if add {
        return None;
    }
    match kind {
        "service" if value == "ssh" => Some(FirewallSshGuard::RemoveServiceSsh),
        "port" if firewall_port_covers_22(value) => Some(FirewallSshGuard::RemovePort22),
        _ => None,
    }
}

/// The GROUP 2 SSH lockout guard for a rich rule: an added drop or reject
/// targeting the ssh service or a port range covering 22.
fn firewall_rich_rule_guard(
    add: bool,
    parsed: &root_policy::FirewallRichRule,
) -> Option<FirewallSshGuard> {
    use root_policy::{FirewallRichRuleAction, FirewallRichRuleElement};
    if !add {
        return None;
    }
    let targets_ssh = match &parsed.element {
        FirewallRichRuleElement::Service(name) => name == "ssh",
        FirewallRichRuleElement::Port { port, .. } => {
            firewall_port_covers_22(&format!("{port}/tcp"))
        }
        FirewallRichRuleElement::Protocol(_) => false,
    };
    if !targets_ssh {
        return None;
    }
    match parsed.action {
        FirewallRichRuleAction::Drop => Some(FirewallSshGuard::RichDropSsh),
        FirewallRichRuleAction::Reject => Some(FirewallSshGuard::RichRejectSsh),
        FirewallRichRuleAction::Accept => None,
    }
}

/// Best-effort SSH session detection inside the helper. `None` means the
/// probe failed — an inconclusive, not a negative, answer.
fn detect_ssh_sessions() -> Option<Vec<String>> {
    let ss = ss_bin()?;
    let output = std::process::Command::new(ss)
        .args(["-tnp", "state", "established", "( sport = :22 )"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_ss_sessions(&stdout))
}

/// Parse `ss` output into session rows. Every header shape `ss` produces is
/// filtered: "Netid …" (narrow output), "Recv-Q …" (flagless `ss`) and
/// "State Recv-Q …" (`ss state established`). An established row itself
/// never starts with those headers, so this filter cannot hide a session.
fn parse_ss_sessions(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Netid"))
        .filter(|line| !line.starts_with("Recv-Q"))
        .filter(|line| !line.starts_with("State"))
        .map(str::to_string)
        .collect()
}

/// Enforce the SSH confirmation gate *before* any firewall command runs.
///
/// The confirmation contract: the caller must pass the exact trailing
/// argument `confirm` to authorize a protected operation. A missing
/// argument and any other text are both "not confirmed" — there is no false
/// value to misuse, and nothing else can be smuggled through validation.
///
/// Detection stays best-effort per the approved GROUP 2 policy: an empty
/// session list is a negative answer, an inconclusive probe fails open with
/// a warning on stderr.
fn enforce_ssh_guard(guard: Option<FirewallSshGuard>, confirmed: bool) -> Result<(), String> {
    let Some(guard) = guard else {
        return Ok(());
    };
    match detect_ssh_sessions() {
        Some(sessions) if sessions.is_empty() => Ok(()),
        Some(_) if confirmed => Ok(()),
        Some(_) => Err(ssh_guard_confirmation_error(guard)),
        None => {
            eprintln!(
                "lcp-admin: warning: SSH session detection was inconclusive; \
                 proceeding with {} without confirmation",
                guard.message()
            );
            Ok(())
        }
    }
}

/// The interface-zone variant of the SSH guard: unlike the service/port
/// cases, the gate re-checks that the detected session is plausibly carried
/// by the interface being moved. `sessions` is the single, action-wide
/// probe result shared with the guard computation — probing again could
/// disagree and silently clear a detected session. The inconclusive policy
/// and the confirmation message are the approved GROUP 2 ones.
fn enforce_ssh_guard_for_interface(
    guard: Option<FirewallSshGuard>,
    sessions: Option<&[String]>,
    interface: &str,
    confirmed: bool,
) -> Result<(), String> {
    let Some(guard) = guard else {
        return Ok(());
    };
    match sessions {
        Some(_) if confirmed => Ok(()),
        Some(sessions)
            if !sessions.is_empty() && ssh_sessions_touch_interface(sessions, interface) =>
        {
            Err(ssh_guard_confirmation_error(guard))
        }
        Some(_) => Ok(()),
        None => {
            eprintln!(
                "lcp-admin: warning: SSH session detection was inconclusive; \
                 proceeding with {} without confirmation",
                guard.message()
            );
            Ok(())
        }
    }
}

/// The confirmation error a GROUP 2/4 SSH guard produces when it fires
/// unconfirmed. One constructor so the message stays byte-identical
/// everywhere it is issued.
fn ssh_guard_confirmation_error(guard: FirewallSshGuard) -> String {
    format!(
        "CONFIRMATION_REQUIRED: This change can interrupt an active SSH session ({}) \
         detected by lcp-admin. It may make the firewall impossible to administer \
         remotely. Retry the same action with the explicit 'confirm' argument to proceed.",
        guard.message()
    )
}

fn ufw_bin() -> Option<&'static str> {
    UFW_CANDIDATES
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
        .copied()
}

fn firewalld_available() -> bool {
    std::path::Path::new(FIREWALL_CMD_BIN).is_file()
}

fn firewall_program_detail(output: std::process::Output, label: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = match (stderr.is_empty(), stdout.is_empty()) {
        (false, false) => format!("{stderr}; stdout: {stdout}"),
        (false, true) => stderr,
        (true, false) => stdout,
        (true, true) => format!("exit status {}", output.status),
    };
    format!("{label} failed: {detail}")
}

fn run_firewall_program(program: &str, args: &[&str], label: &str) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{label} could not be started ({e})"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(firewall_program_detail(output, label))
}

/// Apply the runtime rule first, then the permanent one. The permanent rule
/// is never attempted if the runtime rule already failed. If permanent fails,
/// the caller is told exactly what remains changed at runtime.
fn run_firewall_two_phase(
    program: &str,
    runtime_args: &[&str],
    permanent_args: &[&str],
    label: &str,
    add: bool,
) -> Result<(), String> {
    run_firewall_program(program, runtime_args, label)?;
    if let Err(permanent_error) = run_firewall_program(program, permanent_args, label) {
        let operation = if add { "added" } else { "removed" };
        return Err(format!(
            "{permanent_error}; the runtime rule was {operation}, but the permanent rule was not"
        ));
    }
    Ok(())
}

fn firewall_rule_flag(add: bool, kind: &str, value: &str) -> String {
    match kind {
        "service" => format!("--{}-service={}", if add { "add" } else { "remove" }, value),
        "port" => format!("--{}-port={}", if add { "add" } else { "remove" }, value),
        _ => unreachable!("validated firewall rule kind"),
    }
}

fn firewall_rich_address_text(address: root_policy::FirewallRichAddress) -> String {
    match address.prefix {
        Some(prefix) => format!("{}/{prefix}", address.address),
        None => address.address.to_string(),
    }
}

fn firewall_rich_rule_text(rule: &FirewallRichRule) -> String {
    let family = match rule.family {
        FirewallRichRuleFamily::Ipv4 => "ipv4",
        FirewallRichRuleFamily::Ipv6 => "ipv6",
    };
    let mut text = format!("rule family=\"{family}\"");
    if let Some(address) = rule.source {
        text.push_str(&format!(" source address=\"{}\"", firewall_rich_address_text(address)));
    }
    if let Some(address) = rule.destination {
        text.push_str(&format!(" destination address=\"{}\"", firewall_rich_address_text(address)));
    }
    match &rule.element {
        FirewallRichRuleElement::Service(name) => {
            text.push_str(&format!(" service name=\"{name}\""));
        }
        FirewallRichRuleElement::Port { port, protocol } => {
            text.push_str(&format!(" port port=\"{port}\" protocol=\"{protocol}\""));
        }
        FirewallRichRuleElement::Protocol(value) => {
            text.push_str(&format!(" protocol value=\"{value}\""));
        }
    }
    match rule.action {
        FirewallRichRuleAction::Accept => text.push_str(" accept"),
        FirewallRichRuleAction::Reject => text.push_str(" reject"),
        FirewallRichRuleAction::Drop => text.push_str(" drop"),
    }
    text
}

fn firewall_action_add(verb: &str, action: &str) -> Result<bool, String> {
    match verb {
        "add" => Ok(true),
        "remove" => Ok(false),
        other => Err(format!("lcp-admin: the {action} verb must be add or remove, got: {other}")),
    }
}

/// Split the optional trailing `confirm` argument off a firewall action's
/// argv. The extension argument is only ever the exact literal `confirm` —
/// a missing trailing argument and any other text both mean "not
/// confirmed", so neither a missing field nor a stray value can authorize.
fn firewall_confirm_split(
    args: &[String],
    min_len: usize,
    max_len: usize,
) -> Result<(&[String], bool), String> {
    if args.len() == min_len {
        return Ok((args, false));
    }
    if args.len() == max_len && args[min_len] == FIREWALL_CONFIRM_ARG {
        return Ok((&args[..min_len], true));
    }
    if args.len() == max_len {
        return Err(format!(
            "lcp-admin: the trailing argument '{}' is not a valid confirmation \
             (only '{FIREWALL_CONFIRM_ARG}' confirms); without it the action is unconfirmed.",
            args[min_len]
        ));
    }
    Err(format!(
        "lcp-admin: this action takes {min_len} arguments, optionally followed by \
         '{FIREWALL_CONFIRM_ARG}'."
    ))
}

/// `firewall_zone_rule <add|remove> <zone> <service|port> <value>`.
/// firewalld runs runtime and permanent rules separately; ufw writes its
/// persistent rule directly. The SSH confirmation contract is enforced here,
/// inside the privileged boundary, before any tool runs.
fn firewall_zone_rule_action(
    add: bool,
    zone: &str,
    kind: &str,
    value: &str,
    confirmed: bool,
) -> Result<(), String> {
    // The zone is empty only on the documented ufw path: ufw has no zone
    // concept. firewalld always needs a real zone.
    if firewalld_available() && zone.is_empty() {
        return Err("lcp-admin: a zone is required for a firewalld zone rule".to_string());
    }
    if !zone.is_empty() {
        firewall_zone_name_valid(zone)?;
    }
    match kind {
        "service" => firewall_service_name_valid(value)?,
        "port" => firewall_port_value_valid(value)?,
        other => return Err(format!("lcp-admin: the firewall rule kind must be service or port, got: {other}")),
    }

    // The guard runs after validation and before any command executes: a
    // refused operation must leave the tooling untouched.
    let guard = firewall_zone_rule_guard(kind, value, add);
    enforce_ssh_guard(guard, confirmed)?;

    if firewalld_available() {
        let flag = firewall_rule_flag(add, kind, value);
        let zone_flag = format!("--zone={zone}");
        let permanent_flag = "--permanent";
        return run_firewall_two_phase(
            FIREWALL_CMD_BIN,
            &[zone_flag.as_str(), flag.as_str()],
            &[zone_flag.as_str(), flag.as_str(), permanent_flag],
            "firewall-cmd",
            add,
        );
    }

    if let Some(ufw) = ufw_bin() {
        // Never discard the zone silently: ufw has no zone concept, so a
        // requested firewalld-style zone cannot be honored. Refuse with an
        // explicit explanation instead of implying the zone was modified.
        if !zone.is_empty() {
            return Err(format!(
                "lcp-admin: ufw has no firewalld-style zones; the requested zone '{zone}' \
                 cannot be honored — the rule '{value}' would apply globally. \
                 No rule was changed: submit a zone-less request or use a firewalld backend."
            ));
        }
        let mut args = Vec::with_capacity(3);
        if !add {
            args.push("delete");
        }
        args.push("allow");
        args.push(value);
        return run_firewall_program(ufw, &args, "ufw");
    }

    Err("lcp-admin: neither firewalld nor ufw is available".to_string())
}

/// `firewall_rich_rule <add|remove> <zone> <rule>`. firewalld only; ufw has
/// no rich-language equivalent and is refused explicitly. The rule string is
/// rebuilt from the parsed structure before the SSH guard evaluates it.
fn firewall_rich_rule_action(
    add: bool,
    zone: &str,
    rule: &str,
    confirmed: bool,
) -> Result<(), String> {
    // firewall_rich_rule is firewalld only: a zone is always required.
    if zone.is_empty() {
        return Err("lcp-admin: a zone is required for a firewalld rich rule".to_string());
    }
    firewall_zone_name_valid(zone)?;
    let parsed = firewall_rich_rule_valid(rule)?;

    let guard = firewall_rich_rule_guard(add, &parsed);
    enforce_ssh_guard(guard, confirmed)?;

    let rule = firewall_rich_rule_text(&parsed);
    if firewalld_available() {
        let flag = format!("--{}-rich-rule={rule}", if add { "add" } else { "remove" });
        let zone_flag = format!("--zone={zone}");
        let permanent_flag = "--permanent";
        return run_firewall_two_phase(
            FIREWALL_CMD_BIN,
            &[zone_flag.as_str(), flag.as_str()],
            &[zone_flag.as_str(), flag.as_str(), permanent_flag],
            "firewall-cmd",
            add,
        );
    }

    if ufw_bin().is_some() {
        return Err("lcp-admin: firewall_rich_rule is supported by firewalld only; ufw does not manage rich rules".to_string());
    }

    Err("lcp-admin: neither firewalld nor ufw is available".to_string())
}

/// Does this `ss` session line bind to the given local interface? The local
/// endpoint of session lines may carry the scope marker
/// `192.168.0.103%wlp1s0:22`. `None` means the line does not name an
/// interface at all — the caller treats that as "possibly this interface",
/// never as "provably elsewhere": the lockout risk must not be waved away
/// because a marker is missing.
fn ssh_session_binds_interface(line: &str, interface: &str) -> Option<bool> {
    if line.is_empty() || interface.is_empty() {
        return None;
    }
    let local = line.split_whitespace().nth(3)?;
    local.split_once('%').map(|(_, tail)| {
        tail.split(':').next() == Some(interface)
    })
}

/// True when any detected session could plausibly be carried by
/// `interface`. A session is provably elsewhere only when its scope marker
/// names a different interface.
fn ssh_sessions_touch_interface(sessions: &[String], interface: &str) -> bool {
    sessions
        .iter()
        .any(|line| ssh_session_binds_interface(line, interface) != Some(false))
}

/// Read-only probe: does the firewalld zone permit the ssh service? A probe
/// that cannot run or an explicit "no" both count as not permitting: the
/// guard then applies and, if it fires, explicit confirmation still allows
/// the operator to proceed — never a silent application.
fn firewall_zone_permits_ssh(zone: &str) -> bool {
    if let Ok(output) = std::process::Command::new(FIREWALL_CMD_BIN)
        .args(["--zone", zone, "--query-service=ssh"])
        .output()
    {
        output.status.success()
    } else {
        false
    }
}

/// The GROUP 4 SSH guard for interface-zone moves. The guard fires only
/// when the destination zone does not permit ssh AND the detected session
/// could plausibly be carried by the interface being moved. An inconclusive
/// session probe is handed down unchanged to the GROUP 2 warning/fail-open
/// path. `sessions` is computed exactly once per action by the caller and
/// reused by the enforcement: two probes could disagree and let a detected
/// session disappear between decisions.
///
/// The zone's ssh permission is probed only when a session was actually
/// seen: no session, no probe.
fn firewall_interface_zone_guard(
    zone: &str,
    interface: &str,
    sessions: Option<&[String]>,
) -> Option<FirewallSshGuard> {
    match sessions {
        Some(sessions) if sessions.is_empty() => None,
        Some(sessions) => {
            if firewall_zone_permits_ssh(zone)
                || !ssh_sessions_touch_interface(sessions, interface)
            {
                None
            } else {
                Some(FirewallSshGuard::MoveSshInterface)
            }
        }
        // Inconclusive: the guard variant still applies; enforcement warns
        // on stderr and proceeds per the approved GROUP 2 policy.
        None if firewall_zone_permits_ssh(zone) => None,
        None => Some(FirewallSshGuard::MoveSshInterface),
    }
}

/// `firewall_interface_zone <zone> <interface> [confirm]`. firewalld only:
/// ufw binds all interfaces globally and is refused explicitly before
/// anything runs. Runtime first, then permanent; a permanent failure after
/// runtime success reports the partial state and promises no rollback.
fn firewall_interface_zone_action(
    zone: &str,
    interface: &str,
    confirmed: bool,
) -> Result<(), String> {
    if zone.is_empty() {
        return Err("lcp-admin: a zone is required for an interface-zone binding".to_string());
    }
    firewall_zone_name_valid(zone)?;
    firewall_interface_name_valid(interface)?;

    if !firewalld_available() {
        if ufw_bin().is_some() {
            return Err(
                "lcp-admin: firewall_interface_zone is supported by firewalld only; ufw \
                 has no firewalld-style zones and binds interfaces globally"
                    .to_string(),
            );
        }
        return Err("lcp-admin: neither firewalld nor ufw is available".to_string());
    }

    // The SSH probe runs exactly once per action: guard computation and
    // enforcement share this single result, so a hypothetical second probe
    // can never disagree with the decision.
    let sessions = detect_ssh_sessions();
    let guard = firewall_interface_zone_guard(zone, interface, sessions.as_deref());
    enforce_ssh_guard_for_interface(guard, sessions.as_deref(), interface, confirmed)?;

    let zone_flag = format!("--zone={zone}");
    let change_flag = format!("--change-interface={interface}");
    run_firewall_program(
        FIREWALL_CMD_BIN,
        &[zone_flag.as_str(), change_flag.as_str()],
        "firewall-cmd",
    )?;
    if let Err(permanent_error) = run_firewall_program(
        FIREWALL_CMD_BIN,
        &[zone_flag.as_str(), change_flag.as_str(), "--permanent"],
        "firewall-cmd",
    ) {
        return Err(format!(
            "{permanent_error}; the interface {interface} was bound to zone {zone} at \
             runtime, but the persistent configuration was not"
        ));
    }
    Ok(())
}

/// `firewall_enable_hardened <firewalld|ufw> [confirm]`. Enables the chosen
/// backend with a deny-by-default posture. Confirmation is mandatory: both
/// backends can cut the administrator's network access. The backend is named
/// explicitly by the caller — there is no silent fallback, so an ambiguous or
/// unsupported host is refused instead of guessing.
fn firewall_enable_hardened_action(backend: &str, confirmed: bool) -> Result<(), String> {
    const FIREWALLD: &str = "firewalld";
    const UFW: &str = "ufw";
    if backend != FIREWALLD && backend != UFW {
        return Err(format!(
            "lcp-admin: the firewall backend must be firewalld or ufw, got: {backend}"
        ));
    }

    let firewalld = firewalld_available();
    let ufw = ufw_bin();
    match backend {
        FIREWALLD if !firewalld => {
            return Err(match ufw {
                Some(_) => "lcp-admin: firewalld is not installed on this host; \
                           ufw is the available firewall backend"
                    .to_string(),
                None => "lcp-admin: firewalld is not installed on this host".to_string(),
            })
        }
        UFW if ufw.is_none() => {
            return Err(if firewalld {
                "lcp-admin: ufw is not installed on this host; \
                 firewalld is the available firewall backend"
                    .to_string()
            } else {
                "lcp-admin: ufw is not installed on this host".to_string()
            })
        }
        FIREWALLD if !std::path::Path::new(SYSTEMCTL_BIN).is_file() => {
            return Err("lcp-admin: systemctl is not available; firewalld cannot be enabled".to_string())
        }
        _ => {}
    }

    // GROUP 3 safety policy: probe exactly once and fail closed. An
    // inconclusive probe is a hard stop even with explicit confirmation: the
    // operation applies a deny-by-default policy and could cut off an
    // undetected SSH session.
    let sessions = match detect_ssh_sessions() {
        Some(sessions) => sessions,
        None => {
            return Err(
                "lcp-admin: SSH session detection failed; hardening the firewall is blocked \
                 (fail closed) because it would restrict an undetected SSH session"
                    .to_string(),
            )
        }
    };

    // Confirmation is mandatory for both backends and both definite probe
    // results: an affirmative session list and a clean negative can both be
    // followed by a network-restricting change.
    if !confirmed {
        let detail = if sessions.is_empty() {
            " No active SSH session was visible, but the new policy still restricts network \
             access."
                .to_string()
        } else {
            format!(
                " Active SSH session(s) detected: {}.",
                sessions.join("; ")
            )
        };
        return Err(format!(
            "CONFIRMATION_REQUIRED: Enabling {backend} with a deny-by-default policy can \
             interrupt network access, including the SSH session used to administer this \
             host.{detail} Retry the same action with the explicit 'confirm' argument to \
             proceed."
        ));
    }

    match backend {
        FIREWALLD => {
            run_firewall_program(SYSTEMCTL_BIN, &["enable", "--now", "firewalld"], "systemctl")?;
            if let Err(runtime_error) = run_firewall_program(
                FIREWALL_CMD_BIN,
                &["--set-default-zone=drop"],
                "firewall-cmd",
            ) {
                return Err(format!(
                    "{runtime_error}; firewalld was enabled and is running, but its default \
                     zone was not changed to drop and the permanent configuration was not \
                     updated"
                ));
            }
            if let Err(permanent_error) = run_firewall_program(
                FIREWALL_CMD_BIN,
                &["--set-default-zone=drop", "--permanent"],
                "firewall-cmd",
            ) {
                return Err(format!(
                    "{permanent_error}; the default zone was set to drop at runtime, but the \
                     persistent configuration was not"
                ));
            }
        }
        _ => {
            let ufw = ufw_bin().expect("ufw availability checked above");
            run_firewall_program(ufw, &["default", "deny", "incoming"], "ufw")?;
            if let Err(enable_error) = run_firewall_program(ufw, &["--force", "enable"], "ufw") {
                return Err(format!(
                    "{enable_error}; the default incoming policy was set to deny, but ufw was \
                     not enabled"
                ));
            }
        }
    }
    Ok(())
}

/// `firewall_panic <enable|disable> [confirm]`. firewalld only: ufw has no
/// panic mode and is refused explicitly before anything runs. Enabling runs
/// the GROUP 3 fail-closed SSH guard inside the helper; disabling needs no
/// confirmation. Panic applies globally: there is no zone argument.
fn firewall_panic_action(enable: bool, confirmed: bool) -> Result<(), String> {
    if !firewalld_available() {
        if ufw_bin().is_some() {
            return Err(
                "lcp-admin: firewall_panic is supported by firewalld only; ufw has no panic mode"
                    .to_string(),
            );
        }
        return Err("lcp-admin: neither firewalld nor ufw is available".to_string());
    }

    // GROUP 3 SSH guard for enabling: unlike GROUP 2's guards this one
    // fails CLOSED — an inconclusive probe blocks panic enablement even
    // with explicit confirmation, because panic mode severs every
    // connection.
    if enable {
        match detect_ssh_sessions() {
            Some(sessions) if !sessions.is_empty() && !confirmed => {
                return Err(format!(
                    "CONFIRMATION_REQUIRED: Enabling firewalld panic mode with an active SSH \
                     session ({}) detected by lcp-admin will sever every connection, including \
                     the session you are working from. Retry the same action with the explicit \
                     'confirm' argument to proceed.",
                    sessions.join("; ")
                ));
            }
            Some(_) => {}
            None => {
                return Err(
                    "lcp-admin: SSH session detection failed; enabling panic mode is blocked \
                     (fail closed) because it would cut off an undetected SSH session"
                        .to_string(),
                );
            }
        }
    }

    let arg = if enable { "--panic-on" } else { "--panic-off" };
    run_firewall_program(FIREWALL_CMD_BIN, &[arg], "firewall-cmd")?;
    if let Err(permanent_error) =
        run_firewall_program(FIREWALL_CMD_BIN, &[arg, "--permanent"], "firewall-cmd")
    {
        let state = if enable { "enabled" } else { "disabled" };
        return Err(format!(
            "{permanent_error}; panic mode was {state} at runtime, but the persistent \
             configuration was not"
        ));
    }
    Ok(())
}

/// Run one package-manager operation. Non-zero exits are always errors: the
/// previous app path silently reported success for autoremove/makecache.
fn run_package_program(program: &str, args: &[&str], label: &str) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("lcp-admin: {label} could not be started ({e})"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    if detail.is_empty() {
        Err(format!("lcp-admin: {label} failed with exit status {}", output.status))
    } else {
        Err(format!("lcp-admin: {label} failed: {detail}"))
    }
}

/// Capture a preview command's combined output. A preview that intentionally
/// answers "no" (`dnf --assumeno`) exits non-zero, so only spawn failures are
/// errors here; the caller parses the transaction text.
fn capture_package_output(program: &str, args: &[&str], label: &str) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("lcp-admin: {label} could not be started ({e})"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

/// A live package-manager lock owned by a running process, if any.
fn package_lock_owner() -> Option<String> {
    for path in PACKAGE_LOCK_PATHS {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let text = content.trim();
        if text.is_empty() {
            continue;
        }
        if let Ok(pid) = text.parse::<u32>() {
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .unwrap_or_else(|_| "unknown".to_string());
                return Some(format!(
                    "process '{}' (PID {pid}) holds {path}",
                    name.trim()
                ));
            }
            continue;
        }
        // dpkg lock files are usually empty; a non-numeric, non-empty value
        // is checked with fuser so a real holder is not missed.
        if let Ok(output) = std::process::Command::new("fuser").arg(path).output() {
            if output.status.success() {
                let pids = String::from_utf8_lossy(&output.stdout);
                if let Some(pid) = pids.split_whitespace().next() {
                    return Some(format!("process (PID {pid}) holds {path}"));
                }
            }
        }
    }
    None
}

/// Free bytes on the root filesystem; `None` means the check could not run.
fn package_disk_free_bytes() -> Option<u64> {
    let df = df_bin()?;
    let output = std::process::Command::new(df)
        .args(["--output=avail", "-B1", "/"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .nth(1)
        .and_then(|line| line.trim().parse::<u64>().ok())
}

fn collect_protected(tokens: impl Iterator<Item = String>) -> Vec<String> {
    let mut found: Vec<String> = tokens
        .filter(|token| root_policy::is_protected_package(token))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Resolved packages a dnf autoremove transaction would remove. dnf's
/// transaction lines are indented; the first token is the package NEVRA, and
/// the shared protected-package prefix list understands NEVRA names.
fn dnf_autoremove_protected(program: &str) -> Result<Vec<String>, String> {
    let text = capture_package_output(program, &["autoremove", "--assumeno"], "dnf")?;
    Ok(collect_protected(text.lines().filter_map(|line| {
        if !line.starts_with(' ') {
            return None;
        }
        line.split_whitespace().next().map(str::to_string)
    })))
}

/// Resolved packages an apt autoremove simulation would remove (`Remv <pkg>`).
fn apt_autoremove_protected(program: &str) -> Result<Vec<String>, String> {
    let text = capture_package_output(program, &["-s", "autoremove"], "apt-get")?;
    Ok(collect_protected(text.lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some("Remv"))
            .then(|| parts.next().map(str::to_string))
            .flatten()
    })))
}

/// `pkg_maintenance <clean|autoremove|makecache>`. dnf is preferred; apt-get
/// runs only when dnf is not installed. Autoremove is the destructive member:
/// a live package-manager lock or low disk space refuses before anything runs,
/// and a resolved-removal preview is hard-blocked when it would remove a
/// protected system package.
fn pkg_maintenance_action(op: &str) -> Result<(), String> {
    root_policy::pkg_maintenance_op_valid(op)?;

    let dnf = dnf_bin();
    let apt = apt_get_bin();

    if op == "autoremove" {
        if let Some(owner) = package_lock_owner() {
            return Err(format!(
                "lcp-admin: the package manager is locked by {owner}; no changes were made"
            ));
        }
        match package_disk_free_bytes() {
            Some(free) if free < PACKAGE_MIN_FREE_BYTES => {
                return Err(format!(
                    "lcp-admin: insufficient disk space: {} MB free on /, at least {} MB is required",
                    free / (1024 * 1024),
                    PACKAGE_MIN_FREE_BYTES / (1024 * 1024)
                ))
            }
            Some(_) => {}
            None => eprintln!(
                "lcp-admin: warning: could not determine free disk space before autoremove"
            ),
        }
    }

    let (program, label) = match (dnf, apt) {
        (Some(program), _) => (program, "dnf"),
        (None, Some(program)) => (program, "apt-get"),
        (None, None) => {
            return Err("lcp-admin: neither dnf nor apt-get is available".to_string())
        }
    };

    match (op, label) {
        ("clean", "dnf") => run_package_program(program, &["clean", "all"], "dnf"),
        ("clean", _) => run_package_program(program, &["clean"], "apt-get"),
        ("autoremove", "dnf") => {
            let protected = dnf_autoremove_protected(program)?;
            if !protected.is_empty() {
                return Err(format!(
                    "lcp-admin: autoremove would remove protected system package(s): {}; \
                     refusing to run it",
                    protected.join(", ")
                ));
            }
            run_package_program(program, &["autoremove", "-y"], "dnf")
        }
        ("autoremove", _) => {
            let protected = apt_autoremove_protected(program)?;
            if !protected.is_empty() {
                return Err(format!(
                    "lcp-admin: autoremove would remove protected system package(s): {}; \
                     refusing to run it",
                    protected.join(", ")
                ));
            }
            run_package_program(program, &["autoremove", "-y"], "apt-get")
        }
        ("makecache", "dnf") => run_package_program(program, &["makecache"], "dnf"),
        ("makecache", _) => run_package_program(program, &["update"], "apt-get"),
        (other, _) => Err(format!("lcp-admin: unsupported maintenance operation: {other}")),
    }
}

/// The running kernel release (`uname -r`), used only to detect a rollback
/// that would affect the kernel the administrator is currently booted into.
fn running_kernel_release() -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let uname = uname_bin()?;
    let mut child = std::process::Command::new(uname)
        .arg("-r")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A bounded wait keeps a hung uname from blocking the privileged action
    // indefinitely; the caller treats `None` as "kernel identity unknown".
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    let release = output.trim().to_string();
    (!release.is_empty()).then_some(release)
}

/// Packages a `dnf history info <id>` entry altered. The transaction block is
/// introduced by "Packages Altered:" and every package line is indented; the
/// action keyword precedes the package identifier(s).
fn parse_dnf_history_packages(text: &str) -> Vec<String> {
    let mut in_altered = false;
    let mut packages: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.trim() == "Packages Altered:" {
            in_altered = true;
            continue;
        }
        if !in_altered {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            // the next top-level section
            in_altered = false;
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(action) = parts.next() else { continue };
        let Some(package) = parts.next() else { continue };
        match action {
            "Install" | "Installed" | "Removed" | "Remove" | "Reinstall"
            | "Reinstalled" | "Obsoleted" | "Obsoletes" | "Upgrade" | "Upgraded"
            | "Downgrade" | "Downgraded" => {
                if package != "->" && package != "none" {
                    packages.push(package.to_string());
                }
                // "Upgraded <old> -> <new>": include the new identifier too.
                if let Some(arrow) = parts.next() {
                    if arrow == "->" {
                        if let Some(new_package) = parts.next() {
                            packages.push(new_package.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    packages.sort();
    packages.dedup();
    packages
}

/// `dnf_history_undo <transaction-id> [confirm]`. dnf only. The transaction
/// preview is mandatory and read-only; the destructive command runs only when
/// the preview is conclusive and, if a protected package or the currently
/// running kernel is involved, the caller has explicitly confirmed.
fn dnf_history_undo_action(id: &str, confirmed: bool) -> Result<(), String> {
    root_policy::dnf_history_transaction_id_valid(id)?;

    let Some(dnf) = dnf_bin() else {
        return Err("lcp-admin: dnf is not available on this system".to_string());
    };

    if let Some(owner) = package_lock_owner() {
        return Err(format!(
            "lcp-admin: the package manager is locked by {owner}; no changes were made"
        ));
    }
    match package_disk_free_bytes() {
        Some(free) if free < PACKAGE_MIN_FREE_BYTES => {
            return Err(format!(
                "lcp-admin: insufficient disk space: {} MB free on /, at least {} MB is required",
                free / (1024 * 1024),
                PACKAGE_MIN_FREE_BYTES / (1024 * 1024)
            ))
        }
        Some(_) => {}
        None => eprintln!(
            "lcp-admin: warning: could not determine free disk space before the rollback"
        ),
    }

    // Read-only preview of the packages the transaction altered.
    let preview = capture_package_output(dnf, &["history", "info", id], "dnf")?;
    let packages = parse_dnf_history_packages(&preview);
    if packages.is_empty() {
        return Err(format!(
            "lcp-admin: could not determine the package set of DNF transaction #{id}; \
             refusing to roll it back"
        ));
    }

    let mut risks: Vec<String> = Vec::new();
    if let Some(release) = running_kernel_release() {
        let kernel_packages: Vec<&String> = packages
            .iter()
            .filter(|package| root_policy::package_matches_running_kernel(package, &release))
            .collect();
        if !kernel_packages.is_empty() {
            risks.push(format!(
                "this would remove or downgrade the kernel you are currently running ({release})"
            ));
        }
    } else if packages
        .iter()
        .any(|package| root_policy::package_is_kernel_package(package))
    {
        // Fail closed: the rollback involves a kernel package, but the running
        // kernel could not be identified, so confirmation is always required.
        risks.push(
            "the running kernel could not be determined, so this rollback may              remove or downgrade the kernel you are running"
                .to_string(),
        );
    } else {
        eprintln!(
            "lcp-admin: warning: the running kernel release could not be determined; \
             no kernel package is affected by this rollback"
        );
    }
    let protected: Vec<&String> = packages
        .iter()
        .filter(|package| root_policy::is_protected_package(package))
        .collect();
    for package in &protected {
        risks.push(format!(
            "this would affect {package}, a system-critical package"
        ));
    }

    if !risks.is_empty() && !confirmed {
        return Err(format!(
            "CONFIRMATION_REQUIRED: Undoing DNF transaction #{id} will affect {} package(s): \
             {}. WARNING: {}. Retry the same action with the explicit 'confirm' argument to \
             proceed.",
            packages.len(),
            packages.join(", "),
            risks.join("; ")
        ));
    }

    // Defense in depth: the exact argv must also pass the shared dnf policy.
    let argv: Vec<String> = vec![
        "history".to_string(),
        "undo".to_string(),
        id.to_string(),
        "-y".to_string(),
    ];
    root_policy::ensure_arguments_for_program(dnf, &argv)?;
    run_package_program(dnf, &["history", "undo", id, "-y"], "dnf")
}

/// Run a user/group tool and format a consistent error. All arguments were
/// validated before this point; the exact argv is passed without a shell.
fn run_user_program(program: &str, args: &[&str], label: &str) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("lcp-admin: {label} could not be started ({e})"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    if detail.is_empty() {
        Err(format!("lcp-admin: {label} failed with exit status {}", output.status))
    } else {
        Err(format!("lcp-admin: {label} failed: {detail}"))
    }
}

/// Validate a constructed argv against the shared per-program policy.
fn ensure_user_argv(program: &str, argv: &[String]) -> Result<(), String> {
    root_policy::ensure_arguments_for_program(program, argv)
}

fn admin_group_name() -> &'static str {
    if let Ok(content) = std::fs::read_to_string("/etc/group") {
        for line in content.lines() {
            if line.starts_with("sudo:") || line.starts_with("wheel:") {
                return if line.starts_with("sudo:") { "sudo" } else { "wheel" };
            }
        }
    }
    "wheel"
}

/// `user_add <username> [fullname]`.
fn user_add_action(username: &str, fullname: Option<&str>) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    let program = require_user_tool(0, "useradd")?;
    let mut argv: Vec<String> = vec!["-m".to_string()];
    if let Some(fullname) = fullname {
        root_policy::user_fullname_valid(fullname)?;
        argv.push("-c".to_string());
        argv.push(fullname.to_string());
    }
    argv.push(username.to_string());
    ensure_user_argv(program, &argv)?;
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_user_program(program, &refs, "useradd")
}

/// `user_delete <username> [remove-home]`.
fn user_delete_action(username: &str, remove_home: bool) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    if root_policy::is_protected_system_user(username) {
        return Err(format!(
            "lcp-admin: '{username}' is a vital system account and cannot be deleted"
        ));
    }
    let program = require_user_tool(1, "userdel")?;
    let mut argv: Vec<String> = vec!["--force".to_string()];
    if remove_home {
        argv.push("--remove".to_string());
    }
    argv.push(username.to_string());
    ensure_user_argv(program, &argv)?;
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_user_program(program, &refs, "userdel")
}

/// `user_set_password <username>` with the password supplied on stdin.
fn user_set_password_action(username: &str) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    if root_policy::is_protected_system_user(username) {
        return Err(format!(
            "lcp-admin: '{username}' is a vital system account; changing its password is blocked"
        ));
    }
    let password = read_stdin_content(4096, "password")?;
    root_policy::user_password_valid(&password)?;
    let program = require_user_tool(5, "chpasswd")?;
    let input = format!("{username}:{password}\n");
    use std::io::Write;
    let mut child = std::process::Command::new(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("lcp-admin: chpasswd could not be started ({e})"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("lcp-admin: chpasswd input could not be written ({e})"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("lcp-admin: chpasswd did not complete ({e})"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: chpasswd failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// `user_toggle_sudo <username> <grant|revoke>`.
fn user_toggle_sudo_action(username: &str, grant: bool) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    let group = admin_group_name();
    if grant {
        let program = require_user_tool(2, "usermod")?;
        let argv: Vec<String> = vec![
            "-aG".to_string(),
            group.to_string(),
            username.to_string(),
        ];
        ensure_user_argv(program, &argv)?;
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run_user_program(program, &refs, "usermod")
    } else {
        let program = require_user_tool(3, "gpasswd")?;
        let argv: Vec<String> = vec![
            "-d".to_string(),
            username.to_string(),
            group.to_string(),
        ];
        run_user_program(program, &argv.iter().map(String::as_str).collect::<Vec<_>>(), "gpasswd")
    }
}

/// `user_modify_group <username> <groupname> <add|remove>`.
fn user_modify_group_action(username: &str, groupname: &str, add: bool) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    root_policy::group_name_valid(groupname)?;
    if add {
        let program = require_user_tool(2, "usermod")?;
        let argv: Vec<String> = vec![
            "-aG".to_string(),
            groupname.to_string(),
            username.to_string(),
        ];
        ensure_user_argv(program, &argv)?;
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run_user_program(program, &refs, "usermod")
    } else {
        let program = require_user_tool(3, "gpasswd")?;
        let argv: Vec<String> = vec![
            "-d".to_string(),
            username.to_string(),
            groupname.to_string(),
        ];
        run_user_program(program, &argv.iter().map(String::as_str).collect::<Vec<_>>(), "gpasswd")
    }
}

/// `group_manage <add|delete> <groupname>`.
fn group_manage_action(verb: &str, groupname: &str) -> Result<(), String> {
    root_policy::group_name_valid(groupname)?;
    if verb == "delete" && root_policy::is_protected_admin_group(groupname) {
        return Err(format!(
            "lcp-admin: '{groupname}' is an essential system-administration group and cannot be deleted"
        ));
    }
    let (index, program_name) = match verb {
        "add" => (6, "groupadd"),
        "delete" => (7, "groupdel"),
        other => {
            return Err(format!(
                "lcp-admin: the group verb must be add or delete, got: {other}"
            ))
        }
    };
    let program = require_user_tool(index, program_name)?;
    let argv: Vec<String> = vec![groupname.to_string()];
    ensure_user_argv(program, &argv)?;
    run_user_program(program, &[groupname], program_name)
}

/// `user_lock <username> <lock|unlock>`.
fn user_lock_action(username: &str, lock: bool) -> Result<(), String> {
    root_policy::user_account_name_valid(username)?;
    if root_policy::is_protected_system_user(username) {
        return Err(format!(
            "lcp-admin: '{username}' is a vital system account and cannot be locked or unlocked"
        ));
    }
    let program = require_user_tool(4, "passwd")?;
    run_user_program(
        program,
        &[if lock { "-l" } else { "-u" }, username],
        "passwd",
    )
}

/// UID of the process that invoked the helper. `pkexec` sets PKEXEC_UID; a
/// sudo invocation sets SUDO_UID. This value never comes from the action's
/// argv, so a direct helper caller cannot claim to be someone else.
fn invoking_uid() -> Option<u32> {
    for name in ["PKEXEC_UID", "SUDO_UID"] {
        if let Ok(value) = std::env::var(name) {
            if let Ok(uid) = value.trim().parse::<u32>() {
                return Some(uid);
            }
        }
    }
    None
}

/// Owner UID of a login session, resolved by loginctl (authoritative).
fn session_owner_uid(program: &str, session_id: &str) -> Result<u32, String> {
    let output = std::process::Command::new(program)
        .args(["show-session", session_id, "-p", "User", "--value"])
        .output()
        .map_err(|e| format!("lcp-admin: loginctl could not be started ({e})"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: the owner of session {session_id} could not be determined: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| {
            format!(
                "lcp-admin: the owner of session {session_id} could not be determined (unexpected loginctl output)"
            )
        })
}

/// `session_kill <session-id>`. The invoking user's own session is never
/// terminated; the owner is read from loginctl, not from the caller.
fn session_kill_action(session_id: &str) -> Result<(), String> {
    root_policy::login_session_id_valid(session_id)?;
    let program = require_user_tool(8, "loginctl")?;
    let caller = invoking_uid().ok_or_else(|| {
        "lcp-admin: the invoking user could not be determined; refusing to terminate a session"
            .to_string()
    })?;
    let owner = session_owner_uid(program, session_id)?;
    if owner == caller {
        return Err(format!(
            "lcp-admin: session {session_id} belongs to the invoking user; refusing to terminate your own session"
        ));
    }
    let argv: Vec<String> = vec!["kill-session".to_string(), session_id.to_string()];
    ensure_user_argv(program, &argv)?;
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_user_program(program, &refs, "loginctl")
}

/// `security_service_toggle <auditd|usbguard|chronyd|systemd-timesyncd>
/// <enable|disable>`: the closed allow-list behind the Security Auditor's
/// service fixes. No arbitrary unit or systemctl verb is reachable.
fn security_service_toggle_action(unit: &str, verb: &str) -> Result<(), String> {
    root_policy::security_service_unit_valid(unit)?;
    root_policy::security_service_verb_valid(verb)?;
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return Err("lcp-admin: systemctl is not available".to_string());
    }
    run_firewall_program(SYSTEMCTL_BIN, &[verb, "--now", unit], "systemctl")
}

/// `security_ctrl_alt_del <mask|unmask>`: only the fixed reboot target.
fn security_ctrl_alt_del_action(verb: &str) -> Result<(), String> {
    root_policy::ctrl_alt_del_verb_valid(verb)?;
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return Err("lcp-admin: systemctl is not available".to_string());
    }
    run_firewall_program(SYSTEMCTL_BIN, &[verb, "ctrl-alt-del.target"], "systemctl")
}

/// `security_ssh_param <param> <value>`: edit the one SSH hardening parameter
/// with a backup, validate the resulting sshd config, then reload.
fn security_ssh_param_action(param: &str, value: &str) -> Result<(), String> {
    root_policy::security_ssh_param_valid(param, value)?;
    let rel = "ssh/sshd_config";
    let original = security_read_file(rel)?;
    rewrite_settings(rel, &[param], &[(param, value)])?;
    let Some(sshd) = sshd_bin() else {
        security_restore_file(rel, &original)?;
        return Err("lcp-admin: sshd is not available".to_string());
    };
    let cfg = security_path(rel);
    if !command_succeeds(sshd, &["-t", "-f", &cfg]) {
        security_restore_file(rel, &original)?;
        return Err("lcp-admin: the edited SSH configuration did not pass sshd -t; it was restored".to_string());
    }
    // Verify the effective value, like the previous flow: another included
    // config can override the edited line, and success must not be claimed.
    let effective = match std::process::Command::new(sshd)
        .args(["-T", "-f", &cfg])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            security_restore_file(rel, &original)?;
            return Err(format!(
                "lcp-admin: sshd -T could not be started ({e}); the edit was restored"
            ));
        }
    };
    let text = String::from_utf8_lossy(&effective.stdout);
    let matched = text.lines().any(|line| {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { return false };
        if !name.eq_ignore_ascii_case(param) {
            return false;
        }
        let actual = parts.next().unwrap_or("");
        let equivalent_grace = param == "LoginGraceTime"
            && ((value == "60" && (actual == "1m" || actual == "60"))
                || (value == "120" && (actual == "2m" || actual == "120")));
        actual == value || equivalent_grace
    });
    if !effective.status.success() || !matched {
        security_restore_file(rel, &original)?;
        return Err(
            "lcp-admin: the requested SSH setting is overridden by another configuration file; no change was kept".to_string(),
        );
    }
    reload_ssh(false)
}

/// `security_kernel_param <key> <value>`: persist one allow-listed sysctl key
/// and apply it live; the file is written before the live change, like the
/// previous flow.
fn security_kernel_param_action(key: &str, value: &str) -> Result<(), String> {
    root_policy::security_kernel_param_valid(key, value)?;
    let Some(sysctl) = sysctl_bin() else {
        return Err("lcp-admin: sysctl is not available".to_string());
    };
    let assignment = format!("{key}={value}");
    // Apply the runtime value FIRST: if the kernel rejects it, no persistent
    // file is left behind claiming a setting that was never accepted.
    run_firewall_program(sysctl, &["-w", &assignment], "sysctl")?;
    let rel = "sysctl.d/99-hardening.conf";
    let persisted_value = format!("= {value}");
    if let Err(error) = rewrite_settings(rel, &[key], &[(key, persisted_value.as_str())]) {
        return Err(format!(
            "{error}; the runtime value was applied but the persistent configuration was not updated"
        ));
    }
    let file = security_path(rel);
    run_firewall_program(sysctl, &["-p", &file], "sysctl")
}

/// `security_lock_account [username]`: lock the named account, or every account
/// whose shadow password field is empty.
fn security_lock_account_action(username: Option<&str>) -> Result<(), String> {
    if let Some(user) = username {
        root_policy::user_account_name_valid(user)?;
        if root_policy::is_protected_system_user(user) {
            return Err(format!(
                "lcp-admin: '{user}' is a vital system or administrative account and cannot be locked"
            ));
        }
    }
    let passwd = require_user_tool(4, "passwd")?;
    let users: Vec<String> = match username {
        Some(user) => vec![user.to_string()],
        None => {
            let shadow = security_read_file("shadow")?;
            shadow
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split(':');
                    let name = parts.next().unwrap_or("");
                    let password = parts.next().unwrap_or("");
                    (password.is_empty()
                        && !name.is_empty()
                        && root_policy::user_account_name_valid(name).is_ok()
                        && !root_policy::is_protected_system_user(name))
                        .then(|| name.to_string())
                })
                .collect()
        }
    };
    if users.is_empty() {
        return Err("lcp-admin: no accounts with empty passwords were found".to_string());
    }
    for user in &users {
        run_user_program(passwd, &["-l", "--", user], "passwd")?;
    }
    Ok(())
}

/// `security_apply <operation>`: the closed set of file/service hardening
/// operations from the Security Auditor. No path, unit or content is supplied
/// by the caller.
fn security_apply_action(op: &str) -> Result<(), String> {
    root_policy::security_apply_op_valid(op)?;
    match op {
        "password_policy" => {
            rewrite_settings(
                "login.defs",
                &["PASS_MAX_DAYS", "PASS_MIN_LEN"],
                &[("PASS_MAX_DAYS", "90"), ("PASS_MIN_LEN", "12")],
            )
        }
        "tmp_sticky_on" | "tmp_sticky_off" => {
            let enable = op == "tmp_sticky_on";
            let mode = if enable { 0o1777 } else { 0o777 };
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                SECURITY_TMP_DIR,
                std::fs::Permissions::from_mode(mode),
            )
            .map_err(|e| format!("lcp-admin: could not change {SECURITY_TMP_DIR} permissions ({e})"))?;
            if enable {
                security_write_file("tmpfiles.d/tmp-sticky.conf", "D /tmp 1777 root root -\n")
            } else {
                security_remove_file("tmpfiles.d/tmp-sticky.conf")
            }
        }
        "passwd_perms" => {
            let path = security_path("passwd");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .map_err(|e| format!("lcp-admin: could not chmod {path} ({e})"))?;
            security_ensure_root_owner(&path)
        }
        "shadow_perms" => {
            let path = security_path("shadow");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))
                .map_err(|e| format!("lcp-admin: could not chmod {path} ({e})"))?;
            security_ensure_root_owner(&path)
        }
        "pam_faillock_on" => {
            security_write_file(
                "security/faillock.conf.d/50-hardening.conf",
                "deny = 5\nunlock_time = 900\nfail_interval = 900\n",
            )?;
            if let Some(authselect) = authselect_bin() {
                let status = std::process::Command::new(authselect)
                    .args(["enable-feature", "with-faillock"])
                    .status()
                    .map_err(|e| format!("lcp-admin: authselect could not be started ({e})"))?;
                if !status.success() {
                    return Err(
                        "lcp-admin: the faillock policy file was written but authselect could not enable the feature"
                            .to_string(),
                    );
                }
            }
            Ok(())
        }
        "pam_faillock_off" => {
            security_remove_file("security/faillock.conf.d/50-hardening.conf")?;
            if let Some(authselect) = authselect_bin() {
                let status = std::process::Command::new(authselect)
                    .args(["disable-feature", "with-faillock"])
                    .status()
                    .map_err(|e| format!("lcp-admin: authselect could not be started ({e})"))?;
                if !status.success() {
                    return Err(
                        "lcp-admin: the faillock policy file was removed but authselect could not disable the feature"
                            .to_string(),
                    );
                }
            }
            Ok(())
        }
        "umask_on" | "umask_off" => {
            let value = if op == "umask_on" { "027" } else { "022" };
            rewrite_settings("login.defs", &["UMASK"], &[("UMASK", value)])
        }
        "ssh_idle_on" => {
            security_write_file(
                "ssh/sshd_config.d/50-idle-timeout.conf",
                "ClientAliveInterval 300\nClientAliveCountMax 3\n",
            )?;
            let _ = reload_ssh(false);
            Ok(())
        }
        "ssh_idle_off" => {
            security_remove_file("ssh/sshd_config.d/50-idle-timeout.conf")?;
            let _ = reload_ssh(false);
            Ok(())
        }
        "blacklist_fs_on" => security_write_file(
            "modprobe.d/disable-unused-fs.conf",
            "install cramfs /bin/true\ninstall freevxfs /bin/true\ninstall hfs /bin/true\ninstall hfsplus /bin/true\ninstall jffs2 /bin/true\ninstall udf /bin/true\n",
        ),
        "blacklist_fs_off" => security_remove_file("modprobe.d/disable-unused-fs.conf"),
        "legal_banner_on" => {
            let banner = "Authorized uses only. All activity may be monitored and reported.\n";
            security_write_file("issue", banner)?;
            security_write_file("issue.net", banner)?;
            security_write_file("ssh/sshd_config.d/50-banner.conf", "Banner /etc/issue.net\n")?;
            let _ = reload_ssh(false);
            Ok(())
        }
        "legal_banner_off" => {
            security_remove_file("ssh/sshd_config.d/50-banner.conf")?;
            let _ = reload_ssh(false);
            Ok(())
        }
        other => Err(format!("lcp-admin: unsupported security operation: {other}")),
    }
}

/// `unit_action <verb> <unit>`: the system-scope unit actions. The shared
/// policy enforces unit-name grammar and the protected-unit rules; --user
/// actions stay unprivileged in the app.
fn unit_action_action(verb: &str, unit: &str) -> Result<(), String> {
    root_policy::unit_action_valid(verb, unit)?;
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return Err("lcp-admin: systemctl is not available".to_string());
    }
    run_firewall_program(SYSTEMCTL_BIN, &[verb, unit], "systemctl")
}

/// Write an already-validated systemd override path atomically with a fixed
/// 0644 root-owned file. Used both for the new content and for rollback (so a
/// rollback never touches the preserved backup).
fn write_systemd_override(path: &str, content: &str) -> Result<(), String> {
    let tmp = format!("{path}.tmp.{}", std::process::id());
    std::fs::write(&tmp, content)
        .map_err(|e| format!("lcp-admin: could not write {tmp} ({e})"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("lcp-admin: could not replace {path} ({e})"));
    }
    Ok(())
}

/// `unit_dropin_write <unit>`: write the fixed `<unit>.d/override.conf`
/// drop-in with content from stdin, then daemon-reload. The unit name is
/// validated, the drop-in tree is symlink-checked, and the content is bounded.
fn unit_dropin_write_action(unit: &str) -> Result<(), String> {
    root_policy::service_unit_name_valid(unit)?;
    let content = read_stdin_content(64 * 1024, "unit override")?;
    if content.contains('\0') {
        return Err("lcp-admin: the unit override must not contain NUL bytes".to_string());
    }
    if root_policy::service_protection_level(unit)
        .is_some_and(|level| level == "critical")
        && content.trim().is_empty()
    {
        return Err(format!(
            "lcp-admin: writing an empty override to critical system unit '{unit}' is blocked"
        ));
    }

    // systemd drop-ins live in `<unit>.d` — keep the full unit name.
    let dir = format!("{SYSTEMD_UNIT_ROOT}/{unit}.d");
    let path = format!("{dir}/override.conf");
    let root = std::path::Path::new(SYSTEMD_UNIT_ROOT);
    if root
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("lcp-admin: refusing a symlinked systemd unit root".to_string());
    }
    let mut current = root.to_path_buf();
    for component in format!("{unit}.d").split('/') {
        current.push(component);
        if let Ok(metadata) = current.symlink_metadata() {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "lcp-admin: refusing a drop-in path containing a symbolic link: {}",
                    current.display()
                ));
            }
        }
    }
    if std::path::Path::new(&path)
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(format!("lcp-admin: refusing to write a symlinked override: {path}"));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("lcp-admin: could not create {dir} ({e})"))?;
    let previous = std::fs::read_to_string(&path).ok();
    if let Some(old) = &previous {
        let backup = format!("{path}.control-panel.bak");
        if std::path::Path::new(&backup)
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(format!("lcp-admin: refusing a symlinked override backup: {backup}"));
        }
        let backup_tmp = format!("{backup}.tmp.{}", std::process::id());
        std::fs::write(&backup_tmp, old)
            .map_err(|e| format!("lcp-admin: could not write {backup_tmp} ({e})"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&backup_tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&backup_tmp, &backup)
            .map_err(|e| format!("lcp-admin: could not replace {backup} ({e})"))?;
    }
    write_systemd_override(&path, &content)?;
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return Err("lcp-admin: the override was written but systemctl is not available for daemon-reload".to_string());
    }
    if let Err(reload_error) = run_firewall_program(SYSTEMCTL_BIN, &["daemon-reload"], "systemctl") {
        let rollback = match &previous {
            Some(old) => write_systemd_override(&path, old),
            None => std::fs::remove_file(&path)
                .map_err(|e| format!("lcp-admin: the override could not be removed ({e})")),
        };
        let retry = if rollback.is_ok() {
            run_firewall_program(SYSTEMCTL_BIN, &["daemon-reload"], "systemctl")
        } else {
            Err("lcp-admin: retry reload skipped because the rollback failed".to_string())
        };
        let disk_state = match (&rollback, &previous) {
            (Ok(()), Some(_)) => {
                "the previous override was restored on disk".to_string()
            }
            (Ok(()), None) => {
                "the newly written override was removed from disk".to_string()
            }
            (Err(e), _) => format!("the on-disk state could not be restored: {e}"),
        };
        let reload_state = match &retry {
            Ok(()) => "a follow-up daemon-reload succeeded".to_string(),
            Err(e) => format!(
                "systemd was NOT reloaded by the retry ({e}); run the unit_daemon_reload recovery action to retry without rewriting configuration"
            ),
        };
        let prefix = if retry.is_err() {
            "RECOVERY_REQUIRED: "
        } else {
            "lcp-admin: "
        };
        return Err(format!("{prefix}{reload_error}; {disk_state}; {reload_state}"));
    }
    Ok(())
}

/// `unit_daemon_reload`: retry the fixed systemd daemon-reload without
/// rewriting any configuration. Used as the recovery path after a failed
/// rollback retry.
fn unit_daemon_reload_action() -> Result<(), String> {
    if !std::path::Path::new(SYSTEMCTL_BIN).is_file() {
        return Err("lcp-admin: systemctl is not available".to_string());
    }
    run_firewall_program(SYSTEMCTL_BIN, &["daemon-reload"], "systemctl")
}

/// Named actions the helper performs itself, instead of executing a root
/// program. Their arguments are validated by `check_helper_action` in
/// root_policy.rs, so the helper only ever `exec`s programs for the generic
/// allow-listed calls.
///
/// This gate list must contain exactly the same action names as
/// `HELPER_ACTIONS` in root_policy.rs — a name missing here is never
/// dispatched and falls through to program resolution ("program X was not
/// found"). Unit tests in this file assert the two lists are equal; a
/// compile-time assertion below keeps them identical at build time too.
///
/// `write_repo_file` (STEP 3B scope C) is fully implemented below — validator
/// reuse from the shared root_policy, a descriptor-pinned write core, unit
/// tests against fixtures — but it is deliberately **not registered yet**:
/// registering it would activate a new root-privileged write channel for
/// every polkit-authorised caller while the app side still cannot use it
/// (its bash-script channel is fail-closed and stays fail-closed). Enabling
/// is a separate, explicitly authorised step: add `"write_repo_file"` here
/// and to `HELPER_ACTIONS` (plus a path-action branch in
/// `check_helper_action`) in root_policy.rs, and register its app-side
/// caller. Until then the action name is refused at dispatch, fail-closed.
const ROOT_ACTIONS: &[&str] = &[
    "admin_keep", "install_polkit_rule", "read_user_ssh_keys", "write_user_ssh_keys",
    "write_profile_sh", "restore_profile_sh",
    "nginx_config_install", "nginx_config_backup", "nginx_restore_backup",
    "nginx_link_site", "nginx_unlink_site", "nginx_config_remove",
    "nginx_www_fs", "nginx_www_install",
    "nmcli_connection_save", "nmcli_connection_delete", "nmcli_connection_import",
    "nmcli_connection_up", "nmcli_connection_down",
    "repo_toggle_enable", "repo_toggle_disable",
    "repo_add",
    "repo_delete", "repo_makecache", "repo_clean_cache",
    "firewall_zone_rule", "firewall_rich_rule",
    "firewall_panic",
    "firewall_interface_zone",
    "firewall_enable_hardened",
    "pkg_maintenance",
    "dnf_history_undo",
    "user_add",
    "user_delete",
    "user_set_password",
    "user_toggle_sudo",
    "user_modify_group",
    "group_manage",
    "user_lock",
    "session_kill",
    "security_service_toggle",
    "security_ctrl_alt_del",
    "security_apply",
    "security_ssh_param",
    "security_kernel_param",
    "security_lock_account",
    "unit_action",
    "unit_dropin_write",
    "unit_daemon_reload",
];

const fn const_str_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

// Compile-time gate for the shared action list: the helper may only
// dispatch names present in both lists, so a name added on one side
// cannot silently become unreachable on the other.
const _: () = {
    let dispatch = ROOT_ACTIONS;
    let validated = root_policy::HELPER_ACTIONS;
    assert!(
        dispatch.len() == validated.len(),
        "ROOT_ACTIONS and HELPER_ACTIONS must contain the same actions"
    );
    let mut index = 0;
    while index < dispatch.len() {
        assert!(
            const_str_eq(dispatch[index], validated[index]),
            "ROOT_ACTIONS and HELPER_ACTIONS differ at the same position"
        );
        index += 1;
    }
};

/// Read everything the caller pipes into the helper (up to `limit + 1`
/// bytes); larger input is refused. Every named action's content channel is
/// stdin, so hostile apps can only influence the action's document, never its
/// argv.
fn read_stdin_content(limit: u64, what: &str) -> Result<String, String> {
    use std::io::Read;
    let mut content = String::new();
    match std::io::BufReader::new(std::io::stdin())
        .take(limit + 1)
        .read_to_string(&mut content)
    {
        Ok(_) if content.len() as u64 > limit => {
            Err(format!("lcp-admin: the {what} is too large."))
        }
        Ok(_) => Ok(content),
        Err(_) => Err(format!("lcp-admin: the {what} is not valid UTF-8 (or unreadable).")),
    }
}

/// The byte-exact twin of `read_stdin_content` for the web-file install:
/// binary content is legitimate there (images, archives, fonts), so no
/// UTF-8 or NUL rule can apply — only the size limit.
fn read_stdin_bytes(limit: u64, what: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut content = Vec::new();
    match std::io::BufReader::new(std::io::stdin())
        .take(limit + 1)
        .read_to_end(&mut content)
    {
        Ok(_) if content.len() as u64 > limit => {
            Err(format!("lcp-admin: the {what} is too large."))
        }
        Ok(_) => Ok(content),
        Err(_) => Err(format!("lcp-admin: the {what} could not be read.")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(requested) = args.first() else {
        eprintln!("lcp-admin: no program given.");
        std::process::exit(2);
    };

    // Named actions are dispatched before any program resolution: their first
    // argument is an action name, not a program. Each action returns its own
    // result — the process only reaches exit 0 for an operation that
    // succeeded, and exit 2 (with the reason on stderr) for everything else.
    if ROOT_ACTIONS.contains(&requested.as_str()) {
        let outcome = if requested == "admin_keep" {
            // Exists only to take the panel's one kept authorisation: the
            // polkit rule grants AUTH_ADMIN_KEEP to this helper and the
            // authorisation is stored for the invoking process. No file is
            // opened, no data is read or written, nothing is printed.
            if args.len() == 1 {
                Ok(())
            } else {
                Err(format!("lcp-admin: the action '{requested}' takes no further arguments."))
            }
        } else if requested == "install_polkit_rule" {
            if args.len() == 1 {
                install_polkit_rule_action()
            } else {
                Err(format!("lcp-admin: the action '{requested}' takes no further arguments."))
            }
        } else if requested == "write_profile_sh" {
            if args.len() == 2 {
                write_profile_sh_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one profile file path."
                ))
            }
        } else if requested == "restore_profile_sh" {
            if args.len() == 3 {
                restore_profile_sh_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one profile file path and one backup sibling path."
                ))
            }
        } else if requested == "nginx_config_install" {
            if args.len() == 2 {
                nginx_config_install_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one nginx configuration path."
                ))
            }
        } else if requested == "nginx_config_backup" {
            if args.len() == 2 {
                nginx_config_backup_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one existing nginx configuration path."
                ))
            }
        } else if requested == "nginx_restore_backup" {
            if args.len() == 3 {
                nginx_restore_backup_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one nginx destination path and one backup sibling path."
                ))
            }
        } else if requested == "nginx_link_site" {
            if args.len() == 3 {
                nginx_link_site_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one sites-available path and one sites-enabled path."
                ))
            }
        } else if requested == "nginx_unlink_site" {
            if args.len() == 2 {
                nginx_unlink_site_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one sites-enabled symlink path to remove."
                ))
            }
        } else if requested == "nginx_config_remove" {
            if args.len() == 2 {
                nginx_config_remove_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one nginx configuration path to remove."
                ))
            }
        } else if requested == "nginx_www_fs" {
            if args.len() >= 2 {
                nginx_www_fs_action(&args[1..])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes a sub-command: mkdir, remove or rename."
                ))
            }
        } else if requested == "nginx_www_install" {
            if args.len() == 2 {
                nginx_www_install_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one web-root destination path."
                ))
            }
        } else if requested == "nmcli_connection_save" {
            nmcli_connection_save_action(&args)
        } else if requested == "nmcli_connection_delete" {
            if args.len() == 2 {
                nmcli_connection_delete_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one connection UUID."
                ))
            }
        } else if requested == "nmcli_connection_up" {
            if args.len() == 2 {
                nmcli_connection_up_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one connection UUID."
                ))
            }
        } else if requested == "nmcli_connection_down" {
            if args.len() == 2 {
                nmcli_connection_down_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one connection UUID."
                ))
            }
        } else if requested == "nmcli_connection_import" {
            if args.len() == 3 {
                nmcli_connection_import_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly an import type and a file path."
                ))
            }
        } else if requested == "repo_toggle_enable" {
            if args.len() == 3 {
                repo_toggle_enable_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly a repo file path and a repo id."
                ))
            }
        } else if requested == "repo_toggle_disable" {
            if args.len() == 3 {
                repo_toggle_disable_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly a repo file path and a repo id."
                ))
            }
        } else if requested == "firewall_zone_rule" {
            // Optional trailing confirmation: [verb, zone, kind, value,
            // confirm?]. Only the exact literal authorizes.
            match firewall_confirm_split(&args, 5, 6) {
                Ok((args, confirmed)) => match firewall_action_add(&args[1], requested) {
                    Ok(add) => {
                        firewall_zone_rule_action(add, &args[2], &args[3], &args[4], confirmed)
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        } else if requested == "firewall_rich_rule" {
            // Optional trailing confirmation: [verb, zone, rule, confirm?].
            match firewall_confirm_split(&args, 4, 5) {
                Ok((args, confirmed)) => match firewall_action_add(&args[1], requested) {
                    Ok(add) => firewall_rich_rule_action(add, &args[2], &args[3], confirmed),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        } else if requested == "firewall_interface_zone" {
            // Optional trailing confirmation: [zone, interface, confirm?].
            // There is no add/remove verb: the change op moves the binding.
            match firewall_confirm_split(&args, 3, 4) {
                Ok((args, confirmed)) => {
                    firewall_interface_zone_action(&args[1], &args[2], confirmed)
                }
                Err(error) => Err(error),
            }
        } else if requested == "firewall_enable_hardened" {
            // Optional trailing confirmation: [firewalld|ufw, confirm?].
            match firewall_confirm_split(&args, 2, 3) {
                Ok((args, confirmed)) => {
                    firewall_enable_hardened_action(&args[1], confirmed)
                }
                Err(error) => Err(error),
            }
        } else if requested == "unit_daemon_reload" {
            if args.len() == 1 {
                unit_daemon_reload_action()
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes no further arguments."
                ))
            }
        } else if requested == "unit_action" {
            if args.len() == 3 {
                unit_action_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes a verb and a unit name."
                ))
            }
        } else if requested == "unit_dropin_write" {
            if args.len() == 2 {
                unit_dropin_write_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one unit name."
                ))
            }
        } else if requested == "security_apply" {
            if args.len() == 2 {
                security_apply_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one security operation."
                ))
            }
        } else if requested == "security_ssh_param" {
            if args.len() == 3 {
                security_ssh_param_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes an SSH parameter and its value."
                ))
            }
        } else if requested == "security_kernel_param" {
            if args.len() == 3 {
                security_kernel_param_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes a sysctl key and its value."
                ))
            }
        } else if requested == "security_lock_account" {
            if args.len() == 1 {
                security_lock_account_action(None)
            } else if args.len() == 2 {
                security_lock_account_action(Some(&args[1]))
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes an optional account name."
                ))
            }
        } else if requested == "security_service_toggle" {
            if args.len() == 3 {
                security_service_toggle_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes a service name and enable|disable."
                ))
            }
        } else if requested == "security_ctrl_alt_del" {
            if args.len() == 2 {
                security_ctrl_alt_del_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes mask|unmask."
                ))
            }
        } else if requested == "user_add" {
            if args.len() == 2 {
                user_add_action(&args[1], None)
            } else if args.len() == 3 {
                user_add_action(&args[1], Some(&args[2]))
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes an account name and an optional full name."
                ))
            }
        } else if requested == "user_delete" {
            match args.as_slice() {
                [_, username] => user_delete_action(username, false),
                [_, username, flag] if flag == "remove-home" => user_delete_action(username, true),
                _ => Err(format!(
                    "lcp-admin: the action '{requested}' takes an account name and the optional literal 'remove-home'."
                )),
            }
        } else if requested == "user_set_password" {
            if args.len() == 2 {
                user_set_password_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one account name."
                ))
            }
        } else if requested == "user_toggle_sudo" {
            match args.as_slice() {
                [_, username, verb] if verb == "grant" => user_toggle_sudo_action(username, true),
                [_, username, verb] if verb == "revoke" => user_toggle_sudo_action(username, false),
                _ => Err(format!(
                    "lcp-admin: the action '{requested}' takes an account name and grant|revoke."
                )),
            }
        } else if requested == "user_modify_group" {
            match args.as_slice() {
                [_, username, group, verb] if verb == "add" => {
                    user_modify_group_action(username, group, true)
                }
                [_, username, group, verb] if verb == "remove" => {
                    user_modify_group_action(username, group, false)
                }
                _ => Err(format!(
                    "lcp-admin: the action '{requested}' takes an account name, a group name and add|remove."
                )),
            }
        } else if requested == "group_manage" {
            if args.len() == 3 {
                group_manage_action(&args[1], &args[2])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes add|delete and a group name."
                ))
            }
        } else if requested == "user_lock" {
            match args.as_slice() {
                [_, username, verb] if verb == "lock" => user_lock_action(username, true),
                [_, username, verb] if verb == "unlock" => user_lock_action(username, false),
                _ => Err(format!(
                    "lcp-admin: the action '{requested}' takes an account name and lock|unlock."
                )),
            }
        } else if requested == "session_kill" {
            if args.len() == 2 {
                session_kill_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one session id."
                ))
            }
        } else if requested == "dnf_history_undo" {
            // Optional trailing confirmation: [id, confirm?]. Only the exact
            // literal authorizes a rollback whose preview is risky.
            match firewall_confirm_split(&args, 2, 3) {
                Ok((args, confirmed)) => dnf_history_undo_action(&args[1], confirmed),
                Err(error) => Err(error),
            }
        } else if requested == "pkg_maintenance" {
            if args.len() == 2 {
                pkg_maintenance_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one maintenance operation."
                ))
            }
        } else if requested == "firewall_panic" {
        // Optional trailing confirmation: [verb, confirm?]. No zone
        // argument: panic mode applies globally.
        match firewall_confirm_split(&args, 2, 3) {
            Ok((args, confirmed)) => match args[1].as_str() {
                "enable" | "disable" => {
                    firewall_panic_action(args[1] == "enable", confirmed)
                }
                other => Err(format!(
                    "lcp-admin: the firewall_panic verb must be enable or disable, got: {other}"
                )),
            },
            Err(error) => Err(error),
        }
        } else if requested == "repo_add" {
            if args.len() == 2 {
                repo_add_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly a repository URL."
                ))
            }
        } else if requested == "repo_delete" {
            if args.len() == 2 {
                repo_delete_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one repository file path."
                ))
            }
        } else if requested == "repo_makecache" {
            if args.len() == 1 {
                repo_makecache_action()
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes no further arguments."
                ))
            }
        } else if requested == "repo_clean_cache" {
            if args.len() == 2 {
                repo_clean_cache_action(&args[1])
            } else {
                Err(format!(
                    "lcp-admin: the action '{requested}' takes exactly one repository id."
                ))
            }
        } else if args.len() == 2 {
            let account = &args[1];
            is_valid_account_name(account).and_then(|_| {
                if requested == "read_user_ssh_keys" {
                    read_user_ssh_keys_action(account)
                } else {
                    write_user_ssh_keys_action(account)
                }
            })
        } else {
            Err(format!(
                "lcp-admin: the action '{requested}' takes exactly one account name."
            ))
        };
        match outcome {
            Ok(()) => {
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            }
            Err(message) => {
                // policy errors arrive without the helper's own prefix;
                // CONFIRMATION_REQUIRED must reach the app with its exact
                // prefix intact — the caller retries on that literal.
                let line = if message.starts_with("CONFIRMATION_REQUIRED:")
                    || message.starts_with("RECOVERY_REQUIRED:")
                {
                    message.clone()
                } else if let Some(rest) = message.strip_prefix("lcp-admin: ") {
                    format!("lcp-admin: {rest}")
                } else {
                    format!("lcp-admin: {message}")
                };
                eprintln!("{line}");
                std::process::exit(2);
            }
        }
    }

    // The program has to be resolved to an absolute path before it is checked
    // — otherwise a relative name could pick up a program from the PATH.
    let Some(program) = resolve_root_program(requested) else {
        eprintln!("lcp-admin: the program '{requested}' was not found.");
        std::process::exit(2);
    };
    let rest: Vec<String> = args[1..].to_vec();

    if let Err(message) = ensure_program_allowed(&program)
        .and_then(|_| ensure_no_shell_escape(&program, &rest))
        .and_then(|_| ensure_safe_arguments(&rest))
        .and_then(|_| ensure_arguments_for_program(&program, &rest))
        .and_then(|_| verify_trusted_binary(&program))
    {
        eprintln!("lcp-admin: {message}");
        std::process::exit(2);
    }

    // `exec` replaces the helper with the program, so the exit status and the
    // output of the program are the ones the app sees.
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(&program).args(&rest).exec();
    eprintln!("lcp-admin: could not start {program}: {error}");
    std::process::exit(2);
}

/// The `install_polkit_rule` action: writes the PolicyKit rule to its one,
/// fixed location so the bootstrap does not need a shell, a temp file in a
/// user-writable directory or base64 in a script.
///
/// The rule text arrives on stdin and is written to `POLKIT_RULE_FILE` only
/// when it is byte-identical to the shared `POLKIT_RULE_TEXT` constant the
/// helper carries itself. The app cannot smuggle a different rule through —
/// even if it were compromised, the strongest it can grant is still exactly
/// "the helper may run as root".
fn install_polkit_rule_action() -> Result<(), String> {
    // The rule is small; refuse anything larger before reading it.
    const MAX_RULE_BYTES: u32 = 128 * 1024;

    let content = read_stdin_content(u64::from(MAX_RULE_BYTES), "rule content")?;

    if content != POLKIT_RULE_TEXT {
        return Err(format!(
            "lcp-admin: the rule content does not match the rule this helper installs; refusing to write '{POLKIT_RULE_FILE}'."
        ));
    }

    // Atomic replacement: write the sibling temp file, then rename over the
    // destination. rename(2) keeps the previous rule until the very last
    // moment and never leaves a half-written rules.d entry (an unreadable
    // rules.d entry makes polkit ignore *all* rules). The temp name is fixed
    // but that cannot race into a mixed file: every run that reaches the
    // write carries the same byte-identical content, and rename(2) itself is
    // atomic. The file ends up root:root implicitly — the helper runs as root.
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    let temp = std::path::Path::new(POLKIT_RULE_FILE).with_extension("rules.tmp");
    let _ = std::fs::remove_file(&temp);
    if let Err(error) = std::fs::write(&temp, content.as_bytes())
        .and_then(|_| std::fs::set_permissions(&temp, Permissions::from_mode(0o644)))
        .and_then(|_| std::fs::rename(&temp, POLKIT_RULE_FILE))
    {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("lcp-admin: could not install the rule: {error}"));
    }
    Ok(())
}

/// Look a local account up in `/etc/passwd` — by exact name match, never by
/// path arithmetic. Returns `(uid, gid, home)`.
///
/// The panel targets Fedora with the local files NSS backend, so this keeps
/// the helper dependency-free instead of linking against libc's getpwnam.
fn passwd_entry(username: &str) -> Option<(u32, u32, String)> {
    let passwd = std::fs::read_to_string(PASSWD_FILE).ok()?;
    for line in passwd.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() == 7 && fields[0] == username {
            let uid: u32 = fields[2].parse().ok()?;
            let gid: u32 = fields[3].parse().ok()?;
            if fields[5].is_empty() || !fields[5].starts_with('/') {
                return None;
            }
            return Some((uid, gid, fields[5].to_string()));
        }
    }
    None
}

// ─── descriptor-pinned filesystem core ───────────────────────────────────────
//
// Everything that touches a *user-owned* directory runs through descriptors
// relative to one verified directory inode:
//
// * the directory is opened `O_NOFOLLOW` and pinned — every later `openat`,
//   `renameat`, `unlinkat`, `statat` call resolves its name against the
//   *inode behind the descriptor*, so a rename or a symlink the account
//   plants inside the directory can never redirect a later step,
// * all ownership and permission changes are applied with `fchown`/`fchmod`
//   *to a descriptor we created ourselves* — there is no check-then-use gap,
//   and there are no path-based `chmod`/`chown` calls at all after the
//   rename, which is exactly the window the old code raced on,
// * the rename(2) is the only step that publishes the file; it replaces
//   whatever sits at the destination name (symlink or not) instead of
//   following it.

/// File-type bits inside `st_mode` (std does not re-export the constants and
/// the helper links no libc).
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

fn open_dir_nofollow(dir: impl AsFd, name: &str, what: &str) -> Result<OwnedFd, String> {
    rustix::fs::openat(
        dir.as_fd(),
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| format!("lcp-admin: '{what}' is not a real directory (or is a symlink); refusing."))
}

fn stat_fd(fd: impl AsFd, what: &str) -> Result<Stat, String> {
    rustix::fs::fstat(fd).map_err(|_| format!("lcp-admin: could not stat {what}."))
}

/// `~/.ssh` of one account, pinned by descriptor and verified: a real
/// directory (opened no-follow), owned by the account, not group/world
/// writable. `create` enables creating it with the fixed shape when missing.
#[derive(Debug)]
struct SshDir {
    dir: OwnedFd,
    uid: u32,
    gid: u32,
}

fn verify_ssh_dir(dir: &OwnedFd, uid: u32, username: &str) -> Result<(), String> {
    let stat = stat_fd(dir, "~/.ssh")?;
    let what = format!("{username}/.ssh");
    if (stat.st_mode & S_IFMT) != S_IFDIR {
        return Err(format!("lcp-admin: '{what}' is not a directory; refusing."));
    }
    // created/owned by the account itself — a home directory the account can
    // not own is also a directory it may not prepare its keys in
    if stat.st_uid != uid {
        return Err(format!("lcp-admin: '{what}' is not owned by '{username}'; refusing."));
    }
    if stat.st_mode & 0o022 != 0 {
        return Err(format!(
            "lcp-admin: '{what}' is group- or world-writable; refusing."
        ));
    }
    Ok(())
}

/// Resolve `~/.ssh` for an account: open it pinned and verified, or (only for
/// the write path) create it with the fixed shape. A symlink planted where
/// the directory belongs fails the no-follow open — it is never followed.
fn ssh_dir_open_or_create(home_fd: &OwnedFd, uid: u32, gid: u32, create: bool) -> Result<SshDir, String> {
    // two passes at most: open → create-on-miss → open again
    for _attempt in 0..3 {
        match rustix::fs::openat(
            home_fd,
            ".ssh",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(dir) => {
                verify_ssh_dir(&dir, uid, "~/.ssh")?;
                return Ok(SshDir { dir, uid, gid });
            }
            Err(err) if err == rustix::io::Errno::NOENT && create => {
                // mkdirat with the fixed mode: either we win the create or
                // something is already sitting at that name — a planted
                // symlink or file then dies on the next no-follow open pass
                // instead of ever being followed (the old code swallowed
                // EEXIST and then ran path-based chmod/chown straight
                // through it).
                if rustix::fs::mkdirat(home_fd, ".ssh", Mode::from_raw_mode(0o700)).is_ok() {
                    // root created it — hand it to the account via
                    // descriptors (open it no-follow first: another pass in
                    // between must not be able to swap it)
                    if let Ok(dir) = rustix::fs::openat(
                        home_fd,
                        ".ssh",
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    ) {
                        if rustix::fs::fchown(
                            &dir,
                            Some(Uid::from_raw(uid)),
                            Some(Gid::from_raw(gid)),
                        )
                        .is_ok()
                            && rustix::fs::fchmod(&dir, Mode::from_raw_mode(0o700)).is_ok()
                            && verify_ssh_dir(&dir, uid, "the created .ssh").is_ok()
                        {
                            return Ok(SshDir { dir, uid, gid });
                        }
                        // failed ownership/mode transfer: fail closed
                        return Err("lcp-admin: could not set the owner and mode of .ssh; refusing.".to_string());
                    }
                }
                continue; // EEXIST or a race — re-check by opening
            }
            Err(err) if err == rustix::io::Errno::NOENT => {
                return Err(format!("lcp-admin: no .ssh directory exists for this account."));
            }
            Err(_) => {
                return Err(format!("lcp-admin: '.ssh' could not be opened as its real directory; refusing."));
            }
        }
    }
    Err("lcp-admin: '.ssh' could not be settled after repeated attempts; refusing.".to_string())
}

/// Unique temp-file suffix helper: pid plus a nanosecond stamp — every temp
/// file is created `O_EXCL`, so name collisions only cost a retry, never
/// correctness.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}.{}", std::process::id(), nanos)
}


/// The `.ssh` directory of an account, resolved from `/etc/passwd`: the home
/// directory is opened (its path is an administrator-controlled value from
/// passwd, not user input) and the `.ssh` entry inside it is opened
/// `O_NOFOLLOW`, pinned and verified.
fn ssh_dir_of(username: &str, create: bool) -> Result<SshDir, String> {
    let Some((uid, gid, home)) = passwd_entry(username) else {
        return Err(format!(
            "lcp-admin: no local account '{username}' with a valid home directory."
        ));
    };
    let home_fd = open_dir_nofollow(&CWD, &home, "the home directory")
        .or_else(|_| {
            // a symlinked home directory (an administrator setup, not user
            // input) is still acceptable: the directory is opened and the
            // `.ssh` entry inside it is verified by inode
            rustix::fs::openat(
                CWD,
                &home,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "lcp-admin: the account's home directory could not be opened.".to_string())
        })?;
    ssh_dir_open_or_create(&home_fd, uid, gid, create)
}

/// Core of the write: install `content` as `authorized_keys` inside the
/// pinned `~/.ssh`. Testable without an account (see the tests module).
fn install_keys(ssh: &SshDir, content: &str) -> Result<(), String> {
    use std::io::Write;

    const MAX_KEYS_BYTES: u64 = 1024 * 1024;
    if content.len() as u64 > MAX_KEYS_BYTES {
        return Err("lcp-admin: the key content is too large.".to_string());
    }

    let temp_name = format!("authorized_keys.tmp.{}", unique_suffix());

    // 1. create our own inode with O_CREAT|O_EXCL|O_NOFOLLOW — no matter what
    //    sits at the temp path, this open never follows it
    let fd = rustix::fs::openat(
        &ssh.dir,
        &temp_name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| "lcp-admin: could not create the temporary key file in ~/.ssh".to_string())?;

    // 2. everything else happens on this descriptor: the inode is ours, so
    //    there is no path left for a concurrent rename/symlink to influence
    let cleanup = |name: &str| -> Result<(), String> {
        rustix::fs::unlinkat(&ssh.dir, name, AtFlags::empty())
            .map_err(|_| "lcp-admin: could not remove the aborted temporary file".to_string())
    };

    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::from(fd);
        rustix::fs::fchmod(&file, Mode::from_raw_mode(0o600))
            .map_err(|_| "lcp-admin: could not set the key file mode".to_string())?;
        file.write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("lcp-admin: could not write the key content: {e}"))?;
        // The account must own its keys — done via the still-held descriptor,
        // before the rename. (When not running as root, e.g. in the tests,
        // fchowning an already-owned file is a no-op; under a different
        // owner it is refused, failing the install closed.)
        if rustix::fs::fchown(&file, Some(Uid::from_raw(ssh.uid)), Some(Gid::from_raw(ssh.gid))).is_err() {
            return Err("lcp-admin: could not set the key file owner".to_string());
        }
        // 3. publish with renameat — atomic; replaces whatever the
        //    destination name points to instead of following it. After this
        //    there are deliberately *no further mode/owner calls*: the file
        //    already has them, and a path-based call here would be the
        //    symlink race the old code had.
        rustix::fs::renameat(
            ssh.dir.as_fd(),
            temp_name.as_str(),
            ssh.dir.as_fd(),
            "authorized_keys",
        )
        .map_err(|_| "lcp-admin: could not publish the authorized_keys file".to_string())
    })();
    // the owned descriptor lives inside the closure's File and is closed
    // when the result is computed — nothing leaks past the install
    match result {
        Ok(()) => Ok(()),
        Err(message) => {
            let _ = cleanup(&temp_name);
            Err(message)
        }
    }
}

/// Core of the read: the account's authorized_keys, via a no-follow open and
/// an owner/type check on the descriptor. `None` when there is no file.
fn read_keys(ssh: &SshDir) -> Result<Option<String>, String> {
    let fd = match rustix::fs::openat(
        &ssh.dir,
        "authorized_keys",
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(err) if err == rustix::io::Errno::NOENT => return Ok(None),
        Err(_) => {
            return Err(
                "lcp-admin: could not open the authorized_keys file; refusing.".to_string()
            )
        }
    };
    // stale nofollow guarantee + inode check: the fd is the file that the
    // name referred to during the open
    let stat = stat_fd(&fd, "authorized_keys")?;
    if (stat.st_mode & S_IFMT) != S_IFREG || stat.st_uid != ssh.uid {
        return Err(format!(
            "lcp-admin: the authorized_keys is not a regular file owned by the account; refusing to read it."
        ));
    }
    let mut buffer = Vec::new();
    use std::io::Read;
    std::fs::File::from(fd)
        .read_to_end(&mut buffer)
        .map_err(|_| "lcp-admin: could not read the authorized_keys file".to_string())?;
    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
}

/// The `read_user_ssh_keys <account>` action: print the account's
/// authorized_keys to stdout (empty output when there is none).
///
/// The key file itself must be a regular file owned by the account — a
/// symlink planted on the victim's authorized_keys could otherwise make the
/// root helper copy `/etc/shadow` into the panel's UI.
fn read_user_ssh_keys_action(username: &str) -> Result<(), String> {
    let ssh = match ssh_dir_of(username, false) {
        Ok(ssh) => ssh,
        // a missing .ssh means there is nothing to read — an empty list,
        // not an error
        Err(message) if message.contains("no .ssh directory") => return Ok(()),
        Err(message) => return Err(message),
    };
    let Some(content) = read_keys(&ssh)? else {
        return Ok(());
    };
    print!("{content}");
    Ok(())
}

/// The `write_user_ssh_keys <account>` action: takes the key content on
/// stdin and installs it as the account's authorized_keys.
///
/// * the content travels on stdin — nothing user-supplied reaches an argv,
/// * all filesystem steps run via descriptors pinned to the verified `.ssh`
///   inode (see the core section header),
/// * ownership and modes are fixed by the helper on its own descriptor:
///   `.ssh` 0700 account-owned, `authorized_keys` 0600 account-owned —
///   enforced by `fchown`/`fchmod` *before* the rename, never through paths.
fn write_user_ssh_keys_action(username: &str) -> Result<(), String> {
    let content = read_stdin_content(1024 * 1024, "key content")?;
    let ssh = ssh_dir_of(username, true)?;
    install_keys(&ssh, &content)
}

/// The `write_profile_sh <path>` action: writes the shell-environment
/// feature's one allowed root-owned file kind — a `.sh` file directly inside
/// `/etc/profile.d`.
///
/// Both halves of the policy are enforced *here*, independently of anything
/// the app validated: the path is re-checked by `validate_profile_sh_path`
/// and the stdin content by `validate_profile_sh_content`, so a compromised
/// app can at most write valid `export NAME="value"` line sets. Replaces an
/// existing `.sh` atomically and preserves its owner and mode; creates new
/// files as `0644 root:root`. Symlinks are never followed — an existing
/// symlinked destination is refused, not written through.
fn write_profile_sh_action(path: &str) -> Result<(), String> {
    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    validate_profile_sh_path(path)?;
    let content = read_stdin_content(256 * 1024, "profile content")?;
    validate_profile_sh_content(&content)?;
    // The backup path (None for a brand-new file) is the action's only stdout
    // output, so the app can show the user exactly which revision was kept.
    if let Some(backup) = install_root_owned_file(path, &content, "profile file")? {
        println!("{backup}");
    }
    Ok(())
}

/// The `restore_profile_sh <path> <backup>` action: replaces the profile file
/// at `path` with the content of one exact revision sibling the write core
/// itself created.
///
/// The backup's bytes never travel through the app: the helper opens it
/// through the confined directory descriptor (`O_NOFOLLOW` — a symlinked
/// backup is refused, not followed), refuses anything that is not a
/// privileged-owned regular file, and re-validates the historical content
/// with the same `validate_profile_sh_content` policy the write path uses.
/// A backup that fails that policy is refused with a message telling the
/// user to restore it manually — the destination is left untouched.
/// Publishing goes through `install_root_owned_file`, which keeps the
/// current content as a fresh revision backup before the rename; a restore
/// is therefore itself reversible and needs no separate pre-restore backup.
fn restore_profile_sh_action(path: &str, backup_path: &str) -> Result<(), String> {
    use std::io::Read;

    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    validate_profile_sh_path(path)?;
    restore_profile_sh_backup_shape(path, backup_path)?;

    let backup_name = std::path::Path::new(backup_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir = open_dir_nofollow(CWD, PROFILE_D_DIR, PROFILE_D_DIR)?;
    // inspect the backup without ever following a symlink, then open it the
    // same descriptor-pinned way
    let stat = rustix::fs::statat(&dir, backup_name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(
        |_| {
            format!(
                "lcp-admin: the backup '{backup_path}' could not be inspected; refusing to restore."
            )
        },
    )?;
    if (stat.st_mode & S_IFMT) != S_IFREG {
        return Err(format!(
            "lcp-admin: the backup '{backup_path}' is not a regular file; refusing to restore."
        ));
    }
    if stat.st_uid != privileged_uid() {
        return Err(format!(
            "lcp-admin: the backup '{backup_path}' is not owned by the required account; refusing to restore."
        ));
    }
    let fd = rustix::fs::openat(
        &dir,
        backup_name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| {
        format!("lcp-admin: the backup '{backup_path}' could not be opened; refusing to restore.")
    })?;
    let file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    file.take((PROFILE_CONTENT_MAX + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            format!("lcp-admin: the backup '{backup_path}' could not be read; refusing to restore.")
        })?;
    if bytes.len() > PROFILE_CONTENT_MAX {
        return Err("lcp-admin: the backup is too large to be a profile file.".to_string());
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => {
            return Err(format!(
                "lcp-admin: this backup predates the current content policy (it is not plain UTF-8 import lines); restore it manually."
            ))
        }
    };
    validate_profile_sh_content(&content).map_err(|reason| {
        format!(
            "lcp-admin: this backup predates the current content policy ({reason}); restore it manually."
        )
    })?;

    if let Some(backup) = install_root_owned_file(path, &content, "profile file")? {
        println!("{backup}");
    }
    Ok(())
}

/// `YYYYmmdd_HHMMSS` in UTC from the system clock — the nginx backup revision
/// shape. `nginx_list_backups` parses `<name>.<stamp>.bak` by splitting on
/// the last dot, so — unlike the profile write core's `pid.nanos` suffix —
/// this stamp must not contain any dot of its own.
fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|age| age.as_secs())
        .unwrap_or(0);
    let (hh, mm, ss) = ((secs % 86_400) / 3_600, (secs % 3_600) / 60, secs % 60);
    // Howard Hinnant's civil_from_days: the days since 1970-01-01 to a
    // y/m/d date, without any time-table dependency.
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}{month:02}{day:02}_{hh:02}{mm:02}{ss:02}")
}

/// The `nginx_config_install <path>` action: writes one file in the nginx
/// configuration tree from stdin.
///
/// The shared halves: `nginx_config_path_valid` confines the destination
/// (exact `nginx.conf`, `conf.d/*.conf`, plain children of the two sites
/// directories), a structural content gate (size, NUL, control characters
/// other than newlines/tabs) is the *only* content policy — nginx's own `-t`
/// is the grammar gate. Publishing goes through `install_root_owned_file`,
/// which forces root:root 0644 and keeps the previous content as a fresh
/// revision backup before the atomic rename; that backup's path is the
/// action's only stdout output.
fn nginx_config_install_action(path: &str) -> Result<(), String> {
    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    nginx_config_path_valid(path)?;
    let content = read_stdin_content(NGINX_CONFIG_CONTENT_MAX as u64, "nginx configuration")?;
    validate_nginx_config_content(&content)?;
    if let Some(backup) = install_root_owned_file(path, &content, "nginx configuration")? {
        println!("{backup}");
    }
    Ok(())
}

/// The `nginx_config_backup <path>` action: copies one existing file in the
/// nginx configuration tree into the panel's own backups directory as a
/// `<basename>.<UTC stamp>.bak` sibling. The config side and the backups
/// side are both opened through their pinned directory descriptors
/// (`O_NOFOLLOW` — a symlinked "destination" is refused, not followed).
///
/// The backup path is the action's only stdout output and the one exact path
/// a subsequent `nginx_restore_backup` must name.
fn nginx_config_backup_action(path: &str) -> Result<(), String> {
    nginx_config_path_valid(path)?;
    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // pin the source directory: privileged-owned, never group/world
    // writable, never a symlink
    let src_dir = open_dir_nofollow(CWD, &parent, &parent)?;
    let src_stat = stat_fd(&src_dir, &parent)?;
    if src_stat.st_uid != privileged_uid()
        || src_stat.st_gid != privileged_gid()
        || src_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{parent}' is not a safe privileged-owned directory; refusing."
        ));
    }
    let stat = rustix::fs::statat(&src_dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|_| {
        format!("lcp-admin: '{path}' could not be inspected; refusing to back it up.")
    })?;
    if (stat.st_mode & S_IFMT) != S_IFREG {
        return Err(format!(
            "lcp-admin: '{path}' is not a regular file; refusing to back it up."
        ));
    }
    let src_fd = rustix::fs::openat(
        &src_dir,
        name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| format!("lcp-admin: '{path}' could not be opened; refusing to back it up."))?;

    // pin the backups directory the same way, then create the revision as a
    // brand-new file (O_EXCL — an existing name is never overwritten)
    let bak_dir = open_dir_nofollow(CWD, NGINX_BACKUPS_DIR, NGINX_BACKUPS_DIR)?;
    let bak_stat = stat_fd(&bak_dir, NGINX_BACKUPS_DIR)?;
    if bak_stat.st_uid != privileged_uid()
        || bak_stat.st_gid != privileged_gid()
        || bak_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{NGINX_BACKUPS_DIR}' is not a safe privileged-owned directory; refusing."
        ));
    }
    let backup_name = format!("{name}.{}.bak", utc_stamp());
    let result: Result<String, String> = (|| {
        let bak_fd = rustix::fs::openat(
            &bak_dir,
            backup_name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o644),
        )
        .map_err(|_| format!("lcp-admin: could not create the nginx backup; refusing."))?;
        let mut source = std::fs::File::from(src_fd);
        let mut backup = std::fs::File::from(bak_fd);
        std::io::copy(&mut source, &mut backup)
            .and_then(|_| backup.sync_all())
            .and_then(|_| source.sync_all())
            .map_err(|_| format!("lcp-admin: could not write the nginx backup; refusing."))?;
        Ok(format!("{NGINX_BACKUPS_DIR}/{backup_name}"))
    })();

    match result {
        Ok(backup_path) => println!("{backup_path}"),
        Err(message) => {
            let _ = rustix::fs::unlinkat(&bak_dir, backup_name.as_str(), AtFlags::empty());
            return Err(message);
        }
    }
    Ok(())
}

/// The `nginx_restore_backup <dest> <backup>` action: replaces the nginx
/// configuration file at `dest` with the content of one exact revision
/// sibling in the panel's own backups directory — the path the backup action
/// itself reported. An auto-picked or invented path can not even reach the
/// read stage: the shape rule is checked first, inside the confined backups
/// directory descriptor.
///
/// The backup's bytes never travel through the app: they are read through
/// the confined descriptor (`O_NOFOLLOW`; a symlinked backup is refused, not
/// followed), re-checked against the structural content policy, and
/// published through `install_root_owned_file` — which keeps the current
/// content as a fresh revision backup (so a restore is itself reversible and
/// needs no separate pre-restore step).
fn nginx_restore_backup_action(path: &str, backup_path: &str) -> Result<(), String> {
    use std::io::Read;

    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    nginx_config_path_valid(path)?;
    revision_sibling_shape(path, backup_path, NGINX_BACKUPS_DIR)?;

    let backup_name = std::path::Path::new(backup_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir = open_dir_nofollow(CWD, NGINX_BACKUPS_DIR, NGINX_BACKUPS_DIR)?;
    let stat = rustix::fs::statat(&dir, backup_name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(
        |_| {
            format!(
                "lcp-admin: the backup '{backup_path}' could not be inspected; refusing to restore."
            )
        },
    )?;
    if (stat.st_mode & S_IFMT) != S_IFREG {
        return Err(format!(
            "lcp-admin: the backup '{backup_path}' is not a regular file; refusing to restore."
        ));
    }
    if stat.st_uid != privileged_uid() {
        return Err(format!(
            "lcp-admin: the backup '{backup_path}' is not owned by the required account; refusing to restore."
        ));
    }
    let fd = rustix::fs::openat(
        &dir,
        backup_name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| {
        format!("lcp-admin: the backup '{backup_path}' could not be opened; refusing to restore.")
    })?;
    let file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    file.take((NGINX_CONFIG_CONTENT_MAX + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            format!(
                "lcp-admin: the backup '{backup_path}' could not be read; refusing to restore."
            )
        })?;
    if bytes.len() > NGINX_CONFIG_CONTENT_MAX {
        return Err("lcp-admin: the backup is too large to be an nginx configuration.".to_string());
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => {
            return Err(format!(
                "lcp-admin: this backup predates the current content policy (it is not plain text); restore it manually."
            ))
        }
    };
    validate_nginx_config_content(&content).map_err(|reason| {
        format!(
            "lcp-admin: this backup predates the current content policy ({reason}); restore it manually."
        )
    })?;

    if let Some(backup) = install_root_owned_file(path, &content, "nginx configuration")? {
        println!("{backup}");
    }
    Ok(())
}

/// The `nginx_link_site <available> <enabled>` action: creates the one
/// enabling symlink `<sites-enabled>/<same name>` that points at
/// `<sites-available>/<same name>` — never a rename-in-disguise (the
/// same-basename rule is structural, checked before anything is pinned).
///
/// Both directories are opened through their pinned descriptors; the source
/// must be one regular, non-symlink file, and an already-existing enabled
/// entry (a symlink included) is refused instead of replaced.
fn nginx_link_site_action(available_path: &str, enabled_path: &str) -> Result<(), String> {
    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    nginx_link_as_valid(available_path, enabled_path)?;
    let available_name = std::path::Path::new(available_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let enabled_name = std::path::Path::new(enabled_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    // pin the source directory: privileged-owned, never group/world
    // writable, never a symlink
    let src_dir = open_dir_nofollow(CWD, NGINX_SITES_AVAILABLE, NGINX_SITES_AVAILABLE)?;
    let src_stat = stat_fd(&src_dir, NGINX_SITES_AVAILABLE)?;
    if src_stat.st_uid != privileged_uid()
        || src_stat.st_gid != privileged_gid()
        || src_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{NGINX_SITES_AVAILABLE}' is not a safe privileged-owned directory; refusing."
        ));
    }
    let src_entry = rustix::fs::statat(
        &src_dir,
        available_name.as_str(),
        AtFlags::SYMLINK_NOFOLLOW,
    )
    .map_err(|_| {
        format!(
            "lcp-admin: the site '{available_path}' could not be inspected; refusing to link it."
        )
    })?;
    if (src_entry.st_mode & S_IFMT) != S_IFREG {
        return Err(format!(
            "lcp-admin: the site '{available_path}' is not a regular file; refusing to link it."
        ));
    }

    // pin the symlink directory the same way
    let dst_dir = open_dir_nofollow(CWD, NGINX_SITES_ENABLED, NGINX_SITES_ENABLED)?;
    let dst_stat = stat_fd(&dst_dir, NGINX_SITES_ENABLED)?;
    if dst_stat.st_uid != privileged_uid()
        || dst_stat.st_gid != privileged_gid()
        || dst_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{NGINX_SITES_ENABLED}' is not a safe privileged-owned directory; refusing."
        ));
    }
    if rustix::fs::statat(&dst_dir, enabled_name.as_str(), AtFlags::SYMLINK_NOFOLLOW).is_ok() {
        return Err(format!(
            "lcp-admin: '{enabled_path}' already exists; refusing to re-link over it."
        ));
    }
    rustix::fs::symlinkat(available_path, &dst_dir, enabled_name.as_str()).map_err(|_| {
        format!("lcp-admin: could not create the site link '{enabled_path}'; refusing.")
    })?;
    Ok(())
}

/// The `nginx_unlink_site <enabled>` action: removes exactly the one
/// enabling symlink that disables a site — the inverse of
/// `nginx_link_site`, and the successor of the old `rm -f` disable path.
///
/// The constraint is the *opposite* of `nginx_config_remove`: the target
/// must *be* a symlink (a regular file, a directory, nothing at all is
/// refused), it must live directly inside a pinned `sites-enabled`
/// directory, and it must be one of this panel's own links — pointing
/// exactly at `<sites-enabled>/<same name>`'s site in `sites-available`.
/// A link to anywhere else is refused, never followed, and a regular file
/// sitting there is never unlinked as if it were a site toggle.
fn nginx_unlink_site_action(enabled_path: &str) -> Result<(), String> {
    // both halves of the policy hold independently — the helper must not
    // depend on the app having validated anything
    nginx_site_unlink_path_valid(enabled_path)?;
    let name = std::path::Path::new(enabled_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // pin the symlink directory: privileged-owned, never group/world
    // writable, never a symlink
    let dir = open_dir_nofollow(CWD, NGINX_SITES_ENABLED, NGINX_SITES_ENABLED)?;
    let dir_stat = stat_fd(&dir, NGINX_SITES_ENABLED)?;
    if dir_stat.st_uid != privileged_uid()
        || dir_stat.st_gid != privileged_gid()
        || dir_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{NGINX_SITES_ENABLED}' is not a safe privileged-owned directory; refusing."
        ));
    }
    let stat = rustix::fs::statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|_| {
        format!("lcp-admin: '{enabled_path}' does not exist; refusing to unlink.")
    })?;
    if (stat.st_mode & S_IFMT) != S_IFLNK {
        return Err(format!(
            "lcp-admin: '{enabled_path}' is not a symlink; a regular file or directory cannot be disabled by unlinking."
        ));
    }
    let target = rustix::fs::readlinkat(&dir, name.as_str(), Vec::new()).map_err(|_| {
        format!("lcp-admin: the link '{enabled_path}' could not be read; refusing to unlink.")
    })?;
    let expected = format!("{NGINX_SITES_AVAILABLE}/{name}");
    if target.to_string_lossy() != expected {
        return Err(format!(
            "lcp-admin: '{enabled_path}' does not point at its own site ({expected}); refusing to unlink a foreign link."
        ));
    }
    rustix::fs::unlinkat(&dir, name.as_str(), AtFlags::empty())
        .map_err(|_| format!("lcp-admin: could not unlink '{enabled_path}'; refusing."))?;
    Ok(())
}

/// The `nginx_config_remove <path>` action: removes exactly one regular
/// file in the nginx configuration tree. There are no `-r` semantics (a
/// directory target is refused) and no `rm -f` semantics (a missing file is
/// a refusal, not a success); neither `nginx.conf` nor any symlinked entry
/// can be removed — the exact main configuration file is the protected
/// core, and a symlink is opened at, never followed.
fn nginx_config_remove_action(path: &str) -> Result<(), String> {
    nginx_config_remove_path_valid(path)?;
    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // pin the parent directory: privileged-owned, never group/world
    // writable, never a symlink
    let dir = open_dir_nofollow(CWD, &parent, &parent)?;
    let dir_stat = stat_fd(&dir, &parent)?;
    if dir_stat.st_uid != privileged_uid()
        || dir_stat.st_gid != privileged_gid()
        || dir_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{parent}' is not a safe privileged-owned directory; refusing."
        ));
    }
    let stat = rustix::fs::statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(
        |_| {
            format!(
            "lcp-admin: '{path}' does not exist; refusing a missing-path removal."
        )
        },
    )?;
    if (stat.st_mode & S_IFMT) != S_IFREG {
        return Err(format!(
            "lcp-admin: '{path}' is not a regular file; refusing a non-file removal (no -r semantics)."
        ));
    }
    rustix::fs::unlinkat(&dir, name.as_str(), AtFlags::empty())
        .map_err(|_| format!("lcp-admin: could not remove '{path}'; refusing."))?;
    Ok(())
}

// ─── web-root internals ──────────────────────────────────────────────────────

/// The deepest a web-root operation's path may reach — a hard cap so no
/// crafted path (or crafted filesystem, or a race between many levels) can
/// make the panel loop forever or exhaust the descriptor budget.
const WWW_TREE_DEPTH_MAX: usize = 16;

/// Pin the web root itself: opened with per-component no-follow and verified
/// privileged-owned with no group/world-writable bits — the same contract
/// every other action asserts on its directory.
fn pin_www_root() -> Result<OwnedFd, String> {
    let root = open_dir_nofollow(CWD, VAR_WWW_DIR, VAR_WWW_DIR)?;
    let stat = stat_fd(&root, VAR_WWW_DIR)?;
    if stat.st_uid != privileged_uid()
        || stat.st_gid != privileged_gid()
        || stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{VAR_WWW_DIR}' is not a safe privileged-owned directory; refusing."
        ));
    }
    Ok(root)
}

/// Walk the path *below* the pinned web root down to the parent of its final
/// component. Every intermediate component is stat'ed without following
/// (`O_NOFOLLOW`-equivalent at the descriptor level); a symlink anywhere
/// along the walk is a refusal, never a traversal. Missing intermediates are
/// refused (`require_all` is only false for the mkdir verb, which walks for
/// itself). Returns the pinned parent descriptor and the final name.
fn www_walk_parent(root: &OwnedFd, rest: &str, full: &str) -> Result<(OwnedFd, String), String> {
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() || parts.len() > WWW_TREE_DEPTH_MAX {
        return Err(format!("lcp-admin: '{full}' exceeds the {WWW_TREE_DEPTH_MAX}-level depth cap; refusing."));
    }
    let name = parts.last().unwrap().to_string();
    let mut chain: Vec<OwnedFd> = Vec::new();
    let mut walked = String::new();
    for (index, part) in parts.iter().enumerate() {
        if index + 1 == parts.len() {
            break; // the final name is *not* entered — only its parent is
        }
        walked.push('/');
        walked.push_str(part);
        let entry =
            rustix::fs::statat(
            chain.last().unwrap_or(root).as_fd(),
            *part,
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|_| {
            format!(
                "lcp-admin: '{full}' could not be inspected at {walked:?}; refusing."
            )
        })?;
        match entry.st_mode & S_IFMT {
            S_IFLNK => {
                return Err(format!(
                    "lcp-admin: '{full}' contains a symlinked component {part:?}; refusing to walk through a link."
                ))
            }
            S_IFDIR => {}
            _ => {
                return Err(format!(
                    "lcp-admin: the intermediate component {walked:?} is not a directory; refusing."
                ))
            }
        }
        let fd = open_dir_nofollow(
            chain.last().unwrap_or(root).as_fd(),
            part,
            walked.as_str(),
        )?;
        chain.push(fd);
    }
    let parent: OwnedFd = match chain.pop() {
        Some(fd) => fd,
        None => root
            .try_clone()
            .map_err(|_| format!("lcp-admin: the web root '{VAR_WWW_DIR}' could not be pinned."))?,
    };
    Ok((parent, name))
}

/// The `mkdir` verb's own walk: descend from the pinned web root, verifying
/// every existing component the no-follow way, and create the first missing
/// component (and everything below it) at 0755 through the pinned chain.
fn www_mkdir_from(root: &OwnedFd, rest: &str, full: &str) -> Result<(), String> {
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() || parts.len() > WWW_TREE_DEPTH_MAX {
        return Err(format!("lcp-admin: '{full}' exceeds the {WWW_TREE_DEPTH_MAX}-level depth cap; refusing."));
    }
    let mut chain: Vec<OwnedFd> = Vec::new();
    let mut walked = String::new();
    for part in parts {
        walked.push('/');
        walked.push_str(part);
        match rustix::fs::statat(
            chain.last().unwrap_or(root).as_fd(),
            part,
            AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Ok(entry) => match entry.st_mode & S_IFMT {
                S_IFLNK => {
                    return Err(format!(
                        "lcp-admin: '{full}' contains a symlinked component {part:?}; refusing to walk through a link."
                    ))
                }
                S_IFDIR => {}
                _ => {
                    return Err(format!(
                        "lcp-admin: the path component {walked:?} exists and is not a directory; refusing."
                    ))
                }
            },
            Err(err) if err == rustix::io::Errno::NOENT => {
                rustix::fs::mkdirat(
                    chain.last().unwrap_or(root).as_fd(),
                    part,
                    Mode::from_raw_mode(0o755),
                )
                .map_err(|_| {
                    format!("lcp-admin: could not create the directory {walked:?}; refusing.")
                })?;
            }
            Err(_) => {
                return Err(format!(
                    "lcp-admin: '{full}' could not be inspected at {walked:?}; refusing."
                ))
            }
        }
        let fd = open_dir_nofollow(
            chain.last().unwrap_or(root).as_fd(),
            part,
            walked.as_str(),
        )?;
        chain.push(fd);
    }
    Ok(())
}

/// Depth-first removal of exactly the target below an already-pinned parent:
/// a regular file is unlinked, a directory is emptied (through its own
/// pinned descriptor) and then removed. A symlink at any reachable spot —
/// the target itself or a child of the subtree — is a refusal, never a
/// follow, and nothing at all is modified when the action refuses.
fn www_remove_tree(
    parent: &OwnedFd,
    name: &std::ffi::CStr,
    depth: usize,
    full: &str,
) -> Result<(), String> {
    if depth > WWW_TREE_DEPTH_MAX {
        return Err(format!(
            "lcp-admin: the subtree of '{full}' is deeper than {WWW_TREE_DEPTH_MAX} levels; refusing."
        ));
    }
    let stat = rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| {
        format!("lcp-admin: '{full}' could not be inspected; refusing to remove it.")
    })?;
    if (stat.st_mode & S_IFMT) == S_IFLNK {
        return Err(format!(
            "lcp-admin: '{full}' is a symlink; refusing to remove a link in the web root."
        ));
    }
    if (stat.st_mode & S_IFMT) != S_IFDIR {
        rustix::fs::unlinkat(parent, name, AtFlags::empty())
            .map_err(|_| format!("lcp-admin: could not remove '{full}'; refusing."))?;
        return Ok(());
    }
    let dir_fd = open_dir_nofollow(parent, name.to_string_lossy().as_ref(), full)?;
    let mut dir = rustix::fs::Dir::read_from(&dir_fd)
        .map_err(|_| format!("lcp-admin: the subtree of '{full}' could not be read; refusing."))?;
    let mut children: Vec<std::ffi::CString> = Vec::new();
    while let Some(entry) = dir.read() {
        let entry = entry.map_err(|_| {
            format!("lcp-admin: the subtree of '{full}' could not be read; refusing.")
        })?;
        let child = entry.file_name().to_owned();
        if child.as_bytes() == b"." || child.as_bytes() == b".." {
            continue;
        }
        children.push(child);
    }
    drop(dir);
    for child in children {
        let child_full = format!("{full}/{}", child.to_string_lossy());
        www_remove_tree(&dir_fd, child.as_c_str(), depth + 1, &child_full)?;
    }
    rustix::fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(|_| {
        format!("lcp-admin: could not remove the directory '{full}'; refusing.")
    })?;
    Ok(())
}

/// Publish one web file through the pinned parent: the destination shape is
/// verified without following (an existing symlink or non-regular file
/// refuses), then a unique temp file is written (forced 0644, owner forced
/// to the privileged identity — a rewritten web file must not keep
/// group/world bits or a stale owner), synced, and atomically renamed over
/// the destination. There is deliberately no backup-on-write here — a
/// `<name>.*.bak` sibling *inside the web root* would become a publicly
/// served copy of the previous content.
fn www_install_binary_file(
    dir: &OwnedFd,
    name: &str,
    content: &[u8],
    full: &str,
) -> Result<(), String> {
    use std::io::Write as _;

    // verify the destination's agreed shape before anything is touched
    match rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) if (stat.st_mode & S_IFMT) == S_IFLNK => {
            return Err(format!(
                "lcp-admin: '{full}' is a symlink; refusing to replace a link in the web root."
            ));
        }
        Ok(stat) if (stat.st_mode & S_IFMT) != S_IFREG => {
            return Err(format!(
                "lcp-admin: '{full}' exists and is not a regular file; refusing to replace it."
            ));
        }
        Ok(_) => {}
        Err(err) if err == rustix::io::Errno::NOENT => {}
        Err(_) => return Err(format!("lcp-admin: '{full}' could not be inspected; refusing.")),
    }

    let temp_name = std::ffi::CString::new(format!(".{name}.tmp.{}", unique_suffix()))
        .map_err(|_| format!("lcp-admin: '{full}' is not a usable file name; refusing."))?;
    let fd = rustix::fs::openat(
        dir,
        temp_name.as_c_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o644),
    )
    .map_err(|_| format!("lcp-admin: could not create the temporary web file; refusing."))?;
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::from(fd);
        // the destination is always 0644 with the privileged owner — a new
        // file must not keep group/world bits, and in the fixture build the
        // privileged identity is the invoker's own (a self-chown, which
        // succeeds); a failure here is never silently ignored
        rustix::fs::fchmod(&file, Mode::from_raw_mode(0o644))
            .map_err(|_| format!("lcp-admin: could not set the web file mode"))?;
        rustix::fs::fchown(
            &file,
            Some(Uid::from_raw(privileged_uid())),
            Some(Gid::from_raw(privileged_gid())),
        )
        .map_err(|_| format!("lcp-admin: could not set the web file owner"))?;
        file.write_all(content)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("lcp-admin: could not write the web file content: {e}"))?;
        Ok(())
    })();
    match result {
        Ok(()) => {}
        Err(message) => {
            let _ = rustix::fs::unlinkat(dir, temp_name.as_c_str(), AtFlags::empty());
            return Err(message);
        }
    }
    rustix::fs::renameat(dir.as_fd(), temp_name.as_c_str(), dir.as_fd(), name).map_err(|_| {
        let _ = rustix::fs::unlinkat(dir, temp_name.as_c_str(), AtFlags::empty());
        format!("lcp-admin: could not publish '{full}'; refusing.")
    })?;
    Ok(())
}

/// The `nginx_www_fs <mkdir|remove|rename> …` action: the three web-root
/// operations, each one re-validated independently (the helper must not
/// trust the app), each one walking descriptors at its own no-follow pace.
fn nginx_www_fs_action(rest: &[String]) -> Result<(), String> {
    let mut args = rest.to_vec();
    let op = args.remove(0);
    match op.as_str() {
        "mkdir" => {
            let [path] = args.as_slice() else {
                return Err("lcp-admin: the mkdir sub-command takes exactly one web-root path.".to_string());
            };
            var_www_path_valid(path)?;
            let root = pin_www_root()?;
            let stripped = path.strip_prefix(var_www_root_prefix()).unwrap_or_default();
            www_mkdir_from(&root, stripped, path)?;
            Ok(())
        }
        "remove" => {
            let [path] = args.as_slice() else {
                return Err("lcp-admin: the remove sub-command takes exactly one web-root path.".to_string());
            };
            var_www_path_valid(path)?;
            if var_www_is_protected(path) {
                return Err(format!(
                    "lcp-admin: '{path}' is the web root or its distro-managed html tree; refusing to remove it."
                ));
            }
            let root = pin_www_root()?;
            let stripped = path.strip_prefix(var_www_root_prefix()).unwrap_or_default();
            let (parent, name) = www_walk_parent(&root, stripped, path)?;
            let name_c = std::ffi::CString::new(name.as_str())
                .map_err(|_| format!("lcp-admin: '{path}' is not a usable file name; refusing."))?;
            www_remove_tree(&parent, name_c.as_c_str(), 0, path)?;
            Ok(())
        }
        "rename" => {
            let [from, to] = args.as_slice() else {
                return Err("lcp-admin: the rename sub-command takes exactly one source path and one destination path.".to_string());
            };
            var_www_path_valid(from)?;
            var_www_path_valid(to)?;
            if var_www_is_protected(from) || var_www_is_protected(to) {
                return Err(format!(
                    "lcp-admin: '{from}' → '{to}' touches the web root or its distro-managed html tree; refusing to rename."
                ));
            }
            let root = pin_www_root()?;
            let prefix = var_www_root_prefix();
            let (from_parent, from_name) = www_walk_parent(&root, from.strip_prefix(prefix).unwrap_or_default(), from)?;
            let source = rustix::fs::statat(&from_parent, from_name.as_bytes(), AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| format!("lcp-admin: '{from}' does not exist; refusing to rename."))?;
            if (source.st_mode & S_IFMT) == S_IFLNK {
                return Err(format!(
                    "lcp-admin: '{from}' is a symlink; refusing to rename a link in the web root."
                ));
            }
            let (to_parent, to_name) = www_walk_parent(&root, to.strip_prefix(prefix).unwrap_or_default(), to)?;
            if rustix::fs::statat(&to_parent, to_name.as_bytes(), AtFlags::SYMLINK_NOFOLLOW).is_ok() {
                return Err(format!(
                    "lcp-admin: '{to}' already exists; refusing to rename over it."
                ));
            }
            rustix::fs::renameat(
                from_parent.as_fd(),
                from_name.as_str(),
                to_parent.as_fd(),
                to_name.as_str(),
            )
            .map_err(|_| format!("lcp-admin: could not rename '{from}' to '{to}'; refusing."))?;
            Ok(())
        }
        _ => Err(format!(
            "lcp-admin: '{}' is not a sub-command of nginx_www_fs (mkdir, remove or rename).",
            op
        )),
    }
}

/// The prefix the web verbs strip: the fixture-twin-aware web root plus its
/// trailing slash, exactly what `var_www_path_valid` guarantees about the
/// accepted shape.
fn var_www_root_prefix() -> &'static str {
    #[cfg(not(feature = "test-fixture"))]
    {
        "/var/www/"
    }
    #[cfg(feature = "test-fixture")]
    {
        "/tmp/lcp-admin-test-fixture/var/www/"
    }
}

/// The `nginx_www_install <dest>` action: writes one file in the web root
/// from stdin. The category of content is binary-tolerant — no UTF-8, no
/// NUL, no structural rule, only the size cap — so a dragged-in image or
/// archive travels exactly as it is. The publishing core forces root:root
/// 0644 through the pinned parent's temp-and-rename sequence; a symlinked
/// or non-regular destination refuses, and no source path is ever accepted
/// as an argument (the content travels stdin, nothing else does).
fn nginx_www_install_action(dest: &str) -> Result<(), String> {
    var_www_path_valid(dest)?;
    let content = read_stdin_bytes(NGINX_WWW_INSTALL_MAX as u64, "web file")?;
    let root = pin_www_root()?;
    let stripped = dest.strip_prefix(var_www_root_prefix()).unwrap_or_default();
    let (parent, name) = www_walk_parent(&root, stripped, dest)?;
    www_install_binary_file(&parent, &name, &content, dest)?;
    Ok(())
}

/// The `nmcli_connection_save <add|modify> <connection> <setting.prop> <value>...`
/// action: creates or modifies an nmcli connection profile. All operands are
/// validated by `check_nmcli_connection_save` before this point runs.
fn nmcli_connection_save_action(args: &[String]) -> Result<(), String> {
    // args[0] is the action name; args[1..] = [verb, connection, prop, value, ...]
    let verb = args.get(1).ok_or_else(|| {
        "lcp-admin: nmcli_connection_save requires a verb (add/modify).".to_string()
    })?;
    let connection = args.get(2).ok_or_else(|| {
        "lcp-admin: nmcli_connection_save requires a connection name.".to_string()
    })?;
    let mut cmd = std::process::Command::new("nmcli");
    cmd.arg("connection").arg(verb.as_str()).arg(connection.as_str());
    for pair in args[3..].chunks(2) {
        cmd.arg(pair[0].as_str()).arg(pair[1].as_str());
    }
    let output = cmd.output().map_err(|e| {
        format!("lcp-admin: failed to spawn nmcli: {e}")
    })?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: nmcli connection {verb} {connection} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `nmcli_connection_delete <uuid>` action: deletes exactly one
/// connection profile by UUID.
fn nmcli_connection_delete_action(uuid: &str) -> Result<(), String> {
    let output = std::process::Command::new("nmcli")
        .args(["connection", "delete", uuid])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn nmcli: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: nmcli connection delete {uuid} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `nmcli_connection_up <uuid>` action: activates exactly one
/// connection profile by UUID.
fn nmcli_connection_up_action(uuid: &str) -> Result<(), String> {
    let output = std::process::Command::new("nmcli")
        .args(["connection", "up", uuid])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn nmcli: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: nmcli connection up {uuid} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `nmcli_connection_down <uuid>` action: deactivates exactly one
/// connection profile by UUID.
fn nmcli_connection_down_action(uuid: &str) -> Result<(), String> {
    let output = std::process::Command::new("nmcli")
        .args(["connection", "down", uuid])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn nmcli: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: nmcli connection down {uuid} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `nmcli_connection_import <openvpn|wireguard> <file_path>` action:
/// imports a VPN profile from a user-readable file. The helper reads the
/// file itself (never trusting app-passed content), so the path confinement
/// in `check_nmcli_import` is the only gate on which file may be read.
fn nmcli_connection_import_action(import_type: &str, file_path: &str) -> Result<(), String> {
    // Both halves of the policy hold independently — re-validate here even
    // though the app already validated.
    use root_policy::{
        check_nmcli_import, ensure_safe_operand, nmcli_import_path_valid,
    };
    ensure_safe_operand(file_path, "import file path")?;
    nmcli_import_path_valid(file_path)?;
    check_nmcli_import(&[
        import_type.to_string(),
        file_path.to_string(),
    ])?;

    let output = std::process::Command::new("nmcli")
        .args([
            "connection",
            "import",
            "type",
            import_type,
            "file",
            file_path,
        ])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn nmcli: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: nmcli connection import type {import_type} file {file_path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Escape a string for safe use in a sed basic regular expression.
/// Escapes sed metacharacters (`\ . [ ] * ^ $ / &`) so a repo id validated
/// by `ensure_safe_operand` can be interpolated into a sed script without
/// changing its meaning.
fn sed_basic_re_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' | '.' | '[' | ']' | '*' | '^' | '$' | '/' | '&' => {
                format!("\\{c}").into_bytes()
            }
            _ => vec![c as u8],
        })
        .map(|b| b as char)
        .collect()
}

/// The `repo_toggle_enable <file_path> <repo_id>` action: enables one repo
/// section in a repository file. Handles .sources, .list, and .repo file types.
fn repo_toggle_enable_action(file_path: &str, repo_id: &str) -> Result<(), String> {
    // Both halves of the policy hold independently — re-validate here even
    // though the app already validated.
    use root_policy::{ensure_repo_file_path, ensure_safe_operand};
    ensure_repo_file_path(file_path)?;
    ensure_safe_operand(repo_id, "repo id")?;

    if file_path.ends_with(".sources") {
        let output = std::process::Command::new("sed")
            .args(["-i", "s/^Enabled:\\s*.*/Enabled: yes/I", file_path])
            .output()
            .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "lcp-admin: sed enable .sources repo in {file_path} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }

    if file_path.ends_with(".list") || file_path.ends_with("sources.list") {
        let output = std::process::Command::new("sed")
            .args(["-i", "s/^#\\s*\\(deb\\s.*\\)/\\1/", file_path])
            .output()
            .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "lcp-admin: sed enable .list repo in {file_path} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }

    // .repo files: escape repo_id for sed basic regex
    let escaped = sed_basic_re_escape(repo_id);
    let script = format!(
        "/^\\[{}\\]/,/^\\[/ s/^enabled\\s*=.*/enabled=1;/",
        escaped
    );
    let output = std::process::Command::new("sed")
        .args(["-i", &script, file_path])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: sed enable repo {repo_id} in {file_path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `repo_toggle_disable <file_path> <repo_id>` action: disables one repo
/// section in a repository file. Handles .sources, .list, and .repo file types.
fn repo_toggle_disable_action(file_path: &str, repo_id: &str) -> Result<(), String> {
    // Both halves of the policy hold independently — re-validate here even
    // though the app already validated.
    use root_policy::{ensure_repo_file_path, ensure_safe_operand};
    ensure_repo_file_path(file_path)?;
    ensure_safe_operand(repo_id, "repo id")?;

    if file_path.ends_with(".sources") {
        let output = std::process::Command::new("sed")
            .args(["-i", "s/^Enabled:\\s*.*/Enabled: no/I", file_path])
            .output()
            .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "lcp-admin: sed disable .sources repo in {file_path} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }

    if file_path.ends_with(".list") || file_path.ends_with("sources.list") {
        let output = std::process::Command::new("sed")
            .args(["-i", "s/^\\(deb\\s.*\\)/# \\1/", file_path])
            .output()
            .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "lcp-admin: sed disable .list repo in {file_path} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }

    // .repo files: escape repo_id for sed basic regex
    let escaped = sed_basic_re_escape(repo_id);
    let script = format!(
        "/^\\[{}\\]/,/^\\[/ s/^enabled\\s*=.*/enabled=0;/",
        escaped
    );
    let output = std::process::Command::new("sed")
        .args(["-i", &script, file_path])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn sed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: sed disable repo {repo_id} in {file_path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Why a package-manager program failed. The kind decides whether any
/// fallback may run at all:
///
/// * `NotAvailable` — the program could not be started; nothing was executed
///   and no configuration was touched. Falling back is safe and correct.
/// * `Unsupported` — the program ran but refused the command/option surface
///   *at parsing time* (e.g. dnf5's "Unknown argument --add-repo"), so it
///   demonstrably could not have acted. Falling back is justified because
///   the interface, not the operation, failed.
/// * `Failed` — the program started and failed mid-operation. The
///   repository configuration may already have partially changed; a network
///   error or invalid configuration is equally a problem for the next
///   package manager. **No fallback may run for this kind** — stop and
///   surface the reason.
#[derive(Debug, PartialEq)]
enum RepoProgramFailure {
    NotAvailable(String),
    Unsupported(String),
    Failed(String),
}

impl RepoProgramFailure {
    /// The human-readable reason only.
    fn reason(&self) -> &str {
        match self {
            RepoProgramFailure::NotAvailable(reason)
            | RepoProgramFailure::Unsupported(reason)
            | RepoProgramFailure::Failed(reason) => reason,
        }
    }
}

/// Exit-text markers that mean the program rejected the *command shape*
/// before doing anything — recorded from real observations (dnf5's
/// "Unknown argument --add-repo", dnf's "Missing value for named argument")
/// plus the usual upstream shapes. Anything not matched is `Failed`.
fn exit_text_is_unsupported(detail: &str) -> bool {
    detail.contains("Unknown argument")
        || detail.contains("Unknown command")
        || detail.contains("unknown command")
        || detail.contains("Unrecognized option")
        || detail.contains("unknown option")
        || detail.contains("Missing value for")
        || detail.contains("No such command")
}

/// Runs one package-manager program for a repo action and normalises its
/// failure into a *kind* (`RepoProgramFailure`) — stderr when present,
/// stdout otherwise, the spawn error when the program could not be started.
/// Returns `Ok(())` only for exit 0. Tests inject a replacement for this
/// function — real package managers are never spawned from unit tests.
fn run_repo_program(
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<(), RepoProgramFailure> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| RepoProgramFailure::NotAvailable(format!(
            "{label} could not be started ({e})"
        )))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    if detail.is_empty() {
        Err(RepoProgramFailure::Failed(format!(
            "{label} failed with exit status {}",
            output.status
        )))
    } else if exit_text_is_unsupported(&detail) {
        Err(RepoProgramFailure::Unsupported(format!(
            "{label} refused the command: {detail}"
        )))
    } else {
        Err(RepoProgramFailure::Failed(format!("{label} failed: {detail}")))
    }
}

/// How dnf's `config-manager addrepo` must be invoked for a URL. dnf5 has
/// two shapes: the URL points at a downloadable repository configuration
/// file, or it is the repository's base URL and the repo is defined from
/// options (`--set=baseurl=…`).
#[derive(Debug, PartialEq)]
enum RepoAddShape {
    RepofileUrl,
    BaseUrl,
}

/// Chooses dnf5's `config-manager addrepo` shape for `url`.
///
/// Chosen behaviour: the classification looks only at the *path* component —
/// everything after `://`, with any query string (`?…`) or fragment (`#…`)
/// stripped — and classifies `.repo`-suffixed paths as `--from-repofile`
/// download URLs, everything else as base URLs. The scheme is deliberately
/// not considered here: https-only is enforced independently by
/// `repo_url_https_only`, and classification of an https URL must not depend
/// on the validator. A URL nobody would call well-formed (e.g. `https://`)
/// still classifies instead of panicking, and dnf delivers the final verdict
/// on whether it is usable.
fn classify_repo_add_url(url: &str) -> RepoAddShape {
    let rest = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let path = &rest[..path_end];
    if path.ends_with(".repo") {
        RepoAddShape::RepofileUrl
    } else {
        RepoAddShape::BaseUrl
    }
}

/// The `repo_add <url>` action: adds a new repository source via dnf or apt.
///
/// The URL has already been https-only validated by the app and by
/// `check_helper_action`; the two validators below re-establish that policy
/// internally — this is the last gate before any program runs.
///
/// The command shape choice lives in `classify_repo_add_url`. `run` is the
/// command runner, injected so tests can record arguments and simulate
/// outcomes without spawning real package managers; production always calls
/// `repo_add_action`, which injects `run_repo_program`.
///
/// Fallback policy (conservative):
///   1. the dnf5 shape chosen for the URL — success short-circuits
///      everything;
///   2. on `Failed` (dnf started, failed mid-operation): STOP. The config
///      may have partially changed; another package manager cannot judge
///      that. The reason is returned alone.
///   3. on `Unsupported` (dnf refused the command at parsing time, e.g. the
///      dnf5 flag surface): only then the dnf4 legacy flag may run — same
///      conservative rules for it.
///   4. apt never runs while dnf has *executed something*. apt runs only
///      when dnf was demonstrably unavailable (spawn failure, i.e. not a
///      dnf system) — or, at the very end, when dnf exists but refused both
///      command shapes (interface incompatible, no operation was possible).
/// A wrong-shape attempt fails at argument parsing with no side effects,
/// and every kept reason is reported in order so the primary package
/// manager's reason is never masked.
fn repo_add_action_with(
    run: impl Fn(&str, &[&str], &str) -> Result<(), RepoProgramFailure>,
    url: &str,
) -> Result<(), String> {
    use root_policy::{ensure_safe_operand, repo_url_https_only};
    ensure_safe_operand(url, "repository URL")?;
    repo_url_https_only(url)?;

    // Duplicate-handling policy, pinned in tests: the helper never passes
    // --overwrite or --add-or-replace, so an existing repository file is
    // only ever modified if the package manager refuses or reports anyway.
    // Reliable cross-manager duplicate detection is not implemented: dnf5
    // derives the repository id itself, dnf4 and apt use their own rules,
    // and probing for it would need more privileged introspection than this
    // design allows. If the package manager reports a duplicate, that
    // message passes through unchanged (fail-safe: refuse, never overwrite).
    let mut reasons: Vec<String> = Vec::new();
    let from_repofile = format!("--from-repofile={url}");
    let set_baseurl = format!("--set=baseurl={url}");
    match classify_repo_add_url(url) {
        RepoAddShape::RepofileUrl => {
            match run(
                "dnf",
                &["config-manager", "addrepo", &from_repofile],
                "dnf config-manager addrepo --from-repofile",
            ) {
                Ok(()) => return Ok(()),
                Err(RepoProgramFailure::Failed(reason)) => {
                    return Err(format!("lcp-admin: {reason}"))
                }
                Err(RepoProgramFailure::NotAvailable(reason)) => {
                    // dnf does not exist on this box: respawning it for the
                    // dnf4 shape is pointless. The only other supported
                    // manager is apt — try it, report the chain on failure.
                    reasons.push(reason);
                    match run("add-apt-repository", &["-y", url], "add-apt-repository") {
                        Ok(()) => return Ok(()),
                        Err(other) => reasons.push(other.reason().to_string()),
                    }
                    return Err(format!("lcp-admin: {}", reasons.join("; ")));
                }
                Err(RepoProgramFailure::Unsupported(reason)) => reasons.push(reason),
            }
        }
        RepoAddShape::BaseUrl => {
            match run(
                "dnf",
                &["config-manager", "addrepo", &set_baseurl],
                "dnf config-manager addrepo --set=baseurl",
            ) {
                Ok(()) => return Ok(()),
                Err(RepoProgramFailure::Failed(reason)) => {
                    return Err(format!("lcp-admin: {reason}"))
                }
                Err(RepoProgramFailure::NotAvailable(reason)) => {
                    reasons.push(reason);
                    match run("add-apt-repository", &["-y", url], "add-apt-repository") {
                        Ok(()) => return Ok(()),
                        Err(other) => reasons.push(other.reason().to_string()),
                    }
                    return Err(format!("lcp-admin: {}", reasons.join("; ")));
                }
                Err(RepoProgramFailure::Unsupported(reason)) => reasons.push(reason),
            }
        }
    }
    // Reaching here means the dnf5 shape was refused at parsing time
    // (`Unsupported`) — dnf demonstrably ran and could not have acted. Only
    // now may the dnf4 legacy flag be attempted; its own mid-flight failure
    // stops the chain like any other execution attempt.
    match run(
        "dnf",
        &["config-manager", "--add-repo", url],
        "dnf config-manager --add-repo (dnf4)",
    ) {
        Ok(()) => return Ok(()),
        Err(RepoProgramFailure::Failed(reason)) => return Err(format!("lcp-admin: {reason}")),
        Err(other) => reasons.push(other.reason().to_string()),
    }
    // Both dnf shapes were refused at parsing time — no repository operation
    // was possible through dnf, so apt gets its one chance.
    match run("add-apt-repository", &["-y", url], "add-apt-repository") {
        Ok(()) => return Ok(()),
        Err(other) => reasons.push(other.reason().to_string()),
    }
    // Every attempt ended without an operation being possible: report the
    // chain so the primary package manager's reason is never masked.
    Err(format!("lcp-admin: {}", reasons.join("; ")))
}

/// Production entry point — see `repo_add_action_with`.
fn repo_add_action(url: &str) -> Result<(), String> {
    repo_add_action_with(run_repo_program, url)
}

/// The `repo_delete <file_path>` action: deletes a repository file with root
/// rights. Used when a repo file has at most one section; multi-section files
/// are handled by rewriting (write_file_as_root) from the app side.
fn repo_delete_action(file_path: &str) -> Result<(), String> {
    // Both halves of the policy hold independently — re-validate here even
    // though the app already validated.
    use root_policy::ensure_repo_file_path;
    ensure_repo_file_path(file_path)?;

    let output = std::process::Command::new("rm")
        .args(["-f", file_path])
        .output()
        .map_err(|e| format!("lcp-admin: failed to spawn rm: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "lcp-admin: rm -f {file_path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// The `repo_makecache` action: refreshes package metadata via dnf or apt.
/// Takes no further arguments.
///
/// Fallback policy matches `repo_add_action_with`: apt runs only when dnf
/// was demonstrably unavailable (could not be started). A `Failed` (dnf ran
/// and failed — network, configured-repo or permission trouble) stops
/// immediately: apt would hit the same world and its output would only
/// mask dnf's reason. `Unsupported` cannot occur for a bare `makecache`
/// (no flags); it is treated conservatively — also stop, return the reason.
fn repo_makecache_action() -> Result<(), String> {
    match run_repo_program("dnf", &["makecache"], "dnf") {
        Ok(()) => return Ok(()),
        Err(RepoProgramFailure::NotAvailable(reason)) => {
            match run_repo_program("apt-get", &["update"], "apt-get") {
                Ok(()) => return Ok(()),
                Err(other) => Err(format!(
                    "lcp-admin: {reason}; fallback: {}",
                    other.reason()
                )),
            }
        }
        Err(failure) => Err(format!("lcp-admin: {}", failure.reason())),
    }
}

/// The `repo_clean_cache <repo_id>` action: clears package metadata cache for
/// one repo via dnf or apt.
///
/// Fallback policy matches `repo_add_action_with`: apt runs only when dnf
/// was demonstrably unavailable. A mid-operation failure (repo id unknown,
/// permission trouble) stops immediately — `dnf clean --repo X` and
/// `apt-get clean` do not even address the same cache, so an apt fallback
/// would silently change the meaning of the operation.
fn repo_clean_cache_action(repo_id: &str) -> Result<(), String> {
    // Both halves of the policy hold independently — re-validate here even
    // though the app already validated.
    use root_policy::ensure_safe_operand;
    ensure_safe_operand(repo_id, "repository id")?;

    match run_repo_program(
        "dnf",
        &["clean", "expire-cache", "--repo", repo_id],
        "dnf",
    ) {
        Ok(()) => return Ok(()),
        Err(RepoProgramFailure::NotAvailable(reason)) => {
            match run_repo_program("apt-get", &["clean"], "apt-get") {
                Ok(()) => return Ok(()),
                Err(other) => Err(format!(
                    "lcp-admin: {reason}; fallback: {}",
                    other.reason()
                )),
            }
        }
        Err(failure) => Err(format!("lcp-admin: {}", failure.reason())),
    }
}
/// The `write_repo_file <path>` action: writes the repository-manager's one
/// allowed root-owned file kind — a `.repo`/`.list`/`.sources` file in one of
/// the package manager's own repository directories.
///
/// Every half of the policy holds independently (the helper does not depend
/// on the app having validated anything):
///
/// * `ensure_repo_file_path` — absolute, no relative parts, no symlink in
///   any path element, only the repository directories of dnf/yum/apt,
///   repository file extensions only,
/// * the content arrives on stdin and is given to
///   `ensure_repo_file_content` — the same allowlist-only validator the app
///   uses: only directives the panel writes, `%releasever`-style URL
///   variables only, internet-only schemes, and *signature verification can
///   never be switched off* (an apt `signed-by`/`Signed-By` keyring must sit
///   in an approved root-owned keyring directory),
/// * the write itself runs through the descriptor-pinned core below.
///
/// **Dispatch status: deliberately not registered yet.** It is not in
/// `ROOT_ACTIONS`, so `lcp-admin write_repo_file …` currently exits 2 as an
/// unknown action — registering it means a new live root-privileged path and
/// is a separate, explicitly authorised step (see the `ROOT_ACTIONS` note).
// dead_code is intentional while the action is unregistered; remove this
// attribute when the action is registered in ROOT_ACTIONS.
#[allow(dead_code)]
fn write_repo_file_action(path: &str) -> Result<(), String> {
    ensure_repo_file_path(path)?;
    let content = read_stdin_content(REPO_CONTENT_MAX as u64, "repository content")?;
    ensure_repo_file_content(path, &content)?;
    install_root_owned_file(path, &content, "repository file").map(|_| ())
}

/// The descriptor-pinned write core behind the root-owned config-file
/// actions (`write_profile_sh`, and the prepared `write_repo_file`):
/// replaces a *validated* file at `path` with `content`.
///
/// * `/etc`-level parent directory is pinned: opened `O_NOFOLLOW`, and it
///   must be the privileged-owned, non group/world-writable directory the
///   policy confined `path` to before this core ever sees a name,
/// * an existing destination is stat'ed *without* following symlinks — a
///   symlinked destination is refused, never written through,
/// * an existing destination is copied to a timestamped `<name>.<ts>.bak`
///   sibling (fsynced) **before** the rename; if the backup cannot be made
///   the write is abandoned and the original is left untouched. A new file
///   has no backup,
/// * the content lands in a uniquely named `O_CREAT|O_EXCL|O_NOFOLLOW` temp
///   sibling, then one `renameat` publishes it (replacing whatever sits at
///   the destination name instead of following it),
/// * owner/mode transfer happens on the temp descriptor *before* the
///   rename, so there is no path-based `chmod`/`chown` race window,
/// * an existing file keeps its owner but the mode is **forced to 0644**; a
///   new file is 0644 `privileged:privileged`.
///
/// Returns the absolute path of the backup, or `None` for a new file.
fn install_root_owned_file(
    path: &str,
    content: &str,
    what: &str,
) -> Result<Option<String>, String> {
    use std::io::Write;

    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // pin the parent: privileged-owned, never group/world-writable, never a
    // symlink
    let dir = open_dir_nofollow(CWD, &parent, &parent)?;
    let dir_stat = stat_fd(&dir, &parent)?;
    if dir_stat.st_uid != privileged_uid()
        || dir_stat.st_gid != privileged_gid()
        || dir_stat.st_mode & 0o022 != 0
    {
        return Err(format!(
            "lcp-admin: '{parent}' is not a safe privileged-owned directory; refusing."
        ));
    }

    // existing destination? stat *without* following and refuse symlinks;
    // otherwise the new file becomes privileged:privileged 0644
    let existing = match rustix::fs::statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            if (stat.st_mode & S_IFMT) != S_IFREG {
                return Err(format!(
                    "lcp-admin: '{path}' exists but is not a regular file; refusing to replace it."
                ));
            }
            Some((stat.st_uid, stat.st_gid))
        }
        Err(err) if err == rustix::io::Errno::NOENT => None,
        Err(_) => {
            return Err(format!(
                "lcp-admin: '{path}' could not be inspected; refusing to replace it."
            ))
        }
    };

    let temp_name = format!(".{name}.tmp.{}", unique_suffix());
    let fd = rustix::fs::openat(
        &dir,
        &temp_name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o644),
    )
    .map_err(|_| format!("lcp-admin: could not create the temporary {what}"))?;

    let result = (|| -> Result<Option<String>, String> {
        let mut file = std::fs::File::from(fd);
        let (target_uid, target_gid) = existing.unwrap_or((dir_stat.st_uid, dir_stat.st_gid));
        // the destination is always 0644 — an existing file must not keep a
        // group/world-writable mode it may have had before
        rustix::fs::fchmod(&file, Mode::from_raw_mode(0o644))
            .map_err(|_| format!("lcp-admin: could not set the {what} mode"))?;
        // the helper runs as root, so a *new* file is already
        // privileged:privileged; an existing file keeps its owner — set it
        // via the descriptor when the owners differ (a no-op failure in the
        // fixture tests, where files are self-owned)
        if rustix::fs::fchown(
            &file,
            Some(Uid::from_raw(target_uid)),
            Some(Gid::from_raw(target_gid)),
        )
        .is_err()
        {
            let owned = stat_fd(&file, &format!("the temporary {what}"))?;
            if owned.st_uid != target_uid || owned.st_gid != target_gid {
                return Err(format!("lcp-admin: could not keep the {what} owner"));
            }
        }
        file.write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("lcp-admin: could not write the {what} content: {e}"))?;
        // keep the previous revision, flushed, before the rename can replace
        // it — a failed backup aborts the whole write
        let backup = if existing.is_some() {
            Some(create_backup_sibling(&dir, name.as_str(), &parent, what)?)
        } else {
            None
        };
        rustix::fs::renameat(
            dir.as_fd(),
            temp_name.as_str(),
            dir.as_fd(),
            name.as_str(),
        )
        .map_err(|_| format!("lcp-admin: could not publish the {what}"))?;
        Ok(backup)
    })();

    match result {
        Ok(backup) => Ok(backup),
        Err(message) => {
            let _ = rustix::fs::unlinkat(&dir, temp_name.as_str(), AtFlags::empty());
            Err(message)
        }
    }
}

/// Copy the current destination to a timestamped sibling through the pinned
/// directory descriptor, before the rename publishes the new content.
/// Refuses a symlinked destination (the caller already checked) and unlinking
/// its own partial file if anything fails.
fn create_backup_sibling(
    dir: &rustix::fd::OwnedFd,
    name: &str,
    parent: &str,
    what: &str,
) -> Result<String, String> {
    use std::io::{Read, Write};

    let src_fd = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| {
        format!("lcp-admin: could not open the existing {what} for its backup; refusing.")
    })?;
    let mut src = std::fs::File::from(src_fd);
    let mut bytes = Vec::new();
    src.read_to_end(&mut bytes).map_err(|_| {
        format!("lcp-admin: could not read the existing {what} for its backup; refusing.")
    })?;

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|age| age.as_secs())
        .unwrap_or(0);
    let backup_name = format!("{name}.{secs}_{}.bak", unique_suffix());
    let backup_fd = rustix::fs::openat(
        dir,
        backup_name.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o644),
    )
    .map_err(|_| format!("lcp-admin: could not create the {what} backup; refusing."))?;
    let mut backup = std::fs::File::from(backup_fd);

    let write_result = (|| -> Result<(), String> {
        backup
            .write_all(&bytes)
            .map_err(|_| format!("lcp-admin: could not write the {what} backup; refusing."))?;
        rustix::fs::fchmod(&backup, Mode::from_raw_mode(0o644))
            .map_err(|_| format!("lcp-admin: could not set the {what} backup mode; refusing."))?;
        backup
            .sync_all()
            .map_err(|_| format!("lcp-admin: could not flush the {what} backup; refusing."))
    })();
    if let Err(message) = write_result {
        let _ = rustix::fs::unlinkat(dir, backup_name.as_str(), AtFlags::empty());
        return Err(message);
    }
    Ok(format!("{parent}/{backup_name}"))
}

// ─── tests: descriptor-pinned filesystem core ────────────────────────────────
//
// These unit tests exercise `ssh_dir_open_or_create`, `install_keys` and
// `read_keys` against isolated fixtures inside the test's own temp space.
// They never touch the invoking user's real ~/.ssh, /etc/ssh or any system
// authentication material; symlinks are planted inside the *fixture*
// directory (they point at test files only, never at real system files).
// The helper runs as root in production but as the test user here, so all
// ownership assertions use the test's own uid/gid (from the fixture's own
// metadata).

#[cfg(test)]
mod ssh_fs_tests {
    use super::{install_keys, read_keys, ssh_dir_open_or_create, SshDir};
    use rustix::fd::OwnedFd;
    use rustix::fs::{Mode, OFlags, CWD};
    use std::os::unix::fs::MetadataExt;

    const KEY: &str = "ssh-ed25519 AAATESTKEY test@example.com\n";

    struct Fixture {
        home: OwnedFd,
        path: std::path::PathBuf,
        uid: u32,
        gid: u32,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn fixture(name: &str) -> Fixture {
        let path = std::env::temp_dir().join(format!(
            "lcp-admin-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let home = rustix::fs::openat(
            CWD,
            path.as_os_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap();
        let stat = rustix::fs::fstat(&home).unwrap();
        Fixture { home, path, uid: stat.st_uid, gid: stat.st_gid }
    }

    /// The fixture's `.ssh`, opened and verified through the production core.
    fn ssh_dir(home: &Fixture, create: bool) -> SshDir {
        ssh_dir_open_or_create(&home.home, home.uid, home.gid, create).unwrap()
    }

    fn auth_path(home: &Fixture) -> std::path::PathBuf {
        home.path.join(".ssh").join("authorized_keys")
    }

    #[test]
    fn write_creates_dir_and_file_with_the_fixed_shape() {
        let home = fixture("create");
        assert!(!home.path.join(".ssh").exists());
        let ssh = ssh_dir(&home, true);
        install_keys(&ssh, KEY).unwrap();

        let dir_meta = std::fs::metadata(home.path.join(".ssh")).unwrap();
        assert!(dir_meta.is_dir());
        assert_eq!(dir_meta.mode() & 0o777, 0o700, ".ssh must be 0700");
        assert_eq!(dir_meta.uid(), home.uid, ".ssh must be owned by the account");

        let meta = std::fs::metadata(auth_path(&home)).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o600, "authorized_keys must be 0600");
        assert_eq!(meta.uid(), home.uid, "authorized_keys must be owned by the account");

        let content = read_keys(&ssh).unwrap().unwrap();
        assert_eq!(content, KEY);
        assert!(
            !home.path.join(".ssh").read_dir().unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp")),
            "no temp files may be left behind"
        );
    }

    #[test]
    fn write_replaces_a_symlinked_authorized_keys_without_following_it() {
        // The old code's post-rename chmod/chown followed exactly this class
        // of symlink. Now: install first, then plant a symlink — the file is
        // replaced on the next write, never written through.
        let home = fixture("symlink-victim");
        std::fs::create_dir(home.path.join(".ssh")).unwrap();
        let victim = home.path.join(".ssh").join("victim file");
        std::fs::write(&victim, b"victim\n").unwrap();

        let ssh = ssh_dir(&home, false);
        install_keys(&ssh, KEY).unwrap();

        // Plant the old exploit: authorized_keys → 'victim file' (a relative
        // symlink an attacker-with-home-access can create at any time).
        let auth = auth_path(&home);
        std::fs::remove_file(&auth).unwrap();
        std::os::unix::fs::symlink("victim file", &auth).unwrap();

        let shell_meta = std::fs::metadata(&victim).unwrap();
        install_keys(&ssh, KEY).unwrap();

        // the victim file is untouched — no mode/owner change ever followed
        // the planted link
        let after = std::fs::metadata(&victim).unwrap();
        assert_eq!(after.mode(), shell_meta.mode(), "victim mode unchanged");
        assert_eq!(after.uid(), shell_meta.uid(), "victim owner unchanged");
        assert_eq!(std::fs::read(&victim).unwrap(), b"victim\n", "victim content unchanged");
        // and authorized_keys is our real regular file again
        let auth_meta = std::fs::symlink_metadata(&auth).unwrap();
        assert!(auth_meta.is_file(), "authorized_keys must not stay a symlink");
        assert!(std::fs::read_to_string(&auth).unwrap().contains(KEY.trim()));
    }

    #[test]
    fn read_refuses_a_planted_symlink_instead_of_following_it() {
        let home = fixture("symlink-read");
        std::fs::create_dir(home.path.join(".ssh")).unwrap();
        let probe = home.path.join(".ssh").join("probe");
        std::fs::write(&probe, b"secret").unwrap();
        std::os::unix::fs::symlink("probe", auth_path(&home)).unwrap();

        let ssh = ssh_dir(&home, false);
        let result = read_keys(&ssh);
        assert!(
            result.is_err(),
            "a planted authorized_keys symlink must be refused, not followed"
        );
    }

    #[test]
    fn read_refuses_a_file_owned_by_someone_else() {
        let home = fixture("wrong-owner");
        std::fs::create_dir(home.path.join(".ssh")).unwrap();
        std::fs::write(auth_path(&home), "other's data").unwrap();
        // chown is unavailable without root, so simulate the check by
        // presenting an owner mismatch directly
        let ssh = SshDir { dir: rustix::fs::openat(
            CWD,
            home.path.join(".ssh").as_os_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        ).unwrap(), uid: home.uid + 12345, gid: home.gid };
        let result = read_keys(&ssh);
        assert!(result.is_err(), "a foreign-owned authorized_keys must be refused");
    }

    #[test]
    fn symlinked_ssh_dir_is_refused_not_followed() {
        let home = fixture("dir-symlink");
        let decoy = home.path.join("decoy");
        std::fs::create_dir(&decoy).unwrap();
        std::os::unix::fs::symlink("decoy", home.path.join(".ssh")).unwrap();

        let result = ssh_dir_open_or_create(&home.home, home.uid, home.gid, true);
        assert!(result.is_err(), "a symlinked ~/.ssh must be refused");
        // the read action must report the same failure (not an empty list)
        assert!(result.unwrap_err().chars().count() > 0);
    }

    #[test]
    fn group_or_world_writable_ssh_dir_is_refused() {
        let home = fixture("loose-perms");
        std::fs::create_dir(home.path.join(".ssh")).unwrap();
        // make it group-writable (the test user can chmod its own dir)
        let p = home.path.join(".ssh");
        use std::os::unix::fs::PermissionsExt;
        let wavos = std::fs::metadata(&p).unwrap().permissions();
        let mut writable = wavos;
        writable.set_mode(0o770);
        std::fs::set_permissions(&p, writable).unwrap();

        let result = ssh_dir_open_or_create(&home.home, home.uid, home.gid, true);
        assert!(result.is_err(), "a group-writable ~/.ssh must be refused");
    }

    #[test]
    fn concurrent_temp_wedge_cannot_stop_a_correct_install() {
        let home = fixture("temp-wedge");
        ssh_dir(&home, true);
        // Pre-plant a *file* (not a symlink) at the old fixed temp path —
        // unique temp names make this harmless again (the old code removed
        // and reused one fixed name: `authorized_keys.tmp`).
        std::fs::write(home.path.join(".ssh").join("authorized_keys.tmp"), "wedge").unwrap();

        let ssh = ssh_dir(&home, false);
        install_keys(&ssh, KEY).unwrap();
        assert!(std::fs::read_to_string(auth_path(&home)).unwrap().contains(KEY.trim()));
    }

    #[test]
    fn missing_dir_read_reports_empty_without_an_error() {
        let home = fixture("missing-dir");
        let result = ssh_dir_open_or_create(&home.home, home.uid, home.gid, false);
        assert!(
            result.unwrap_err().contains("no .ssh"),
            "read path treats a missing .ssh as empty, not an error"
        );
    }
}

// ─── tests: the write_repo_file action core (deliberately unregistered) ─────
//
// Scope C of the STEP 3B plan: the future helper-side repository write
// boundary, prepared but not activated. These tests exercise the action's
// validation + descriptor-pinned write core directly (not via dispatch — the
// action is refused there, see `dispatch-tests/fixture_dispatch.rs`) against
// the fixture repository directories, never against the real /etc. They only
// run under `cargo test --features test-fixture`, where the repository
// allowlist itself is the fixture twin of /etc/yum.repos.d & friends.
#[cfg(all(test, feature = "test-fixture"))]
mod repo_fs_tests {
    use super::{install_root_owned_file, privileged_gid, privileged_uid};
    use crate::root_policy::{ensure_repo_file_content, ensure_repo_file_path};
    use std::os::unix::fs::MetadataExt;

    fn fixture_repo_dir(subdir: &str) -> std::path::PathBuf {
        let path = std::path::Path::new("/tmp/lcp-admin-test-fixture")
            .join("etc")
            .join(subdir);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    const GOOD: &str = "[the-repo]\n\
        name=Fixture Repo\n\
        baseurl=https://mirror.example.org/fedora/$releasever/Everything/$basearch/os/\n\
        gpgkey=https://mirror.example.org/fedora/RPM-GPG-KEY\n\
        gpgcheck=1\n\
        enabled=1\n";

    #[test]
    fn policy_validated_repo_file_is_written_by_the_descriptor_core() {
        let dir = fixture_repo_dir("yum.repos.d");
        let path = dir.join("the-repo.repo").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        ensure_repo_file_path(&path).unwrap();
        ensure_repo_file_content(&path, GOOD).unwrap();
        install_root_owned_file(&path, GOOD, "repository file").unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o644, "a new repo file must be 0644");
        assert_eq!(meta.uid(), privileged_uid());
        assert_eq!(meta.gid(), privileged_gid());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            GOOD,
            "the exact validated content must be written"
        );
        let no_temp_left = std::fs::read_dir(&dir)
            .unwrap()
            .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains(".tmp"));
        assert!(no_temp_left, "no temp files may be left behind");
    }

    #[test]
    fn untouched_repo_file_when_the_content_policy_refuses() {
        // the signature policy and directive allowlist stay enforced by the
        // very same validator the app uses
        let dir = fixture_repo_dir("yum.repos.d");
        let path = dir.join("no-signatures.repo").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);
        assert!(
            ensure_repo_file_path(&path).is_ok(),
            "the fixture repo path must validate"
        );
        assert!(
            ensure_repo_file_content(&path, "[r]\nbaseurl=https://example.com/repo\ngpgcheck=0\n")
                .is_err(),
            "gpgcheck=0 must be refused"
        );
        assert!(
            ensure_repo_file_content(
                &path,
                "[r]\nbaseurl=file:///home/attacker/repo\ngpgcheck=1\n"
            )
            .is_err(),
            "a local-file baseurl must be refused"
        );
        assert!(
            ensure_repo_file_content(&path, "[r]\nbaseurl=https://x/one\nproxy__exec=/bin/sh\n")
                .is_err(),
            "unknown directives must be refused"
        );
        // because validation precedes the write core, nothing was written
        assert!(!std::path::Path::new(&path).exists(), "a refused content must never reach the disk");
    }

    #[test]
    fn hostile_signed_by_list_content_is_refused() {
        let dir = fixture_repo_dir("apt/sources.list.d");
        let path = dir.join("hostile.list").to_string_lossy().into_owned();
        assert!(
            ensure_repo_file_path(&path).is_ok(),
            "the fixture apt dir must validate"
        );
        assert!(
            ensure_repo_file_content(
                &path,
                "deb [signed-by=/home/attacker/evil.gpg] https://mirror.example.org/deb ./\n"
            )
            .is_err(),
            "a user-controlled keyring must be refused"
        );
        assert!(
            !std::path::Path::new(&path).exists(),
            "refused content never reaches the disk"
        );
    }

    #[test]
    fn core_refuses_a_symlinked_destination_even_if_production_paths_shift() {
        // belt-and-braces: even when a destination passed every path check
        // now sits behind a planted symlink (a race between the validation
        // step and the write core), the core refuses instead of following
        let dir = fixture_repo_dir("yum.repos.d");
        let victim = dir.join("victim file");
        std::fs::write(&victim, b"victim\n").unwrap();
        let dest = dir.join("planted.repo");
        let _ = std::fs::remove_file(&dest);
        std::os::unix::fs::symlink("victim file", &dest).unwrap();

        let result = install_root_owned_file(
            dest.to_string_lossy().as_ref(),
            GOOD,
            "repository file",
        );
        assert!(result.is_err(), "a symlinked destination must be refused");
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"victim\n",
            "the symlink target must not be touched"
        );
    }

    #[test]
    fn repository_paths_outside_the_confined_directories_are_refused() {
        // the real directories are never in the fixture allowlist, so a
        // fixture build cannot be used to touch them either
        assert!(ensure_repo_file_path("/etc/yum.repos.d/real.repo").is_err());
        assert!(ensure_repo_file_path("/etc/apt/sources.list.d/real.list").is_err());
        assert!(ensure_repo_file_path("/etc/shadow").is_err());
    }
}

/// The dispatch gate and the validation list must agree: an action known to
/// `check_helper_action` but missing from `ROOT_ACTIONS` never reaches its
/// dispatch branch — the name falls through to program resolution and the
/// caller sees "the program 'X' was not found" while every argv validator
/// test still passes. This is exactly the drift the network and repo
/// migrations introduced; these tests pin the two lists together so it
/// cannot recur silently.
#[cfg(test)]
mod root_actions_gate_tests {
    use crate::root_policy::HELPER_ACTIONS;
    use crate::ROOT_ACTIONS;

    #[test]
    fn every_registered_helper_action_is_in_root_actions() {
        for action in HELPER_ACTIONS {
            assert!(
                ROOT_ACTIONS.contains(action),
                "action '{action}' is validated by check_helper_action but \
                 missing from ROOT_ACTIONS — it would fall through to \
                 program resolution instead of being dispatched"
            );
        }
    }

    #[test]
    fn root_actions_has_no_gate_only_names() {
        for action in ROOT_ACTIONS {
            assert!(
                HELPER_ACTIONS.contains(action),
                "ROOT_ACTIONS contains '{action}' but HELPER_ACTIONS does \
                 not validate it — the name would be dispatched to a \
                 non-existent branch"
            );
        }
    }

    #[test]
    fn the_gate_is_not_stale_nginx_edition() {
        // regression pin: the exact 11 names the two migrations added
        // without updating the gate
        for action in [
            "nmcli_connection_save",
            "nmcli_connection_delete",
            "nmcli_connection_up",
            "nmcli_connection_down",
            "nmcli_connection_import",
            "repo_toggle_enable",
            "repo_toggle_disable",
            "repo_add",
            "repo_delete",
            "repo_makecache",
            "repo_clean_cache",
        ] {
            assert!(ROOT_ACTIONS.contains(&action), "gate lost '{action}'");
        }
    }
}

/// Targeted behaviour tests for `repo_add_action_with`, using an *injected
/// command runner*: the recorded (program, args, label) calls and simulated
/// outcomes stand in for dnf/apt, so no real package manager is ever
/// spawned and no repository configuration is ever touched by a test.
/// These are therefore argument-shape/ordering/error-preservation tests —
/// they do NOT verify real dnf4/apt runtime behaviour, real duplicate-ID
/// handling, or that dnf accepts the shapes (that was verified live by an
/// operator on dnf5 5.4.6 and must be re-verified per platform).
#[cfg(test)]
mod repo_add_action_tests {
    use super::classify_repo_add_url;
    use super::repo_add_action_with;
    use super::{RepoAddShape, RepoProgramFailure};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Call = (String, Vec<String>, String);
    type Recorder = Rc<RefCell<Vec<Call>>>;

    /// Simulated runner outcome per call.
    enum Sim {
        /// fail with a mid-operation error — the *ambiguous* kind
        Failed(String),
        /// fail with an argument-parse refusal — non-acting kind
        Unsupported(String),
        /// fail with a spawn error — nothing ran
        NotAvailable(String),
        /// succeed
        Ok,
    }

    fn sim_kind(label: &str, kind: &Sim) -> Result<(), RepoProgramFailure> {
        match kind {
            Sim::Ok => Ok(()),
            Sim::Failed(detail) => Err(RepoProgramFailure::Failed(format!(
                "{label} {detail}"
            ))),
            Sim::Unsupported(detail) => Err(RepoProgramFailure::Unsupported(format!(
                "{label} {detail}"
            ))),
            Sim::NotAvailable(detail) => Err(RepoProgramFailure::NotAvailable(format!(
                "{label} {detail}"
            ))),
        }
    }

    /// A runner that records every call and replays `script` in order
    /// (repeating the last entry thereafter).
    fn scripted(script: Vec<Sim>) -> (impl Fn(&str, &[&str], &str) -> Result<(), RepoProgramFailure>, Recorder) {
        let calls: Recorder = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let run = move |program: &str, args: &[&str], label: &str| {
            let visited = sink.borrow().len();
            sink.borrow_mut().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
                label.to_string(),
            ));
            let kind = script.get(visited).unwrap_or(script.last().unwrap());
            sim_kind(label, kind)
        };
        (run, calls)
    }

    #[test]
    fn https_base_url_runs_dnf5_set_baseurl() {
        let (run, calls) = scripted(vec![Sim::Ok]);
        repo_add_action_with(run, "https://example.com/path/").unwrap();
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1, "success must short-circuit every fallback");
        assert_eq!(calls[0].0, "dnf");
        assert_eq!(
            calls[0].1,
            vec![
                "config-manager",
                "addrepo",
                "--set=baseurl=https://example.com/path/"
            ]
        );
    }

    #[test]
    fn https_repofile_url_runs_dnf5_from_repofile() {
        let (run, calls) = scripted(vec![Sim::Ok]);
        let _ = repo_add_action_with(run, "https://example.com/path/nitrux.repo").unwrap();
        let calls = calls.borrow();
        assert_eq!(calls[0].0, "dnf");
        assert_eq!(
            calls[0].1,
            vec![
                "config-manager",
                "addrepo",
                "--from-repofile=https://example.com/path/nitrux.repo"
            ]
        );
    }

    #[test]
    fn repofile_classification_ignores_query_and_fragment() {
        assert_eq!(
            classify_repo_add_url("https://example.com/x.repo?download=1"),
            RepoAddShape::RepofileUrl
        );
        assert_eq!(
            classify_repo_add_url("https://example.com/x.repo#anchor"),
            RepoAddShape::RepofileUrl
        );
        assert_eq!(
            classify_repo_add_url("https://example.com/x.repo?f=/x.repo"),
            RepoAddShape::RepofileUrl
        );
        // the query is present in the *argument*, only stripped for the shape
        let (run, calls) = scripted(vec![Sim::Ok]);
        repo_add_action_with(run, "https://example.com/x.repo?download=1").unwrap();
        assert_eq!(
            calls.borrow()[0].1,
            vec!["config-manager", "addrepo", "--from-repofile=https://example.com/x.repo?download=1"]
        );
    }

    #[test]
    fn base_url_with_query_string_stays_a_base_url() {
        assert_eq!(
            classify_repo_add_url("https://example.com/x?mirror=list"),
            RepoAddShape::BaseUrl
        );
        assert_eq!(
            classify_repo_add_url("https://example.com/x#section"),
            RepoAddShape::BaseUrl
        );
    }

    #[test]
    fn non_https_url_is_refused_before_any_program_runs() {
        for url in ["http://example.com/x.repo", "file:///etc/passwd"] {
            let (run, calls) = scripted(vec![]);
            let err = repo_add_action_with(run, url).unwrap_err();
            assert!(calls.borrow().is_empty(), "nothing may run before validation");
            assert!(
                !err.contains("simulated"),
                "the error is the validator's, not a simulated one: {err}"
            );
        }
    }

    #[test]
    fn malformed_url_still_classifies_and_reaches_the_base_url_shape() {
        // chosen behaviour: classification is mechanical, the validator is
        // the authority on accepting the URL. "https://" is well-formed
        // enough for the validators (https, no control chars) and classifies
        // as a base URL — dnf reports the real verdict.
        assert_eq!(classify_repo_add_url("https://"), RepoAddShape::BaseUrl);
        assert_eq!(classify_repo_add_url("not a url at all"), RepoAddShape::BaseUrl);
    }

    /// The heart of the conservative policy: a *failed execution* stops the
    /// chain. dnf ran and something went wrong mid-flight (network, config
    /// validity, partial write) — no other package manager may be tried for
    /// it, and the error is dnf's reason alone, not a chain.
    #[test]
    fn ambiguous_dnf_failure_stops_the_chain_and_names_only_dnf() {
        for fail in ["the metadata download failed", "Error in configuration file"] {
            let (run, calls) = scripted(vec![Sim::Failed(fail.to_string())]);
            let err = repo_add_action_with(run, "https://example.com/x").unwrap_err();
            let borrowed = calls.borrow();
            assert_eq!(borrowed.len(), 1, "no fallback may run after an execution attempt");
            assert_eq!(borrowed[0].0, "dnf");
            assert!(
                err.contains("dnf config-manager addrepo") && err.ends_with(fail),
                "the error is dnf's alone: {err}"
            );
            assert!(!err.contains("add-apt-repository"), "apt must not be mentioned as tried: {err}");
        }
    }

    /// A parsing-time refusal ("Unknown argument …") demonstrates the option
    /// surface is incompatible without any operation having run — only then
    /// may the dnf4 legacy flag be attempted.
    #[test]
    fn unsupported_dnf_shape_falls_back_to_the_dnf4_flag() {
        let (run, calls) = scripted(vec![
            Sim::Unsupported("Unknown argument \"--set=baseurl\"".to_string()),
            Sim::Ok,
        ]);
        repo_add_action_with(run, "https://example.com/x").unwrap();
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].2.contains("addrepo --set=baseurl"));
        assert!(calls[1].2.contains("--add-repo (dnf4)"), "second attempt is the dnf4 flag");
    }

    /// When even the dnf4 shape is refused at parsing time (no operation
    /// possible through dnf), apt gets exactly one chance.
    #[test]
    fn both_dnf_shapes_unsupported_falls_back_to_apt() {
        let (run, calls) = scripted(vec![
            Sim::Unsupported("Unknown argument".to_string()),
            Sim::Unsupported("Unknown argument".to_string()),
            Sim::Ok,
        ]);
        repo_add_action_with(run, "https://example.com/x").unwrap();
        let calls = calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[2].0, "add-apt-repository");
    }

    /// A missing dnf binary spawns nothing and never configures anything —
    /// only then apt is the primary manager (Debian-shaped system).
    #[test]
    fn missing_dnf_falls_back_to_apt() {
        let (run, calls) = scripted(vec![
            Sim::NotAvailable("could not be started".to_string()),
            Sim::Ok,
        ]);
        repo_add_action_with(run, "https://deb.example.org/debian").unwrap();
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "dnf");
        assert_eq!(calls[1].0, "add-apt-repository", "apt runs once dnf is demonstrably absent");
        // only one dnf call happened — after NotAvailable there is no point
        // respawning the same missing binary for the dnf4 legacy shape
        assert_eq!(
            calls.iter().filter(|(p, _, _)| p == "dnf").count(),
            1,
            "the missing dnf must not be respawned for the dnf4 shape"
        );
    }

    /// Every simulated path that keeps running records its reasons in order
    /// in the final error (no masks, no invented text).
    #[test]
    fn kept_reasons_appear_in_order_in_the_final_error() {
        let (run, calls) = scripted(vec![
            Sim::Unsupported("Unsupported one".to_string()),
            Sim::Unsupported("Unsupported two".to_string()),
            Sim::NotAvailable("Fallback three".to_string()),
        ]);
        let err = repo_add_action_with(run, "https://example.com/x").unwrap_err();
        let _ = calls;
        let first = err.find("Unsupported one").unwrap();
        let second = err.find("Unsupported two").unwrap();
        let third = err.find("Fallback three").unwrap();
        assert!(first < second && second < third, "reason order preserved: {err}");
    }

    #[test]
    fn repo_add_never_passes_overwrite_or_replace_options() {
        // safety pin for duplicate/exists handling: the helper never adds
        // --overwrite / --add-or-replace, so an existing repository file is
        // only ever replaced if the package manager does it on its own
        // initiative. Real duplicate-handling behaviour of each package
        // manager is out of unit-test scope by design.
        let (run, calls) = scripted(vec![Sim::Failed("unknown repo already exists here".to_string())]);
        let _ = repo_add_action_with(run, "https://example.com/dupe.repo").unwrap_err();
        for (_, args, _) in calls.borrow().iter() {
            assert!(
                !args.iter().any(|a| a == "--overwrite" || a == "--add-or-replace"),
                "repo_add must not silently overwrite: {args:?}"
            );
        }
    }
}

/// Real-process integration for `run_repo_program` itself — the one place
/// raw processes are spawned. These tests use only the fixed absolute
/// binaries below (always present on the fixture hosts, never package
/// managers), need no root privileges and touch no system configuration.
/// They verify the failure-kind classification, stderr/stdout capture
/// ordering, and that argument contents (potential secrets) never leak
/// into failure messages.
#[cfg(test)]
mod run_repo_program_tests {
    use super::{run_repo_program, RepoProgramFailure};

    const TRUE_BIN: &str = "/usr/bin/true";
    const FALSE_BIN: &str = "/usr/bin/false";
    const SH_BIN: &str = "/bin/sh";
    const MISSING_BIN: &str = "/nonexistent-lcp-test-binary-xyz";

    #[test]
    fn successful_process_is_ok() {
        assert!(run_repo_program(TRUE_BIN, &[], "/bin/true").is_ok());
    }

    #[test]
    fn nonzero_exit_without_any_output_is_a_plain_failure() {
        let err = run_repo_program(FALSE_BIN, &[], "/bin/false").unwrap_err();
        match err {
            RepoProgramFailure::Failed(detail) => {
                assert!(detail.contains("exit status") && detail.contains('1'), "{detail}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn stderr_wins_over_stdout() {
        // sh writes to stdout and completes with an error — both available
        let err = run_repo_program(SH_BIN, &["-c", "echo stdout-line; echo stderr-line 1>&2; exit 3"], "sh").unwrap_err();
        let RepoProgramFailure::Failed(detail) = err else { panic!("expected Failed") };
        assert!(detail.contains("stderr-line"), "stderr must be captured: {detail}");
        assert!(!detail.contains("stdout-line"), "stderr must win when present: {detail}");
    }

    #[test]
    fn stdout_is_used_when_stderr_is_silent() {
        let err = run_repo_program(
            SH_BIN,
            &["-c", "echo only-stdout; exit 4"],
            "sh",
        ).unwrap_err();
        let RepoProgramFailure::Failed(detail) = err else { panic!("expected Failed") };
        assert!(detail.contains("only-stdout"), "stdout must back the error when stderr is empty: {detail}");
    }

    #[test]
    fn parse_refusal_text_is_classified_unsupported() {
        let err = run_repo_program(
            SH_BIN,
            &["-c", "echo 'Unknown argument \"--add-repo\" for command \"config-manager\"' 1>&2; exit 2"],
            "dnf",
        ).unwrap_err();
        let RepoProgramFailure::Unsupported(detail) = err else { panic!("expected Unsupported, got {err:?}") };
        assert!(detail.contains("Unknown argument"), "{detail}");
    }

    #[test]
    fn missing_binary_is_not_available() {
        let err = run_repo_program(MISSING_BIN, &["arg"], "missing-test").unwrap_err();
        let RepoProgramFailure::NotAvailable(detail) = err else { panic!("expected NotAvailable") };
        assert!(detail.contains("could not be started"), "{detail}");
    }

    #[test]
    fn arguments_are_never_reproduced_into_failure_reasons() {
        // a "secret" operand must not appear in the spawn-failure message
        let err = run_repo_program(
            MISSING_BIN,
            &["secret-operand-do-not-repeat"],
            "missing-test",
        ).unwrap_err();
        let RepoProgramFailure::NotAvailable(detail) = err else { panic!("expected NotAvailable") };
        assert!(
            !detail.contains("secret-operand-do-not-repeat"),
            "failure messages must not echo operands: {detail}"
        );
    }
}

#[cfg(test)]
mod firewall_ssh_guard_tests {
    use super::{
        firewall_confirm_split, firewall_port_covers_22, firewall_rich_rule_guard,
        firewall_zone_rule_guard, parse_ss_sessions, ssh_session_binds_interface,
        ssh_sessions_touch_interface, FirewallSshGuard,
    };
    use crate::root_policy::{firewall_rich_rule_valid, FirewallRichRuleAction};

    fn parse(rule: &str) -> crate::root_policy::FirewallRichRule {
        firewall_rich_rule_valid(rule)
            .unwrap_or_else(|err| panic!("test rule must be valid: {rule} — {err}"))
    }

    #[test]
    fn port_covers_22_for_single_ports_and_ranges() {
        assert!(firewall_port_covers_22("22/tcp"));
        assert!(firewall_port_covers_22("20-25/tcp"));
        assert!(firewall_port_covers_22("1-1024/udp"));
        assert!(!firewall_port_covers_22("23/tcp"));
        assert!(!firewall_port_covers_22("1025-65535/udp"));
        // fail closed on malformed shapes (cannot occur post-validation)
        assert!(firewall_port_covers_22("garbage"));
    }

    #[test]
    fn zone_rule_guard_covers_only_ssh_removals() {
        assert_eq!(
            firewall_zone_rule_guard("service", "ssh", false),
            Some(FirewallSshGuard::RemoveServiceSsh)
        );
        assert_eq!(
            firewall_zone_rule_guard("port", "22/tcp", false),
            Some(FirewallSshGuard::RemovePort22)
        );
        assert_eq!(firewall_zone_rule_guard("service", "ssh", true), None);
        assert_eq!(firewall_zone_rule_guard("service", "https", false), None);
        assert_eq!(firewall_zone_rule_guard("port", "1025-65535/udp", false), None);
    }

    #[test]
    fn rich_rule_guard_covers_only_added_drop_and_reject_targeting_ssh() {
        assert_eq!(
            firewall_rich_rule_guard(true, &parse(r#"rule family="ipv4" service name="ssh" drop"#)),
            Some(FirewallSshGuard::RichDropSsh)
        );
        assert_eq!(
            firewall_rich_rule_guard(true, &parse(r#"rule family="ipv4" port port="22" protocol="tcp" reject"#)),
            Some(FirewallSshGuard::RichRejectSsh)
        );
        assert_eq!(
            firewall_rich_rule_guard(false, &parse(r#"rule family="ipv4" service name="ssh" drop"#)),
            None,
            "removals are never a lockout"
        );
        assert_eq!(
            firewall_rich_rule_guard(true, &parse(r#"rule family="ipv4" service name="ssh" accept"#)),
            None,
            "accept never locks out"
        );
        assert_eq!(
            firewall_rich_rule_guard(true, &parse(r#"rule family="ipv4" service name="https" drop"#)),
            None
        );
        assert_eq!(
            firewall_rich_rule_guard(true, &parse(r#"rule family="ipv4" port port="1025-65535" protocol="udp" drop"#)),
            None
        );
        assert_eq!(FirewallRichRuleAction::Drop, FirewallRichRuleAction::Drop);
    }

    #[test]
    fn session_scope_marker_binds_only_the_named_interface() {
        let line = "ESTAB 0 0 192.168.0.103%wlp1s0:22 10.0.0.5:53342";
        assert_eq!(ssh_session_binds_interface(line, "wlp1s0"), Some(true));
        assert_eq!(ssh_session_binds_interface(line, "eth0"), Some(false));
        // an IPv6 scope works the same way
        let line = "ESTAB 0 0 fe80::1%enp3s0:22 fe80::2:51532";
        assert_eq!(ssh_session_binds_interface(line, "enp3s0"), Some(true));
        assert_eq!(ssh_session_binds_interface(line, "eth0"), Some(false));
    }

    #[test]
    fn session_without_scope_marker_is_never_provably_elsewhere() {
        let line = "ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342";
        assert_eq!(ssh_session_binds_interface(line, "eth0"), None);
        // and a session without a local endpoint at all stays unknown
        assert_eq!(ssh_session_binds_interface("ESTAB 0 0", "eth0"), None);
        assert_eq!(ssh_session_binds_interface("", "eth0"), None);
    }

    #[test]
    fn interface_touch_is_true_for_any_unprovable_or_matching_session() {
        let marked = "ESTAB 0 0 192.168.0.103%wlp1s0:22 10.0.0.5:53342".to_string();
        let unmarked = "ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342".to_string();
        // session provably elsewhere: no touch
        assert!(!ssh_sessions_touch_interface(&[marked.clone()], "eth0"));
        // matching or unmarked sessions: touch
        assert!(ssh_sessions_touch_interface(&[marked.clone()], "wlp1s0"));
        assert!(ssh_sessions_touch_interface(&[unmarked.clone()], "eth0"));
        // mixed: one provably elsewhere, one unmarked → still touches
        assert!(ssh_sessions_touch_interface(&[marked.clone(), unmarked.clone()], "eth0"));
        assert!(!ssh_sessions_touch_interface(&[], "eth0"));
    }

    #[test]
    fn confirm_split_treats_absence_as_unconfirmed() {        let args: Vec<String> = ["firewall_zone_rule", "add", "public", "service", "ssh"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (rest, confirmed) = firewall_confirm_split(&args, 5, 6).unwrap();
        assert!(!confirmed, "a missing confirmation field is never confirmation");
        assert_eq!(rest.len(), 5);
    }

    #[test]
    fn confirm_split_accepts_only_the_exact_literal() {
        let args: Vec<String> = ["firewall_rich_rule", "add", "public", "rule family=\"ipv4\" protocol value=\"1\" drop", "confirm"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (_, confirmed) = firewall_confirm_split(&args, 4, 5).unwrap();
        assert!(confirmed);

        for bogus in ["yes", "confirmed", "false", "confirm " ] {
            let args: Vec<String> = ["firewall_rich_rule", "add", "public", "rule", bogus]
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert!(
                firewall_confirm_split(&args, 4, 5).is_err(),
                "'{bogus}' must not count as confirmation"
            );
        }
    }

    #[test]
    fn confirm_split_rejects_wrong_argument_counts() {
        let args: Vec<String> = ["firewall_zone_rule", "add", "public", "service"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(firewall_confirm_split(&args, 5, 6).is_err());
        let args: Vec<String> = ["firewall_zone_rule", "add", "public", "service", "ssh", "confirm", "extra"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(firewall_confirm_split(&args, 5, 6).is_err());
    }

    #[test]
    fn ss_header_only_output_means_no_sessions() {
        let headers = [
            "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
            "Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
            "Netid State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
        ];
        for header in headers {
            assert!(
                parse_ss_sessions(header).is_empty(),
                "a header line is never a session: {header:?}"
            );
        }
    }

    #[test]
    fn ss_header_plus_one_established_row_means_one_session() {
        let output = "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n\
                      ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342 users:((\"sshd\",pid=1,fd=3))\n";
        let sessions = parse_ss_sessions(output);
        assert_eq!(sessions.len(), 1, "{sessions:?}");
        assert!(sessions[0].starts_with("ESTAB"), "{sessions:?}");
    }

    #[test]
    fn ss_empty_output_means_no_sessions() {
        assert!(parse_ss_sessions("").is_empty());
        assert!(parse_ss_sessions("\n\n").is_empty());
    }
}
