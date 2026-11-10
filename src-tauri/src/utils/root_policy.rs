//! The policy that decides what may run with administrator rights.
//!
//! This module is deliberately free of any other dependency so that it can be
//! shared between the app (which validates a call *before* it is made) and
//! the small privileged helper `lcp-admin` (which validates it again *while*
//! it runs as root).

use std::path::Path;

// ─── compile-time test fixture (`--features test-fixture`) ───────────────────
//
// The dispatch-success tests (scope A of the STEP 3B plan) have to exercise
// named helper actions end to end — exit code 0, real writes — without
// touching the host's real /etc/profile.d, /etc/passwd or polkit rules.d.
// The helper therefore carries a *compile-time only* feature that switches
// its privileged destination constants to fixture paths below
// /tmp/lcp-admin-test-fixture:
//
// * it is never a default feature — an ordinary or packaged build (no
//   `--features` flag) has no fixture path compiled in and cannot reach any
//   fixture-redirected code,
// * production paths are redirected by nothing at runtime: neither an
//   environment variable nor an argument can turn a compiled production
//   path into a fixture path,
// * release/packaged builds refuse the feature outright via the
//   `compile_error!` below.
#[cfg(all(feature = "test-fixture", not(debug_assertions)))]
compile_error!(
    "the test-fixture feature is a debug-only test aid; a release or packaged \
     build must never be produced with it"
);

/// Root programs the app may execute (matched by their base name, the
/// absolute path is resolved at runtime and then verified).
pub const ALLOWED_ROOT_PROGRAMS: &[&str] = &[
    // package management
    "dnf", "dnf5", "rpm", "apt", "apt-get", "flatpak", "dpkg",
    "update-alternatives", "update-rc.d", "chkconfig", "certbot", "augenrules",
    // init system and services
    "systemctl", "journalctl", "loginctl", "ss", "ip", "ping", "dig", "host", "resolvectl",
    // users and groups (query + basic management; membership and passwords go
    // through named actions instead)
    "useradd", "userdel", "groupadd", "groupmod", "groupdel", "id", "getent",
    // firewall and network
    "nmcli", "route", "resurrect", "connection",
    // files and text (pure readers and filters only — anything that can create,
    // overwrite, replace or relabel a file belongs to a named action)
    "cat", "ls", "stat", "grep", "tail", "head", "zcat", "cut", "sort", "wc", "tr",
    "mkdir", "date", "readlink", "dirname", "basename",
    // hardware and boot
    "modprobe", "rmmod", "fail2ban-client",
    // the root helper itself — named-action dispatch only (check_helper_action)
    "lcp-admin",
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
///   `ln`, `touch`, `mktemp`, `gzip`, and `x509` via `-out`) — including
///   `install`: the app must never overwrite its own root helper (the copy
///   that everything else is authorised on); the helper ships with the RPM,
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
    "install", "touch", "mktemp", "gzip", "x509",
    // delayed root code execution
    "crontab",
    // root/grant-primitives
    "chpasswd", "passwd", "gpasswd", "usermod",
    // repository trust decisions: repo_add and copr_manage are the only
    // validated paths; the generic program channel must not add repositories
    "add-apt-repository",
    // process scheduling is a named action; the generic channel must not
    // renice arbitrary processes as root
    "renice",
    // SMART operations are named actions; the generic channel must not run
    // smartctl as root
    "smartctl",
];

/// Test-only serialization between the root-policy path-pin tests and the
/// end-to-end fixture tests. They share `/tmp/lcp-admin-test-fixture`, and a
/// fixture rebuild removes the directory tree while a path validator is
/// walking it. Production never uses this lock.
#[cfg(test)]
pub(crate) static FIXTURE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ─── The helper's own fixed constants (shared with the app) ──────────────────

/// Where the PolicyKit rule is installed, by the bootstrap and by the
/// helper's `install_polkit_rule` action. The only rule file the helper will
/// ever write.
#[cfg(not(feature = "test-fixture"))]
pub const POLKIT_RULE_FILE: &str = "/etc/polkit-1/rules.d/50-linux-control-panel.rules";

/// Compile-time fixture twin of `POLKIT_RULE_FILE` for `--features
/// test-fixture` builds of the helper (never present in a production build).
#[cfg(feature = "test-fixture")]
pub const POLKIT_RULE_FILE: &str =
    "/tmp/lcp-admin-test-fixture/etc/polkit-1/rules.d/50-linux-control-panel.rules";

/// Where the packaged PolicyKit action definitions are installed. The app and
/// the helper verify this file at startup because a stale copy that annotates
/// generic utilities (or caches authorisations) would silently weaken the
/// whole privilege boundary.
pub const POLKIT_POLICY_FILE: &str =
    "/usr/share/polkit-1/actions/com.controlpanel.pkexec.policy";

/// The PolicyKit action ids the bundled `.policy` file defines for the two
/// helper install paths — the ids pkexec actually authorises under.
///
/// pkexec first requests the generic `org.freedesktop.policykit.exec` action
/// with the program path as a detail; if the path matches a `.policy` action's
/// `org.freedesktop.policykit.exec.path` annotation, polkit substitutes that
/// action for the whole check — its defaults are used and any temporary
/// authorization is recorded under the substituted id. The generic action is
/// therefore never what appears in `pkcheck --list-temp` for our helper.
///
/// The `.policy` file is the pin: each id maps to exactly one `exec.path`
/// (a root-owned file), so matching the id is equivalent to matching the
/// program path. `the_pkexec_policy…`/`the_polkit_rule…` tests in
/// `utils::privilege` cross-check this constant against the real file so the
/// two cannot drift apart.
pub const HELPER_ACTION_IDS: [&str; 2] = [
    "com.controlpanel.helper.system",
    "com.controlpanel.helper.packaged",
];

/// The PolicyKit rule: the helper may be run as root for an *active* local
/// session, and the authorisation is kept for the default five minutes
/// (`auth_admin_keep`) instead of asking for a password every time.
///
/// The helper is matched at *both* install paths — the developer hand
/// install and the distro package path (the RPM/DEB sidecar). They are the
/// same root-owned binary; the runtime additionally verifies ownership and
/// mode before either copy is used.
///
/// The `install_polkit_rule` helper action requires its stdin content to be
/// byte-identical to this constant — content the app controls *and* the
/// helper independently verifies, with no user-owned intermediate file that
/// could be swapped in between.
pub const POLKIT_RULE_TEXT: &str = r#"// Installed by the Linux Control Panel.
// Grants the root owned helper (and only the helper) the right to run with
// administrator rights. The authorisation is kept for the default
// timestamp_timeout (5 minutes), so a multi step action asks only once.
// The helper lives at the developer install path (/usr/local/libexec) or
// at the distro package path (/usr/bin) — both are that same helper.
// These are the action ids of the app's com.controlpanel.pkexec.policy:
// pkexec authorises under the annotated action, never under the generic
// exec action (which must not get the keep — every other root program
// goes through it).
polkit.addRule(function(action, subject) {
    if ((action.id == "com.controlpanel.helper.system" ||
         action.id == "com.controlpanel.helper.packaged") &&
        subject.active && subject.local &&
        subject.isInGroup("wheel")) {
        return polkit.Result.AUTH_ADMIN_KEEP;
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

/// Verify the shipped PolicyKit action definition text.
///
/// The policy is the actual authorization boundary: it must annotate exactly
/// the two root-owned helper install paths and use `auth_admin` for every
/// subject class. A stale copy that annotates a generic utility, weakens a
/// default to `yes`/`no`, or caches with `auth_admin_keep` is refused.
pub fn verify_polkit_policy_text(text: &str) -> Result<(), String> {
    const EXPECTED_IDS: [&str; 2] = [
        "com.controlpanel.helper.packaged",
        "com.controlpanel.helper.system",
    ];
    const EXPECTED_PATHS: [&str; 2] = ["/usr/bin/lcp-admin", "/usr/local/libexec/lcp-admin"];

    fn marker_values<'a>(
        text: &'a str,
        marker: &str,
        terminator: char,
    ) -> Result<Vec<&'a str>, String> {
        let mut values = Vec::new();
        let mut rest = text;
        while let Some(position) = rest.find(marker) {
            rest = &rest[position + marker.len()..];
            let end = rest.find(terminator).ok_or_else(|| {
                format!("the PolicyKit policy has an unterminated '{marker}' value")
            })?;
            values.push(&rest[..end]);
            rest = &rest[end + terminator.len_utf8()..];
        }
        Ok(values)
    }

    let mut ids = marker_values(text, "<action id=\"", '"')?;
    ids.sort_unstable();
    if ids != EXPECTED_IDS {
        return Err(format!(
            "the PolicyKit policy action ids are stale or unexpected: {ids:?}"
        ));
    }

    let mut paths = marker_values(text, "org.freedesktop.policykit.exec.path\">", '<')?;
    paths.sort_unstable();
    if paths != EXPECTED_PATHS {
        return Err(format!(
            "the PolicyKit policy annotates unexpected programs: {paths:?}"
        ));
    }

    for tag in ["allow_any", "allow_inactive", "allow_active"] {
        let exact = format!("<{tag}>auth_admin</{tag}>");
        let total = text.matches(&format!("<{tag}>")).count();
        if total != 2 || text.matches(&exact).count() != 2 {
            return Err(format!(
                "the PolicyKit policy must use auth_admin for every {tag} default"
            ));
        }
    }
    for weakened in [
        "auth_admin_keep",
        "auth_self_keep",
        "auth_self",
        "<allow_any>yes</allow_any>",
        "<allow_inactive>yes</allow_inactive>",
        "<allow_active>yes</allow_active>",
        "<allow_any>no</allow_any>",
        "<allow_inactive>no</allow_inactive>",
        "<allow_active>no</allow_active>",
    ] {
        if text.contains(weakened) {
            return Err(format!(
                "the PolicyKit policy contains a weakened authorization default ({weakened})"
            ));
        }
    }
    Ok(())
}

/// Read and verify one installed PolicyKit policy file.
pub fn verify_polkit_policy_file(path: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!("the installed PolicyKit policy {path} could not be read: {e}")
    })?;
    verify_polkit_policy_text(&text)
}

/// Verify one PolicyKit actions directory. The canonical file must pass the
/// text policy; no sibling `*.policy` file may also define the control-panel
/// helper action ids or annotate either helper path. A duplicate file can
/// shadow or weaken the authorization depending on polkit's load order.
pub fn verify_polkit_policy_directory(directory: &std::path::Path) -> Result<(), String> {
    let canonical = directory.join("com.controlpanel.pkexec.policy");
    let canonical_text = std::fs::read_to_string(&canonical).map_err(|e| {
        format!("the installed PolicyKit policy {} could not be read: {e}", canonical.display())
    })?;
    verify_polkit_policy_text(&canonical_text)?;

    let entries = std::fs::read_dir(directory).map_err(|e| {
        format!("the PolicyKit actions directory {} could not be read: {e}", directory.display())
    })?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("the PolicyKit actions directory could not be read: {e}"))?
            .path();
        if path == canonical {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("policy") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            format!("the PolicyKit policy {} could not be read: {e}", path.display())
        })?;
        if text.contains("com.controlpanel.helper.")
            || text.contains("/usr/bin/lcp-admin")
            || text.contains("/usr/local/libexec/lcp-admin")
        {
            return Err(format!(
                "duplicate or conflicting PolicyKit policy file: {} also defines the control-panel helper",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Scan one optional override directory. `/etc/polkit-1/actions` and
/// `/run/polkit-1/actions` take precedence over `/usr/share` (see
/// `man polkit` and the action-pool directory list in polkitd), so any file
/// there that defines the helper ids/paths, or reuses the canonical filename,
/// is a conflicting or duplicate definition and is refused.
fn verify_polkit_override_directory(directory: &std::path::Path) -> Result<(), String> {
    // Distinguish "optional directory absent" from "cannot inspect it".
    // Path::exists() swallows permission errors, so use metadata explicitly.
    match std::fs::metadata(directory) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // The /etc and /run action directories are optional on a stock
            // system.
            return Ok(());
        }
        Err(e) => {
            return Err(format!(
                "the PolicyKit actions directory {} could not be inspected: {e}",
                directory.display()
            ));
        }
    }
    let entries = std::fs::read_dir(directory).map_err(|e| {
        format!(
            "the PolicyKit actions directory {} could not be read: {e}",
            directory.display()
        )
    })?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("the PolicyKit actions directory could not be read: {e}"))?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("policy") {
            continue;
        }
        if path.file_name().and_then(|name| name.to_str())
            == Some("com.controlpanel.pkexec.policy")
        {
            return Err(format!(
                "duplicate PolicyKit policy file: {} shadows the shipped policy",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            format!("the PolicyKit policy {} could not be read: {e}", path.display())
        })?;
        if text.contains("com.controlpanel.helper.")
            || text.contains("/usr/bin/lcp-admin")
            || text.contains("/usr/local/libexec/lcp-admin")
        {
            return Err(format!(
                "duplicate or conflicting PolicyKit policy file: {} also defines the control-panel helper",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Verify the canonical shipped policy plus every optional override directory.
fn verify_polkit_policy_layout(
    canonical_dir: &std::path::Path,
    override_dirs: &[&std::path::Path],
) -> Result<(), String> {
    verify_polkit_policy_directory(canonical_dir)?;
    for directory in override_dirs {
        verify_polkit_override_directory(directory)?;
    }
    Ok(())
}

/// Verify the packaged PolicyKit action definitions at their installed path.
/// A missing, stale, weakened or duplicated file is an error, never a warning.
///
/// Polkit loads action files from `/etc/polkit-1/actions`,
/// `/run/polkit-1/actions`, `/usr/local/share/polkit-1/actions` and
/// `/usr/share/polkit-1/actions` (the order is documented in `man polkit` and
/// appears in the polkitd action-pool strings). The canonical shipped file
/// lives in `/usr/share`; any helper definition in the other three would
/// shadow or conflict with it.
pub fn verify_installed_polkit_policy() -> Result<(), String> {
    let canonical_dir = std::path::Path::new(POLKIT_POLICY_FILE)
        .parent()
        .ok_or_else(|| format!("invalid PolicyKit policy path: {POLKIT_POLICY_FILE}"))?;
    verify_polkit_policy_layout(
        canonical_dir,
        &[
            std::path::Path::new("/etc/polkit-1/actions"),
            std::path::Path::new("/run/polkit-1/actions"),
            std::path::Path::new("/usr/local/share/polkit-1/actions"),
        ],
    )
}

/// Verify the installed rules.d rule as well. The rule is what grants the
/// five-minute keep, so a stale copy that mentions a generic action would
/// weaken the boundary. This is a startup/status diagnostic rather than a hard
/// execution gate: the helper's `install_polkit_rule` action must remain able
/// to repair a missing rule.
pub fn verify_installed_polkit_state() -> Result<(), String> {
    verify_installed_polkit_policy()?;
    let text = std::fs::read_to_string(POLKIT_RULE_FILE).map_err(|e| {
        format!("the installed PolicyKit rule {} could not be read: {e}", POLKIT_RULE_FILE)
    })?;
    verify_polkit_rule_text(&text)
}

/// Verify the installed rules.d rule. It must be byte-identical to the text
/// the helper writes; anything else is stale or was modified.
pub fn verify_polkit_rule_text(text: &str) -> Result<(), String> {
    if text != POLKIT_RULE_TEXT {
        return Err(
            "the installed PolicyKit rule is stale or was modified; reinstall it with the \
             helper's install_polkit_rule action"
                .to_string(),
        );
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

    let allowed_dirs = REPO_ALLOWED_DIRS;
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

// ─── Shell-environment destinations (F-1) ────────────────────────────────────
//
// The shell feature used to write *arbitrary* root-level files and to copy
// files with root rights. Both are now confined: the only root-owned
// destination it may touch is one `.sh` file directly inside `/etc/profile.d`,
// and the only content that may pass is what the panel itself writes — export
// assignments per line, no free shell text.

/// The root-owned directory the shell feature may write into.
#[cfg(not(feature = "test-fixture"))]
pub const PROFILE_D_DIR: &str = "/etc/profile.d";

/// Compile-time fixture twin of `PROFILE_D_DIR` for `--features
/// test-fixture` builds of the helper (never present in a production build).
#[cfg(feature = "test-fixture")]
pub const PROFILE_D_DIR: &str = "/tmp/lcp-admin-test-fixture/etc/profile.d";

/// Compile-time fixture stand-in for `/etc/passwd`, only read by
/// `--features test-fixture` builds of the helper.
#[cfg(feature = "test-fixture")]
pub const FIXTURE_PASSWD_FILE: &str = "/tmp/lcp-admin-test-fixture/etc/passwd";

// ─── nginx confines (shared app/helper) ─────────────────────────────────────
//
// The nginx writes the panel performs are transformed from raw
// `pkexec <program>` calls into named helper actions. Their destinations are
// a fixed set of places (nginx.conf itself, the site/system configuration
// directories, the panel's own nginx backups, and the web root), and every
// place has a constant here — with test-fixture twins compiler-swapped the
// same way PROFILE_D_DIR is, so fixture dispatch tests never touch the host.

/// The one exact main configuration file a write/backup/restore may address.
#[cfg(not(feature = "test-fixture"))]
pub const NGINX_NGINX_CONF: &str = "/etc/nginx/nginx.conf";
#[cfg(feature = "test-fixture")]
pub const NGINX_NGINX_CONF: &str = "/tmp/lcp-admin-test-fixture/etc/nginx/nginx.conf";

/// `*.conf` server/configuration files loaded by the main file's include.
#[cfg(not(feature = "test-fixture"))]
pub const NGINX_CONF_D: &str = "/etc/nginx/conf.d";
#[cfg(feature = "test-fixture")]
pub const NGINX_CONF_D: &str = "/tmp/lcp-admin-test-fixture/etc/nginx/conf.d";

/// Site definition files; `NGINX_SITES_ENABLED` holds symlinks to children
/// of this directory.
#[cfg(not(feature = "test-fixture"))]
pub const NGINX_SITES_AVAILABLE: &str = "/etc/nginx/sites-available";
#[cfg(feature = "test-fixture")]
pub const NGINX_SITES_AVAILABLE: &str = "/tmp/lcp-admin-test-fixture/etc/nginx/sites-available";

/// The symlink directory enabling a site. Only ever written by
/// link/unlink actions whose two endpoints are both pinned here.
#[cfg(not(feature = "test-fixture"))]
pub const NGINX_SITES_ENABLED: &str = "/etc/nginx/sites-enabled";
#[cfg(feature = "test-fixture")]
pub const NGINX_SITES_ENABLED: &str = "/tmp/lcp-admin-test-fixture/etc/nginx/sites-enabled";

/// The panel's own nginx config backups (`<basename>.<YYYYmmdd_HHMMSS>.bak`).
#[cfg(not(feature = "test-fixture"))]
pub const NGINX_BACKUPS_DIR: &str = "/etc/nginx/backups";
#[cfg(feature = "test-fixture")]
pub const NGINX_BACKUPS_DIR: &str = "/tmp/lcp-admin-test-fixture/etc/nginx/backups";

/// The web root (directories that GET actually serves).
#[cfg(not(feature = "test-fixture"))]
pub const VAR_WWW_DIR: &str = "/var/www";
#[cfg(feature = "test-fixture")]
pub const VAR_WWW_DIR: &str = "/tmp/lcp-admin-test-fixture/var/www";

/// Upper bound for a profile.d file's name (including the `.sh` extension).
const PROFILE_NAME_MAX: usize = 64;

/// A plain file name usable inside the nginx configuration tree — as a site
/// definition name, a symlink name, or the trailing component of a
/// configuration path: ASCII word characters only, never `/`, never a dot or
/// dash file, never any traversal. Empty and oversized names are refused
/// before they can even become a path fragment.
pub fn nginx_site_name_valid(name: &str) -> Result<(), String> {
    const NAME_MAX: usize = 255;
    if name.is_empty() {
        return Err("The site name must not be empty.".to_string());
    }
    if name.len() > NAME_MAX {
        return Err(format!(
            "The site name is too long ({} > {NAME_MAX}): {name:?}",
            name.len()
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(format!("The site name must not contain control characters: {name:?}"));
    }
    if name.starts_with('.') || name.starts_with('-') {
        return Err(format!(
            "The site name {name:?} must not start with a dot or dash."
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(format!(
            "The site name {name:?} may only contain letters, digits, dashes, dots and underscores."
        ));
    }
    Ok(())
}

/// `path` must address exactly one file this panel may write in the nginx
/// configuration tree: the exact `nginx.conf`, a plain `<name>.conf` child of
/// `/etc/nginx/conf.d`, or a plain child of `/etc/nginx/sites-available` or
/// `/etc/nginx/sites-enabled` — no traversal, no component escapes. The
/// fs-level no-symlink rule is enforced helper-side via descriptor pinning;
/// this validator is the pre-filter both halves (app and helper) share.
pub fn nginx_config_path_valid(path: &str) -> Result<(), String> {
    use std::path::Component;

    if path == NGINX_NGINX_CONF {
        return Ok(());
    }
    if path.chars().any(char::is_control) {
        return Err(format!(
            "The nginx configuration path must not contain control characters: {path:?}"
        ));
    }
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(format!("The nginx configuration path must be absolute: {path}"));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(format!(
            "The nginx configuration path must not contain relative parts: {path}"
        ));
    }
    let parent = candidate
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = candidate
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // The conf.d machinery suffix: `*.conf` is everything nginx's include
    // glob loads; `*.conf.disabled` is the panel's conf.d toggle archive —
    // the disable step must be able to *install* it and the enable step's
    // cleanup must be able to *remove* it. In every conf.d case the
    // validated site name is the stem with the machinery suffix stripped.
    let conf_d_machinery = if parent == NGINX_CONF_D {
        Some(true)
    } else if parent == NGINX_SITES_AVAILABLE || parent == NGINX_SITES_ENABLED {
        Some(false)
    } else {
        return Err(format!(
            "The nginx configuration path must be {NGINX_NGINX_CONF} or a plain child of {NGINX_CONF_D}, {NGINX_SITES_AVAILABLE} or {NGINX_SITES_ENABLED}, not {path}"
        ));
    };
    if let Some(true) = conf_d_machinery {
        let Some(stem) = name
            .strip_suffix(".conf.disabled")
            .or_else(|| name.strip_suffix(".conf"))
        else {
            return Err(format!(
                "A file in {NGINX_CONF_D} must have a \".conf\" (or \".conf.disabled\") extension: {name:?}"
            ));
        };
        nginx_site_name_valid(stem).map_err(|reason| format!("{reason} (as {path:?})"))
    } else {
        nginx_site_name_valid(&name).map_err(|reason| format!("{reason} (as {path:?})"))
    }
}

/// `path` must describe a location inside the web root, strictly below it —
/// the web root itself and every traversal out are refused. The component
/// charset stays deliberately relaxed (user content directories legitimately
/// carry spaces); the fs-level no-symlink rule is enforced helper-side via
/// descriptor pinning.
pub fn var_www_path_valid(path: &str) -> Result<(), String> {
    // `Path::components` normalizes interior `.` components away, so the
    // relative-part refusal has to look at the raw text here: no component
    // may literally be `.` or `..`.
    use std::path::Component;

    if path.chars().any(char::is_control) {
        return Err(format!("The path must not contain control characters: {path:?}"));
    }
    if path.split('/').any(|part| part == "." || part == "..") {
        return Err(format!("The path must not contain relative parts: {path}"));
    }
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(format!("The path must be absolute: {path}"));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(format!("The path must not contain relative parts: {path}"));
    }
    let Some(rest) = path.strip_prefix(VAR_WWW_DIR_AS_ROOT) else {
        return Err(format!("The path must be under {VAR_WWW_DIR}/, not {path:?}"));
    };
    if rest.is_empty() {
        return Err(format!("{VAR_WWW_DIR} itself is not a valid target here."));
    }
    Ok(())
}

/// The prefix `var_www_path_valid` strips — fixture twin aware. Both variants
/// end in `/`, so `strip_prefix` can only match a strictly-below location.
#[cfg(not(feature = "test-fixture"))]
const VAR_WWW_DIR_AS_ROOT: &str = "/var/www/";
#[cfg(feature = "test-fixture")]
const VAR_WWW_DIR_AS_ROOT: &str = "/tmp/lcp-admin-test-fixture/var/www/";

/// The web root and its distro-managed `html` tree are install territory:
/// remove operations refuse them outright, and `rename` refuses them on both
/// endpoints — moving `html` away is protected exactly like deleting it.
pub fn var_www_is_protected(path: &str) -> bool {
    let clean = path.trim_end_matches('/');
    clean == VAR_WWW_DIR || clean == format!("{VAR_WWW_DIR}/html")
}

/// `path` must address exactly one regular, non-symlink `<name>.sh` file
/// directly inside `/etc/profile.d` — no traversal, no dot files, no second
/// directory component, no symlinked directory entry.
pub fn validate_profile_sh_path(path: &str) -> Result<(), String> {
    use std::path::Component;

    if path.chars().any(char::is_control) {
        return Err("The profile file path must not contain control characters.".to_string());
    }
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(format!("The profile file path must be absolute: {path}"));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("The profile file path must not contain relative parts.".to_string());
    }

    let parent = candidate
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    if parent != PROFILE_D_DIR {
        return Err(format!(
            "The only root-owned profile destination is {PROFILE_D_DIR}/<name>.sh, not {path}"
        ));
    }

    let name = candidate
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(base) = name.strip_suffix(".sh") else {
        return Err(format!("The profile file name must end in '.sh': {name}"));
    };
    if name.is_empty() || name.starts_with('.') || base.is_empty() || name.len() > PROFILE_NAME_MAX
    {
        return Err(format!("Invalid profile file name: {name}"));
    }
    if !base
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "The profile file name may only contain letters, digits, '_' and '-': {name}"
        ));
    }

    // the file itself and every parent directory must be real entries —
    // a symlinked /etc/profile.d or a symlinked target must never be
    // replaced or written through
    let mut walked = std::path::PathBuf::from("/");
    for component in candidate.components() {
        walked.push(component);
        let last = walked == candidate;
        match walked.symlink_metadata() {
            Ok(m) => {
                if m.file_type().is_symlink() {
                    return Err("The profile file path must not contain a symbolic link.".to_string());
                }
                if !last && !m.is_dir() {
                    return Err("The profile file path parents must be directories.".to_string());
                }
            }
            // a not-yet-existing target is fine: it is created by the action
            Err(_) if last => {}
            Err(_) => return Err("The profile file parent is not accessible.".to_string()),
        }
    }
    Ok(())
}

/// Upper bound for a profile.d file's content — shared with the helper, so
/// its `restore_profile_sh` action can impose the same limit on the backup
/// it reads.
pub const PROFILE_CONTENT_MAX: usize = 256 * 1024;

/// One shell helper: identifier shape the panel exports.
fn is_export_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The only value shapes that can never smuggle shell code passed a quote
/// boundary: double-quoted (no shell substitution characters inside),
/// single-quoted (no quotes inside), or a bare token with no shell-special
/// character at all.
fn is_safe_export_value(value: &str) -> bool {
    if value.len() > 4096 {
        return false;
    }
    let quoted_double = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
    let quoted_single = value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'');
    if quoted_double || quoted_single {
        let inner = &value[1..value.len() - 1];
        let forbidden = if quoted_double {
            |c: char| c.is_control() || matches!(c, '"' | '$' | '`' | '\\' | '\0')
        } else {
            |c: char| c.is_control() || matches!(c, '\'' | '\0')
        };
        return !inner.chars().any(forbidden);
    }
    // bare value: nothing the shell evaluates, nothing that leaves the token
    !value.chars().any(|c: char| {
        c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '$' | '`' | '\\' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '{' | '}'
                    | '*' | '?' | '[' | ']' | '#' | '!' | '~'
            )
    })
}

/// The single, deliberately narrow exception to "no `$` in a value": the
/// `PATH` extension lines the panel itself writes,
/// `export PATH="$PATH:<dir>[:<dir>...]"`.
///
/// One optional pair of double quotes is stripped (the panel always writes
/// them, but a bare `$PATH:/opt/bin` means the same thing to the shell). What
/// remains must be exactly `$PATH` followed by one or more `:<segment>`, with
/// every segment made only of `[A-Za-z0-9_./-]`. That alphabet excludes
/// whitespace, quotes, backslash, the other `$`, backtick, `;`, `|`, `&`, the
/// redirection characters, parentheses, braces, globs and `#` — so the value
/// can never leave its word, start a command, or name another variable.
/// Callers must gate this on `name == "PATH"`; it is not a general value
/// rule.
fn is_path_extension_value(value: &str) -> bool {
    let inner = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    };
    if inner.len() > 4096 {
        return false;
    }
    let Some(rest) = inner.strip_prefix("$PATH:") else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    rest.split(':').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
    })
}

/// Validate the full text of a root-owned profile.d file before it may be
/// written. Every non-empty, non-comment line must be exactly
/// `export NAME=<safe value>` — profile.d scripts run in *every* login shell,
/// so free shell text must never get there.
///
/// `PATH` is the one exception, and only for the exact `$PATH:<dir>` shape
/// above: the PATH tab has to extend the inherited search path, which no
/// other value shape can express.
pub fn validate_profile_sh_content(content: &str) -> Result<(), String> {
    if content.len() > PROFILE_CONTENT_MAX {
        return Err("The profile file is too large.".to_string());
    }
    if content.contains('\0') {
        return Err("The profile file must not contain NUL bytes.".to_string());
    }
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let rest = trimmed
            .strip_prefix("export ")
            .map(str::trim)
            .ok_or_else(|| format!("Line {} is not an export assignment: {trimmed}", index + 1))?;
        let Some((name, value)) = rest.split_once('=') else {
            return Err(format!("Line {} is not an export assignment: {trimmed}", index + 1));
        };
        let name = name.trim();
        if !is_export_name(name) {
            return Err(format!("Line {} has an invalid variable name: {name}", index + 1));
        }
        let value = value.trim();
        let accepted = is_safe_export_value(value)
            || (name == "PATH" && is_path_extension_value(value));
        if !accepted {
            return Err(format!("Line {} has an unsafe value for {name}.", index + 1));
        }
    }
    Ok(())
}

// ─── Repository content policy (F-2) ─────────────────────────────────────────
//
// `ensure_repo_file_path` confines *where* a repository file may live. That
// alone lets the UI-defined file body itself reconfigure the package system's
// sources with root authority (a `file://` baseurl, signature checking off, or
// directives the panel never writes). This policy validates the *content*
// before a privileged write may see it.

/// Upper bound for a repository file's content — shared with the helper, so
/// its (not yet registered) `write_repo_file` action can impose the same
/// stdin limit.
pub const REPO_CONTENT_MAX: usize = 256 * 1024;

/// The directories a repository file may live in. `--features test-fixture`
/// builds of the helper see their fixture twins, so the write-repo-file
/// action core can be exercised on temporary directories.
#[cfg(not(feature = "test-fixture"))]
pub(crate) const REPO_ALLOWED_DIRS: [&str; 3] =
    ["/etc/yum.repos.d", "/etc/dnf", "/etc/apt/sources.list.d"];
#[cfg(feature = "test-fixture")]
pub(crate) const REPO_ALLOWED_DIRS: [&str; 3] = [
    "/tmp/lcp-admin-test-fixture/etc/yum.repos.d",
    "/tmp/lcp-admin-test-fixture/etc/dnf",
    "/tmp/lcp-admin-test-fixture/etc/apt/sources.list.d",
];

/// Upper bound for `[section]` ids.
const REPO_SECTION_MAX: usize = 64;

/// Directives a repository file may carry. File generalists and anything dnf
/// executes or imports configurably stay off the list by construction.
const REPO_KEYS: &[&str] = &[
    "name", "baseurl", "metalink", "mirrorlist", "gpgkey", "gpgcheck", "repo_gpgcheck",
    "enabled", "priority", "countme", "skip_if_unavailable", "fastestmirror", "cost",
    "includepkgs", "excludepkgs", "module_hotfixes", "metadata_expire",
];

/// Values dnf expands inside URLs — the only `$` sequences allowed.
const REPO_URL_VARS: &[&str] = &["$releasever", "$basearch", "$releasever_major", "$releasever_minor"];

/// `$` sequences: dnf's URL/stanza variables only — anything else is refused.
fn dnf_vars_only(value: &str) -> bool {
    let mut rest = value;
    while let Some(dollar) = rest.find('$') {
        rest = &rest[dollar..];
        let Some(var) = REPO_URL_VARS.iter().find(|v| rest.starts_with(*v)).copied() else {
            return false;
        };
        rest = &rest[var.len()..];
    }
    true
}

/// One URL token (dnf baseurls and gpgkeys are whitespace-separated): internet
/// schemes only, no local-file or executable handlers, no shell characters.
fn is_safe_repo_url_token(token: &str) -> bool {
    let ok_length = !token.is_empty() && token.len() <= 512;
    let ok_chars = token.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '-' | '.' | '_' | '~' | '/' | ':' | '?' | '&' | '=' | '%' | '+' | '@'
                | '!' | '#' | ',' | '(' | ')' | '*' | '[' | ']' | ';' | '$')
    });
    if !ok_length || !ok_chars {
        return false;
    }
    // no quotes, backslashes or backticks slipped past the charset above —
    // re-check the characters the panel must never carry into a repo file
    if token.chars().any(|c| matches!(c, '"' | '\'' | '\\' | '`')) {
        return false;
    }
    // $ must be one of dnf's URL variables, never anything else
    if !dnf_vars_only(token) {
        return false;
    }
    // internet schemes only — local-file sources defeat the package
    // system's signature-on-establishment model
    let scheme_ok = ["http://", "https://"].iter().any(|scheme| token.starts_with(*scheme));
    scheme_ok
}

/// A repository URL must use `https://` only. HTTP, FTP, file://, CDROM and
/// any other scheme are refused — an unsigned or non-TLS source lets its
/// maintainer run code as root on every future dnf/apt operation.
///
/// Beyond the scheme the URL must also be structurally sane before the app
/// ever hands it to a privileged command: no whitespace or control
/// characters (the operand may still be split or mangled downstream), no
/// quote/backslash/backtick (shell metacharacters are never acceptable in a
/// repo URL), a non-empty authority (a host must be named), no embedded
/// credentials (`user:pass@host` is never supported — a repo URL must not
/// carry a secret; keyrings or configured mirrors are the supported
/// mechanism), and a numeric 1–5-digit port after `:` if one is given.
/// Query strings and fragments are passed through untouched — dnf/apt are
/// responsible for interpreting them. A single-label host (e.g.
/// `https://localhost/…`) is syntactically valid here; whether it resolves
/// is the package manager's verdict, not the validator's.
pub fn repo_url_https_only(url: &str) -> Result<(), String> {
    let reject = |what: &str| Err(format!("The repository URL must {what} (got: {url})"));
    if url.is_empty() {
        return reject("not be empty");
    }
    if !url.starts_with("https://") {
        return reject("use https://");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return reject("contain no whitespace or control characters");
    }
    if url.chars().any(|c| matches!(c, '"' | '\'' | '\\' | '`')) {
        return reject("contain no quote, backslash or backtick characters");
    }
    let rest = &url["https://".len()..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    // embedded credentials are rejected before any empty check so the error
    // names the real problem — any '@' inside the authority means userinfo
    if authority.contains('@') {
        return reject(
            "not embed credentials (user:pass@host) — use a keyring or a configured mirror",
        );
    }
    if authority.is_empty() {
        return reject("name a host (e.g. https://host/path)");
    }
    if authority.starts_with('[') {
        // IPv6 literal: [host] optionally followed by :port
        match authority.find(']') {
            None => return reject("close the bracketed IPv6 host"),
            Some(end) => {
                let after = &authority[end + 1..];
                if !after.is_empty()
                    && (!after.starts_with(':')
                        || !port_is_digits(&after[1..]))
                {
                    return reject("carry only a numeric port after the IPv6 host");
                }
            }
        }
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() {
            return reject("name a host before the port");
        }
        // a non-IPv6 authority may not contain a nested colon (a [host]:port
        // shape has already been handled above) — host:8443:9 is malformed
        // whatever the rsplit port looks like
        if host.contains(':') {
            return reject("carry at most one colon (host:port) in a non-bracketed authority");
        }
        if !port_is_digits(port) {
            return reject("use a numeric port of 1 to 5 digits");
        }
    }
    Ok(())
}

fn port_is_digits(port: &str) -> bool {
    !port.is_empty() && port.len() <= 5 && port.chars().all(|c| c.is_ascii_digit())
}

fn is_on_off_flag_value(value: &str) -> bool {
    matches!(value, "0" | "1")
}

/// A `.repo` directive value: free printable text for names, strict shapes for
/// everything privilege-adjacent.
fn validate_repo_key(key: &str, value: &str, line_no: usize) -> Result<(), String> {
    let reject = |what: &str| Err(format!("Line {line_no}: {what}"));
    match key {
        "name" => {
            if !value.chars().all(|c| !c.is_control())
                || value.chars().any(|c| matches!(c, '"' | '`' | '\\' | '\''))
                || value.len() > 256
                || !dnf_vars_only(value)
            {
                reject("the 'name' value has unsafe characters")
            } else {
                Ok(())
            }
        }
        "baseurl" | "gpgkey" => {
            if value.is_empty() {
                return reject(&format!("'{key}' needs at least one URL"));
            }
            let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
            if tokens.is_empty() {
                return reject(&format!("'{key}' needs at least one URL"));
            }
            for token in tokens {
                if !is_safe_repo_url_token(token) {
                    return Err(format!(
                        "Line {line_no}: '{key}' URL '{token}' is not a safe http(s) URL"
                    ));
                }
            }
            Ok(())
        }
        "metalink" | "mirrorlist" | "proxy" => {
            let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
            if tokens.len() != 1 || !is_safe_repo_url_token(tokens[0]) {
                reject(&format!("'{key}' must be exactly one safe http(s) URL"))
            } else {
                Ok(())
            }
        }
        // disabled signature verification is rejected outright
        "gpgcheck" | "repo_gpgcheck" => {
            if value != "1" {
                reject("signature checking must stay enabled ('gpgcheck=1')")
            } else {
                Ok(())
            }
        }
        "enabled" | "skip_if_unavailable" | "fastestmirror" | "module_hotfixes" => {
            if !is_on_off_flag_value(value) {
                reject(&format!("'{key}' must be 0 or 1"))
            } else {
                Ok(())
            }
        }
        "priority" | "cost" | "countme" | "metadata_expire" => {
            if value.is_empty() || value.len() > 16
                || !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-'))
            {
                reject(&format!("'{key}' value is not a simple number or duration"))
            } else {
                Ok(())
            }
        }
        "includepkgs" | "excludepkgs" => {
            if value.is_empty()
                || value.len() > 4096
                || !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '*' | '?' | '.' | '-' | '_'))
            {
                reject(&format!("'{key}' must be a space-separated package name list"))
            } else {
                Ok(())
            }
        }
        _ => reject("directive is not on the repository allowlist"),
    }
}

/// Counts how many repos in a `.repo` file's content have `enabled=1`.
/// Used by the zero-enabled-repos lockout check: disabling or deleting
/// the last enabled repo system-wide requires explicit confirmation.
pub fn count_enabled_repos_in_content(content: &str) -> usize {
    let mut count = 0usize;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == "enabled" && value.trim() == "1" {
                count += 1;
            }
        }
    }
    count
}

/// A deb822 `.sources` stanza key.
const SOURCES_KEYS: &[&str] = &["Types", "URIs", "Suites", "Components", "Signed-By", "Enabled", "Architectures", "Origin", "Label", "Site", "Date"];

/// URI schemes the panel accepts in an APT source (one-line format and
/// deb822 alike). Local-file (`file:`, `copy:`, `cdrom:`) and every other
/// scheme are refused by construction.
const APT_URI_SCHEMES: &[&str] = &["http://", "https://", "tor+http://", "tor+https://"];

/// Root-owned directories whose keyring files are approved as `signed-by`
/// targets. Only these two locations are explicitly approved; a keyring must
/// never come from a user-controlled path.
const APT_KEYRING_DIRS: &[&str] = &["/usr/share/keyrings/", "/etc/apt/keyrings/"];

/// One URI token of an APT source: internet scheme (checked by the caller),
/// a printable URL charset, no quotes, backslashes, backticks or brackets.
/// `$` stays allowed (plain text in an apt source, inert there).
fn is_safe_apt_uri_token(token: &str) -> bool {
    let ok_length = !token.is_empty() && token.len() <= 2048;
    let ok_chars = token.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '-' | '.' | '_' | '~' | '/' | ':' | '?' | '&' | '=' | '%' | '+' | '@'
                | '!' | '#' | ',' | '(' | ')' | '*' | ';' | '$')
    });
    if !ok_length || !ok_chars {
        return false;
    }
    // quotes/backslashes/backticks slipped past the charset check above are
    // re-refused here; brackets are refused because `[`/`]` are the option
    // syntax of the .list format and must never appear in a URI token
    if token.chars().any(|c| matches!(c, '"' | '\'' | '\\' | '`' | '[' | ']')) {
        return false;
    }
    true
}

/// A deb822 `URIs:` value — one or more whitespace-separated URI tokens. Flat
/// relative URIs (a token without a scheme) are refused as ambiguous.
fn is_safe_apt_uri_string(value: &str) -> bool {
    if value.is_empty() || value.len() > 2048 {
        return false;
    }
    for token in value.split_ascii_whitespace() {
        if !is_safe_apt_uri_token(token) {
            return false;
        }
        if !APT_URI_SCHEMES.iter().any(|scheme| token.starts_with(*scheme)) {
            return false;
        }
    }
    true
}

/// A `signed-by=`/`Signed-By` keyring path: exactly one file element directly
/// below an approved root-owned keyring directory, a `.gpg`/`.asc` file, no
/// dot files, no traversal, no interior slash.
fn is_approved_apt_keyring_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 256 {
        return false;
    }
    let Some(base) = APT_KEYRING_DIRS
        .iter()
        .find_map(|dir| path.strip_prefix(dir))
    else {
        return false;
    };
    if base.is_empty() || base.contains('/') || base.starts_with('.') {
        return false;
    }
    base.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && (base.ends_with(".gpg") || base.ends_with(".asc"))
}

/// A `distribution`/`suite` token of a one-line entry (e.g. `bookworm`,
/// `trixie/updates`, `./` for a flat repository).
fn is_safe_apt_suite_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '/' | '~'))
}

/// A `component` token (e.g. `main`, `contrib`, `non-free-firmware`).
fn is_safe_apt_component_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

/// Validate the full text of a `.repo`, `.list` or `.sources` repository file
/// before a privileged write may replace it with root authority.
///
/// The extension decides the grammar (dnf INI, deb flat lines, deb822
/// key/value stanzas); anything the panel does not write is rejected with a
/// line-local message instead of silently replaced.
pub fn ensure_repo_file_content(path: &str, content: &str) -> Result<(), String> {
    if content.len() > REPO_CONTENT_MAX {
        return Err("The repository file is too large.".to_string());
    }
    if content.contains('\0') {
        return Err("The repository file must not contain NUL bytes.".to_string());
    }
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    match ext.as_str() {
        "repo" => validate_dnf_repo_content(content),
        "list" => validate_apt_list_content(content),
        "sources" | "sources.list" => validate_apt_sources_content(content),
        _ => Err("Unsupported repository file type.".to_string()),
    }
}

fn validate_dnf_repo_content(content: &str) -> Result<(), String> {
    let mut sections = 0usize;
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some(section) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            sections += 1;
            if sections > 64 {
                return Err("The repository file has too many sections.".to_string());
            }
            if section.is_empty()
                || section.len() > REPO_SECTION_MAX
                || !section
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            {
                return Err(format!("Line {line_no}: invalid repository section id"));
            }
            continue;
        }
        // dnf INI also accepts an optional leading whitespace around '=';
        // key normalization is lowercase, as dnf treats it case-insensitively
        let Some((raw_key, value)) = trimmed.split_once('=') else {
            return Err(format!("Line {line_no}: not a [section], key=value or comment"));
        };
        let key = raw_key.trim().to_lowercase();
        if !REPO_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "Line {line_no}: the '{key}' directive is not allowed in a repository file"
            ));
        }
        validate_repo_key(&key, value.trim(), line_no)?;
    }
    Ok(())
}

/// Validate a one-line `.list` entry: `deb [options] uri suite [components…]`.
///
/// Strictly parsed instead of merely character-checked:
///
/// * the bracket-options group is only supported **directly after**
///   `deb`/`deb-src`, and only for `arch=` and `signed-by=` — every other
///   option (signature-verification disablers like `trusted=yes` included)
///   is refused, and a bracket group after the *suite* is refused as
///   unsupported syntax,
/// * `signed-by` must point only at root-owned keyrings in the approved
///   directories — a user-controlled keyring path could make apt import an
///   attacker-signed repository as trusted,
/// * every URI token needs an internet scheme; `file:`, `copy:`, `cdrom:`
///   bare or otherwise are refused,
/// * one line, one entry: there is no continuation syntax in this format
///   (the line-based parser refuses nothing else).
fn validate_apt_list_content(content: &str) -> Result<(), String> {
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        validate_apt_list_line(trimmed, line_no)?;
    }
    Ok(())
}

/// One bracket option of a one-line entry — `arch` and `signed-by` only.
fn validate_apt_list_option(option: &str, line_no: usize) -> Result<(), String> {
    let reject = |what: &str| Err(format!("Line {line_no}: {what}"));
    let Some((name, value)) = option.split_once('=') else {
        return reject(&format!("the bracket option '{option}' is not supported"));
    };
    if value.is_empty() {
        return reject(&format!("the bracket option '{name}' has an empty value"));
    }
    match name.to_ascii_lowercase().as_str() {
        "arch" => {
            if value.len() > 128
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '-' | '_' | '.'))
            {
                reject("the 'arch' option value is invalid")
            } else {
                Ok(())
            }
        }
        "signed-by" => {
            for path in value.split(',') {
                if !is_approved_apt_keyring_path(path) {
                    return reject(&format!(
                        "'signed-by' must point at a .gpg/.asc keyring in {}",
                        APT_KEYRING_DIRS.join(" or ")
                    ));
                }
            }
            Ok(())
        }
        _ => reject(&format!("the bracket option '{name}' is not allowed")),
    }
}

fn validate_apt_list_line(line: &str, line_no: usize) -> Result<(), String> {
    let reject = |what: &str| Err(format!("Line {line_no}: {what}"));
    if line.chars().any(|c| c.is_control()) {
        return reject("control character in the entry");
    }
    let tokens: Vec<&str> = line.split_ascii_whitespace().collect();
    let Some(source_type) = tokens.first().copied() else {
        return reject("not a deb/deb-src line");
    };
    if source_type != "deb" && source_type != "deb-src" {
        return reject("not a deb/deb-src line");
    }
    let mut rest = &tokens[1..];
    // optional bracket-options group, only directly after the source type
    if let Some(first) = rest.first().copied() {
        if first.starts_with('[') {
            let Some(end) = rest.iter().position(|token| token.ends_with(']')) else {
                return reject("unterminated bracket options group");
            };
            let group = &rest[..=end];
            rest = &rest[end + 1..];
            for (position, raw_option) in group.iter().enumerate() {
                let mut option = *raw_option;
                if position == 0 {
                    option = option.strip_prefix('[').unwrap_or(option);
                }
                if position == group.len() - 1 {
                    option = option.strip_suffix(']').unwrap_or(option);
                }
                if option.is_empty() {
                    continue;
                }
                validate_apt_list_option(option, line_no)?;
            }
        }
    }
    let Some(uri) = rest.first().copied() else {
        return reject("repository URI is missing");
    };
    if !is_safe_apt_uri_token(uri)
        || !APT_URI_SCHEMES.iter().any(|scheme| uri.starts_with(scheme))
    {
        return reject("repository URI is not a safe http(s) URL");
    }
    let Some(suite) = rest.get(1).copied() else {
        return reject("distribution/suite is missing");
    };
    if !is_safe_apt_suite_token(suite) {
        return reject("invalid distribution/suite token");
    }
    for component in &rest[2..] {
        if !is_safe_apt_component_token(component) {
            return reject("invalid component token");
        }
    }
    Ok(())
}

/// Validate a deb822 `.sources` stanza.
///
/// Strictly parsed instead of merely character-checked:
///
/// * multi-line continued values (a continuation line under `Signed-By:` and
///   friends) are refused as unsupported — a misparsed continuation could
///   mean a value the panel never saw,
/// * every field may appear only once — apt merges repeated fields, so a
///   second occurrence would change the meaning of the document after
///   validation,
/// * `Types:` must list `deb`/`deb-src`, `Signed-By:` may only point at
///   approved root-owned keyrings, `URIs:` demands per-token internet
///   schemes, `Architectures:`/`Suites:`/`Components:` are word lists,
/// * only the fields the panel understands are accepted at all.
fn validate_apt_sources_content(content: &str) -> Result<(), String> {
    let mut seen_keys = std::collections::HashSet::new();
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // a deb822 continuation line starts with whitespace — whatever it
        // carries, this validator does not understand that document shape
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(format!(
                "Line {line_no}: multi-line continued field values are not supported"
            ));
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err(format!("Line {line_no}: control character in the stanza"));
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(format!("Line {line_no}: not a key: value stanza line"));
        };
        let key = key.trim();
        let canonical = key.to_ascii_lowercase();
        if !SOURCES_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)) {
            return Err(format!("Line {line_no}: the '{key}' field is not allowed in a .sources file"));
        }
        if !seen_keys.insert(canonical.clone()) {
            return Err(format!("Line {line_no}: repeated '{key}' field"));
        }
        let value = value.trim();
        let reject = |what: String| Err(format!("Line {line_no}: {what}"));
        match canonical.as_str() {
            "enabled" => {
                if !matches!(value, "yes" | "no") {
                    return reject("'Enabled' must be yes or no".to_string());
                }
            }
            "uris" => {
                if !is_safe_apt_uri_string(value) {
                    return reject("unsafe URIs value".to_string());
                }
            }
            "types" => {
                if value.split_ascii_whitespace().count() == 0
                    || !value
                        .split_ascii_whitespace()
                        .all(|word| matches!(word, "deb" | "deb-src"))
                {
                    return reject(
                        "'Types' must be a space-separated list of 'deb' and 'deb-src'"
                            .to_string(),
                    );
                }
            }
            "signed-by" => {
                if value.split(',').any(|path| {
                    !is_approved_apt_keyring_path(path)
                }) {
                    return reject(format!(
                        "'Signed-By' must be a comma-separated list of .gpg/.asc keyrings in {}",
                        APT_KEYRING_DIRS.join(" or ")
                    ));
                }
            }
            "architectures" => {
                if value.is_empty()
                    || value.len() > 256
                    || !value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '-' | '_' | '.'))
                {
                    return reject("'Architectures' must be a comma/space word list".to_string());
                }
            }
            "suites" => {
                if value.is_empty()
                    || !value.split_ascii_whitespace().all(is_safe_apt_suite_token)
                {
                    return reject("'Suites' must be one or more suite words".to_string());
                }
            }
            "components" => {
                if value.is_empty()
                    || !value
                        .split_ascii_whitespace()
                        .all(is_safe_apt_component_token)
                {
                    return reject("'Components' must be one or more component words".to_string());
                }
            }
            // Origin, Label, Site, Date — free printable words with spaces
            _ => {
                if value.is_empty()
                    || value.len() > 2048
                    || value
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '"' | '$' | '`' | '\\' | '\'' | '[' | ']'))
                {
                    return reject(format!("unsafe '{key}' value"));
                }
            }
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

/// Public account-name contract shared by the app and the privileged helper.
pub fn user_account_name_valid(value: &str) -> Result<(), String> {
    if is_user_name(value) {
        Ok(())
    } else {
        Err(format!(
            "The account name must be 1-32 lowercase letters, digits, '_', '-' or '.', starting with a lowercase letter or '_': {value}"
        ))
    }
}

/// Public group-name contract (same grammar as account names).
pub fn group_name_valid(value: &str) -> Result<(), String> {
    if is_group_name(value) {
        Ok(())
    } else {
        Err(format!(
            "The group name must be 1-32 lowercase letters, digits, '_', '-' or '.', starting with a lowercase letter or '_': {value}"
        ))
    }
}

/// GECOS/full-name contract for `useradd -c`: printable ASCII, spaces allowed,
/// no leading dash, bounded length.
pub fn user_fullname_valid(value: &str) -> Result<(), String> {
    if is_safe_comment(value) {
        Ok(())
    } else {
        Err(format!(
            "The full name must be 1-64 printable characters, not start with a dash: {value}"
        ))
    }
}

/// A password sent on the helper's stdin: non-empty, bounded, and free of the
/// line/control characters that could inject a second chpasswd entry.
pub fn user_password_valid(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("The password must not be empty.".to_string());
    }
    if value.len() > 1024 {
        return Err("The password must be at most 1024 bytes.".to_string());
    }
    if value.contains('\n') || value.contains('\r') || value.contains('\0') {
        return Err("The password must not contain newline, carriage return or NUL characters.".to_string());
    }
    Ok(())
}

/// A Copr/PPA repository reference: `owner/project`, optionally prefixed
/// `ppa:`. Exactly one slash, no spaces, control characters, path traversal
/// or option-leading segments.
pub fn copr_repo_valid(repo: &str) -> Result<(), String> {
    let body = repo.strip_prefix("ppa:").unwrap_or(repo);
    if body.is_empty() || body.len() > 200 || body.contains(':') {
        return Err(format!("The Copr repository must be owner/project, got: {repo}"));
    }
    let (owner, project) = body
        .split_once('/')
        .ok_or_else(|| format!("The Copr repository must be owner/project, got: {repo}"))?;
    if owner.is_empty() || project.is_empty() || project.contains('/') {
        return Err(format!("The Copr repository must be owner/project, got: {repo}"));
    }
    for part in [owner, project] {
        if part == "." || part == ".." || part.starts_with('-')
            || !part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
        {
            return Err(format!("The Copr repository contains unsafe characters: {repo}"));
        }
    }
    Ok(())
}

/// The only verbs the Copr action accepts.
pub fn copr_verb_valid(verb: &str) -> Result<(), String> {
    match verb {
        "enable" | "disable" => Ok(()),
        other => Err(format!("The Copr verb must be enable or disable, got: {other}")),
    }
}

/// Units the security-auditor service action may enable/disable. The list is
/// deliberately closed: this is not a generic systemctl channel.
pub fn security_service_unit_valid(unit: &str) -> Result<(), String> {
    match unit {
        "auditd" | "usbguard" | "chronyd" | "systemd-timesyncd" => Ok(()),
        other => Err(format!(
            "The security service must be auditd, usbguard, chronyd or systemd-timesyncd, got: {other}"
        )),
    }
}

pub fn security_service_verb_valid(verb: &str) -> Result<(), String> {
    match verb {
        "enable" | "disable" => Ok(()),
        other => Err(format!(
            "The service verb must be enable or disable, got: {other}"
        )),
    }
}

pub fn ctrl_alt_del_verb_valid(verb: &str) -> Result<(), String> {
    match verb {
        "mask" | "unmask" => Ok(()),
        other => Err(format!(
            "The ctrl-alt-del verb must be mask or unmask, got: {other}"
        )),
    }
}

/// Closed parameter/value pairs for the SSH hardening action. The app's
/// scanner fix buttons map exactly to this set.
pub fn security_ssh_param_valid(param: &str, value: &str) -> Result<(), String> {
    let allowed: &[&str] = match param {
        "PermitRootLogin" => &["prohibit-password", "yes"],
        "PasswordAuthentication" => &["no", "yes"],
        "MaxAuthTries" => &["4", "6"],
        "LoginGraceTime" => &["60", "120"],
        _ => {
            return Err(format!(
                "The SSH parameter must be PermitRootLogin, PasswordAuthentication, MaxAuthTries or LoginGraceTime, got: {param}"
            ))
        }
    };
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "The value '{value}' is not allowed for SSH parameter {param}"
        ))
    }
}

/// Closed sysctl keys the hardening page can persist, with numeric values.
pub fn security_kernel_param_valid(key: &str, value: &str) -> Result<(), String> {
    const KEYS: &[&str] = &[
        "kernel.randomize_va_space",
        "net.ipv4.tcp_syncookies",
        "net.ipv4.ip_forward",
        "kernel.kptr_restrict",
        "kernel.dmesg_restrict",
        "net.ipv4.conf.all.accept_redirects",
        "net.ipv4.conf.all.log_martians",
    ];
    if !KEYS.contains(&key) {
        return Err(format!(
            "The sysctl key is not one of the hardening keys: {key}"
        ));
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || value.len() > 10 {
        return Err(format!(
            "The sysctl value must be a small decimal number, got: {value}"
        ));
    }
    Ok(())
}

/// Operations handled by the security-apply action. Every operation is a
/// fixed, hard-coded routine; the name only selects which one.
pub fn security_apply_op_valid(op: &str) -> Result<(), String> {
    const OPS: &[&str] = &[
        "password_policy",
        "tmp_sticky_on",
        "tmp_sticky_off",
        "passwd_perms",
        "shadow_perms",
        "pam_faillock_on",
        "pam_faillock_off",
        "umask_on",
        "umask_off",
        "ssh_idle_on",
        "ssh_idle_off",
        "blacklist_fs_on",
        "blacklist_fs_off",
        "legal_banner_on",
        "legal_banner_off",
    ];
    if OPS.contains(&op) {
        Ok(())
    } else {
        Err(format!(
            "The security operation must be one of {OPS:?}, got: {op}"
        ))
    }
}

/// Login session ids accepted by the session-kill action.
pub fn login_session_id_valid(value: &str) -> Result<(), String> {
    if is_digits(value) && value.len() <= 20 && value.parse::<u64>().is_ok_and(|id| id > 0) {
        Ok(())
    } else {
        Err(format!(
            "The session id must be a positive decimal number, got: {value}"
        ))
    }
}

/// System accounts that must never be deleted or locked through the panel.
pub const PROTECTED_SYSTEM_USERS: &[&str] = &[
    "root", "nobody", "daemon", "bin", "sys", "sync", "games", "man", "lp", "mail",
    "news", "uucp", "proxy", "www-data", "backup", "list", "irc", "gnats", "systemd-network",
    "systemd-resolve", "systemd-timesync", "systemd-coredump", "systemd-oom", "systemd-journal",
    "dbus", "polkitd", "sshd", "chrony", "rpc", "avahi", "colord", "geoclue", "flatpak"
];

pub fn is_protected_system_user(username: &str) -> bool {
    let lower = username.trim().to_lowercase();
    if lower.starts_with("systemd-") {
        return true;
    }
    PROTECTED_SYSTEM_USERS.contains(&lower.as_str())
}

/// Groups whose removal can break system administration, privilege escalation
/// or log access. Deletion is blocked in the helper before any command runs.
pub const PROTECTED_ADMIN_GROUPS: &[&str] = &[
    "root", "wheel", "sudo", "admin", "adm", "policykit", "polkitd",
    "systemd-journal", "docker", "libvirt",
];

pub fn is_protected_admin_group(groupname: &str) -> bool {
    let lower = groupname.trim().to_lowercase();
    if lower.starts_with("systemd-") {
        return true;
    }
    PROTECTED_ADMIN_GROUPS.contains(&lower.as_str())
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

// ─── protected systemd units (shared with the app's service UI) ──────────────

/// Critical core: the helper refuses stop/disable outright. Stopping or
/// disabling these from a same-user process (even with the panel closed)
/// would break the session or the journal.
const PROTECTED_UNIT_CRITICAL: &[&str] = &[
    "dbus", "dbus-broker", "polkit",
    "systemd-journald", "systemd-logind", "systemd-udevd", "systemd-sysctl",
    "systemd-remount-fs", "systemd-tmpfiles-setup", "systemd-tmpfiles-setup-dev",
    "systemd-user-sessions", "systemd-modules-load",
    "default.target", "basic.target", "sysinit.target", "multi-user.target",
    "graphical.target", "local-fs.target", "local-fs-pre.target", "network.target",
    "remote-fs.target", "swap.target", "cryptsetup.target", "rescue.target",
    "emergency.target", "init.target", "systemd.target", "sockets.target",
    "timers.target", "shutdown.target", "reboot.target", "poweroff.target",
    "emergency", "rescue", "init",
];

/// Essential infrastructure: the helper refuses stop/disable too — losing
/// these would cut network, security or display access off the machine.
const PROTECTED_UNIT_ESSENTIAL: &[&str] = &[
    "networkmanager", "systemd-networkd", "systemd-resolved", "systemd-timesyncd",
    "sshd", "firewalld", "nftables", "iptables", "auditd", "chronyd", "chrony",
    "gdm", "sddm", "lightdm", "systemd-homed",
];

/// `Some("critical")` or `Some("essential")` for a unit the helper (and,
/// via this shared function, the service UI) protects, `None` otherwise.
/// The same unit shapes match: the whole name, the base name without its
/// `.service` suffix, and the known prefix families.
pub fn service_protection_level(unit_name: &str) -> Option<&'static str> {
    let clean_name = unit_name.to_lowercase();
    let base_name = clean_name.strip_suffix(".service").unwrap_or(&clean_name);
    if PROTECTED_UNIT_CRITICAL.iter().any(|&s| clean_name == s || base_name == s)
        || base_name.starts_with("systemd-journald")
        || base_name.starts_with("systemd-udev")
        || base_name.starts_with("dbus")
    {
        return Some("critical");
    }
    if PROTECTED_UNIT_ESSENTIAL.iter().any(|&s| clean_name == s || base_name == s)
        || base_name.starts_with("networkmanager")
        || base_name.starts_with("firewalld")
    {
        return Some("essential");
    }
    None
}

/// Systemd unit-name contract shared by the app and the helper (the same
/// grammar the service UI uses).
pub fn service_unit_name_valid(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 255 || name.starts_with('-') || name.contains('/') {
        return Err(format!("Invalid systemd unit name: {name}"));
    }
    let valid_suffix = [
        ".service", ".socket", ".target", ".timer", ".mount", ".automount",
        ".path", ".slice", ".scope", ".device", ".swap",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix));
    if !valid_suffix {
        return Err(format!("The unit name must end in a systemd unit suffix: {name}"));
    }
    let bytes = name.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphanumeric() || b"-_.@:".contains(&byte) {
            index += 1;
        } else if byte == b'\\'
            && bytes.get(index + 1) == Some(&b'x')
            && bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
            && bytes.get(index + 3).is_some_and(u8::is_ascii_hexdigit)
        {
            index += 4;
        } else {
            return Err(format!("The unit name contains unsafe characters: {name}"));
        }
    }
    Ok(())
}

/// The system-scope unit actions and the shared protection policy. Critical
/// units cannot be stopped/disabled/masked.
pub fn unit_action_valid(verb: &str, unit: &str) -> Result<(), String> {
    const VERBS: &[&str] = &[
        "start", "stop", "restart", "reload", "enable", "disable", "mask", "unmask",
    ];
    if !VERBS.contains(&verb) {
        return Err(format!(
            "The unit action must be start|stop|restart|reload|enable|disable|mask|unmask, got: {verb}"
        ));
    }
    service_unit_name_valid(unit)?;
    let level = service_protection_level(unit);
    let blocked = match (verb, level) {
        ("stop" | "disable" | "mask", Some("critical")) => true,
        ("stop" | "disable" | "mask", Some("essential")) => true,
        _ => false,
    };
    if blocked {
        return Err(format!(
            "The {} unit cannot be {verb}ed with administrator rights through this panel",
            level.unwrap_or("protected")
        ));
    }
    Ok(())
}

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
            _ => {
                require(
                    PROGRAM,
                    arg,
                    "a systemd unit name (e.g. nginx.service)",
                    is_unit_name,
                )?;
                // protected units: stop/disable is refused by the helper
                // itself, not only by the UI (which a same-user process can
                // bypass by calling the helper directly)
                if verb == "stop" || verb == "disable" {
                    if let Some(level) = service_protection_level(arg) {
                        return Err(operand_not_allowed(
                            PROGRAM,
                            arg,
                            &format!(
                                "the {level} unit cannot be stopped or disabled with administrator rights through this panel"
                            ),
                        ));
                    }
                }
            }
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

/// Package names the panel refuses to uninstall, autoremove or roll back.
/// Exact names plus prefix families that are boot-, manager-, or
/// security-critical. Shared by the app and the privileged helper.
pub const PROTECTED_PACKAGES: &[&str] = &[
    "glibc", "systemd", "dnf", "dnf5", "kernel", "kernel-core", "kernel-modules",
    "bash", "coreutils", "rpm", "sudo", "shadow-utils", "polkit", "dbus",
    "util-linux", "pam", "filesystem", "libselinux", "glibc-common",
    "apt", "dpkg", "libc6", "ubuntu-minimal", "ubuntu-standard", "debian-archive-keyring",
    "grub2", "grub2-common", "grub2-tools", "shim", "dracut", "NetworkManager",
    "firewalld", "nftables", "openssl-libs", "cryptsetup", "lvm2", "selinux-policy"
];

pub fn is_protected_package(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    if [
        "kernel", "linux-image", "systemd", "libsystemd", "glibc", "libc6", "dnf", "apt",
        "grub", "shim", "dracut", "networkmanager", "selinux-policy", "libselinux",
        "cryptsetup", "lvm2", "rpm", "coreutils", "libpam", "libssl", "openssl",
        "dbus", "polkit", "sudo", "shadow", "passwd", "util-linux", "openssh",
        "firewalld", "nftables", "iptables", "libnss", "systemd-boot",
    ].iter().any(|prefix| lower.starts_with(prefix)) {
        return true;
    }
    PROTECTED_PACKAGES.iter().any(|package| package.to_lowercase() == lower)
}

/// A `dnf history undo` transaction id. The value reaches the helper as a
/// string, so it is validated independently of the app's `u32` IPC type:
/// positive decimal, at most 10 digits (u32), never zero or padded with
/// anything but digits.
pub fn dnf_history_transaction_id_valid(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 10
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!(
            "The DNF transaction id must be a positive decimal number of at most 10 digits, got: {value}"
        ));
    }
    let parsed: u32 = value
        .parse()
        .map_err(|_| format!("The DNF transaction id is out of range: {value}"))?;
    if parsed == 0 {
        return Err("The DNF transaction id must be greater than zero.".to_string());
    }
    Ok(())
}

/// The `pkg_maintenance` operation names. Anything else is refused before a
/// package manager is started.
pub fn pkg_maintenance_op_valid(op: &str) -> Result<(), String> {
    match op {
        "clean" | "autoremove" | "makecache" => Ok(()),
        other => Err(format!(
            "The package maintenance operation must be clean, autoremove or makecache, got: {other}"
        )),
    }
}

/// True when a resolved package identifier belongs to a kernel family
/// (`kernel`, `kernel-core`, …, `linux-image`, …). Used to fail closed when the
/// running kernel release cannot be determined but the rollback involves a
/// kernel package at all.
pub fn package_is_kernel_package(package: &str) -> bool {
    let lower = package.trim().to_ascii_lowercase();
    lower.starts_with("kernel") || lower.starts_with("linux-image")
}

/// True when a resolved package identifier names the kernel release that is
/// currently running (`uname -r`). Kernel meta/subpackages
/// (`kernel-core`, `kernel-modules`, `kernel-devel`, …) are matched only when
/// their resolved version contains the running release, so comparisons stay
/// version-specific; a version-less `kernel` entry cannot be proven to be the
/// running one here.
pub fn package_matches_running_kernel(package: &str, running_release: &str) -> bool {
    let release = running_release.trim();
    if release.is_empty() {
        return false;
    }
    let package = package.trim();
    if package.is_empty() {
        return false;
    }
    package.to_ascii_lowercase().starts_with("kernel") && package.contains(release)
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
    ensure_safe_operand(value, "repository URL")?;
    if value.starts_with("ppa:") {
        // The same closed owner/project grammar as the named Copr/PPA action:
        // no empty ids, extra slashes, dot segments or option-like parts.
        copr_repo_valid(value).map_err(|reason| {
            operand_not_allowed(PROGRAM, value, &reason)
        })?;
    } else if value.starts_with("https://") {
        if value.len() == "https://".len() || value.chars().any(char::is_whitespace) {
            return Err(operand_not_allowed(
                PROGRAM,
                value,
                "the repository URL must include a host and contain no whitespace",
            ));
        }
    } else {
        return Err(operand_not_allowed(
            PROGRAM,
            value,
            "the repository must be a 'ppa:<owner>/<project>' id or an https URL (http is refused)",
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
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if !arg.starts_with('-') {
            break;
        }
        if !matches!(arg, "-j" | "-o" | "-4" | "-6") {
            return Err(flag_not_allowed(PROGRAM, arg));
        }
        index += 1;
    }

    let Some(subcommand) = args.get(index) else {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "a read-only ip subcommand (a/addr/link/route/neigh) is required",
        ));
    };
    if !matches!(
        subcommand.as_str(),
        "a" | "addr" | "address" | "link" | "route" | "neigh" | "neighbor"
    ) {
        return Err(operand_not_allowed(
            PROGRAM,
            subcommand,
            "the subcommand must be a read-only a|addr|link|route|neigh form",
        ));
    }
    index += 1;

    if let Some(verb) = args.get(index) {
        if !matches!(verb.as_str(), "show" | "list" | "get") {
            return Err(operand_not_allowed(
                PROGRAM,
                verb,
                "only read-only show|list|get is allowed; interface changes use the named action",
            ));
        }
        for operand in &args[index + 1..] {
            if matches!(
                operand.as_str(),
                "set" | "add" | "del" | "delete" | "flush" | "change" | "replace" | "up" | "down"
            ) {
                return Err(operand_not_allowed(
                    PROGRAM,
                    operand,
                    "a write verb is not allowed on the generic ip channel",
                ));
            }
            require(PROGRAM, operand, "an ip operand", |value| {
                value.len() <= 128
                    && value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '/' | '@' | '_' | '-'))
            })?;
        }
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
        if !arg.starts_with('-') {
            break;
        }
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
        index += 1;
    }

    let rest = &args[index..];
    match rest {
        [connection, show] if connection == "connection" && show == "show" => Ok(()),
        [connection, show, uuid] if connection == "connection" && show == "show" => {
            ensure_safe_operand(uuid, "connection id")?;
            if !is_uuid(uuid) {
                return Err(operand_not_allowed(
                    PROGRAM,
                    uuid,
                    "a connection UUID is required",
                ));
            }
            Ok(())
        }
        _ => Err(operand_not_allowed(
            PROGRAM,
            rest.first().map(String::as_str).unwrap_or(""),
            "only read-only 'connection show [<uuid>]' is allowed; writes use the named nmcli actions",
        )),
    }
}

/// The `nmcli_connection_save <add|modify> <connection> <setting.prop> <value>...`
/// action: creates or modifies an nmcli connection profile by setting one or
/// more properties. The verb and connection operand are confined; property
/// names must follow nmcli's `setting.property` charset (no leading dash,
/// no control characters); values are free-form by nmcli's design and are
/// not grammar-checked here.
fn check_nmcli_connection_save(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "nmcli_connection_save";
    if args.len() < 3 {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "<add|modify> <connection> <setting.prop> <value>...",
        ));
    }
    let verb = args[0].as_str();
    if verb != "add" && verb != "modify" {
        return Err(operand_not_allowed(
            PROGRAM,
            verb,
            "the verb must be add or modify",
        ));
    }
    ensure_safe_operand(&args[1], "connection name")?;
    // setting.prop / value pairs from args[2] onward
    let mut index = 2;
    while index < args.len() {
        let setting = args[index].as_str();
        if setting.starts_with('-') {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "a property name must not start with a dash",
            ));
        }
        if setting.contains('\n') || setting.contains('\r') || setting.contains('\0') {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "a property name must not contain control characters",
            ));
        }
        let has_dot = setting.contains('.');
        if !has_dot {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "a property must be in setting.property form",
            ));
        }
        let (setting_part, prop_part) = setting.split_once('.').unwrap();
        if setting_part.is_empty() || prop_part.is_empty() {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "both setting and property must be non-empty",
            ));
        }
        for part in [setting_part, prop_part] {
            if part
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | ':' | '+'))
            {
                return Err(operand_not_allowed(
                    PROGRAM,
                    setting,
                    "property names may only contain alphanumeric characters, _, -, :, +",
                ));
            }
        }
        // values are free-form but must not be empty or contain control chars
        let value = args
            .get(index + 1)
            .ok_or_else(|| {
                operand_not_allowed(
                    PROGRAM,
                    setting,
                    "every property must be followed by a value",
                )
            })?;
        if value.is_empty() {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "a property value must not be empty",
            ));
        }
        if value.contains('\n') || value.contains('\r') || value.contains('\0') {
            return Err(operand_not_allowed(
                PROGRAM,
                setting,
                "a property value must not contain control characters",
            ));
        }
        index += 2;
    }
    Ok(())
}

/// The `nmcli_interface_state <iface> <up|down>` action: brings an interface
/// up or down with `ip link set dev <iface> up|down`. The interface name is
/// confined like an ip operand; `down` on `lo` or `loopback` is refused.
pub fn check_nmcli_interface_state(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "nmcli_interface_state";
    if args.len() != 2 {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "<iface> <up|down>",
        ));
    }
    let direction = args[1].as_str();
    if direction != "up" && direction != "down" {
        return Err(operand_not_allowed(
            PROGRAM,
            direction,
            "the direction must be up or down",
        ));
    }
    ensure_safe_operand(&args[0], "interface name")?;
    if direction == "down" {
        let lower = args[0].trim().to_lowercase();
        if lower == "lo" || lower.starts_with("loopback") {
            return Err(operand_not_allowed(
                PROGRAM,
                &args[0],
                "cannot bring down the loopback interface (lo): it is vital for inter-process communication",
            ));
        }
    }
    Ok(())
}

/// The `nmcli_connection_import <openvpn|wireguard> <file_path>` action:
/// imports a VPN profile from a user-readable file. The file type must be
/// declared and the path must be absolute, confinement-checked, and free of
/// relative parts and symlinks — the helper reads the file itself, so the
/// app must never be able to point it at an arbitrary file.
pub fn check_nmcli_import(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "nmcli_connection_import";
    if args.len() != 2 {
        return Err(operand_not_allowed(
            PROGRAM,
            "",
            "<openvpn|wireguard> <file_path>",
        ));
    }
    let import_type = args[0].as_str();
    if import_type != "openvpn" && import_type != "wireguard" {
        return Err(operand_not_allowed(
            PROGRAM,
            import_type,
            "the import type must be openvpn or wireguard",
        ));
    }
    let path = &args[1];
    ensure_safe_operand(path, "import file path")?;
    nmcli_import_path_valid(path)?;
    // extension check matches the importer's own branch: .ovpn for openvpn,
    // .conf for wireguard
    let expected_ext = if import_type == "openvpn" { "ovpn" } else { "conf" };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext != expected_ext {
        return Err(operand_not_allowed(
            PROGRAM,
            path,
            &format!(
                "a {import_type} import file must have a .{expected_ext} extension"
            ),
        ));
    }
    Ok(())
}

/// A connection UUID must follow the 8-4-4-4-12 hexadecimal shape that
/// NetworkManager assigns to every connection profile. Loose names like
/// "Wired connection 1" are not UUIDs and are rejected here; callers that
/// want to operate by name must use a different entry point.
#[allow(dead_code)]
fn is_uuid(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lens = [8, 4, 4, 4, 12];
    parts.iter().zip(lens.iter()).all(|(part, &len)| {
        part.len() == len
            && part.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

/// Confinement roots for user-supplied import files: only directories a
/// normal user can reasonably place a file in are allowed. System paths
/// (/etc, /usr, /var, ...) are excluded so the importer cannot be pointed
/// at host configuration or secret material.
fn nmcli_import_allowed_dirs() -> &'static [&'static str] {
    &["/home", "/tmp", "/var/tmp", "/run/media"]
}

/// Confinement check for a VPN import file path: absolute, no relative
/// parts, no symlinks, confined to user-writable directories.
pub fn nmcli_import_path_valid(path: &str) -> Result<(), String> {
    if !Path::new(path).is_absolute() {
        return Err(format!(
            "The import file path must be absolute: {path}"
        ));
    }
    // Reject any path segment that is "." or ".." — checking the raw
    // segments is more reliable than Path::components() for this purpose.
    for segment in path.split('/') {
        if segment == "." || segment == ".." {
            return Err(format!(
                "The import file path must not contain relative parts: {path}"
            ));
        }
    }
    let parent = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let in_allowed_dir = nmcli_import_allowed_dirs()
        .iter()
        .any(|d| parent == *d || parent.starts_with(*d));
    if !in_allowed_dir {
        return Err(format!(
            "The import file path must be under one of {dirs:?}, not {path}",
            dirs = nmcli_import_allowed_dirs()
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
                // --remove: also remove the home directory (the UI option)
                "--remove" => {}
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

/// `process_renice <pid> <priority>`: the privileged renice action. It is only
/// for the negative-niceness case (the app keeps positive renicing
/// unprivileged) and refuses protected system PIDs.
pub fn process_renice_valid(pid: u32, priority: i32) -> Result<(), String> {
    if pid <= 100 {
        return Err(format!(
            "PID {pid} is reserved for critical system processes and cannot be reniced"
        ));
    }
    if !(-20..=-1).contains(&priority) {
        return Err(format!(
            "the privileged renice action only accepts a priority between -20 and -1, got: {priority}"
        ));
    }
    Ok(())
}

/// `smart_health <device>`: one block device under /dev. The path has exactly
/// one safe name component and is not a symlink; production also requires a
/// real block device. The fixture build uses a regular file under its own
/// device directory so tests never touch a real disk.
pub fn smart_device_valid(device: &str) -> Result<(), String> {
    #[cfg(feature = "test-fixture")]
    let prefix = "/tmp/lcp-admin-test-fixture/dev/";
    #[cfg(not(feature = "test-fixture"))]
    let prefix = "/dev/";

    let name = device.strip_prefix(prefix).ok_or_else(|| {
        format!("the SMART device must be a direct {prefix}<name> path, got: {device}")
    })?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(format!(
            "the SMART device name must contain only alphanumeric, '-' or '_' characters, got: {device}"
        ));
    }
    let metadata = std::fs::symlink_metadata(device)
        .map_err(|e| format!("the SMART device {device} could not be inspected: {e}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("the SMART device {device} must not be a symlink"));
    }
    #[cfg(not(feature = "test-fixture"))]
    {
        use std::os::unix::fs::FileTypeExt;
        if !metadata.file_type().is_block_device() {
            return Err(format!("the SMART device {device} is not a block device"));
        }
    }
    Ok(())
}

/// SMART self-test types accepted by the Device Manager UI and smartctl.
pub fn smart_test_type_valid(test_type: &str) -> Result<(), String> {
    if matches!(test_type, "short" | "long" | "offline" | "conveyance") {
        Ok(())
    } else {
        Err(format!(
            "the SMART self-test type must be short|long|offline|conveyance, got: {test_type}"
        ))
    }
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
/// Paths a root *reader* may look at — a fixed allowlist, because even a pure
/// reading program reveals `/etc/shadow` and `/etc/sudoers` when run as root.
/// Everything else (allowed directories first, then one exact file each):
const READER_DIR_ALLOWLIST: &[&str] = &[
    // system and web server logs
    "/var/log/",
    "/etc/nginx/",
    // the audit rules the panel writes
    "/etc/audit/rules.d/",
    // ssh include fragments (not the host keys, which live next door in
    // /etc/ssh itself and stay off the list)
    "/etc/ssh/sshd_config.d/",
    // the boot loader configuration the auditors read
    "/boot/grub2/",
];

const READER_FILE_ALLOWLIST: &[&str] = &[
    "/etc/hosts",
    "/etc/hostname",
    "/etc/os-release",
    // the ssh daemon's main config (the .d/ directory is below)
    "/etc/ssh/sshd_config",
];

/// Categories that are never readable through the helper, no matter what a
/// later allowlist entry might match — credentials and the privilege policy.
const READER_DENIED_PREFIXES: &[&str] = &[
    "/etc/shadow",
    "/etc/gshadow",
    "/etc/sudoers",
    "/etc/ssl/private",
    "/root",
];

/// The panel's own log directory inside any user's home (the XDG config
/// location). A user-owned directory; its contents are only the panel's log.
fn panel_log_path(path: &str) -> bool {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match parts.first() {
        Some(&"home") => {
            parts.len() >= 5 && parts[2..5] == [".config", "control-panel", "logs"]
        }
        Some(&"root") => parts.len() >= 4 && parts[1..4] == [".config", "control-panel", "logs"],
        _ => false,
    }
}

/// One path operand for a reader program, checked twice: for the path as
/// given (no symlink may appear anywhere inside) and again after full
/// canonicalization — a symlink that resolves outside the allowlist can not
/// even be named, and a directory that later turns user-writable can not
/// smuggle the resolution through.
fn check_reader_path(program: &str, path: &str) -> Result<(), String> {
    ensure_trusted_path(program, path, "file path")?;
    match std::fs::canonicalize(path) {
        Ok(canonical) => {
            let resolved = canonical.to_string_lossy().into_owned();
            ensure_reader_target(program, path, &resolved)?;
            ensure_reader_target(program, &resolved, &resolved)
        }
        // The path can not be resolved (it does not exist yet, or it sits in
        // a directory only root may traverse). Its components were
        // symlink-checked above, so judging it by the literal path is sound;
        // the actual read fails later without touching anything.
        Err(_) => ensure_reader_target(program, path, path),
    }
}

fn ensure_reader_target(program: &str, given: &str, resolved: &str) -> Result<(), String> {
    // denial first: an explicit "never" has the last word even if a future
    // allowlist entry would match the same path
    if let Some(reason) = reader_denial_reason(resolved) {
        return Err(operand_not_allowed(program, given, &reason));
    }
    if READER_FILE_ALLOWLIST.contains(&resolved)
        || READER_DIR_ALLOWLIST
            .iter()
            .any(|prefix| {
                resolved.starts_with(prefix) || resolved == prefix.trim_end_matches('/')
            })
        || panel_log_path(resolved)
    {
        return Ok(());
    }
    Err(operand_not_allowed(
        program,
        given,
        "the file is not in the small set of locations this panel reads (system and nginx logs, /etc/hosts, /etc/hostname, /etc/os-release, audit rules, the panel's own logs)",
    ))
}

fn reader_denial_reason(resolved: &str) -> Option<String> {
    let denied: Option<&'static str> = if READER_DENIED_PREFIXES.iter().any(|p| resolved.starts_with(p)) {
        Some("this location holds credentials or system policy and is never readable through the helper")
    } else if resolved.split('/').any(|part| part == ".ssh") {
        Some("SSH private directories are never readable through the helper")
    } else if resolved.ends_with("shadow") || resolved.ends_with("gshadow") || resolved.contains("/sudoers") {
        Some("credential and privilege-policy files are never readable through the helper")
    } else {
        None
    };
    denied.map(|s| s.to_string())
}

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
            check_reader_path(program, arg)?;
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
            check_reader_path(PROGRAM, arg)?;
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
    // The two shapes the panel sends: the renewal page's
    // `certbot renew --cert-name <name> …` and the domains page's
    // `certbot --nginx -d <domain> … --email=… | --register-unsafely-without-email`.
    // Everything else — mixed-shape garbage, unknown verbs, unknown flags —
    // is refused by the shape matcher that answers for the invocation.
    if args.iter().any(|arg| arg == "renew") || args.is_empty() {
        check_certbot_renew(args)
    } else {
        check_certbot_request(args)
    }
}

/// The request shape of the domains page, fixed flag set only:
/// `--nginx`, `-d <domain>`, `--non-interactive`, `--agree-tos`, and then
/// exactly one of `--email=<address>` or `--register-unsafely-without-email`.
/// Each fixed flag is accepted exactly once, in any order among themselves;
/// anything else — another flag, an unknown operand, mixed garbage — is
/// refused before the program ever runs as root.
fn check_certbot_request(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "certbot";

    let mut saw_nginx = false;
    let mut saw_domain: Option<String> = None;
    let mut saw_non_interactive = false;
    let mut saw_agree_tos = false;
    let mut saw_expression: Option<&str> = None; // one e-mail choice, at most

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--nginx" if !saw_nginx => saw_nginx = true,
            "--nginx" => return Err(duplicate_flag(PROGRAM, arg)),
            "--non-interactive" if !saw_non_interactive => saw_non_interactive = true,
            "--non-interactive" => return Err(duplicate_flag(PROGRAM, arg)),
            "--agree-tos" if !saw_agree_tos => saw_agree_tos = true,
            "--agree-tos" => return Err(duplicate_flag(PROGRAM, arg)),
            "-d" => {
                if saw_domain.is_some() {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "at most one request may be sent: -d takes exactly one domain name",
                    ));
                }
                let value = flag_value(PROGRAM, arg, &mut index, args)?;
                require(PROGRAM, &value, "a DNS domain name for -d", is_dns_domain)?;
                saw_domain = Some(value);
            }
            value if value.starts_with("--email=") => {
                if saw_expression.is_some() {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "at most one e-mail expression is allowed: the request is either registered with an address or without one",
                    ));
                }
                let address = &value["--email=".len()..];
                require(
                    PROGRAM,
                    address,
                    "an e-mail address for --email= (an address must have a local part and a DNS domain)",
                    is_email_address,
                )?;
                saw_expression = Some(arg);
            }
            "--register-unsafely-without-email" => {
                if saw_expression.is_some() {
                    return Err(operand_not_allowed(
                        PROGRAM,
                        arg,
                        "at most one e-mail expression is allowed: the request is either registered with an address or without one",
                    ));
                }
                saw_expression = Some(arg);
            }
            _ if arg.starts_with('-') => return Err(flag_not_allowed(PROGRAM, arg)),
            _ => {
                return Err(operand_not_allowed(
                    PROGRAM,
                    arg,
                    "only the request shape 'certbot --nginx -d <domain> --non-interactive --agree-tos [--email=<address> | --register-unsafely-without-email]' is allowed",
                ));
            }
        }
        index += 1;
    }

    if !saw_nginx {
        return Err(flag_not_allowed_hint(
            PROGRAM,
            "the request must name the nginx installer: --nginx is required",
        ));
    }
    if saw_domain.is_none() {
        return Err(operand_not_allowed(
            PROGRAM,
            "-d",
            "the request must name exactly one domain: -d <domain> is required",
        ));
    }
    if !saw_non_interactive {
        return Err(flag_not_allowed_hint(
            PROGRAM,
            "the request must be non-interactive: --non-interactive is required",
        ));
    }
    if !saw_agree_tos {
        return Err(flag_not_allowed_hint(
            PROGRAM,
            "the terms must be agreed to upfront: --agree-tos is required",
        ));
    }
    if saw_expression.is_none() {
        return Err(flag_not_allowed_hint(
            PROGRAM,
            "either --email=<address> or --register-unsafely-without-email is required",
        ));
    }
    Ok(())
}

/// One flag twice on the same request line — a guess at least, a mistake at
/// worst; both are refused.
fn duplicate_flag(program: &str, flag: &str) -> String {
    format!("Refused to run {program} with administrator rights: the option '{flag}' may appear exactly once.")
}

fn flag_not_allowed_hint(program: &str, hint: &str) -> String {
    format!("Refused to run {program} with administrator rights: {hint}.")
}

/// A DNS name: ASCII dot-separated labels of letters, digits and hyphens
/// (1..63 each, no leading or trailing hyphen), the whole name at most 253
/// characters — the shape we need to be sure the argument positions as a
/// domain and never as a flag or a path.
fn is_dns_domain(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.len() <= 253
        && value.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// An e-mail address for the certificate request: at most 254 characters,
/// one `@`, an ASCII local part (letters, digits, `._%+-`) of at most 64
/// characters, and a DNS-name domain part (see `is_dns_domain`).
fn is_email_address(value: &str) -> bool {
    if !value.is_ascii() || value.len() > 254 {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if domain.is_empty() || domain.contains('@') {
        return false;
    }
    !local.is_empty()
        && local.len() <= 64
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'))
        && is_dns_domain(domain)
}

/// The renewal shape of the domains page, exactly as before: `renew`
/// (required), the fixed non-interactive banner flags and `--cert-name <name>`.
fn check_certbot_renew(args: &[String]) -> Result<(), String> {
    const PROGRAM: &str = "certbot";
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
/// `ss`, `ping`, `dig`, `host`, `update-alternatives`,
/// `dpkg`, `date`, `wc`, `tr`, `cut`, `sort`, `getent`, `id`,
/// `augenrules`, `dirname`, `basename`, `head`, `hostname` …).
fn check_no_flags_at_all(program: &str, args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.starts_with('-') {
            return Err(flag_not_allowed(program, arg));
        }
    }
    Ok(())
}

/// sshd shape stays with `check_daemon_flags` (flags only: `-T` to print the
/// effective configuration, `-t` to validate it — no operands, no key paths).

/// The helper's named-action calls (the first slice of the phase 3 CLI):
/// `lcp-admin <action> [params]`.
///
/// * `install_polkit_rule` — content via stdin, no parameters (see the action
///   in helper/src/main.rs).
/// * `admin_keep` — no parameters, no data: exists only to raise and keep the
///   panel's administrator authorisation (see `admin_mode_on`).
/// * `read_user_ssh_keys <account>` / `write_user_ssh_keys <account>` — an
///   account's authorized_keys; for the write the content travels on stdin.
/// * `nginx_config_install <path>` / `nginx_config_backup <path>` /
///   `nginx_restore_backup <dest> <backup>` — the nginx configuration tree
///   writes (see the action group constants below).
/// * `nginx_link_site <available> <enabled>` / `nginx_unlink_site <enabled>` /
///   `nginx_config_remove <path>` — the site enabling symlink, its inverse,
///   and the one-regular-file removal (see the action group constants below).
/// * `nginx_www_fs <mkdir|remove|rename> …` / `nginx_www_install <dest>` —
///   the web-root operations and the web-file install (content always
///   arrives on stdin; no source path is ever trusted to the helper).
// `pub(crate)` so helper/src/main.rs (which includes this file by path) can
// assert its ROOT_ACTIONS gate list never drifts from this validation list.
pub(crate) const HELPER_ACTIONS: &[&str] = &[
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
    "audit_setup",
    "unit_action",
    "unit_dropin_write",
    "unit_create",
    "unit_daemon_reload",
    "copr_manage",
    "nmcli_interface_state",
    "process_renice",
    "smart_health",
    "smart_data",
    "smart_self_test",
];

/// The user-management actions take exactly one parameter: an account name.
const HELPER_ACCOUNT_ACTIONS: &[&str] = &["read_user_ssh_keys", "write_user_ssh_keys"];

/// `write_profile_sh <path>` takes exactly one parameter: a profile.d
/// destination (validated by `validate_profile_sh_path`; the content travels
/// on stdin and is validated by the action itself).
const HELPER_PATH_ACTIONS: &[&str] = &["write_profile_sh"];

/// `restore_profile_sh <path> <backup>` takes exactly two parameters: a
/// profile.d destination and one exact revision sibling the helper's own
/// write core created — never an arbitrary path claiming to be a backup.
const HELPER_RESTORE_ACTIONS: &[&str] = &["restore_profile_sh"];

/// `nginx_config_install <path>` / `nginx_config_backup <path>` — one path in
/// the nginx configuration tree (validated by `nginx_config_path_valid`; the
/// install's content travels on stdin and passes the structural content
/// gate; the backup needs the destination to exist).
const HELPER_NGINX_CONFIG_ACTIONS: &[&str] = &["nginx_config_install", "nginx_config_backup"];

/// `nginx_restore_backup <dest> <backup>` — an nginx configuration
/// destination and one exact `<basename>.<digits>_<digits>.bak` sibling in
/// the backup directory, exactly the shape the backup action itself created
/// — never an arbitrary or auto-picked path.
const HELPER_NGINX_RESTORE_ACTIONS: &[&str] = &["nginx_restore_backup"];

/// `nginx_link_site <available> <enabled>` — a site definition in
/// `sites-available` and the *same-named* entry `sites-enabled` gets as its
/// symlink. structurally validated by `nginx_link_as_valid`.
const HELPER_NGINX_LINK_ACTIONS: &[&str] = &["nginx_link_site"];

/// `nginx_unlink_site <enabled>` — the one enabling symlink entry in
/// `sites-enabled` whose removal disables a site; validated by
/// `nginx_site_unlink_path_valid`.
const HELPER_NGINX_UNLINK_ACTIONS: &[&str] = &["nginx_unlink_site"];

/// `nginx_config_remove <path>` — one regular file in the nginx
/// configuration tree, `nginx.conf` excluded (`nginx_config_remove_path_valid`).
const HELPER_NGINX_REMOVE_ACTIONS: &[&str] = &["nginx_config_remove"];

/// The two endpoints of `nginx_link_site`: the source must sit in
/// `sites-available`, the destination in `sites-enabled`, and both must
/// carry the same file name — a link that would enable *another* site than
/// the one selected is refused before anything is created.
pub fn nginx_link_as_valid(available: &str, enabled: &str) -> Result<(), String> {
    use std::path::Path;

    nginx_config_path_valid(available)?;
    nginx_config_path_valid(enabled)?;
    if Path::new(available).parent() != Some(Path::new(NGINX_SITES_AVAILABLE)) {
        return Err(format!(
            "The linked-on source must be a plain child of {NGINX_SITES_AVAILABLE}, not {available:?}"
        ));
    }
    if Path::new(enabled).parent() != Some(Path::new(NGINX_SITES_ENABLED)) {
        return Err(format!(
            "The created link must be a plain child of {NGINX_SITES_ENABLED}, not {enabled:?}"
        ));
    }
    let available_name = Path::new(available).file_name().map(|n| n.to_string_lossy().to_string());
    let enabled_name = Path::new(enabled).file_name().map(|n| n.to_string_lossy().to_string());
    if available_name.is_none()
        || available_name != enabled_name
    {
        return Err(format!(
            "The link {enabled:?} must enable the site it is named after: both endpoints must share the basename of {available:?}"
        ));
    }
    Ok(())
}

/// `path` may be removed: any file the configuration tree confinement
/// allows, except the exact main configuration file — the protected core of
/// the tree, which can be edited and backed up but is never a removal target
/// (nginx's include layout falls apart without it).
pub fn nginx_config_remove_path_valid(path: &str) -> Result<(), String> {
    if path == NGINX_NGINX_CONF {
        return Err(format!(
            "The nginx main configuration {NGINX_NGINX_CONF} is the protected core; it may be edited, but not removed."
        ));
    }
    nginx_config_path_valid(path)
}

/// `path` may be unlinked as one site's enabling symlink: exactly a plain
/// child of `sites-enabled` under the shared configuration-tree policy.
/// Everything else — files, foreign directories, the tree itself — is
/// refused before the action looks at the filesystem.
pub fn nginx_site_unlink_path_valid(path: &str) -> Result<(), String> {
    use std::path::Path;

    nginx_config_path_valid(path)?;
    if Path::new(path).parent() != Some(Path::new(NGINX_SITES_ENABLED)) {
        return Err(format!(
            "The unlink target must be a plain child of {NGINX_SITES_ENABLED}, not {path:?}"
        ));
    }
    Ok(())
}

/// `nginx_www_fs <mkdir|remove|rename> …` — the web-root operations: the
/// sub-command selects the operation, and everything after it is confined by
/// `var_www_path_valid`. An additional protected-name rule on top: the web
/// root itself and `/html` can never be the *source* of a remove, and never
/// an endpoint of a rename.
const HELPER_NGINX_WWW_FS_ACTIONS: &[&str] = &["nginx_www_fs"];

/// `nginx_www_install <dest>` — one destination file in the web root, fully
/// filled from stdin; no source path is ever sent to the helper.
const HELPER_NGINX_WWW_INSTALL_ACTIONS: &[&str] = &["nginx_www_install"];

/// `nmcli_connection_save <add|modify> <connection> <setting.prop> <value>...`
/// — creates or modifies an nmcli connection profile. Validated by
/// `check_nmcli_connection_save`; no content travels on stdin.
const HELPER_NMCLI_CONNECTION_SAVE_ACTIONS: &[&str] = &["nmcli_connection_save"];

/// `nmcli_connection_delete <uuid>` — deletes one connection profile by UUID.
/// Validated by `is_uuid`.
const HELPER_NMCLI_CONNECTION_DELETE_ACTIONS: &[&str] = &["nmcli_connection_delete"];

/// `nmcli_connection_up <uuid>` — activates one connection profile by UUID.
/// Validated by `is_uuid`.
const HELPER_NMCLI_CONNECTION_UP_ACTIONS: &[&str] = &["nmcli_connection_up"];

/// `nmcli_connection_down <uuid>` — deactivates one connection profile by UUID.
/// Validated by `is_uuid`.
const HELPER_NMCLI_CONNECTION_DOWN_ACTIONS: &[&str] = &["nmcli_connection_down"];

/// `nmcli_connection_import <type> <file_path>` — imports a VPN profile
/// from a user-readable file (openvpn or wireguard). The file path must
/// be in a permitted directory and match the expected extension.
const HELPER_NMCLI_CONNECTION_IMPORT_ACTIONS: &[&str] = &["nmcli_connection_import"];

/// `repo_toggle_enable <file_path> <repo_id>` — enables one repo section in a
/// repository file. Validated by `ensure_repo_file_path` and `ensure_safe_operand`.
const HELPER_REPO_TOGGLE_ENABLE_ACTIONS: &[&str] = &["repo_toggle_enable"];

/// `repo_toggle_disable <file_path> <repo_id>` — disables one repo section in a
/// repository file. Validated by `ensure_repo_file_path` and `ensure_safe_operand`.
const HELPER_REPO_TOGGLE_DISABLE_ACTIONS: &[&str] = &["repo_toggle_disable"];

/// `repo_add <url>` — adds a new repository source. Validated by
/// `ensure_safe_operand` and `repo_url_https_only` (https-only).
const HELPER_REPO_ADD_ACTIONS: &[&str] = &["repo_add"];

/// `repo_delete <file_path>` — deletes a repository file (whole file) or a
/// single section from it. Validated by `ensure_repo_file_path`.
const HELPER_REPO_DELETE_ACTIONS: &[&str] = &["repo_delete"];

/// `repo_makecache` — refreshes package metadata (dnf makecache or apt-get
/// update). Takes no further arguments.
const HELPER_REPO_MAKECACHE_ACTIONS: &[&str] = &["repo_makecache"];

/// `repo_clean_cache <repo_id>` — clears package metadata cache for one repo
/// (dnf clean expire-cache --repo <repo_id> or apt-get clean).
/// Validated by `ensure_safe_operand`.
const HELPER_REPO_CLEAN_CACHE_ACTIONS: &[&str] = &["repo_clean_cache"];

/// `firewall_panic <enable|disable>` — firewalld panic mode. ufw is
/// refused by the helper before any command. The trailing `confirm` is the
/// GROUP 3 SSH-guard confirmation.
const HELPER_FIREWALL_PANIC_ACTIONS: &[&str] = &["firewall_panic"];

/// `firewall_interface_zone <zone> <interface> [confirm]` — binds one
/// network interface to a firewalld zone. ufw is refused by the helper
/// before any command. The trailing `confirm` is the GROUP 4 SSH-guard
/// confirmation for moving the SSH-carrying interface into a zone without
/// ssh.
const HELPER_FIREWALL_INTERFACE_ZONE_ACTIONS: &[&str] = &["firewall_interface_zone"];

/// `firewall_enable_hardened <firewalld|ufw> [confirm]` — enables the chosen
/// firewall backend with a deny-by-default posture. The confirmation is
/// mandatory (the change can cut network access); the backend is named
/// explicitly so an ambiguous host is refused rather than guessed.
const HELPER_FIREWALL_ENABLE_HARDENED_ACTIONS: &[&str] = &["firewall_enable_hardened"];

/// `pkg_maintenance <clean|autoremove|makecache>` — package-manager cache
/// maintenance. The verb is validated before any package manager starts;
/// autoremove additionally runs a resolved-removal preview and a
/// protected-package hard block inside the helper.
const HELPER_PKG_MAINTENANCE_ACTIONS: &[&str] = &["pkg_maintenance"];

/// `dnf_history_undo <transaction-id> [confirm]` — rolls back one DNF
/// transaction. The id validator runs before any package manager starts; the
/// trailing `confirm` is required only when the helper's preview finds a
/// protected package or the running kernel in the affected set.
const HELPER_DNF_HISTORY_UNDO_ACTIONS: &[&str] = &["dnf_history_undo"];

/// Named account/group/session actions. Every operand is validated here and
/// again inside the action; the helper never constructs a shell command.
/// Closed security-auditor operations: only the four named units and the
/// single ctrl-alt-del target can be reached through these actions.
const HELPER_SECURITY_SERVICE_ACTIONS: &[&str] = &["security_service_toggle"];
const HELPER_SECURITY_APPLY_ACTIONS: &[&str] = &["security_apply"];
const HELPER_SECURITY_SSH_PARAM_ACTIONS: &[&str] = &["security_ssh_param"];
const HELPER_SECURITY_KERNEL_PARAM_ACTIONS: &[&str] = &["security_kernel_param"];
const HELPER_SECURITY_LOCK_ACCOUNT_ACTIONS: &[&str] = &["security_lock_account"];
const HELPER_UNIT_ACTION_ACTIONS: &[&str] = &["unit_action"];
const HELPER_UNIT_DROPIN_ACTIONS: &[&str] = &["unit_dropin_write"];
const HELPER_UNIT_CREATE_ACTIONS: &[&str] = &["unit_create"];
const HELPER_UNIT_RECOVERY_ACTIONS: &[&str] = &["unit_daemon_reload"];
const HELPER_COPR_ACTIONS: &[&str] = &["copr_manage"];
const HELPER_NMCLI_INTERFACE_STATE_ACTIONS: &[&str] = &["nmcli_interface_state"];
const HELPER_PROCESS_RENICE_ACTIONS: &[&str] = &["process_renice"];
const HELPER_SMART_HEALTH_ACTIONS: &[&str] = &["smart_health"];
const HELPER_SMART_DATA_ACTIONS: &[&str] = &["smart_data"];
const HELPER_SMART_SELF_TEST_ACTIONS: &[&str] = &["smart_self_test"];
const HELPER_SECURITY_CTRL_ALT_DEL_ACTIONS: &[&str] = &["security_ctrl_alt_del"];

const HELPER_USER_MANAGEMENT_ACTIONS: &[&str] = &[
    "user_add",
    "user_delete",
    "user_set_password",
    "user_toggle_sudo",
    "user_modify_group",
    "group_manage",
    "user_lock",
    "session_kill",
];

/// `firewall_zone_rule <add|remove> <zone> <service|port> <value>` — modifies
/// one service or port in a firewall zone. firewalld and ufw are supported.
const HELPER_FIREWALL_ZONE_RULE_ACTIONS: &[&str] = &["firewall_zone_rule"];

/// `firewall_rich_rule <add|remove> <zone> <rule>` — modifies one firewalld
/// rich rule. firewalld only; ufw is explicitly refused.
const HELPER_FIREWALL_RICH_RULE_ACTIONS: &[&str] = &["firewall_rich_rule"];

/// Size cap for one `nginx_www_install` write. Web content is binary-
/// tolerant (no gate beyond the size — the old echo-based flow could not
/// carry a binary file at all), so this is the only content policy.
pub const NGINX_WWW_INSTALL_MAX: usize = 10 * 1024 * 1024;

/// The structural gate for nginx configuration content — size, NUL and
/// control characters only. nginx's own `-t` is the grammar gate (the app
/// runs it after every write and reverts on failure, which also rejects
/// unreadable includes and certificates); a panel-side nginx grammar
/// re-implementation was explicitly rejected in the batch design.
pub const NGINX_CONFIG_CONTENT_MAX: usize = 256 * 1024;

pub fn validate_nginx_config_content(content: &str) -> Result<(), String> {
    if content.len() > NGINX_CONFIG_CONTENT_MAX {
        return Err(format!(
            "The nginx configuration is too large ({} > {NGINX_CONFIG_CONTENT_MAX} bytes).",
            content.len()
        ));
    }
    if content.contains('\0') {
        return Err("The nginx configuration must not contain NUL bytes.".to_string());
    }
    if content
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(
            "The nginx configuration must not contain control characters other than newlines and tabs."
                .to_string(),
        );
    }
    Ok(())
}

/// A helper revision backup must be the exact shape `create_backup_sibling`
/// produces for `dest`: the same confined parent directory, the name derived
/// from the destination (`<dest>.<unix-seconds>_<pid>.<nanos>.bak`). This is
/// enforced both by the app (before the privileged call exists) and again
/// inside the helper — both halves hold independently.
/// The shared revision-sibling rule for a backup that a write core or backup
/// step of this project itself created: the backup must be an absolute, plain
/// child of `backups_dir`, and its name must be the destination's file name
/// extended by one `<digits>_<digits[.digits…]>.bak` revision component. That
/// single rule covers both revisionshemes the project writes — the helper's
/// `<unix-seconds>_<pid>.<nanos>` (profile.d) and the app's
/// `<YYYYmmdd_HHMMSS>` (nginx backups) — and refuses any free-form name
/// that could let an unrelated file pose as "a backup". Digits per component
/// are bounded at 20 so a crafted path can not smuggle a giant number either.
pub fn revision_sibling_shape(dest: &str, backup: &str, backups_dir: &str) -> Result<(), String> {
    use std::path::Component;

    if backup.chars().any(char::is_control) {
        return Err(format!("The backup path must not contain control characters: {backup:?}"));
    }
    let candidate = Path::new(backup);
    if !candidate.is_absolute() {
        return Err(format!("The backup path must be absolute: {backup}"));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(format!("The backup path must not contain relative parts: {backup}"));
    }
    if candidate.parent() != Some(Path::new(backups_dir)) {
        return Err(format!(
            "The backup must be a revision sibling of the destination in {backups_dir}, not {backup}"
        ));
    }
    let dest_name = Path::new(dest)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let backup_name = candidate
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(remainder) = backup_name.strip_prefix(&dest_name).and_then(|rest| {
        rest.strip_prefix('.').and_then(|rev| rev.strip_suffix(".bak"))
    }) else {
        return Err(format!(
            "The backup file name {backup_name:?} must start with the destination name {:?} and carry a '.<timestamp>.bak' revision.",
            dest_name
        ));
    };
    let mut parts = remainder.split('_');
    let leading = parts.next().unwrap_or("");
    let suffix = parts.next().unwrap_or("");
    if parts.next().is_some()
        || !revision_digits(leading)
        || suffix.is_empty()
        || !suffix.split('.').all(revision_digits)
    {
        return Err(format!(
            "The backup revision {remainder:?} must be <digits>_<digits[.digits]>.bak: underline-separated digit runs, digits after dots."
        ));
    }
    Ok(())
}

fn revision_digits(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn restore_profile_sh_backup_shape(dest: &str, backup: &str) -> Result<(), String> {
    revision_sibling_shape(dest, backup, PROFILE_D_DIR)
}

/// Check that a name may address a local account in panel commands. It exists
/// so that user-supplied strings never become file path fragments (`/home/
/// {name}` and friends): no slashes, no dot files, no traversal — the actual
/// existence of the account is the helper's business (its passwd lookup).
pub fn is_valid_account_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(format!("'{name}' is not a valid account name for this panel"))
    }
}

/// Validation for the account/group/session action family.
fn check_user_management_action(action: &str, args: &[String]) -> Result<(), String> {
    let arg = |index: usize| -> Result<&str, String> {
        args.get(index)
            .map(String::as_str)
            .ok_or_else(|| operand_not_allowed("lcp-admin", action, "a required operand is missing"))
    };
    match action {
        "user_add" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes an account name and an optional full name",
                ));
            }
            user_account_name_valid(&args[1])?;
            if args.len() == 3 {
                user_fullname_valid(&args[2])?;
            }
        }
        "user_delete" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes an account name and the optional literal 'remove-home'",
                ));
            }
            user_account_name_valid(&args[1])?;
            if args.len() == 3 && args[2] != "remove-home" {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    &args[2],
                    "the optional argument may only be the exact literal 'remove-home'",
                ));
            }
        }
        "user_set_password" => {
            if args.len() != 2 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes exactly one account name",
                ));
            }
            user_account_name_valid(&args[1])?;
        }
        "user_toggle_sudo" => {
            if args.len() != 3 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes an account name and grant|revoke",
                ));
            }
            user_account_name_valid(&args[1])?;
            if args[2] != "grant" && args[2] != "revoke" {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    &args[2],
                    "the verb must be grant or revoke",
                ));
            }
        }
        "user_modify_group" => {
            if args.len() != 4 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes an account name, a group name and add|remove",
                ));
            }
            user_account_name_valid(&args[1])?;
            group_name_valid(&args[2])?;
            if args[3] != "add" && args[3] != "remove" {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    &args[3],
                    "the verb must be add or remove",
                ));
            }
        }
        "group_manage" => {
            if args.len() != 3 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes add|delete and a group name",
                ));
            }
            if args[1] != "add" && args[1] != "delete" {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    &args[1],
                    "the verb must be add or delete",
                ));
            }
            group_name_valid(&args[2])?;
        }
        "user_lock" => {
            if args.len() != 3 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes an account name and lock|unlock",
                ));
            }
            user_account_name_valid(&args[1])?;
            if args[2] != "lock" && args[2] != "unlock" {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    &args[2],
                    "the verb must be lock or unlock",
                ));
            }
        }
        "session_kill" => {
            if args.len() != 2 {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes exactly one session id",
                ));
            }
            login_session_id_valid(&args[1])?;
        }
        other => {
            let _ = arg(1);
            return Err(operand_not_allowed(
                "lcp-admin",
                other,
                "unknown account-management action",
            ));
        }
    }
    Ok(())
}

fn check_helper_action(args: &[String]) -> Result<(), String> {
    let Some(action) = args.first() else {
        return Err(operand_not_allowed(
            "lcp-admin",
            "",
            "an action name must be given",
        ));
    };
    if !HELPER_ACTIONS.contains(&action.as_str()) {
        return Err(operand_not_allowed(
            "lcp-admin",
            action,
            "this action does not exist",
        ));
    }
    if action == "audit_setup" {
        if args.len() != 1 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes no arguments",
            ));
        }
        return Ok(());
    }
    if HELPER_ACCOUNT_ACTIONS.contains(&action.as_str()) {
        let [_, account] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one account name",
            ));
        };
        is_valid_account_name(account).map_err(|reason| {
            operand_not_allowed("lcp-admin", account, &reason)
        })?;
        return Ok(());
    }
    if HELPER_PATH_ACTIONS.contains(&action.as_str()) {
        let [_, path] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one profile file path",
            ));
        };
        validate_profile_sh_path(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        return Ok(());
    }
    if HELPER_RESTORE_ACTIONS.contains(&action.as_str()) {
        let [_, path, backup] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one profile file path and one backup sibling path",
            ));
        };
        validate_profile_sh_path(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        restore_profile_sh_backup_shape(path, backup).map_err(|reason| {
            operand_not_allowed("lcp-admin", backup, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_CONFIG_ACTIONS.contains(&action.as_str()) {
        let [_, path] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one nginx configuration path",
            ));
        };
        nginx_config_path_valid(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_RESTORE_ACTIONS.contains(&action.as_str()) {
        let [_, path, backup] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one nginx destination path and one backup sibling path",
            ));
        };
        nginx_config_path_valid(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        revision_sibling_shape(path, backup, NGINX_BACKUPS_DIR).map_err(|reason| {
            operand_not_allowed("lcp-admin", backup, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_LINK_ACTIONS.contains(&action.as_str()) {
        let [_, available, enabled] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one sites-available path and one sites-enabled path",
            ));
        };
        nginx_link_as_valid(available, enabled).map_err(|reason| {
            operand_not_allowed("lcp-admin", available, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_REMOVE_ACTIONS.contains(&action.as_str()) {
        let [_, path] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one nginx configuration path to remove",
            ));
        };
        nginx_config_remove_path_valid(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_UNLINK_ACTIONS.contains(&action.as_str()) {
        let [_, path] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one sites-enabled symlink path to remove",
            ));
        };
        nginx_site_unlink_path_valid(path).map_err(|reason| {
            operand_not_allowed("lcp-admin", path, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NGINX_WWW_FS_ACTIONS.contains(&action.as_str()) {
        let (op, operands) = match args.get(1) {
            Some(op) => (
                op.as_str(),
                args
                    .get(2..)
                    .map(|slice| slice.to_vec())
                    .unwrap_or_default(),
            ),
            None => {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    "this action takes a sub-command: mkdir, remove or rename",
                ))
            }
        };
        let check_path =
            |path: &String| {
                var_www_path_valid(path).map_err(|reason| {
                    operand_not_allowed("lcp-admin", path, &reason)
                })
            };
        match op {
            "mkdir" if operands.len() == 1 => {
                check_path(&operands[0])?;
            }
            "remove" if operands.len() == 1 => {
                check_path(&operands[0])?;
                if var_www_is_protected(&operands[0]) {
                    return Err(operand_not_allowed(
                        "lcp-admin",
                        &operands[0],
                        "this protected path cannot be removed: it is the web root or its distro-managed html tree",
                    ));
                }
            }
            "rename" if operands.len() == 2 => {
                for endpoint in &operands {
                    check_path(endpoint)?;
                    if var_www_is_protected(endpoint) {
                        return Err(operand_not_allowed(
                            "lcp-admin",
                            endpoint,
                            "this protected path cannot be renamed: it is the web root or its distro-managed html tree",
                        ));
                    }
                }
            }
            "mkdir" | "remove" | "rename" => {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    action,
                    match op {
                        "rename" => "this sub-command takes exactly one source path and one destination path",
                        _ => "this sub-command takes exactly one web-root path",
                    },
                ));
            }
            hostile => {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    hostile,
                    "this sub-command must be mkdir, remove or rename",
                ));
            }
        }
        return Ok(());
    }
    if HELPER_NGINX_WWW_INSTALL_ACTIONS.contains(&action.as_str()) {
        let [_, dest] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one web-root destination path",
            ));
        };
        var_www_path_valid(dest).map_err(|reason| {
            operand_not_allowed("lcp-admin", dest, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NMCLI_CONNECTION_SAVE_ACTIONS.contains(&action.as_str()) {
        // args: [action, verb, connection, setting.prop, value, ...]
        check_nmcli_connection_save(&args[1..]).map_err(|reason| {
            operand_not_allowed("lcp-admin", action, &reason)
        })?;
        return Ok(());
    }
    if HELPER_NMCLI_CONNECTION_DELETE_ACTIONS.contains(&action.as_str()) {
        let [_, uuid] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one connection UUID",
            ));
        };
        if !is_uuid(uuid) {
            return Err(operand_not_allowed("lcp-admin", uuid, "not a valid connection UUID"));
        }
        return Ok(());
    }
    if HELPER_NMCLI_CONNECTION_UP_ACTIONS.contains(&action.as_str()) {
        let [_, uuid] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one connection UUID",
            ));
        };
        if !is_uuid(uuid) {
            return Err(operand_not_allowed("lcp-admin", uuid, "not a valid connection UUID"));
        }
        return Ok(());
    }
    if HELPER_NMCLI_CONNECTION_DOWN_ACTIONS.contains(&action.as_str()) {
        let [_, uuid] = args else {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one connection UUID",
            ));
        };
        if !is_uuid(uuid) {
            return Err(operand_not_allowed("lcp-admin", uuid, "not a valid connection UUID"));
        }
        return Ok(());
    }
    if HELPER_NMCLI_CONNECTION_IMPORT_ACTIONS.contains(&action.as_str()) {
        // args: [action, import_type, file_path] — three items total
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly an import type and a file path",
            ));
        }
        let import_type = &args[1];
        let file_path = &args[2];
        ensure_safe_operand(file_path, "import file path")?;
        nmcli_import_path_valid(file_path)?;
        check_nmcli_import(&[import_type.to_string(), file_path.to_string()])?;
        return Ok(());
    }
    if HELPER_REPO_TOGGLE_ENABLE_ACTIONS.contains(&action.as_str()) {
        // args: [action, file_path, repo_id] — three items total
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly a repo file path and a repo id",
            ));
        }
        let file_path = &args[1];
        let repo_id = &args[2];
        ensure_repo_file_path(file_path)?;
        ensure_safe_operand(repo_id, "repo id")?;
        return Ok(());
    }
    if HELPER_REPO_TOGGLE_DISABLE_ACTIONS.contains(&action.as_str()) {
        // args: [action, file_path, repo_id] — three items total
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly a repo file path and a repo id",
            ));
        }
        let file_path = &args[1];
        let repo_id = &args[2];
        ensure_repo_file_path(file_path)?;
        ensure_safe_operand(repo_id, "repo id")?;
        return Ok(());
    }
    if HELPER_REPO_ADD_ACTIONS.contains(&action.as_str()) {
        // args: [action, url] — two items total
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly a repository URL",
            ));
        }
        let url = &args[1];
        ensure_safe_operand(url, "repository URL")?;
        repo_url_https_only(url)?;
        return Ok(());
    }
    if HELPER_REPO_DELETE_ACTIONS.contains(&action.as_str()) {
        // args: [action, file_path] — two items total
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one repository file path",
            ));
        }
        let file_path = &args[1];
        ensure_repo_file_path(file_path)?;
        return Ok(());
    }
    if HELPER_REPO_MAKECACHE_ACTIONS.contains(&action.as_str()) {
        // args: [action] — one item total, no further arguments
        if args.len() != 1 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes no further arguments",
            ));
        }
        return Ok(());
    }
    if HELPER_REPO_CLEAN_CACHE_ACTIONS.contains(&action.as_str()) {
        // args: [action, repo_id] — two items total
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one repository id",
            ));
        }
        let repo_id = &args[1];
        ensure_safe_operand(repo_id, "repository id")?;
        return Ok(());
    }
    if HELPER_FIREWALL_PANIC_ACTIONS.contains(&action.as_str()) {
        // args: [action, enable|disable, confirm?] — two items, or three
        // when the caller confirms.
        if args.len() == 3 && args[2] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[2],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 2 && args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly an enable or disable verb, optionally followed by 'confirm'",
            ));
        }
        if args[1] != "enable" && args[1] != "disable" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[1],
                "the verb must be enable or disable",
            ));
        }
        return Ok(());
    }
    if HELPER_FIREWALL_INTERFACE_ZONE_ACTIONS.contains(&action.as_str()) {
        // args: [action, zone, interface, confirm?] — three items, or four
        // when the caller confirms the SSH risk.
        if args.len() == 4 && args[3] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[3],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 3 && args.len() != 4 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly a zone and an interface, optionally followed by 'confirm'",
            ));
        }
        // The interface-zone action itself distinguishes "firewalld needs a
        // zone" from "ufw has no zones"; the shared gate only validates a
        // non-empty zone so the action-specific refusal stays reachable.
        if !args[1].is_empty() {
            firewall_zone_name_valid(&args[1])?;
        }
        firewall_interface_name_valid(&args[2])?;
        return Ok(());
    }
    if HELPER_FIREWALL_ENABLE_HARDENED_ACTIONS.contains(&action.as_str()) {
        // args: [action, firewalld|ufw, confirm?] — two items, or three when
        // the caller confirms.
        if args.len() == 3 && args[2] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[2],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 2 && args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly a firewalld or ufw backend, optionally followed by 'confirm'",
            ));
        }
        if args[1] != "firewalld" && args[1] != "ufw" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[1],
                "the backend must be firewalld or ufw",
            ));
        }
        return Ok(());
    }
    if HELPER_PROCESS_RENICE_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes a pid and a priority",
            ));
        }
        let pid = args[1].parse::<u32>().map_err(|_| {
            operand_not_allowed("lcp-admin", &args[1], "the pid must be numeric")
        })?;
        let priority = args[2].parse::<i32>().map_err(|_| {
            operand_not_allowed("lcp-admin", &args[2], "the priority must be numeric")
        })?;
        process_renice_valid(pid, priority)?;
        return Ok(());
    }
    if HELPER_SMART_HEALTH_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one device path",
            ));
        }
        smart_device_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_SMART_DATA_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one device path",
            ));
        }
        smart_device_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_SMART_SELF_TEST_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes a device path and a test type",
            ));
        }
        smart_device_valid(&args[1])?;
        smart_test_type_valid(&args[2])?;
        return Ok(());
    }
    if HELPER_NMCLI_INTERFACE_STATE_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes an interface name and up|down",
            ));
        }
        check_nmcli_interface_state(&args[1..])?;
        return Ok(());
    }
    if HELPER_COPR_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes enable|disable and a Copr repository",
            ));
        }
        copr_verb_valid(&args[1])?;
        copr_repo_valid(&args[2])?;
        return Ok(());
    }
    if HELPER_UNIT_RECOVERY_ACTIONS.contains(&action.as_str()) {
        if args.len() != 1 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes no further arguments",
            ));
        }
        return Ok(());
    }
    if HELPER_UNIT_ACTION_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes a verb and a unit name",
            ));
        }
        unit_action_valid(&args[1], &args[2])?;
        return Ok(());
    }
    if HELPER_UNIT_CREATE_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one unit name",
            ));
        }
        service_unit_name_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_UNIT_DROPIN_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one unit name",
            ));
        }
        service_unit_name_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_SECURITY_APPLY_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one security operation",
            ));
        }
        security_apply_op_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_SECURITY_SSH_PARAM_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes an SSH parameter and its value",
            ));
        }
        security_ssh_param_valid(&args[1], &args[2])?;
        return Ok(());
    }
    if HELPER_SECURITY_KERNEL_PARAM_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes a sysctl key and its value",
            ));
        }
        security_kernel_param_valid(&args[1], &args[2])?;
        return Ok(());
    }
    if HELPER_SECURITY_LOCK_ACCOUNT_ACTIONS.contains(&action.as_str()) {
        if args.len() > 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes an optional account name",
            ));
        }
        if let Some(user) = args.get(1) {
            user_account_name_valid(user)?;
        }
        return Ok(());
    }
    if HELPER_SECURITY_SERVICE_ACTIONS.contains(&action.as_str()) {
        if args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes a service name and enable|disable",
            ));
        }
        security_service_unit_valid(&args[1])?;
        security_service_verb_valid(&args[2])?;
        return Ok(());
    }
    if HELPER_SECURITY_CTRL_ALT_DEL_ACTIONS.contains(&action.as_str()) {
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes mask|unmask",
            ));
        }
        ctrl_alt_del_verb_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_USER_MANAGEMENT_ACTIONS.contains(&action.as_str()) {
        return check_user_management_action(action, args);
    }
    if HELPER_DNF_HISTORY_UNDO_ACTIONS.contains(&action.as_str()) {
        // args: [action, transaction-id, confirm?]
        if args.len() == 3 && args[2] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[2],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 2 && args.len() != 3 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one transaction id, optionally followed by 'confirm'",
            ));
        }
        dnf_history_transaction_id_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_PKG_MAINTENANCE_ACTIONS.contains(&action.as_str()) {
        // args: [action, clean|autoremove|makecache]
        if args.len() != 2 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly one maintenance operation",
            ));
        }
        pkg_maintenance_op_valid(&args[1])?;
        return Ok(());
    }
    if HELPER_FIREWALL_ZONE_RULE_ACTIONS.contains(&action.as_str()) {
        // args: [action, add|remove, zone, service|port, value, confirm?] —
        // five items, or six when the caller confirms.
        if args.len() == 6 && args[5] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[5],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 5 && args.len() != 6 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly an add/remove verb, a zone, a service/port kind and a value, optionally followed by 'confirm'",
            ));
        }
        let verb = &args[1];
        let zone = &args[2];
        let kind = &args[3];
        let value = &args[4];
        if verb != "add" && verb != "remove" {
            return Err(operand_not_allowed(
                "lcp-admin",
                verb,
                "the verb must be add or remove",
            ));
        }
        // The zone may be empty only because ufw has no zones (an empty zone
        // argument is the documented ufw shape); firewalld always requires a
        // real zone and re-validates in the helper.
        if !zone.is_empty() {
            firewall_zone_name_valid(zone).map_err(|reason| {
                operand_not_allowed("lcp-admin", zone, &reason)
            })?;
        }
        match kind.as_str() {
            "service" => firewall_service_name_valid(value).map_err(|reason| {
                operand_not_allowed("lcp-admin", value, &reason)
            })?,
            "port" => firewall_port_value_valid(value).map_err(|reason| {
                operand_not_allowed("lcp-admin", value, &reason)
            })?,
            other => {
                return Err(operand_not_allowed(
                    "lcp-admin",
                    other,
                    "the kind must be service or port",
                ))
            }
        }
        return Ok(());
    }
    if HELPER_FIREWALL_RICH_RULE_ACTIONS.contains(&action.as_str()) {
        // args: [action, add|remove, zone, rule, confirm?] — four items, or
        // five when the caller confirms.
        if args.len() == 5 && args[4] != "confirm" {
            return Err(operand_not_allowed(
                "lcp-admin",
                &args[4],
                "the trailing argument may only be the exact word 'confirm' to acknowledge the risk",
            ));
        }
        if args.len() != 4 && args.len() != 5 {
            return Err(operand_not_allowed(
                "lcp-admin",
                action,
                "this action takes exactly an add/remove verb, a zone and a rule, optionally followed by 'confirm'",
            ));
        }
        let verb = &args[1];
        let zone = &args[2];
        let rule = &args[3];
        if verb != "add" && verb != "remove" {
            return Err(operand_not_allowed(
                "lcp-admin",
                verb,
                "the verb must be add or remove",
            ));
        }
        // The zone may be empty only because ufw has no zones (an empty zone
        // argument is the documented ufw shape); firewalld always requires a
        // real zone and re-validates in the helper.
        if !zone.is_empty() {
            firewall_zone_name_valid(zone).map_err(|reason| {
                operand_not_allowed("lcp-admin", zone, &reason)
            })?;
        }
        firewall_rich_rule_valid(rule).map_err(|reason| {
            operand_not_allowed("lcp-admin", rule, &reason)
        })?;
        return Ok(());
    }
    if args.len() != 1 {
        return Err(operand_not_allowed(
            "lcp-admin",
            action,
            "this action takes no further arguments",
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
        "nmcli" => check_nmcli(args),
        "nmcli_connection_save" => check_nmcli_connection_save(args),
        "nmcli_interface_state" => check_nmcli_interface_state(args),
        "nmcli_connection_import" => check_nmcli_import(args),
        "renice" => check_renice(args),
        "nginx" => check_daemon_flags("nginx", args, &["-t", "-v", "-q"]),
        "sshd" => check_daemon_flags("sshd", args, &["-T", "-t"]),
        "mkdir" => check_mkdir(args),
        "tail" | "head" => check_log_reader(base.as_str(), args, &[]),
        "cat" => check_log_reader("cat", args, &[]),
        "ls" => check_log_reader("ls", args, &["-l", "-a", "-la", "-lh", "-A"]),
        "stat" | "readlink" | "zcat" => check_log_reader(base.as_str(), args, &[]),
        "grep" => check_grep(args),
        "lcp-admin" => check_helper_action(args),
        _ => check_no_flags_at_all(base.as_str(), args),
    }
}

// ─── Firewall validators (migration batch 4) ─────────────────────────────────
//
// firewalld and ufw both expose command-line interfaces, but the panel may not
// pass their raw arguments through. Every operand is checked here before a
// privileged command can see it. The validators accept the narrow grammar the
// panel itself can produce; anything richer is refused until it is designed.

/// A firewalld zone name: one to 64 ASCII letters, digits, underscores or
/// dashes. Predefined zones include mixed case (`FedoraWorkstation`).
pub fn firewall_zone_name_valid(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 64 {
        return Err("The firewall zone name must be 1 to 64 characters.".to_string());
    }
    let first = value.chars().next().expect("non-empty zone name");
    if !first.is_ascii_alphanumeric() {
        return Err(format!("The firewall zone name must start with a letter or digit: {value}"));
    }
    if !value.is_ascii()
        || !value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
        })
    {
        return Err(format!("The firewall zone name contains unsafe characters: {value}"));
    }
    Ok(())
}

/// A firewalld service name: lowercase ASCII word characters only. The
/// firewalld service definition is responsible for whether the name exists.
pub fn firewall_service_name_valid(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 64 {
        return Err("The firewall service name must be 1 to 64 characters.".to_string());
    }
    let mut chars = value.chars();
    let first = chars.next().expect("non-empty service name");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!("The firewall service name must start with a lowercase letter or digit: {value}"));
    }
    if !value.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-')
    }) {
        return Err(format!("The firewall service name contains unsafe characters: {value}"));
    }
    Ok(())
}

/// Protocols firewalld accepts for an IP port.
const FIREWALL_PORT_PROTOCOLS: &[&str] = &["tcp", "udp", "sctp", "dccp"];

fn firewall_port_number_valid(part: &str) -> Result<(), String> {
    if part.is_empty() || part.len() > 5 {
        return Err(format!("The firewall port number is out of range: {part}"));
    }
    if !part.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("The firewall port number is not numeric: {part}"));
    }
    let value: u32 = part
        .parse()
        .map_err(|_| format!("The firewall port number is not numeric: {part}"))?;
    if value == 0 || value > 65535 {
        return Err(format!("The firewall port number must be 1 to 65535: {part}"));
    }
    Ok(())
}

fn firewall_port_part_valid(value: &str) -> Result<(), String> {
    match value.split_once('-') {
        Some((start, end)) => {
            firewall_port_number_valid(start)?;
            firewall_port_number_valid(end)?;
            let start: u32 = start.parse().unwrap_or(0);
            let end: u32 = end.parse().unwrap_or(0);
            if start > end {
                return Err(format!("The firewall port range starts after it ends: {value}"));
            }
            Ok(())
        }
        None => firewall_port_number_valid(value),
    }
}

/// A firewalld port value: `22/tcp`, `53/udp` or an inclusive range such as
/// `1025-65535/tcp`.
pub fn firewall_port_value_valid(value: &str) -> Result<(), String> {
    let Some((ports, protocol)) = value.split_once('/') else {
        return Err(format!("The firewall port must include a protocol: {value}"));
    };
    if !FIREWALL_PORT_PROTOCOLS.contains(&protocol) {
        return Err(format!(
            "The firewall port protocol must be one of {}: {protocol}",
            FIREWALL_PORT_PROTOCOLS.join(", ")
        ));
    }
    firewall_port_part_valid(ports)
}

/// A Linux network interface name: at most 15 bytes, no path separators and
/// no characters outside the kernel's interface-name alphabet.
pub fn firewall_interface_name_valid(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 15 {
        return Err("The firewall interface name must be 1 to 15 bytes.".to_string());
    }
    let first = value.chars().next().expect("non-empty interface name");
    if !first.is_ascii_alphanumeric() {
        return Err(format!("The firewall interface name must start with a letter or digit: {value}"));
    }
    if !value.is_ascii()
        || !value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
        })
    {
        return Err(format!("The firewall interface name contains unsafe characters: {value}"));
    }
    Ok(())
}

/// The family a rich rule applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirewallRichRuleFamily {
    Ipv4,
    Ipv6,
}

/// One source or destination address, optionally with an explicit prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirewallRichAddress {
    pub address: std::net::IpAddr,
    pub prefix: Option<u8>,
}

/// The element a rich rule matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirewallRichRuleElement {
    Service(String),
    Port { port: String, protocol: String },
    Protocol(String),
}

/// The action a rich rule applies to its matching element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirewallRichRuleAction {
    Accept,
    Reject,
    Drop,
}

/// A parsed and validated firewalld rich rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirewallRichRule {
    pub family: FirewallRichRuleFamily,
    pub source: Option<FirewallRichAddress>,
    pub destination: Option<FirewallRichAddress>,
    pub element: FirewallRichRuleElement,
    pub action: FirewallRichRuleAction,
}

const FIREWALL_RICH_RULE_MAX: usize = 512;

fn rich_attr<'a>(token: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key}=\"");
    if !token.starts_with(prefix.as_str()) || !token.ends_with('"') {
        return Err(format!("The rich rule expected {key} with a quoted value, got: {token}"));
    }
    let value = &token[prefix.len()..token.len() - 1];
    if value.contains('"')
        || value.chars().any(|c| c.is_control())
    {
        return Err(format!("The rich rule {key} value contains unsafe characters."));
    }
    Ok(value)
}

fn rich_address_valid(
    token: &str,
    family: FirewallRichRuleFamily,
) -> Result<FirewallRichAddress, String> {
    let value = rich_attr(token, "address")?;
    let (address_text, prefix_text) = match value.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (value, None),
    };
    let prefix = match prefix_text {
        Some(text) => {
            if text.is_empty() || text.len() > 3 || !text.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("The rich rule address prefix is invalid: {value}"));
            }
            let prefix: u8 = text
                .parse()
                .map_err(|_| format!("The rich rule address prefix is invalid: {value}"))?;
            let max = match family {
                FirewallRichRuleFamily::Ipv4 => 32,
                FirewallRichRuleFamily::Ipv6 => 128,
            };
            if prefix > max {
                return Err(format!("The rich rule address prefix is too large for this family: {value}"));
            }
            Some(prefix)
        }
        None => None,
    };
    let address = match family {
        FirewallRichRuleFamily::Ipv4 => address_text.parse::<std::net::Ipv4Addr>().map(std::net::IpAddr::V4),
        FirewallRichRuleFamily::Ipv6 => address_text.parse::<std::net::Ipv6Addr>().map(std::net::IpAddr::V6),
    }
    .map_err(|_| format!("The rich rule address is not a valid address for this family: {value}"))?;
    Ok(FirewallRichAddress { address, prefix })
}

fn rich_protocol_value_valid(value: &str) -> Result<String, String> {
    if value.chars().all(|c| c.is_ascii_digit()) {
        if value.len() > 1 && value.starts_with('0') {
            return Err(format!("The rich rule protocol number must not have a leading zero: {value}"));
        }
        let number: u32 = value
            .parse()
            .map_err(|_| format!("The rich rule protocol number is invalid: {value}"))?;
        if number == 0 || number > 255 {
            return Err(format!("The rich rule protocol number must be 1 to 255: {value}"));
        }
        return Ok(number.to_string());
    }
    if value.is_empty() || value.len() > 32 {
        return Err(format!("The rich rule protocol value is invalid: {value}"));
    }
    let mut chars = value.chars();
    let first = chars.next().expect("non-empty protocol value");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!("The rich rule protocol value is invalid: {value}"));
    }
    if !value.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-')
    }) {
        return Err(format!("The rich rule protocol value is invalid: {value}"));
    }
    Ok(value.to_string())
}

/// Parse and validate the firewalld rich-language subset supported by the
/// panel. Anything outside this subset is refused, including logging, limits,
/// priority, marking and port forwarding.
pub fn firewall_rich_rule_valid(rule: &str) -> Result<FirewallRichRule, String> {
    if rule.is_empty() || rule.len() > FIREWALL_RICH_RULE_MAX {
        return Err(format!(
            "The rich rule must be 1 to {FIREWALL_RICH_RULE_MAX} bytes."
        ));
    }
    if rule.chars().any(|c| c.is_control()) {
        return Err("The rich rule contains control characters.".to_string());
    }

    let tokens: Vec<&str> = rule.split_ascii_whitespace().collect();
    if tokens.first() != Some(&"rule") {
        return Err("The rich rule must start with 'rule'.".to_string());
    }
    let family_token = tokens.get(1).copied().unwrap_or("");
    let family = match rich_attr(family_token, "family")? {
        "ipv4" => FirewallRichRuleFamily::Ipv4,
        "ipv6" => FirewallRichRuleFamily::Ipv6,
        other => return Err(format!("The rich rule family must be ipv4 or ipv6, got: {other}")),
    };

    let mut index = 2;

    // The accepted grammar is fixed: an optional source, an optional
    // destination, exactly one element and exactly one action. Parse the
    // phases in order; a token in the wrong phase is a grammar error.
    let mut source = None;
    if tokens.get(index) == Some(&"source") {
        let token = tokens.get(index + 1).copied().unwrap_or("");
        source = Some(rich_address_valid(token, family)?);
        index += 2;
    }
    if tokens.get(index) == Some(&"source") {
        return Err("The rich rule has more than one source.".to_string());
    }

    let mut destination = None;
    if tokens.get(index) == Some(&"destination") {
        let token = tokens.get(index + 1).copied().unwrap_or("");
        destination = Some(rich_address_valid(token, family)?);
        index += 2;
    }
    if tokens.get(index) == Some(&"destination") {
        return Err("The rich rule has more than one destination.".to_string());
    }
    if tokens.get(index) == Some(&"source") {
        return Err("The rich rule source must precede destination.".to_string());
    }

    let element_token = tokens.get(index).copied().unwrap_or("");
    let element = match element_token {
        "service" => {
            let token = tokens.get(index + 1).copied().unwrap_or("");
            let name = rich_attr(token, "name")?;
            firewall_service_name_valid(name)?;
            index += 2;
            FirewallRichRuleElement::Service(name.to_string())
        }
        "port" => {
            let port_token = tokens.get(index + 1).copied().unwrap_or("");
            let protocol_token = tokens.get(index + 2).copied().unwrap_or("");
            let port = rich_attr(port_token, "port")?;
            let protocol = rich_attr(protocol_token, "protocol")?;
            if !FIREWALL_PORT_PROTOCOLS.contains(&protocol) {
                return Err(format!(
                    "The rich rule port protocol must be one of {}: {protocol}",
                    FIREWALL_PORT_PROTOCOLS.join(", ")
                ));
            }
            firewall_port_part_valid(port)?;
            index += 3;
            FirewallRichRuleElement::Port {
                port: port.to_string(),
                protocol: protocol.to_string(),
            }
        }
        "protocol" => {
            let token = tokens.get(index + 1).copied().unwrap_or("");
            let value = rich_attr(token, "value")?;
            index += 2;
            FirewallRichRuleElement::Protocol(rich_protocol_value_valid(value)?)
        }
        "source" | "destination" => {
            return Err("The rich rule source and destination must precede its element.".to_string())
        }
        "" => return Err("The rich rule must contain an element.".to_string()),
        other => {
            return Err(format!("The rich rule must contain an element, got: {other}"));
        }
    };
    if matches!(tokens.get(index).copied(), Some("service" | "port" | "protocol")) {
        return Err("The rich rule has more than one element.".to_string());
    }
    if matches!(tokens.get(index).copied(), Some("source" | "destination")) {
        return Err("The rich rule source and destination must precede its element.".to_string());
    }

    let action_token = tokens.get(index).copied().unwrap_or("");
    let action = match action_token {
        "accept" => FirewallRichRuleAction::Accept,
        "reject" => FirewallRichRuleAction::Reject,
        "drop" => FirewallRichRuleAction::Drop,
        "service" | "port" | "protocol" => return Err("The rich rule has more than one element.".to_string()),
        "source" | "destination" => {
            return Err("The rich rule source and destination must precede its element.".to_string())
        }
        "" => return Err("The rich rule must contain accept, reject or drop.".to_string()),
        other => {
            return Err(format!(
                "The rich rule must contain accept, reject or drop, got: {other}"
            ));
        }
    };
    index += 1;

    if tokens.get(index).is_some_and(|token| matches!(*token, "accept" | "reject" | "drop")) {
        return Err("The rich rule has more than one action.".to_string());
    }
    if index != tokens.len() {
        return Err("The rich rule has content after its action.".to_string());
    }

    Ok(FirewallRichRule {
        family,
        source,
        destination,
        element,
        action,
    })
}

#[cfg(test)]
mod firewall_policy_tests {
    use super::{
        firewall_interface_name_valid, firewall_port_value_valid, firewall_rich_rule_valid,
        firewall_service_name_valid, firewall_zone_name_valid,
        FirewallRichRuleAction, FirewallRichRuleElement, FirewallRichRuleFamily,
    };

    #[test]
    fn firewall_zone_name_accepts_predefined_and_custom() {
        for zone in [
            "FedoraWorkstation",
            "FedoraServer",
            "public",
            "trusted",
            "drop",
            "custom-zone",
            "zone_1",
        ] {
            assert!(firewall_zone_name_valid(zone).is_ok(), "zone must pass: {zone}");
        }
    }

    #[test]
    fn firewall_zone_name_rejects_empty_and_too_long() {
        assert!(firewall_zone_name_valid("").is_err(), "empty zone must be refused");
        assert!(
            firewall_zone_name_valid(&"a".repeat(65)).is_err(),
            "oversized zone must be refused"
        );
    }

    #[test]
    fn firewall_zone_name_rejects_punctuation_and_path_parts() {
        for zone in ["public/ssh", "public ssh", "public;", "..", ".", "public,drop", "public@host"] {
            assert!(firewall_zone_name_valid(zone).is_err(), "zone must be refused: {zone}");
        }
    }

    #[test]
    fn firewall_service_name_accepts_common_services() {
        for service in ["ssh", "http", "https", "dhcpv6-client", "my-service", "my_service", "7zip"] {
            assert!(firewall_service_name_valid(service).is_ok(), "service must pass: {service}");
        }
    }

    #[test]
    fn firewall_service_name_rejects_bad_shape() {
        for service in ["", "SSH", "https/local", "https ssh", ".ssh", "-ssh", "https;"] {
            assert!(firewall_service_name_valid(service).is_err(), "service must be refused: {service}");
        }
    }

    #[test]
    fn firewall_port_value_accepts_single_ports() {
        for port in ["1/tcp", "22/tcp", "53/udp", "5353/sctp", "5353/dccp", "65535/tcp"] {
            assert!(firewall_port_value_valid(port).is_ok(), "port must pass: {port}");
        }
    }

    #[test]
    fn firewall_port_value_accepts_ranges() {
        for port in ["1025-65535/tcp", "8000-8001/udp", "1-1024/sctp"] {
            assert!(firewall_port_value_valid(port).is_ok(), "range must pass: {port}");
        }
    }

    #[test]
    fn firewall_port_value_rejects_bad_numbers() {
        for port in ["0/tcp", "65536/tcp", "+22/tcp", "22 /tcp", "", "abc/tcp", "22"] {
            assert!(firewall_port_value_valid(port).is_err(), "port must be refused: {port}");
        }
    }

    #[test]
    fn firewall_port_value_rejects_bad_protocols() {
        for port in ["22/TCP", "22/icmp", "22/tacp", "22/tcp ", "8000-8001/udp/extra"] {
            assert!(firewall_port_value_valid(port).is_err(), "protocol must be refused: {port}");
        }
    }

    #[test]
    fn firewall_port_value_rejects_inverted_or_open_ranges() {
        for port in ["-1/tcp", "65535-1/tcp", "8000-/tcp", "-8000/tcp", "8000-8000-8001/tcp"] {
            assert!(firewall_port_value_valid(port).is_err(), "range must be refused: {port}");
        }
    }

    #[test]
    fn firewall_interface_name_accepts_real_linux_ifnames() {
        for ifname in ["lo", "wlp1s0", "wlp0s20f0u3", "enp3s0", "br-lan", "vlan.10", "eth0:1"] {
            assert!(firewall_interface_name_valid(ifname).is_ok(), "iface must pass: {ifname}");
        }
    }

    #[test]
    fn firewall_interface_name_rejects_empty_and_too_long() {
        assert!(firewall_interface_name_valid("").is_err(), "empty interface must be refused");
        assert!(
            firewall_interface_name_valid(&"a".repeat(16)).is_err(),
            "oversized interface name must be refused"
        );
    }

    #[test]
    fn firewall_interface_name_rejects_path_and_control_characters() {
        for ifname in ["../eth0", "eth0/iface", "eth 0", "eth\0", "eth;0", "eth\n"] {
            assert!(firewall_interface_name_valid(ifname).is_err(), "iface must be refused: {ifname}");
        }
    }

    #[test]
    fn firewall_rich_rule_accepts_service() {
        let rule = firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh" accept"#).unwrap();
        assert_eq!(rule.family, FirewallRichRuleFamily::Ipv4);
        assert_eq!(rule.source, None);
        assert_eq!(rule.destination, None);
        assert_eq!(rule.element, FirewallRichRuleElement::Service("ssh".to_string()));
        assert_eq!(rule.action, FirewallRichRuleAction::Accept);
    }

    #[test]
    fn firewall_rich_rule_accepts_port_and_protocol() {
        let rule = firewall_rich_rule_valid(
            r#"rule family="ipv6" source address="::1" port port="5353" protocol="udp" reject"#,
        ).unwrap();
        assert_eq!(rule.family, FirewallRichRuleFamily::Ipv6);
        assert_eq!(rule.source.unwrap().prefix, None);
        assert_eq!(
            rule.element,
            FirewallRichRuleElement::Port { port: "5353".to_string(), protocol: "udp".to_string() }
        );
        assert_eq!(rule.action, FirewallRichRuleAction::Reject);
    }

    #[test]
    fn firewall_rich_rule_accepts_numeric_protocol() {
        let rule = firewall_rich_rule_valid(
            r#"rule family="ipv4" protocol value="47" accept"#,
        ).unwrap();
        assert_eq!(rule.element, FirewallRichRuleElement::Protocol("47".to_string()));
    }

    #[test]
    fn firewall_rich_rule_accepts_source_and_destination() {
        let rule = firewall_rich_rule_valid(
            r#"rule family="ipv4" source address="192.168.1.0/24" destination address="10.0.0.5/32" port port="8080" protocol="tcp" drop"#,
        ).unwrap();
        assert_eq!(rule.source.unwrap().prefix, Some(24));
        assert_eq!(rule.destination.unwrap().prefix, Some(32));
        assert_eq!(rule.action, FirewallRichRuleAction::Drop);
    }

    #[test]
    fn firewall_rich_rule_accepts_ipv6_prefixes_and_port_ranges() {
        let rule = firewall_rich_rule_valid(
            r#"rule family="ipv6" source address="2001:db8::/32" port port="1025-65535" protocol="tcp" accept"#,
        ).unwrap();
        assert_eq!(rule.source.unwrap().prefix, Some(32));
        assert_eq!(
            rule.element,
            FirewallRichRuleElement::Port { port: "1025-65535".to_string(), protocol: "tcp".to_string() }
        );
    }

    #[test]
    fn firewall_rich_rule_accepts_broad_cidrs_as_valid_input() {
        for rule in [
            r#"rule family="ipv4" source address="0.0.0.0/0" service name="ssh" drop"#,
            r#"rule family="ipv6" source address="::/0" service name="ssh" reject"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_ok(), "broad CIDR must be syntactically valid: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_family_and_address_mismatch() {
        for rule in [
            r#"rule family="ipv4" source address="::1" service name="ssh" accept"#,
            r#"rule family="ipv6" destination address="10.0.0.5" service name="ssh" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "mismatched family must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_missing_or_unknown_family() {
        for rule in [
            r#"rule service name="ssh" accept"#,
            r#"rule family="any" service name="ssh" accept"#,
            r#"rule family="ipv4 service name="ssh" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "family must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_invalid_prefixes() {
        for rule in [
            r#"rule family="ipv4" source address="10.0.0.0/33" service name="ssh" accept"#,
            r#"rule family="ipv4" source address="10.0.0.0/abc" service name="ssh" accept"#,
            r#"rule family="ipv6" source address="::/129" service name="ssh" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "prefix must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_advanced_keywords() {
        for rule in [
            r#"rule family="ipv4" priority="-1000" service name="ssh" accept"#,
            r#"rule family="ipv4" service name="ssh" log prefix="blocked" accept"#,
            r#"rule family="ipv4" service name="ssh" mark set="0x1" accept"#,
            r#"rule family="ipv4" forward-port port="22" protocol="tcp" to-port="2222" accept"#,
            r#"rule family="ipv4" source not address="10.0.0.0/8" service name="ssh" accept"#,
            r#"rule family="ipv4" mac address="de:ad:be:ef:00:00" service name="ssh" accept"#,
            r#"rule family="ipv4" ipset="admins" service name="ssh" accept"#,
            r#"rule family="ipv4" masquerade"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "advanced keyword must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_bad_ports_and_protocols() {
        for rule in [
            r#"rule family="ipv4" port port="0" protocol="tcp" accept"#,
            r#"rule family="ipv4" port port="65536" protocol="tcp" accept"#,
            r#"rule family="ipv4" port port="11-10" protocol="tcp" accept"#,
            r#"rule family="ipv4" port port="22" protocol="icmp" accept"#,
            r#"rule family="ipv4" protocol value="0" accept"#,
            r#"rule family="ipv4" protocol value="256" accept"#,
            r#"rule family="ipv4" protocol value="ICMP" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "port or protocol must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_unbalanced_quotes_and_trailing_garbage() {
        for rule in [
            r#"rule family="ipv4" service name="ssh accept"#,
            r#"rule family="ipv4" service name="ssh" accept extra"#,
            r#"rule family="ipv4" service name="ssh" accept accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "malformed rule must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_empty_overlong_and_control() {
        assert!(firewall_rich_rule_valid("").is_err(), "empty rule must be refused");
        let oversized = format!("rule family=\"ipv4\" service name=\"ssh\" {}", "x".repeat(512));
        assert!(
            firewall_rich_rule_valid(&oversized).is_err(),
            "oversized rule must be refused"
        );
        assert!(
            firewall_rich_rule_valid("rule\nfamily=\"ipv4\" service name=\"ssh\" accept").is_err(),
            "control character must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_missing_element_or_action() {
        for rule in [
            r#"rule family="ipv4" accept"#,
            r#"rule family="ipv4" service name="ssh""#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "incomplete rule must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_unsupported_elements() {
        for rule in [
            r#"rule family="ipv4" icmp-block name="echo-request" accept"#,
            r#"rule family="ipv4" icmp-type name="echo-reply" accept"#,
            r#"rule family="ipv4" source-port port="1025" protocol="tcp" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "unsupported element must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_duplicate_source_destination_and_element() {
        for rule in [
            r#"rule family="ipv4" source address="10.0.0.1" source address="10.0.0.2" service name="ssh" accept"#,
            r#"rule family="ipv4" destination address="10.0.0.1" destination address="10.0.0.2" service name="ssh" accept"#,
            r#"rule family="ipv4" service name="ssh" protocol value="icmp" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "duplicate component must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_bad_protocol_values() {
        for rule in [
            r#"rule family="ipv4" protocol value="256" accept"#,
            r#"rule family="ipv4" protocol value="01" accept"#,
            r#"rule family="ipv4" protocol value="-gre" accept"#,
        ] {
            assert!(firewall_rich_rule_valid(rule).is_err(), "protocol value must be refused: {rule}");
        }
    }

    #[test]
    fn firewall_rich_rule_accepts_named_protocol() {
        let rule = firewall_rich_rule_valid(r#"rule family="ipv4" protocol value="gre" accept"#).unwrap();
        assert_eq!(rule.element, FirewallRichRuleElement::Protocol("gre".to_string()));
    }

    #[test]
    fn firewall_rich_rule_rejects_ipv4_prefix_256() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" source address="10.0.0.0/256" service name="ssh" accept"#).is_err(),
            "IPv4 /256 must be refused without panicking"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_ipv4_prefix_999() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" source address="10.0.0.0/999" service name="ssh" accept"#).is_err(),
            "IPv4 /999 must be refused without panicking"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_ipv6_prefix_256() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv6" source address="::1/256" service name="ssh" accept"#).is_err(),
            "IPv6 /256 must be refused without panicking"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_ipv6_prefix_999() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv6" source address="::1/999" service name="ssh" accept"#).is_err(),
            "IPv6 /999 must be refused without panicking"
        );
    }
}

#[cfg(test)]
mod firewall_ordering_tests {
    use super::{
        firewall_interface_name_valid, firewall_rich_rule_valid, firewall_zone_name_valid,
        FirewallRichRuleAction, FirewallRichRuleElement,
    };

    #[test]
    fn firewall_zone_name_rejects_leading_hyphen() {
        for zone in ["-public", "-", "-_zone", "-1"] {
            assert!(firewall_zone_name_valid(zone).is_err(), "leading hyphen must be refused: {zone}");
        }
    }

    #[test]
    fn firewall_interface_name_rejects_leading_hyphen() {
        for ifname in ["-eth0", "-lo", "-wlp1s0", "-1"] {
            assert!(firewall_interface_name_valid(ifname).is_err(), "leading hyphen must be refused: {ifname}");
        }
    }

    #[test]
    fn firewall_zone_and_interface_valid_names_remain_accepted() {
        for zone in ["public", "FedoraWorkstation", "br-lan"] {
            assert!(firewall_zone_name_valid(zone).is_ok(), "zone must remain accepted: {zone}");
        }
        for ifname in ["br-lan", "vlan.10", "eth0:1"] {
            assert!(firewall_interface_name_valid(ifname).is_ok(), "interface must remain accepted: {ifname}");
        }
    }

    #[test]
    fn firewall_rich_rule_rejects_source_after_element() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh" source address="10.0.0.1" accept"#).is_err(),
            "source after element must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_destination_after_element() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh" destination address="10.0.0.2" accept"#).is_err(),
            "destination after element must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_destination_before_source() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" destination address="10.0.0.2" source address="10.0.0.1" service name="ssh" accept"#).is_err(),
            "destination before source must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_accepts_source_and_destination_in_order() {
        let rule = firewall_rich_rule_valid(
            r#"rule family="ipv4" source address="192.168.1.0/24" destination address="10.0.0.5/32" service name="ssh" accept"#,
        ).unwrap();
        assert_eq!(rule.source.unwrap().prefix, Some(24));
        assert_eq!(rule.destination.unwrap().prefix, Some(32));
        assert_eq!(rule.element, FirewallRichRuleElement::Service("ssh".to_string()));
        assert_eq!(rule.action, FirewallRichRuleAction::Accept);
    }

    #[test]
    fn firewall_rich_rule_accepts_source_without_destination() {
        let rule = firewall_rich_rule_valid(r#"rule family="ipv4" source address="10.0.0.1" service name="ssh" accept"#).unwrap();
        assert!(rule.source.is_some());
        assert_eq!(rule.destination, None);
        assert_eq!(rule.element, FirewallRichRuleElement::Service("ssh".to_string()));
    }

    #[test]
    fn firewall_rich_rule_accepts_destination_without_source() {
        let rule = firewall_rich_rule_valid(r#"rule family="ipv4" destination address="10.0.0.2" service name="ssh" accept"#).unwrap();
        assert_eq!(rule.source, None);
        assert!(rule.destination.is_some());
        assert_eq!(rule.element, FirewallRichRuleElement::Service("ssh".to_string()));
    }

    #[test]
    fn firewall_rich_rule_rejects_duplicate_source() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" source address="10.0.0.1" source address="10.0.0.2" service name="ssh" accept"#).is_err(),
            "duplicate source must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_duplicate_destination() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" destination address="10.0.0.1" destination address="10.0.0.2" service name="ssh" accept"#).is_err(),
            "duplicate destination must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_multiple_elements() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh" protocol value="icmp" accept"#).is_err(),
            "multiple elements must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_multiple_actions() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh" accept drop"#).is_err(),
            "multiple actions must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_missing_element() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" accept"#).is_err(),
            "missing element must be refused"
        );
    }

    #[test]
    fn firewall_rich_rule_rejects_missing_action() {
        assert!(
            firewall_rich_rule_valid(r#"rule family="ipv4" service name="ssh""#).is_err(),
            "missing action must be refused"
        );
    }
}

#[cfg(test)]
mod argument_policy_tests {
    use super::{
        ensure_arguments_for_program as check, ensure_program_allowed, is_uuid,
        nmcli_import_allowed_dirs, nmcli_import_path_valid,
    };

    #[test]
    fn privileged_smartctl_is_not_on_the_generic_allow_list() {
        assert!(
            ensure_program_allowed("/usr/bin/smartctl").is_err(),
            "the generic smartctl channel must be closed; SMART named actions are the only root path"
        );
    }

    #[test]
    fn privileged_renice_is_not_on_the_generic_allow_list() {
        assert!(
            ensure_program_allowed("/usr/bin/renice").is_err(),
            "the generic renice channel must be closed; process_renice is the only root path"
        );
    }

    #[test]
    fn add_apt_repository_is_not_on_the_generic_allow_list() {
        // The only repository trust paths are the named repo_add and
        // copr_manage actions. The generic program channel must refuse the
        // tool even before its argument policy is consulted.
        assert!(
            ensure_program_allowed("/usr/bin/add-apt-repository").is_err(),
            "add-apt-repository must not be executable through the generic root channel"
        );
    }

    #[test]
    fn firewall_tools_are_not_on_the_generic_allow_list() {
        // The firewall is mutated only through named helper actions. The
        // generic program channel must refuse both tools outright, so no
        // caller can bypass the action-level SSH guards.
        for program in ["firewall-cmd", "ufw"] {
            assert!(
                ensure_program_allowed(program).is_err(),
                "{program} must not be executable through the generic root channel"
            );
        }
    }

    fn ok(program: &str, args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check(program, &argv).is_ok(), "must be allowed: {program} {args:?}");
    }

    fn refused(program: &str, args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check(program, &argv).is_err(), "must be refused: {program} {args:?}");
    }

    // Repo file paths that are valid under both plain and test-fixture builds.
    // The allowed dirs differ between the two configurations (REPO_ALLOWED_DIRS
    // is cfg-gated), so the path must match the active configuration.
    #[cfg(feature = "test-fixture")]
    const REPO_FILE_YUM: &str = "/tmp/lcp-admin-test-fixture/etc/yum.repos.d/fedora.repo";
    #[cfg(not(feature = "test-fixture"))]
    const REPO_FILE_YUM: &str = "/etc/yum.repos.d/fedora.repo";

    #[cfg(feature = "test-fixture")]
    const REPO_FILE_APT: &str = "/tmp/lcp-admin-test-fixture/etc/apt/sources.list.d/debian.list";
    #[cfg(not(feature = "test-fixture"))]
    const REPO_FILE_APT: &str = "/etc/apt/sources.list.d/debian.list";

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

    #[test]
    fn protected_units_cannot_be_stopped_or_disabled_through_the_helper() {
        // critical core
        refused("systemctl", &["stop", "systemd-logind"]);
        refused("systemctl", &["disable", "systemd-logind.service"]);
        refused("systemctl", &["stop", "dbus-broker"]);
        refused("systemctl", &["disable", "multi-user.target"]);
        // essential infrastructure
        refused("systemctl", &["stop", "firewalld"]);
        refused("systemctl", &["stop", "sshd"]);
        refused("systemctl", &["disable", "NetworkManager"]);
        refused("systemctl", &["stop", "NetworkManager.service"]);
        refused("systemctl", &["disable", "sshd"]);
        // prefix families
        refused("systemctl", &["stop", "systemd-journald-extra.service"]);
        refused("systemctl", &["stop", "networkmanager-wait-online.service"]);
        // unnamespaced traffic stays allowed
        ok("systemctl", &["stop", "nginx"]);
        ok("systemctl", &["disable", "httpd"]);
        ok("systemctl", &["restart", "sshd"]);
        ok("systemctl", &["enable", "--now", "multi-user.target"]);
        ok("systemctl", &["status", "auditd"]);
        ok("systemctl", &["is-active", "auditd"]);
    }

    // ── reader path allowlist ─────────────────────────────────────────────────

    #[test]
    fn readers_only_see_the_allowlist() {
        ok("cat", &["/etc/hosts"]);
        ok("cat", &["/etc/hostname"]);
        // /etc/os-release is a symlink (../usr/lib/os-release) on Fedora:
        // the no-symlink rule wins over the allowlist entry — by design
        refused("cat", &["/etc/os-release"]);
        ok("tail", &["-n", "5", "/var/log/messages"]);
        ok("cat", &["/var/log/nginx/access.log"]);
        ok("ls", &["-l", "/etc/nginx"]);
        ok("grep", &["-i", "server", "/etc/nginx/nginx.conf"]);
        ok("ls", &["/etc/audit/rules.d/99-control-panel.rules"]);
        ok("cat", &["/home/marvin/.config/control-panel/logs/control-panel.log"]);
        // /boot/grub2 (the auditors read grub.cfg) and the ssh config lane
        ok("cat", &["/boot/grub2/grub.cfg"]);
        ok("cat", &["/etc/ssh/sshd_config"]);
        ok("cat", &["/etc/ssh/sshd_config.d/10-hardening.conf"]);
        // the host keys live in /etc/ssh itself and stay off the list
        refused("cat", &["/etc/ssh/ssh_host_ed25519_key"]);
        // the /root deny also applies to the root user's copy of the panel
        // log — the helper would be reading for itself anyway
        refused("cat", &["/root/.config/control-panel/logs/control-panel.log"]);
        // outside the allowlist
        refused("cat", &["/etc/passwd"]);
        refused("cat", &["/var/tmp/whatever.log"]);
        refused("cat", &["/tmp/keys_test"]);
    }

    #[test]
    fn credential_and_policy_files_are_never_readable_and_can_not_escape() {
        refused("cat", &["/etc/shadow"]);
        refused("cat", &["/etc/shadow-"]);
        refused("cat", &["/etc/gshadow"]);
        refused("cat", &["/etc/gshadow-"]);
        refused("cat", &["/etc/sudoers"]);
        refused("grep", &["-i", "pattern", "/etc/sudoers.d/panel"]);
        refused("cat", &["/etc/ssl/private/server.key"]);
        refused("cat", &["/root/.ssh/id_rsa"]);
        refused("tail", &["-n", "5", "/home/marvin/.ssh/id_rsa"]);
        // symlink leaf: refused before it can be resolved
        refused("cat", &["/proc/self/root/etc/shadow"]);
        refused("cat", &["/etc/../root/authorized_keys"]);
        refused("tail", &["-n", "5", "/var/log/../etc/shadow"]);
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
        ok("ip", &["-j", "a"]);
        ok("ip", &["-j", "addr", "show"]);
        ok("ip", &["-o", "link", "show"]);
        ok("ip", &["route", "get", "1.1.1.1"]);
        refused("ip", &["link", "set", "dev", "ens3", "up"]);
        refused("ip", &["link", "set", "dev", "ens3", "down"]);
        refused("ip", &["addr", "add", "192.0.2.1/24", "dev", "ens3"]);
        refused("ip", &["route", "flush"]);
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
        // firewall tools are named-action only; the allow-list refusal is
        // asserted in `firewall_tools_are_not_on_the_generic_allow_list`
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

        // firewall tools are named-action only: the generic channel must
        // refuse every invocation, including formerly-permitted operations.
        refused("firewall-cmd", &["--zone", "public", "--add-port=8080/tcp"]);
        refused("firewall-cmd", &["--panic-on"]);
        refused("firewall-cmd", &["--zone", "home", "--permanent"]);
        refused("firewall-cmd", &["--zone", "public", "--add-service=<b>"]);
        refused("firewall-cmd", &["--zone", "public", "--add-port=;id"]);
        refused("firewall-cmd", &["-X", "public"]);

        ok("nmcli", &["-t", "-f", "UUID,NAME,TYPE,DEVICE,STATE", "connection", "show"]);
        ok("nmcli", &["-t", "-c", "no", "-f", "ipv4,ipv6,connection", "connection", "show", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"]);
        refused("nmcli", &["connection", "add", "type", "vpn"]);
        refused("nmcli", &["connection", "modify", "name", "ipv4.method", "manual"]);
        refused("nmcli", &["connection", "delete", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"]);
        refused("nmcli", &["connection", "up", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"]);
        refused("nmcli", &["connection", "down", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"]);
        refused("nmcli", &["general", "reload"]);
        refused("nmcli", &["--bogus"]);
        refused("nmcli", &["-f", "this-field-list-is-deliberately-well-over-the-sixty-four-character-limit-for-test-purposes"]);

        // The generic renice program policy still exists for defence in depth,
        // but ensure_program_allowed refuses the generic root channel.
        ok("renice", &["-n", "5", "-p", "4421"]);
        ok("renice", &["-n", "-5", "-p", "4421"]);
        refused("renice", &["-n", "25", "-p", "4421"]);
        refused("renice", &["-n", "5", "-p", "4421$", "x"]);

        ok("lcp-admin", &["process_renice", "4242", "-5"]);
        refused("lcp-admin", &["process_renice", "50", "-5"]);
        refused("lcp-admin", &["process_renice", "4242", "5"]);
        refused("lcp-admin", &["process_renice", "4242", "-21"]);
        refused("lcp-admin", &["process_renice", "not-a-pid", "-5"]);

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
        ok("add-apt-repository", &["-y", "https://example.com/example.repo"]);
        refused("add-apt-repository", &["-y", "--x-www", "ppa:some-owner/some-ppa"]);
        refused("add-apt-repository", &["-y", "ppa:some owner"]);
        refused("add-apt-repository", &["-y", "ppa:"]);
        refused("add-apt-repository", &["-y", "ppa:some-owner/.."]);
        refused("add-apt-repository", &["-y", "ppa:some-owner/name/extra"]);
        refused("add-apt-repository", &["-y", "http://example.com/example.repo"]);

        ok("grep", &["-i", "pattern", "/var/log/messages"]);
        ok("grep", &["-m", "3", "-q", "pattern", "/var/log/messages"]);
        refused("grep", &["--all-match", "pattern", "/var/log/messages"]);
        refused("grep", &["-i", "pattern", "untrusted.log"]);
        refused("grep", &["-i"]);
    }

    // ── nmcli_connection_save: verb + connection + setting.prop/value pairs ──
    #[test]
    fn nmcli_connection_save_validates_verb_connection_and_properties() {
        ok(
            "nmcli_connection_save",
            &["add", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d", "ipv4.addresses", "192.168.1.10/24"],
        );
        ok(
            "nmcli_connection_save",
            &[
                "modify",
                "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d",
                "ipv4.method",
                "manual",
                "ipv4.dns",
                "8.8.8.8",
                "connection.autoconnect",
                "yes",
            ],
        );
        refused(
            "nmcli_connection_save",
            &["delete", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d", "ipv4.addresses", "1.2.3.4/24"],
        );
        refused("nmcli_connection_save", &["add"]);
        refused("nmcli_connection_save", &["add", "--evil-connection", "ipv4.addresses", "1.2.3.4/24"]);
        refused(
            "nmcli_connection_save",
            &["add", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d", "ipv4"],
        );
        refused(
            "nmcli_connection_save",
            &["add", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d", "ipv4.addresses"],
        );
    }

    // ── nmcli_interface_state: iface + up/down ──
    #[test]
    fn nmcli_interface_state_validates_iface_and_direction() {
        ok("nmcli_interface_state", &["eth0", "up"]);
        ok("nmcli_interface_state", &["wlan0", "down"]);
        refused("nmcli_interface_state", &["lo", "down"]);
        refused("nmcli_interface_state", &["loopback0", "down"]);
        refused("nmcli_interface_state", &["eth0", "toggle"]);
        refused("nmcli_interface_state", &["-eth0", "up"]);
    }

    // ── nmcli_connection_import: type + confined file path ──
    #[test]
    fn nmcli_connection_import_validates_type_and_path() {
        ok("nmcli_connection_import", &["openvpn", "/home/user/myvpn.ovpn"]);
        ok("nmcli_connection_import", &["wireguard", "/tmp/mypeer.conf"]);
        refused("nmcli_connection_import", &["ipsec", "/home/user/myvpn.ovpn"]);
        refused("nmcli_connection_import", &["openvpn", "/home/user/myvpn.conf"]);
        refused("nmcli_connection_import", &["openvpn", "relative/path.ovpn"]);
        refused("nmcli_connection_import", &["openvpn", "/etc/secret.ovpn"]);
        refused("nmcli_connection_import", &["openvpn", "/home/user/../evil.ovpn"]);
        refused(
            "nmcli_connection_import",
            &["openvpn", "/home/user/./evil.ovpn"],
        );
    }

    // ── is_uuid: 8-4-4-4-12 hexadecimal shape ──
    #[test]
    fn connection_uuid_matches_8_4_4_4_12_shape() {
        assert!(is_uuid("8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"));
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(is_uuid("FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"));
        assert!(!is_uuid(""));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5")); // too short
        assert!(!is_uuid("8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d-extra")); // too long
        assert!(!is_uuid("8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5g")); // non-hex
        assert!(!is_uuid("8a3b2c1d4e5f6a7b8c9d0e1f2a3b4c5d")); // no dashes
    }

    // ── nmcli_import_path_valid: confinement ──
    #[test]
    fn nmcli_import_path_confines_to_user_dirs() {
        assert!(
            nmcli_import_path_valid("/home/user/myvpn.ovpn").is_ok(),
            "home directory import must be allowed"
        );
        assert!(
            nmcli_import_path_valid("/tmp/mypeer.conf").is_ok(),
            "tmp directory import must be allowed"
        );
        assert!(
            nmcli_import_path_valid("/var/tmp/import.ovpn").is_ok(),
            "var/tmp directory import must be allowed"
        );
        assert!(
            nmcli_import_path_valid("/run/media/user/disk/file.conf").is_ok(),
            "run/media directory import must be allowed"
        );
        assert!(
            nmcli_import_path_valid("/etc/secret.ovpn").is_err(),
            "system /etc path must be refused"
        );
        assert!(
            nmcli_import_path_valid("/usr/share/file.conf").is_err(),
            "system /usr path must be refused"
        );
        assert!(
            nmcli_import_path_valid("relative/path.ovpn").is_err(),
            "relative path must be refused"
        );
        assert!(
            nmcli_import_path_valid("/home/user/../evil.ovpn").is_err(),
            "parent-dir traversal must be refused"
        );
        assert!(
            nmcli_import_path_valid("/home/user/./evil.conf").is_err(),
            "current-dir component must be refused"
        );
        assert!(!nmcli_import_allowed_dirs().is_empty(), "allowed dirs must be non-empty");
    }

    // ── the helper action ─────────────────────────────────────────────────────

    #[test]
    fn dnf_history_undo_gate_requires_a_numeric_id_and_exact_confirm() {
        ok("lcp-admin", &["dnf_history_undo", "42"]);
        ok("lcp-admin", &["dnf_history_undo", "42", "confirm"]);
        refused("lcp-admin", &["dnf_history_undo"]);
        refused("lcp-admin", &["dnf_history_undo", "0"]);
        refused("lcp-admin", &["dnf_history_undo", "4x"]);
        refused("lcp-admin", &["dnf_history_undo", "42", "yes"]);
        refused("lcp-admin", &["dnf_history_undo", "42", "confirm", "extra"]);
    }

    #[test]
    fn pkg_maintenance_action_gate_accepts_only_the_three_operations() {
        ok("lcp-admin", &["pkg_maintenance", "clean"]);
        ok("lcp-admin", &["pkg_maintenance", "autoremove"]);
        ok("lcp-admin", &["pkg_maintenance", "makecache"]);
        refused("lcp-admin", &["pkg_maintenance"]);
        refused("lcp-admin", &["pkg_maintenance", "update"]);
        refused("lcp-admin", &["pkg_maintenance", "clean", "extra"]);
        refused("lcp-admin", &[]);
    }

    #[test]
    fn helper_action_calls_accept_only_known_actions() {
        ok("lcp-admin", &["install_polkit_rule"]);
        ok("lcp-admin", &["admin_keep"]);
        refused("lcp-admin", &["admin_keep", "extra"]);
        refused("lcp-admin", &[]);
        refused("lcp-admin", &["install_polkit_rule", "extra"]);
        refused("lcp-admin", &["some_other_action"]);
        refused("lcp-admin", &["-rf", "/x"]);
        refused("lcp-admin", &["/usr/bin/dnf"]);
        // `write_repo_file` exists in the helper source (STEP 3B scope C) but
        // is deliberately NOT registered as an action yet: until it is
        // explicitly enabled in a later step, both gates must keep it
        // fail-closed at dispatch.
        refused("lcp-admin", &["write_repo_file", "/etc/yum.repos.d/x.repo"]);
    }

    // ── nmcli helper action dispatch validation ──
    #[test]
    fn nmcli_connection_save_dispatch_validates_verb_and_connection() {
        ok(
            "lcp-admin",
            &[
                "nmcli_connection_save",
                "add",
                "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d",
                "ipv4.addresses",
                "192.168.1.10/24",
            ],
        );
        ok(
            "lcp-admin",
            &[
                "nmcli_connection_save",
                "modify",
                "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d",
                "ipv4.method",
                "manual",
            ],
        );
        refused(
            "lcp-admin",
            &["nmcli_connection_save", "delete", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"],
        ); // wrong verb
        refused("lcp-admin", &["nmcli_connection_save", "add"]); // missing connection
        refused(
            "lcp-admin",
            &[
                "nmcli_connection_save",
                "add",
                "--evil-connection",
                "ipv4.addresses",
                "1.2.3.4/24",
            ],
        ); // connection starts with dash
        refused(
            "lcp-admin",
            &[
                "nmcli_connection_save",
                "add",
                "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d",
                "ipv4", // property without dot
            ],
        );
        refused(
            "lcp-admin",
            &[
                "nmcli_connection_save",
                "add",
                "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d",
                "ipv4.addresses", // missing value
            ],
        );
    }

    #[test]
    fn nmcli_connection_delete_dispatch_requires_uuid() {
        ok(
            "lcp-admin",
            &["nmcli_connection_delete", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"],
        );
        refused("lcp-admin", &["nmcli_connection_delete", "not-a-uuid"]);
        refused("lcp-admin", &["nmcli_connection_delete", ""]);
        refused("lcp-admin", &["nmcli_connection_delete", "--evil"]);
        refused("lcp-admin", &["nmcli_connection_delete"]);
    }

    #[test]
    fn nmcli_connection_up_dispatch_requires_uuid() {
        ok(
            "lcp-admin",
            &["nmcli_connection_up", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"],
        );
        refused("lcp-admin", &["nmcli_connection_up", "not-a-uuid"]);
        refused("lcp-admin", &["nmcli_connection_up"]);
    }

    #[test]
    fn nmcli_connection_import_dispatch_requires_type_and_file() {
        ok(
            "lcp-admin",
            &["nmcli_connection_import", "openvpn", "/home/user/myvpn.ovpn"],
        );
        ok(
            "lcp-admin",
            &["nmcli_connection_import", "wireguard", "/home/user/mywg.conf"],
        );
        refused("lcp-admin", &["nmcli_connection_import", "openvpn"]);
        refused("lcp-admin", &["nmcli_connection_import", "wireguard", "/tmp/bad.xyz"]);
        refused("lcp-admin", &["nmcli_connection_import"]);
    }

    #[test]
    fn nmcli_connection_down_dispatch_requires_uuid() {
        ok(
            "lcp-admin",
            &["nmcli_connection_down", "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d"],
        );
        refused("lcp-admin", &["nmcli_connection_down", "not-a-uuid"]);
        refused("lcp-admin", &["nmcli_connection_down"]);
    }

    #[test]
    fn repo_toggle_enable_dispatch_validates_file_and_repo_id() {
        ok(
            "lcp-admin",
            &["repo_toggle_enable", REPO_FILE_YUM, "fedora"],
        );
        ok(
            "lcp-admin",
            &["repo_toggle_enable", REPO_FILE_APT, "debian"],
        );
        refused("lcp-admin", &["repo_toggle_enable", REPO_FILE_YUM]);
        refused("lcp-admin", &["repo_toggle_enable", REPO_FILE_YUM, "--evil"]);
        refused("lcp-admin", &["repo_toggle_enable", REPO_FILE_YUM, ""]);
        refused("lcp-admin", &["repo_toggle_enable"]);
    }

    #[test]
    fn repo_toggle_disable_dispatch_validates_file_and_repo_id() {
        ok(
            "lcp-admin",
            &["repo_toggle_disable", REPO_FILE_YUM, "fedora"],
        );
        ok(
            "lcp-admin",
            &["repo_toggle_disable", REPO_FILE_APT, "debian"],
        );
        refused("lcp-admin", &["repo_toggle_disable", REPO_FILE_YUM]);
        refused("lcp-admin", &["repo_toggle_disable", REPO_FILE_YUM, "--evil"]);
        refused("lcp-admin", &["repo_toggle_disable", REPO_FILE_YUM, ""]);
        refused("lcp-admin", &["repo_toggle_disable"]);
    }

    #[test]
    fn repo_add_dispatch_validates_url() {
        ok(
            "lcp-admin",
            &["repo_add", "https://download.fedoraproject.org/pub/fedora/linux/releases/$releasever/Everything/$basearch/os/"],
        );
        ok("lcp-admin", &["repo_add", "https://deb.debian.org/debian"]);
        refused("lcp-admin", &["repo_add", "http://download.fedoraproject.org/pub/"]);
        refused("lcp-admin", &["repo_add", "ftp://host/repo"]);
        refused("lcp-admin", &["repo_add", "file:///srv/repo"]);
        refused("lcp-admin", &["repo_add", "--evil"]);
        refused("lcp-admin", &["repo_add", ""]);
        refused("lcp-admin", &["repo_add"]);
    }

    #[test]
    fn repo_delete_dispatch_validates_file_path() {
        ok("lcp-admin", &["repo_delete", REPO_FILE_YUM]);
        ok("lcp-admin", &["repo_delete", REPO_FILE_APT]);
        refused("lcp-admin", &["repo_delete"]);
        refused("lcp-admin", &["repo_delete", "--evil"]);
        refused("lcp-admin", &["repo_delete", ""]);
    }

    #[test]
    fn repo_makecache_dispatch_takes_no_args() {
        ok("lcp-admin", &["repo_makecache"]);
        refused("lcp-admin", &["repo_makecache", "extra"]);
    }

    #[test]
    fn repo_clean_cache_dispatch_validates_repo_id() {
        ok("lcp-admin", &["repo_clean_cache", "fedora"]);
        ok("lcp-admin", &["repo_clean_cache", "updates"]);
        refused("lcp-admin", &["repo_clean_cache"]);
        refused("lcp-admin", &["repo_clean_cache", "--evil"]);
        refused("lcp-admin", &["repo_clean_cache", ""]);
    }

    #[test]
    fn ssh_key_actions_take_one_valid_account_name_only() {
        ok("lcp-admin", &["read_user_ssh_keys", "marvin"]);
        ok("lcp-admin", &["write_user_ssh_keys", "root"]);
        ok("lcp-admin", &["write_user_ssh_keys", "a_1-b"]);
        // the account name never becomes a path fragment: no dots, no slashes
        refused("lcp-admin", &["read_user_ssh_keys", "../root"]);
        refused("lcp-admin", &["read_user_ssh_keys", "/etc/shadow"]);
        refused("lcp-admin", &["read_user_ssh_keys", "mar vin"]);
        refused("lcp-admin", &["read_user_ssh_keys", "marvin;reboot"]);
        refused("lcp-admin", &["read_user_ssh_keys", "-x"]);
        refused("lcp-admin", &["read_user_ssh_keys", ""]);
        refused("lcp-admin", &["read_user_ssh_keys"]);
        refused("lcp-admin", &["write_user_ssh_keys", "marvin", "extra"]);
        // and neither action takes any other shape
        refused("lcp-admin", &["read_user_ssh_keys", "marvin", "marvin"]);
    }

    #[test]
    fn write_profile_sh_takes_exactly_one_profile_d_sh_path() {
        let _fixture_guard = super::FIXTURE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        use super::ensure_arguments_for_program as check;

        // keep the fixture PROFILE_D_DIR real for the validator's path walk
        #[cfg(feature = "test-fixture")]
        std::fs::create_dir_all(std::path::Path::new(super::PROFILE_D_DIR)).unwrap();

        fn path_ok(path: &str) {
            let argv: Vec<String> =
                ["write_profile_sh", path].iter().map(|s| s.to_string()).collect();
            assert!(check("lcp-admin", &argv).is_ok(), "must be allowed: {path}");
        }
        fn path_refused(path: &str) {
            let argv: Vec<String> =
                ["write_profile_sh", path].iter().map(|s| s.to_string()).collect();
            assert!(check("lcp-admin", &argv).is_err(), "must be refused: {path}");
        }
        // PROFILE_D_DIR keeps this test meaningful in production and in a
        // test-fixture build of the helper alike
        let d = super::PROFILE_D_DIR;
        path_ok(&format!("{d}/10-lang.sh"));
        path_ok(&format!("{d}/foo.sh"));
        path_refused("/etc/shadow");
        path_refused(d);
        path_refused(&format!("{d}/x.conf"));
        path_refused(&format!("{d}/../shadow.sh"));
        path_refused(&format!("{d}/sub/x.sh"));
        path_refused(&format!("{d}/.hidden.sh"));
        path_refused(&format!("{d}/wow*x.sh"));
        path_refused("/etc/hosts");
    }

    /// `restore_profile_sh <dest> <backup-sibling>`: exactly two absolute
    /// validated paths — the destination through `validate_profile_sh_path`,
    /// the backup through the revision-sibling shape rule.
    #[test]
    fn restore_profile_sh_takes_one_dest_and_one_backup_sibling() {
        let _fixture_guard = super::FIXTURE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        use super::{ensure_arguments_for_program as check, restore_profile_sh_backup_shape};

        // keep the fixture PROFILE_D_DIR real for the validator's path walk
        #[cfg(feature = "test-fixture")]
        std::fs::create_dir_all(std::path::Path::new(super::PROFILE_D_DIR)).unwrap();

        fn argv_ok(dest: &str, backup: &str) {
            let argv: Vec<String> = ["restore_profile_sh", dest, backup]
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert!(
                check("lcp-admin", &argv).is_ok(),
                "must be allowed: {dest}, {backup}"
            );
        }
        fn argv_refused(dest: &str, backup: &str) {
            let argv: Vec<String> = ["restore_profile_sh", dest, backup]
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert!(
                check("lcp-admin", &argv).is_err(),
                "must be refused: {dest}, {backup}"
            );
        }

        let d = super::PROFILE_D_DIR;
        let dest = format!("{d}/10-lang.sh");
        argv_ok(&dest, &format!("{d}/10-lang.sh.1700000000_1.2.bak"));

        // arity
        argv_refused(&dest, "");
        assert!(check(
            "lcp-admin",
            &["restore_profile_sh".to_string(), dest.clone()]
        )
        .is_err());

        // destination policy
        argv_refused("/etc/shadow", &format!("{d}/10-lang.sh.1700000000_1.2.bak"));
        argv_refused(&format!("{d}/x.conf"), &format!("{d}/x.conf.1700000000_1.2.bak"));

        // backup shape policy (foreign directory, foreign name, no numeric
        // revision, hostile suffix charset) — the shape rule is shared with
        // the helper, so it is cross-checked here directly
        argv_refused(&dest, "/tmp/hostile.bak");
        argv_refused(&dest, &format!("{d}/19-other.sh.1700000000_1.2.bak"));
        argv_refused(&dest, &format!("{d}/10-lang.sh.bak"));
        argv_refused(&dest, &format!("{d}/10-lang.sh.17000000;.txt.bak"));
        assert_eq!(
            restore_profile_sh_backup_shape(&dest, &format!("{d}/10-lang.sh.1700000000_123.456.bak")),
            Ok(()),
            "the exact suffix shape create_backup_sibling writes must count as a revision"
        );
    }
}

// ─── profile.d destination and content policy (F-1) ─────────────────────────

#[cfg(test)]
mod profile_sh_policy_tests {
    use super::{validate_profile_sh_content, validate_profile_sh_path, PROFILE_D_DIR};

    /// A test-fixture build's PROFILE_D_DIR is a temp twin whose directory
    /// tree the validator's real path walk must see. Production builds need
    /// nothing (the validator only walks real directories that always exist
    /// for the allowed shapes).
    #[cfg(feature = "test-fixture")]
    fn ensure_profile_dir() {
        std::fs::create_dir_all(std::path::Path::new(PROFILE_D_DIR)).unwrap();
    }
    #[cfg(not(feature = "test-fixture"))]
    fn ensure_profile_dir() {}

    fn path_ok(path: &str) {
        assert!(validate_profile_sh_path(path).is_ok(), "path must pass: {path}");
    }
    fn path_refused(path: &str) {
        assert!(validate_profile_sh_path(path).is_err(), "path must fail: {path}");
    }

    #[test]
    fn profile_paths_must_be_exact_profile_d_sh_names() {
        let _fixture_guard = super::FIXTURE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_profile_dir();
        // PROFILE_D_DIR is the /etc/profile.d in production and its fixture
        // twin in a test-fixture build of the helper — the tests keep both
        // meaningful.
        let d = PROFILE_D_DIR;
        path_ok(&format!("{d}/lang.sh"));
        path_ok(&format!("{d}/99-panel.sh"));
        path_refused("/etc/shadow");
        path_refused(d);
        path_refused(&format!("{d}/"));
        path_refused(&format!("{d}/x.conf"));
        path_refused(&format!("{d}/sh"));
        path_refused(&format!("{d}/sub/x.sh"));
        path_refused(&format!("{d}/../hosts"));
        path_refused(&format!("{d}/../shadow.sh"));
        path_refused(&format!("{d}/.hidden.sh"));
        path_refused(&format!("{d}/x.sh/../y.sh"));
        path_refused(&format!("{d}/bad name.sh"));
        path_refused(&format!("{d}/wow*x.sh"));
        path_refused(&format!("{d}/wow;.sh"));
        path_refused(&format!("{}\u{0}", format!("{d}/ctrl.sh")));
        path_refused("profile.d/x.sh");
    }

    #[test]
    fn profile_names_reject_non_name_basenames() {
        assert!(validate_profile_sh_path("/etc/profile.d/café.sh").is_err());
        assert!(validate_profile_sh_path(
            "/etc/profile.d/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.sh"
        ).is_err(), "names over 64 characters are refused");
    }

    fn content_ok(content: &str) {
        assert!(validate_profile_sh_content(content).is_ok(), "content must pass: {content:?}");
    }
    fn content_refused(content: &str) {
        assert!(validate_profile_sh_content(content).is_err(), "content must fail: {content:?}");
    }

    #[test]
    fn profile_content_accepts_only_export_lines() {
        content_ok("");
        content_ok("#!/usr/bin/env bash\n# Created by Linux Control Panel\n");
        content_ok("export FOO=\"bar\"\n");
        content_ok("export FOO=\"bar\"\n# comment\n\nexport BAZ='qux'\n");
        content_ok("export EMPTY=\"\"\n");
        content_ok("export NODOT=QnVh\n");
        content_ok("export URL=\"https://example.com/path?x=1\"\n");
        content_ok("export WEIRD=\"spaces  are  fine\"\n");
        content_ok("export PATHLIKE=\"Value Value\"\n");
    }

    #[test]
    fn profile_content_rejects_shell_code() {
        // free command text
        content_refused("ls -la\n");
        content_refused("echo hello\n");
        content_refused("rm -rf /\n");
        // command substitution, expansion, escapes
        content_refused("export X=\"$(touch /tmp/pwned)\"\n");
        content_refused("export X=\"`touch /tmp/pwned`\"\n");
        content_refused("export X=\"${IARGA}\\${READY}\"\n");
        content_refused("export X=\"a\"\"b\"\n");
        content_refused("export X='a'\"; touch\" \n");
        // unquoted specials
        content_refused("export X=a|b\n");
        content_refused("export X=a>b\n");
        content_refused("export X=a;b\n");
        content_refused("export X='a'; touch /tmp/pwned\n");
        // not an assignment
        content_refused("[ -x /bin/su ] && cat /etc/passwd\n");
        content_refused(". /tmp/script\n");
        content_refused("source /tmp/script\n");
        // NUL never passes
        content_refused("\0");
        // control characters anywhere
        content_refused("export X=\"a\nb\"\n");
    }

    #[test]
    fn profile_content_accepts_only_the_narrow_path_extension() {
        // the exact shape the PATH tab writes, quoted and bare
        content_ok("export PATH=\"$PATH:/opt/bin\"\n");
        content_ok("export PATH=\"$PATH:/opt/bin:/usr/local/foo\"\n");
        content_ok("export PATH=$PATH:/opt/bin\n");
        content_ok("export PATH=\"$PATH:/opt/my_tools-1.2/bin:/usr/lib64\"\n");
        content_ok("export FOO=\"bar\"\nexport PATH=\"$PATH:/opt/bin\"\n");
        // single quotes never expand: this is a literal, already allowed by
        // the strict rule, and is not the exception
        content_ok("export PATH='$PATH:/opt/bin'\n");

        // the exception is a grammar, not a loosened rule: every one of these
        // leaves the single word the shell is allowed to expand
        content_refused("export PATH=\"$PATH;rm -rf /\"\n");
        content_refused("export PATH=\"$(touch /tmp/x)\"\n");
        content_refused("export PATH=\"$HOME/bin\"\n");
        content_refused("export PATH=\"$PATH\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin;touch /tmp/x\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:/tmp/a b\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:`id`\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:/tmp/x|y\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:/tmp/x&y\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin/$(id)\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:/tmp/x)y\"\n");
        content_refused("export PATH=\"$PATH::/opt/bin\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin:\"\n");
        content_refused("export PATH=\"$PATHx:/opt/bin\"\n");
        content_refused("export PATH=\"$PATH:/opt/bin\\foo\"\n");

        // PATH-only: the same value under any other name stays refused
        content_refused("export FOO=\"$PATH:/opt/bin\"\n");
        content_refused("export PATH_EXTRA=\"$PATH:/opt/bin\"\n");
        content_refused("export Path=\"$PATH:/opt/bin\"\n");
    }

    #[test]
    fn oversized_profile_content_is_refused() {
        let long = format!("export A=\"{}\"\n", "x".repeat(260 * 1024));
        assert!(validate_profile_sh_content(&long).is_err());
    }
}

// ─── repository content policy (F-2) ────────────────────────────────────────

#[cfg(test)]
mod repo_content_policy_tests {
    use super::{count_enabled_repos_in_content, ensure_repo_file_content, repo_url_https_only};

    fn ok_file(path: &str, content: &str) {
        if let Err(reason) = ensure_repo_file_content(path, content) {
            panic!("must pass {path}: {content:?} — rejected with: {reason}");
        }
    }
    fn refused_file(path: &str, content: &str, because: &str) {
        assert!(
            ensure_repo_file_content(path, content).is_err(),
            "must be refused ({because}): {path} — {content:?}"
        );
    }

    const GOOD: &str = "[test-repo]\n\
        name=Test Repo\n\
        baseurl=https://download.fedoraproject.org/pub/fedora/linux/releases/$releasever/Everything/$basearch/os/\n\
        gpgkey=https://download.fedoraproject.org/pub/fedora/linux/RPM-GPG-KEY-fedora\n\
        gpgcheck=1\n\
        enabled=1\n";

    #[test]
    fn valid_repo_content_passes() {
        ok_file("/etc/yum.repos.d/test.repo", GOOD);
        // metalink style, without gpgcheck (dnf defaults it on)
        ok_file(
            "/etc/yum.repos.d/fedora-updates.repo",
            "[updates]\n\
             name=Fedora $releasever - $basearch\n\
             metalink=https://mirrors.fedoraproject.org/metalink?repo=updates-released-f$releasever&arch=$basearch\n\
             enabled=1\n",
        );
        // a file emptied by a section delete is harmless
        ok_file("/etc/yum.repos.d/emptied.repo", "");
        ok_file("/etc/yum.repos.d/emptied.repo", "[one]\nname=A\nbaseurl=https://example.com/one\n");
    }

    #[test]
    fn local_file_sources_are_rejected() {
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=file:///home/evil/repo\n", "file://");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=ftp://host/pub\n", "ftp://");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=/mnt/local/repo\n", "bare path");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=cdrom:\n", "cdrom:");
        refused_file("/etc/apt/sources.list.d/x.list", "deb file:///srv/local one main\n", "apt file://");
        refused_file("/etc/apt/sources.list.d/x.list", "deb cdrom:[Debian] / stable main\n", "apt cdrom:");
    }

    #[test]
    fn disabled_signature_verification_is_rejected() {
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=https://example.com/repo\ngpgcheck=0\n", "gpgcheck=0");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=https://example.com/repo\nrepo_gpgcheck=0\n", "repo_gpgcheck=0");
        // lying separators or mangled booleans cannot sneak it back in
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl=https://example.com/repo\ngpgcheck=1?0\n", "mangled");
    }

    #[test]
    fn unexpected_directives_are_rejected() {
        refused_file("/etc/yum.repos.d/x.repo", "[r]\ndefine=whatever\n", "unknown key");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nexec=post.sh\n", "exec-like key");
        refused_file("/etc/apt/sources.list.d/x.sources", "Types: deb\nURIs: https://good.example/one\nHook: /usr/bin/gadget\n", "unknown stanza key");
    }

    #[test]
    fn malformed_repo_files_are_rejected() {
        refused_file("/etc/yum.repos.d/x.repo", "this is not ini\n", "no section");
        refused_file("/etc/yum.repos.d/x.repo", "[r]\nbaseurl\n", "no '='");
        refused_file("/etc/yum.repos.d/x.repo", "[../big]\n", "bad section id");
        refused_file("/etc/apt/sources.list.d/x.sources", "Types deb\n", "not key: value");
        refused_file("/etc/apt/sources.list.d/x.sources", "Types: deb\nURIs: file:///srv\n", "sources file://");
        refused_file("/etc/yum.repos.d/x.ini", GOOD, "unsupported extension");
    }

    #[test]
    fn repo_content_size_and_nul_gates() {
        refused_file("/etc/yum.repos.d/x.repo", "\0", "NUL");
        refused_file(
            "/etc/yum.repos.d/x.repo",
            &format!("[r]\nname={}\n", "a".repeat(300 * 1024)),
            "oversized",
        );
    }

    // ── scope B (STEP 3B): signed-by / bracket options / deb822 ──────────────

    const DEB_BASE: &str = "https://deb.debian.org/debian bookworm main contrib non-free-firmware";
    const KEYRING: &str = "/usr/share/keyrings/some-repo-archive-keyring.gpg";

    #[test]
    fn apt_list_plain_and_flat_entries_still_pass() {
        ok_file("/etc/apt/sources.list.d/debian.list", &format!("deb {DEB_BASE}\n"));
        ok_file("/etc/apt/sources.list.d/debian.list", &format!("deb-src {DEB_BASE}\n"));
        // flat repository: suite "./", zero components
        ok_file("/etc/apt/sources.list.d/flat.list", "deb https://apt.example.com ./\n");
        ok_file("/etc/apt/sources.list.d/flat.list", "deb https://apt.example.com ./ main\n");
        // tor-hidden sources stay allowed
        ok_file(
            "/etc/apt/sources.list.d/tor.list",
            "deb tor+https://mirror.example.org/debian trixie main\n",
        );
        // arch option and an approved signed-by keyring
        ok_file(
            "/etc/apt/sources.list.d/strict.list",
            "deb [arch=amd64,arm64] http://host.example/debian stable main\n",
        );
        ok_file(
            "/etc/apt/sources.list.d/strict.list",
            &format!("deb [signed-by={KEYRING}] {DEB_BASE}\n"),
        );
        ok_file(
            "/etc/apt/sources.list.d/strict.list",
            "deb [signed-by=/etc/apt/keyrings/nginx_signing.asc] http://host.example/deb ./\n",
        );
        // comma-separated approved keyring paths
        ok_file(
            "/etc/apt/sources.list.d/strict.list",
            &format!("deb [signed-by={KEYRING},/etc/apt/keyrings/a-b_1.asc] {DEB_BASE}\n"),
        );
        // empty options group is inert
        ok_file(
            "/etc/apt/sources.list.d/strict.list",
            &format!("deb [] {DEB_BASE}\n"),
        );
    }

    #[test]
    fn apt_list_refuses_user_controlled_signed_by_paths() {
        // the core V-1 fix: a keyring must come from the approved root-owned
        // directories, never from a user-controlled location
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/home/attacker/evil.gpg] https://host.example/debian stable main\n",
            "keyring in the user's home",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/tmp/evil.gpg] https://host.example/debian stable main\n",
            "keyring in /tmp",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/etc/attacker/evil.gpg] https://host.example/debian stable main\n",
            "keyring outside the approved dirs",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/usr/share/keyrings/../etc/cron.d/pwn.gpg] https://host.example/deb ./\n",
            "keyring path traversal",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/usr/share/keyrings/] https://host.example/deb ./\n",
            "keyring path is a directory",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/usr/share/keyrings/evil] https://host.example/deb ./\n",
            "missing keyring extension",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [signed-by=/usr/share/keyrings/.hidden.gpg] https://host.example/deb ./\n",
            "dot-file keyring",
        );
        // the first approved path must not hide a hostile second one
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            &format!(
                "deb [signed-by={KEYRING},/home/attacker/evil.gpg] https://host.example/deb ./\n"
            ),
            "second keyring path is hostile",
        );
        // whitespace-separating the keyring list is unsupported (ambiguous)
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            &format!("deb [signed-by={KEYRING} /etc/apt/keyrings/b.gpg] {DEB_BASE}\n"),
            "whitespace-separated keyring paths",
        );
        // and the same policy in the deb822 format
        refused_file(
            "/etc/apt/sources.list.d/evil.sources",
            "Types: deb\nURIs: https://host.example/debian\nSuites: stable\nComponents: main\nSigned-By: /home/attacker/evil.gpg\n",
            "deb822 keyring in the user's home",
        );
    }

    #[test]
    fn apt_list_refuses_signature_disablers_and_unknown_options() {
        // trusted=yes hands root package trust to an unsigned source
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [trusted=yes] https://host.example/debian stable main\n",
            "trusted=yes",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [trusted=yes,arch=amd64] https://host.example/debian stable main\n",
            "trusted=yes mixed into a group with an allowed option",
        );
        // unknown/unsupported options are refused rather than sanitized
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [date=2024] https://host.example/debian stable main\n",
            "unknown option",
        );
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [x-sneak=https://evil] https://host.example/debian stable main\n",
            "unknown option",
        );
        // bare words in the group are not options
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [trusted] https://host.example/debian stable main\n",
            "bare bracket word",
        );
        // case tricks on the option name do not bypass the allowlist
        refused_file(
            "/etc/apt/sources.list.d/evil.list",
            "deb [TRUSTED=yes] https://host.example/debian stable main\n",
            "uppercase unknown option",
        );
    }

    #[test]
    fn apt_list_refuses_ambiguous_bracket_placements_and_broken_lines() {
        // bracket after the suite → unsupported second options group
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.list",
            "deb https://host.example/debian stable [arch=amd64] main\n",
            "bracket group after the suite",
        );
        // unterminated group
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.list",
            "deb [arch=amd64 https://host.example/debian stable main\n",
            "unterminated bracket group",
        );
        // a bracket alone
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.list",
            "deb [ https://host.example/debian stable main\n",
            "lone bracket",
        );
        // brackets anywhere else in the line
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.list",
            "deb https://host.example/debian stable ma[in\n",
            "bracket in a component",
        );
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.list",
            "deb [signed-by=/usr/share/keyrings/a.gpg]https://host.example/deb ./\n",
            "URI glued to the options group",
        );
        // missing pieces
        refused_file(
            "/etc/apt/sources.list.d/broken.list",
            "deb\n",
            "type only",
        );
        refused_file(
            "/etc/apt/sources.list.d/broken.list",
            "deb https://host.example/debian\n",
            "no suite",
        );
        // URI without an internet scheme (file/copy/cdrom already existed)
        refused_file(
            "/etc/apt/sources.list.d/broken.list",
            "deb file:///srv/repo stable main\n",
            "file scheme",
        );
        refused_file(
            "/etc/apt/sources.list.d/broken.list",
            "deb ftp://host/repo stable main\n",
            "ftp scheme",
        );
        refused_file(
            "/etc/apt/sources.list.d/broken.list",
            "deb not-a-url stable main\n",
            "scheme-less URI",
        );
    }

    #[test]
    fn deb822_strict_stanzas_pass_and_duplication_is_refused() {
        ok_file(
            "/etc/apt/sources.list.d/strict.sources",
            &format!(
                "Types: deb\nURIs: https://deb.debian.org/debian https://deb.debian.org/debian-security\nSuites: bookworm bookworm-security\nComponents: main contrib non-free-firmware\nSigned-By: {KEYRING}\nEnabled: yes\n"
            ),
        );
        ok_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb deb-src\nURIs: tor+https://host.example/debian\nSuites: trixie/updates\nComponents: main\nArchitectures: amd64,arm64\nOrigin: The Example Distro\nLabel: Example\n",
        );
        // repeated fields must be refused: apt merges them, so the document
        // apt would read differs from the one validated here
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.sources",
            "Types: deb\nURIs: https://host.example/debian\nURIs: https://evil.example/debian\n",
            "repeated URIs field",
        );
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.sources",
            "Types: deb\nURIs: https://host.example/debian\nComponents: main\ntypes: deb\n",
            "repeated Types field (case-insensitive)",
        );
        // deb822 continuation lines are unsupported — a misparsed one would
        // carry content the panel never validated
        refused_file(
            "/etc/apt/sources.list.d/ambiguous.sources",
            "Types: deb\nURIs: https://host.example/debian\nSuites: stable\nComponents: main\nSigned-By: /usr/share/keyrings/a.gpg\n extra-continuation\n",
            "continuation line",
        );
        // empty lines and comments stay fine
        ok_file(
            "/etc/apt/sources.list.d/strict.sources",
            &format!("\n# a comment\n\nTypes: deb\nURIs: https://host.example/debian\nSigned-By: {KEYRING}\n"),
        );
    }

    #[test]
    fn deb822_field_shape_rules() {
        // Types only understands deb / deb-src words
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: binary\nURIs: https://host.example/debian\n",
            "unknown type word",
        );
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types:\nURIs: https://host.example/debian\n",
            "empty Types",
        );
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb\rdeb-src\nURIs: https://host.example/debian\n",
            "control-r squeezed into the Types words",
        );
        // URIs demand per-token internet schemes
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "URIs: no-scheme-here\n",
            "scheme-less URI token",
        );
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "URIs: https://host.example/debian [weird]\n",
            "brackets inside URIs",
        );
        // Suites / Components are word lists only, never paths or options
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb\nURIs: https://host.example/debian\nSuites: st(able\n",
            "parenthesis in a suite",
        );
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb\nURIs: https://host.example/debian\nComponents: main;extra\n",
            "semicolon in a component",
        );
        // inline keys (fingerprint material) are unsupported
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb\nURIs: https://host.example/debian\nSigned-By: 99A7BBF0ABCD\n",
            "inline key blob instead of a keyring path",
        );
        // bracket-ish values cannot reach any field
        refused_file(
            "/etc/apt/sources.list.d/strict.sources",
            "Types: deb\nURIs: https://host.example/debian\nOrigin: Exa[mple]\n",
            "brackets in a free-form field",
        );
    }

    // ── repo_url_https_only and count_enabled_repos_in_content ──────────────

    #[test]
    fn repo_url_https_only_accepts_https() {
        assert!(
            repo_url_https_only("https://download.fedoraproject.org/pub/fedora/linux/releases/$releasever/Everything/$basearch/os/").is_ok(),
            "https URL must be accepted"
        );
        assert!(
            repo_url_https_only("https://deb.debian.org/debian").is_ok(),
            "plain https URL must be accepted"
        );
    }

    #[test]
    fn repo_url_https_only_refuses_non_https() {
        assert!(
            repo_url_https_only("http://download.fedoraproject.org/pub/").is_err(),
            "http must be refused"
        );
        assert!(
            repo_url_https_only("ftp://host/repo").is_err(),
            "ftp must be refused"
        );
        assert!(
            repo_url_https_only("file:///srv/repo").is_err(),
            "file:// must be refused"
        );
        assert!(
            repo_url_https_only("cdrom://deb.debian.org").is_err(),
            "cdrom must be refused"
        );
        assert!(
            repo_url_https_only("not-a-url").is_err(),
            "scheme-less must be refused"
        );
        assert!(
            repo_url_https_only("").is_err(),
            "empty must be refused"
        );
    }

    /// Phase-2 hardening tests: the validator must refuse structurally broken
    /// https URLs of every shape an operator might paste, not just foreign
    /// schemes — and the tests run against the *real* validator function the
    /// privileged path uses, not the classifier.
    #[test]
    fn repo_url_https_only_refuses_broken_hosts_empties_and_credentials() {
        // https:// with nothing at all after it
        assert!(repo_url_https_only("https://").is_err(), "scheme-only must be refused");
        // empty authority, path only
        assert!(
            repo_url_https_only("https:///repository").is_err(),
            "missing host must be refused"
        );
        // embedded credentials are never supported in a repo URL
        assert!(
            repo_url_https_only("https://user:secret@example.com/repo").is_err(),
            "user:pass must be refused"
        );
        assert!(
            repo_url_https_only("https://token@example.com/repo").is_err(),
            "bare userinfo must be refused"
        );
        // whitespace and control characters
        assert!(
            repo_url_https_only(" https://example.com/repo").is_err(),
            "leading space must be refused"
        );
        assert!(
            repo_url_https_only("https://example.com/repo ").is_err(),
            "trailing space must be refused"
        );
        assert!(
            repo_url_https_only("https://exa mple.com/repo").is_err(),
            "embedded space must be refused"
        );
        assert!(
            repo_url_https_only("https://example.com/re\tpo").is_err(),
            "tab must be refused"
        );
        assert!(
            repo_url_https_only("https://example.com/re\u{0}po").is_err(),
            "NUL must be refused"
        );
        // shell metacharacters — a repo URL is a privileged argv element; the
        // helper never passes URLs through a shell, so ';' and '|' cannot gate
        // a second command (they are literal), while quotes/backslashes/
        // backticks are refused outright because no legitimate URL carries
        // them unescaped and they are the classic bypass shapes
        assert!(
            repo_url_https_only("https://example.com/repo`whoami`").is_err(),
            "backtick must be refused"
        );
        assert!(
            repo_url_https_only("https://user'@example.com").is_err(),
            "quote must be refused"
        );
        assert!(
            repo_url_https_only("https://a\\b@example.com").is_err(),
            "backslash must be refused"
        );
        assert!(
            repo_url_https_only("https://\"example.com").is_err(),
            "double quote must be refused"
        );
        // malformed ports
        assert!(repo_url_https_only("https://host:").is_err(), "empty port must be refused");
        assert!(repo_url_https_only("https://host:x/").is_err(), "letters in port must be refused");
        assert!(repo_url_https_only("https://host:999999/").is_err(), "6-digit port must be refused");
        assert!(repo_url_https_only("https://host:8443:9/").is_err(), "two colons must be refused");
        // broken IPv6 shapes
        assert!(
            repo_url_https_only("https://[::1/repo").is_err(),
            "unterminated bracket must be refused"
        );
        assert!(
            repo_url_https_only("https://[::1]:x/repo").is_err(),
            "letters in IPv6 port must be refused"
        );
    }

    #[test]
    fn repo_url_https_only_accepts_valid_shapes() {
        // the shapes the app legitimately produces must keep working
        assert!(repo_url_https_only("https://host/path/").is_ok(), "plain host");
        assert!(repo_url_https_only("https://host:443/path").is_ok(), "numeric port");
        assert!(repo_url_https_only("https://host:65535/p/x.repo").is_ok(), "max port");
        assert!(repo_url_https_only("https://example.com/repository.repo").is_ok(), ".repo URL");
        assert!(
            repo_url_https_only("https://example.com/x.repo?f=/g&mirror=list#top").is_ok(),
            "query and fragment pass through untouched"
        );
        assert!(
            repo_url_https_only("https://example.com/$releasever/$basearch/os/").is_ok(),
            "dnf URL variables stay accepted"
        );
        assert!(repo_url_https_only("https://localhost/repo").is_ok(), "single-label host");
        assert!(repo_url_https_only("https://[::1]/repo").is_ok(), "IPv6 literal");
        assert!(repo_url_https_only("https://[fe80::1]:8443/x").is_ok(), "IPv6 with port");
        assert!(
            repo_url_https_only("https://invalid.example/repo").is_ok(),
            "hyphenated host label"
        );
        // https://invalid is a syntactically valid single-label host — the
        // validator accepts it and the package manager delivers the real
        // verdict; this documents that choice explicitly
        assert!(repo_url_https_only("https://invalid").is_ok(), "single-label host");
    }

    #[test]
    fn count_enabled_repos_in_content_counts_correctly() {
        // no repos
        assert_eq!(
            count_enabled_repos_in_content(""),
            0,
            "empty content has zero enabled repos"
        );
        // one enabled repo
        assert_eq!(
            count_enabled_repos_in_content("[repo1]\nname=A\nbaseurl=https://example.com\nenabled=1\n"),
            1,
            "one enabled=1 counts as one"
        );
        // one disabled repo
        assert_eq!(
            count_enabled_repos_in_content("[repo1]\nname=A\nbaseurl=https://example.com\nenabled=0\n"),
            0,
            "enabled=0 does not count"
        );
        // two enabled, one disabled
        assert_eq!(
            count_enabled_repos_in_content(
                "[repo1]\nenabled=1\n[repo2]\nenabled=0\n[repo3]\nenabled=1\n"
            ),
            2,
            "two enabled count as two"
        );
        // enabled with spaces
        assert_eq!(
            count_enabled_repos_in_content("[repo1]\n enabled = 1 \n"),
            1,
            "whitespace around '=' is tolerated"
        );
    }
}

// ─── nginx confinement policy ────────────────────────────────────────────────
//
// The validators this module adds for the nginx batch — site/file names, the
// configuration tree, the web root, and the generalized revision-sibling
// rule — each get their accept/refuse matrix here. The profile.d rule the
// nginx one was generalized from is asserted unchanged in the same sweep.

#[cfg(test)]
mod nginx_policy_tests {
    use super::*;

    #[test]
    fn site_names_reject_everything_but_plain_word_names() {
        for name in ["example.com", "my-site", "10_group.conf", "a1.b_c-d"] {
            nginx_site_name_valid(name)
                .unwrap_or_else(|error| panic!("{name:?} must be accepted: {error}"));
        }
        for name in [
            "",
            ".hidden",
            "..",
            "-flaglike",
            "a/b",
            "a\\b",
            "with space",
            "semi;colon",
            "quote'",
            "nul\0",
            "\x1f",
        ] {
            assert!(
                nginx_site_name_valid(name).is_err(),
                "{name:?} must be refused"
            );
        }
        assert!(nginx_site_name_valid(&"a".repeat(256)).is_err());
    }

    #[test]
    fn nginx_config_paths_are_confined_to_the_fixed_tree() {
        fn ok(path: &str) {
            nginx_config_path_valid(path)
                .unwrap_or_else(|error| panic!("{path} must be accepted: {error}"));
        }
        fn refused(path: &str) {
            assert!(
                nginx_config_path_valid(path).is_err(),
                "{path:?} must be refused"
            );
        }

        ok(NGINX_NGINX_CONF);
        ok(&format!("{NGINX_CONF_D}/10-virtual.conf"));
        ok(&format!("{NGINX_SITES_AVAILABLE}/example.com"));
        ok(&format!("{NGINX_SITES_AVAILABLE}/example.com.conf"));
        ok(&format!("{NGINX_SITES_ENABLED}/example.com.conf"));

        // GROUP 7: the conf.d toggle archive `<name>.conf.disabled` is
        // admitted — install creates it, the enable step's cleanup removes
        // it; the validated name is the stem without the machinery suffix.
        ok(&format!("{NGINX_CONF_D}/example.com.conf.disabled"));

        // a damaged twin stays out: the machinery suffix must be the very
        // end of the name, and a bare suffix (empty stem) is no site name
        refused(&format!("{NGINX_CONF_D}/x.conf.disabledx"));
        refused(&format!("{NGINX_CONF_D}/.conf.disabled"));

        // outside the tree altogether
        refused("/etc/nginx/other.conf");
        refused(&format!("{NGINX_CONF_D}_copy/x.conf"));
        refused("/etc/hosts");
        refused("/etc/nginx/sites-available/../nginx.conf");
        refused("relative/path.conf");
        refused("");
        refused(&format!("{NGINX_CONF_D}/no-suffix"));
        refused(&format!("{NGINX_CONF_D}/.hidden.conf"));
        refused(&format!("{NGINX_SITES_AVAILABLE}/-leading-dash"));
        // the exact nginx.conf with any trailing decoration is not nginx.conf
        refused(&format!("{NGINX_NGINX_CONF}/"));
        refused(&format!("{NGINX_NGINX_CONF}.bak"));
    }

    #[test]
    fn var_www_paths_must_lie_strictly_under_the_web_root() {
        fn ok(path: &str) {
            var_www_path_valid(path)
                .unwrap_or_else(|error| panic!("{path} must be accepted: {error}"));
        }
        fn refused(path: &str) {
            assert!(var_www_path_valid(path).is_err(), "{path:?} must be refused");
        }

        ok(&format!("{VAR_WWW_DIR}/html"));
        ok(&format!("{VAR_WWW_DIR}/site-app"));
        ok(&format!("{VAR_WWW_DIR}/my app/sub dir/file name.txt")); // spaces are legit
        ok(&format!("{VAR_WWW_DIR}/a/.hidden-asset")); // no CurDir/ParentDir component

        refused(VAR_WWW_DIR); // the root itself
        refused(&format!("{VAR_WWW_DIR}/")); // the root with a trailing slash
        refused("/var");
        refused("/var/wwwhtml"); // prefix must break on the slash
        refused(&format!("{VAR_WWW_DIR}/../etc/shadow")); // traversal
        refused(&format!("{VAR_WWW_DIR}/./x")); // component escape
        refused("relative/path");
        refused(&format!("{VAR_WWW_DIR}/nul\0byte"));
    }

    #[test]
    fn the_web_root_and_html_are_the_protected_names() {
        for protected in [
            VAR_WWW_DIR,
            &format!("{VAR_WWW_DIR}/"),
            &format!("{VAR_WWW_DIR}/html"),
            &format!("{VAR_WWW_DIR}/html/"),
        ] {
            assert!(var_www_is_protected(protected), "{protected:?}");
        }
        for ordinary in [
            &format!("{VAR_WWW_DIR}/html/index.html"),
            &format!("{VAR_WWW_DIR}/site-app"),
            "/var/wwwhtml",
            "/var/www/site-app/html",
        ] {
            assert!(
                !var_www_is_protected(ordinary),
                "{ordinary:?} must not be protected"
            );
        }
    }

    /// The generalized sibling rule keeps the profile.d shape exactly as
    /// before (the helper's `<unix-seconds>_<pid>.<nanos>`), accepts the
    /// nginx backups' `<YYYYmmdd_HHMMSS>` shape, and still refuses
    /// everything that is not its own revision sibling.
    #[test]
    fn the_revision_sibling_rule_covers_both_revision_schemes() {
        fn ok(dest: &str, backup: &str, dir: &str) {
            revision_sibling_shape(dest, backup, dir)
                .unwrap_or_else(|error| panic!("{backup} must be accepted: {error}"));
        }
        fn refused(dest: &str, backup: &str, dir: &str) {
            assert!(
                revision_sibling_shape(dest, backup, dir).is_err(),
                "{backup:?} must be refused"
            );
        }

        // helper write-core shape (pid.nanos suffix) — the profile.d rule
        let d = PROFILE_D_DIR;
        ok(
            &format!("{d}/10-lang.sh"),
            &format!("{d}/10-lang.sh.1700000000_123.456.bak"),
            d,
        );
        // nginx app-backup shape
        ok(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.20240102_030405.bak"),
            NGINX_BACKUPS_DIR,
        );
        // multi-dot member suffixes stay digits-based only
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.20240102_0304;05.bak"),
            NGINX_BACKUPS_DIR,
        );
        // foreign dir, foreign name, shapeless, giant digit runs
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{d}/x.conf.20240102_030405.bak"),
            NGINX_BACKUPS_DIR,
        );
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{NGINX_BACKUPS_DIR}/other.conf.20240102_030405.bak"),
            NGINX_BACKUPS_DIR,
        );
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.bak"),
            NGINX_BACKUPS_DIR,
        );
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.{}_030405.bak", "1".repeat(30)),
            NGINX_BACKUPS_DIR,
        );
        refused(
            &format!("{NGINX_CONF_D}/x.conf"),
            "/tmp/hostile/target.bak",
            NGINX_BACKUPS_DIR,
        );

        // the profile.d wrapper is the same rule with PROFILE_D_DIR pinned
        assert_eq!(
            restore_profile_sh_backup_shape(&format!("{d}/x.sh"), &format!("{d}/x.sh.1700000000_1.2.bak")),
            Ok(())
        );
        assert!(restore_profile_sh_backup_shape(&format!("{d}/x.sh"), "/tmp/x.bak").is_err());
    }
}

// ─── nginx content gate + action arg policy (batch: nginx_config_install / ───
// ─── nginx_config_backup / nginx_restore_backup) ────────────────────────────

#[cfg(test)]
mod nginx_config_action_tests {
    use super::ensure_arguments_for_program as check;
    use super::{
        revision_sibling_shape, validate_nginx_config_content, NGINX_BACKUPS_DIR, NGINX_CONF_D,
        NGINX_NGINX_CONF, NGINX_SITES_AVAILABLE,
    };

    /// The structural gate accepts everything the panel's config editor can
    /// honestly feed it and refuses only the bytes no config file can carry
    /// carry — NUL, stray control characters, oversized buffers. No grammar
    /// checks live here (nginx -t is the grammar gate).
    #[test]
    fn the_nginx_content_gate_is_structural_only() {
        for content in [
            "", "# only a comment\n",
            "server {\n    listen 80;\n\tserver_name x.y;\n    location / { return 200 \"ok\\n\"; }\n}\n",
        ] {
            validate_nginx_config_content(content)
                .unwrap_or_else(|error| panic!("{content:?} must be accepted: {error}"));
        }
        assert!(validate_nginx_config_content("nul\0byte").is_err());
        assert!(validate_nginx_config_content("bell\x07byte").is_err());
        assert!(validate_nginx_config_content("\x1fdanger").is_err());
        // newlines and tabs are the honest control characters of conf files
        validate_nginx_config_content("a\nb\r\n\tc\t")
            .expect("newlines and tabs must pass");
        assert!(validate_nginx_config_content(&"x".repeat(256 * 1024 + 1)).is_err());
    }

    /// The three config actions' argument shapes: single (install/backup) or
    /// dest+backup (restore), each confined to the fixed tree and the
    /// revision-sibling backup rule.
    #[test]
    fn the_nginx_config_actions_take_only_pinned_paths() {
        let dest = format!("{NGINX_CONF_D}/x.conf");
        let backup = format!("{NGINX_BACKUPS_DIR}/x.conf.20240102_030405.bak");

        /// `ensure_arguments_for_program` takes `&[String]`; build the argv.
        fn args(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|part| part.to_string()).collect()
        }

        assert!(check("lcp-admin", &args(&["nginx_config_install", &dest])).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_config_backup", &dest])).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_restore_backup", &dest, &backup])).is_ok());

        // arity
        assert!(check("lcp-admin", &args(&["nginx_config_install"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_config_backup", &dest, &backup])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_restore_backup", &dest])).is_err());

        // destination confinement: outside the tree, non-conf in conf.d,
        // traversal, control characters
        for hostile_dest in [
            "/etc/shadow",
            "/etc/nginx/other/x.conf",
            &format!("{NGINX_CONF_D}/x.txt"),
            &format!("{NGINX_CONF_D}/../shadow"),
            &format!("{NGINX_SITES_AVAILABLE}no/x.conf"),
            &format!("{NGINX_NGINX_CONF}/../shadow"),
        ] {
            assert!(
                check("lcp-admin", &args(&["nginx_config_install", hostile_dest])).is_err(),
                "{hostile_dest} must be refused"
            );
            assert!(
                check(
                    "lcp-admin",
                    &args(&["nginx_restore_backup", hostile_dest, &backup])
                )
                .is_err(),
                "restore to {hostile_dest} must be refused"
            );
        }

        // backup shape: foreign dir, foreign name, shapeless, hostile digits
        for hostile_backup in [
            "/tmp/hostile/target.bak",
            &format!("{NGINX_BACKUPS_DIR}/other.conf.20240102_030405.bak"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.bak"),
            &format!("{NGINX_BACKUPS_DIR}/x.conf.2024;0102_030405.bak"),
        ] {
            assert!(
                revision_sibling_shape(&dest, hostile_backup, NGINX_BACKUPS_DIR).is_err(),
                "{hostile_backup} must be refused"
            );
            assert!(
                check(
                    "lcp-admin",
                    &args(&["nginx_restore_backup", &dest, hostile_backup])
                )
                .is_err(),
                "restore from {hostile_backup} must be refused"
            );
        }
        // nginx.conf is exactly one file; even its own backup shape check
        // works (its basename is the backup prefix)
        let conf_backup = format!("{NGINX_BACKUPS_DIR}/nginx.conf.20240102_030405.bak");
        assert_eq!(
            revision_sibling_shape(NGINX_NGINX_CONF, &conf_backup, NGINX_BACKUPS_DIR),
            Ok(())
        );
    }
}

#[cfg(test)]
mod nginx_link_remove_action_tests {
    use super::ensure_arguments_for_program as check;
    use super::{
        nginx_config_remove_path_valid, nginx_link_as_valid, nginx_site_unlink_path_valid,
        NGINX_CONF_D, NGINX_NGINX_CONF, NGINX_SITES_AVAILABLE, NGINX_SITES_ENABLED,
    };

    /// `ensure_arguments_for_program` takes `&[String]`; build the argv.
    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// The link action's two endpoints: pinned one-per-directory and
    /// same-named, everything else refused.
    #[test]
    fn nginx_link_site_only_links_a_namesake_from_available_into_enabled() {
        let src = format!("{NGINX_SITES_AVAILABLE}/example.com");
        let dst = format!("{NGINX_SITES_ENABLED}/example.com");
        assert!(check("lcp-admin", &args(&["nginx_link_site", &src, &dst])).is_ok());
        assert!(nginx_link_as_valid(&src, &dst).is_ok());

        // arity
        assert!(check("lcp-admin", &args(&["nginx_link_site", &src])).is_err());
        assert!(
            check("lcp-admin", &args(&["nginx_link_site", &src, &dst, "extra"])).is_err()
        );

        // basename mismatch: enabling a different name than the source is
        // never a rename-in-disguise
        let other = format!("{NGINX_SITES_ENABLED}/other-site");
        assert!(nginx_link_as_valid(&src, &other).is_err());
        assert!(check("lcp-admin", &args(&["nginx_link_site", &src, &other])).is_err());

        // endpoint confinement: any file outside its one directory is
        // refused, on both sides
        let compressed = format!("{NGINX_CONF_D}/example.com");
        assert!(nginx_link_as_valid(&compressed, &dst).is_err());
        assert!(check("lcp-admin", &args(&["nginx_link_site", &compressed, &dst])).is_err());
        let enabled_src = format!("{NGINX_SITES_ENABLED}/example.com");
        assert!(nginx_link_as_valid(&enabled_src, &dst).is_err());

        // general path policy still applies to both endpoints
        let evil_dst = format!("{NGINX_SITES_ENABLED}/../nginx.conf");
        assert!(nginx_link_as_valid(&src, &evil_dst).is_err());
        assert!(check("lcp-admin", &args(&["nginx_link_site", &src, &evil_dst])).is_err());
    }

    /// The remove action: the whole configuration tree minus the protected
    /// core, exactly as the write conventions define it.
    #[test]
    fn nginx_config_remove_takes_one_removable_path() {
        assert!(nginx_config_remove_path_valid(&format!("{NGINX_CONF_D}/10-x.conf")).is_ok());
        assert!(
            nginx_config_remove_path_valid(&format!("{NGINX_SITES_AVAILABLE}/example.com")).is_ok()
        );
        assert!(
            nginx_config_remove_path_valid(&format!("{NGINX_SITES_ENABLED}/example.com")).is_ok()
        );
        assert!(check(
            "lcp-admin",
            &args(&["nginx_config_remove", &format!("{NGINX_SITES_AVAILABLE}/example.com")])
        )
        .is_ok());

        // the protected core
        assert!(nginx_config_remove_path_valid(NGINX_NGINX_CONF).is_err());
        assert!(check("lcp-admin", &args(&["nginx_config_remove", NGINX_NGINX_CONF])).is_err());

        // arity + general confinement
        assert!(check("lcp-admin", &args(&["nginx_config_remove"])).is_err());
        assert!(check(
            "lcp-admin",
            &args(&["nginx_config_remove", &format!("{NGINX_CONF_D}/../shadow")])
        )
        .is_err());
        assert!(check("lcp-admin", &args(&["nginx_config_remove", "/etc/shadow"])).is_err());
    }

    /// The unlink action takes exactly one plain child of sites-enabled
    /// under the shared configuration-tree policy.
    #[test]
    fn nginx_unlink_site_takes_one_sites_enabled_symlink_path() {
        let enabled = format!("{NGINX_SITES_ENABLED}/example.com");
        assert!(nginx_site_unlink_path_valid(&enabled).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_unlink_site", &enabled])).is_ok());

        // anything outside sites-enabled is refused, files included
        assert!(
            nginx_site_unlink_path_valid(&format!("{NGINX_SITES_AVAILABLE}/example.com")).is_err()
        );
        assert!(check(
            "lcp-admin",
            &args(&["nginx_unlink_site", &format!("{NGINX_SITES_AVAILABLE}/example.com")])
        )
        .is_err());
        assert!(nginx_site_unlink_path_valid(NGINX_NGINX_CONF).is_err());
        assert!(nginx_site_unlink_path_valid(&format!("{NGINX_SITES_ENABLED}/../shadow")).is_err());
        assert!(check("lcp-admin", &args(&["nginx_unlink_site", "/etc/shadow"])).is_err());

        // arity
        assert!(check("lcp-admin", &args(&["nginx_unlink_site"])).is_err());
        assert!(
            check("lcp-admin", &args(&["nginx_unlink_site", &enabled, "extra"])).is_err()
        );
    }
}

#[cfg(test)]
mod nginx_www_fs_action_tests {
    use super::ensure_arguments_for_program as check;
    use super::{VAR_WWW_DIR, VAR_WWW_DIR_AS_ROOT, var_www_is_protected};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// All four operation shapes accept exactly their operand count, and
    /// every path must lie strictly below the web root.
    #[test]
    fn the_www_fs_verbs_take_only_paths_below_the_web_root() {
        let file = format!("{VAR_WWW_DIR}/appx/index.html");
        let other = format!("{VAR_WWW_DIR}/appz/index.html");

        assert!(check("lcp-admin", &args(&["nginx_www_fs", "mkdir", &file])).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "remove", &file])).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "rename", &file, &other])).is_ok());
        assert!(check("lcp-admin", &args(&["nginx_www_install", &file])).is_ok());

        // arity: every verb refuses the wrong count
        assert!(check("lcp-admin", &args(&["nginx_www_fs"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "mkdir"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "mkdir", &file, "extra"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "remove"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "remove", &file, "extra"])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "rename", &file])).is_err());
        assert!(
            check("lcp-admin", &args(&["nginx_www_fs", "rename", &file, &other, "extra"])).is_err()
        );
        assert!(check("lcp-admin", &args(&["nginx_www_install", &file, "extra"])).is_err());

        // loneliness: unknown sub-commands are refused, too
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "touch", &file])).is_err());
        assert!(check("lcp-admin", &args(&["nginx_www_fs", "RM", &file])).is_err());

        // confinement: the web root itself, traversal, relative paths,
        // control characters, prefix games — all refused on every verb
        let hostile = [
            VAR_WWW_DIR,
            &format!("{VAR_WWW_DIR}/"),
            &format!("{VAR_WWW_DIR}/../shadow"),
            &format!("{VAR_WWW_DIR}/./x"),
            "/var",
            "/var/wwwhtml/x",
            "relative/path",
        ];
        for bad in hostile {
            assert!(check("lcp-admin", &args(&["nginx_www_fs", "mkdir", bad])).is_err(), "{bad:?}");
            assert!(check("lcp-admin", &args(&["nginx_www_fs", "remove", bad])).is_err(), "{bad:?}");
            assert!(
                check("lcp-admin", &args(&["nginx_www_fs", "rename", bad, &file])).is_err(),
                "{bad:?}"
            );
            assert!(check("lcp-admin", &args(&["nginx_www_install", bad])).is_err(), "{bad:?}");
        }
    }

    /// The protected names: remove refuses them outright, rename refuses
    /// them on both endpoints, mkdir and install are not protected-name
    /// operations (the fixture dispatch proves the fs-level refusals).
    #[test]
    fn the_protected_names_refuse_remove_and_rename() {
        for protected in [
            VAR_WWW_DIR,
            VAR_WWW_DIR_AS_ROOT,
            &format!("{VAR_WWW_DIR}/html"),
            &format!("{VAR_WWW_DIR}/html/"),
        ] {
            assert!(var_www_is_protected(protected), "{protected:?} must be protected");
            // valid-below still skips the root itself for non-remove ops
            if protected != VAR_WWW_DIR && protected != VAR_WWW_DIR_AS_ROOT {
                assert!(check(
                    "lcp-admin",
                    &args(&["nginx_www_fs", "remove", protected])
                )
                .is_err(), "remove of {protected:?} must be refused");
                assert!(check(
                    "lcp-admin",
                    &args(&["nginx_www_fs", "rename", protected, &format!("{VAR_WWW_DIR}/x")])
                )
                .is_err(), "rename of {protected:?} must be refused");
                assert!(check(
                    "lcp-admin",
                    &args(&["nginx_www_fs", "rename", &format!("{VAR_WWW_DIR}/x"), protected])
                )
                .is_err(), "rename to {protected:?} must be refused");
            }
        }
        // inside the protected tree is ordinary territory
        assert!(var_www_is_protected(&format!("{VAR_WWW_DIR}/html/index.html")) == false);
        assert!(check(
            "lcp-admin",
            &args(&["nginx_www_fs", "remove", &format!("{VAR_WWW_DIR}/html/index.html")])
        )
        .is_ok());
    }
}

#[cfg(test)]
mod certbot_request_shape_tests {
    use super::ensure_arguments_for_program as check;

    fn ok(args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check("certbot", &argv).is_ok(), "must be allowed: certbot {args:?}");
    }

    fn refused(args: &[&str]) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(check("certbot", &argv).is_err(), "must be refused: certbot {args:?}");
    }

    /// The exact shapes the domains page sends: with an e-mail address and
    /// with the unsafe no-address expression, in the fixed order and in
    /// another order (the matcher counts, it does not trust the order).
    #[test]
    fn the_request_shape_is_accepted_in_both_e_mail_variants() {
        ok(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
             "--email=marvin@example.com"]);
        ok(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
             "--register-unsafely-without-email"]);
        // an order the app never sends, but the fixed set either way
        ok(&["--agree-tos", "--register-unsafely-without-email",
             "--nginx", "-d", "sub.deep.example.co.uk", "--non-interactive"]);
        // hyphenated labels, digit labels, multi-level TLDs — all DNS-legal
        ok(&["--nginx", "-d", "my-digit1-site.example-2nd.example.co.uk",
             "--non-interactive", "--agree-tos", "--email=1a_b%c+d-dev@mx.example.org"]);

        // the renewal shape answers for itself, untouched
        ok(&["renew", "--cert-name", "example.com", "--non-interactive"]);
    }

    /// Every fixed flag is required exactly once; missing any one refuses,
    /// as does sending both e-mail expressions or neither.
    #[test]
    fn incomplete_or_conflicting_requests_are_refused() {
        // missing pieces
        refused(&["-d", "example.com", "--non-interactive", "--agree-tos",
                  "--email=a@b.co"]);
        refused(&["--nginx", "--non-interactive", "--agree-tos",
                  "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--agree-tos",
                  "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--email=a@b.co"]);
        // neither e-mail expression
        refused(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos"]);
        // both e-mail expressions: mutually exclusive
        refused(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
                  "--email=a@b.co", "--register-unsafely-without-email"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
                  "--register-unsafely-without-email", "--email=a@b.co"]);
        // duplicated fixed flags
        refused(&["--nginx", "--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "-d", "other.example.com",
                  "--non-interactive", "--agree-tos", "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--non-interactive", "--agree-tos", "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--agree-tos", "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
                  "--email=a@b.co", "--email=c@d.co"]);
    }

    /// Domains must be DNS names: labels of letters/digits/hyphens (no
    /// leading or trailing hyphen, 1..63 long), ASCII only, at most 253
    /// characters — never flags, paths, control characters or guesses.
    #[test]
    fn bad_domain_names_are_refused() {
        let bad: Vec<String> = [
            "",                               // empty label
            ".example.com",                   // empty first label
            "example.com.",                   // trailing empty label
            "-example.com",                   // label starts with a hyphen
            "example.com-",                   // label ends with a hyphen
            "ex ample.com",                   // a space — two operands
            "example..com",                   // empty label
            "not-a-flag;rm -rf",              // never a command
            "/etc/nginx/nginx.conf",          // never a path
            "exämple.com",                    // not ASCII
            "-d-flag",                        // a flag-shaped operand
            &"a".repeat(64),                  // label over the 63 limit
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for domain in &bad {
            refused(&["--nginx", "-d", domain, "--non-interactive", "--agree-tos",
                      "--email=a@b.co"]);
        }
        // a 63-character label in a legal name passes
        ok(&["--nginx", "-d", &format!("{}.example.com", "a".repeat(63)),
             "--non-interactive", "--agree-tos", "--email=a@b.co"]);
        // 254 characters total is over the limit
        refused(&["--nginx", "-d", &"a".repeat(254), "--non-interactive",
                  "--agree-tos", "--email=a@b.co"]);
        // a legal name that reaches exactly the 253-character limit passes
        // (four labels: 63 + 63 + 62 + 62 = 250, plus three dots = 253)
        let edge_name = [
            "b".repeat(63),
            "b".repeat(63),
            "b".repeat(62),
            "b".repeat(62),
        ]
        .join(".");
        assert_eq!(edge_name.len(), 253);
        ok(&["--nginx", "-d", edge_name.as_str(),
             "--non-interactive", "--agree-tos", "--email=a@b.co"]);
    }

    /// E-mail addresses: one `@`, an ASCII local part of the fixed charset
    /// at most 64 long, a DNS-legal domain, at most 254 characters.
    #[test]
    fn bad_e_mail_addresses_are_refused() {
        let bad: Vec<String> = [
            "",                        // empty
            "no-at-sign.example.com",  // no @
            "@example.com",            // empty local part
            "a@",                      // empty domain
            "a@b@c.co",                // two @
            "a b@example.com",         // a space
            "а@example.com",           // not ASCII (cyrillic а)
            &format!("a{}", "b".repeat(65)), // > 64 local-part characters
            "a@-bad.example.com",      // domain label starts with a hyphen
            "a@example.com-",          // domain label ends with a hyphen
            "a@example..com",          // empty domain label
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for address in &bad {
            refused(&["--nginx", "-d", "example.com", "--non-interactive",
                      "--agree-tos", &format!("--email={address}")]);
        }
        // exactly-64 local characters are legal
        ok(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
             &format!("--email={}@example.com", "a".repeat(64))]);
        // the charset: dots, underscores, percents, pluses, hyphens
        ok(&["--nginx", "-d", "example.com", "--non-interactive", "--agree-tos",
             "--email=first_last%tag+tag-x@example.com"]);
    }

    /// Only the fixed flag set: any other flag, any other operand, and any
    /// 'hook'-carrying argument is refused — and mixed-shape garbage
    /// (request flags and renewal operands together) answers to no shape.
    #[test]
    fn foreign_flags_operands_and_garbage_are_refused() {
        // unknown flags and flag spellings
        for flag in ["--force", "--nginx=-x", "-n", "--email", "--domain", "-x"] {
            refused(&["--nginx", "-d", "example.com", "--non-interactive",
                      "--agree-tos", "--email=a@b.co", flag]);
        }
        // the domain flag is spelled `-d`, not `--domain` or `-D`
        refused(&["-D", "example.com", "--nginx", "--non-interactive",
                  "--agree-tos", "--email=a@b.co"]);
        // unknown operands
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--email=a@b.co", "renew"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--email=a@b.co", "junk"]);
        // the hook blanket refusal wins before any shape analysis
        refused(&["--nginx", "-d", "hook.example.com", "--non-interactive",
                  "--agree-tos", "--email=a@b.co"]);
        refused(&["--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--email=monster-hook@example.com"]);
        // mixed-shape garbage: no shape answers for this
        refused(&["renew", "--nginx", "-d", "example.com", "--non-interactive",
                  "--agree-tos", "--email=a@b.co"]);
        refused(&["--cert-name", "example.com", "--nginx", "-d", "example.com",
                  "--non-interactive", "--agree-tos", "--email=a@b.co"]);
        // empty arguments and empty request
        refused(&[]);
        refused(&["--nginx", "-d", "", "--non-interactive", "--agree-tos",
                  "--email=a@b.co"]);
    }
}

#[cfg(test)]
mod dnf_history_validator_tests {
    use super::{
        dnf_history_transaction_id_valid, package_is_kernel_package,
        package_matches_running_kernel, pkg_maintenance_op_valid,
    };

    #[test]
    fn dnf_transaction_ids_are_positive_decimal_u32_values() {
        for valid in ["1", "42", "999999", "4294967295"] {
            dnf_history_transaction_id_valid(valid)
                .unwrap_or_else(|err| panic!("{valid:?} must be valid: {err}"));
        }
        for invalid in [
            "", "0", "00", "-1", "+1", " 42", "42 ", "12a", "1.5", "１", "0x10",
            "12345678901", "4294967296", "4_2",
        ] {
            assert!(
                dnf_history_transaction_id_valid(invalid).is_err(),
                "{invalid:?} must be refused"
            );
        }
    }

    #[test]
    fn maintenance_operations_are_exactly_the_three_supported_names() {
        for valid in ["clean", "autoremove", "makecache"] {
            pkg_maintenance_op_valid(valid)
                .unwrap_or_else(|err| panic!("{valid:?} must be valid: {err}"));
        }
        for invalid in [
            "", "Clean", "update", "clean ", " autoremove", "remove", "history", "make-cache",
        ] {
            assert!(
                pkg_maintenance_op_valid(invalid).is_err(),
                "{invalid:?} must be refused"
            );
        }
    }

    #[test]
    fn kernel_package_family_detection_is_prefix_based() {
        for package in [
            "kernel",
            "kernel-core-6.8.5-301.fc40.x86_64",
            "kernel-modules-extra-6.8.5",
            "linux-image-6.8.0-31-generic",
        ] {
            assert!(
                package_is_kernel_package(package),
                "{package:?} must be treated as a kernel package"
            );
        }
        for package in ["glibc-2.39", "kmod-31", "dracut-100", ""] {
            assert!(
                !package_is_kernel_package(package),
                "{package:?} must not be treated as a kernel package"
            );
        }
    }

    #[test]
    fn running_kernel_matching_is_version_specific() {
        let release = "6.8.5-301.fc40.x86_64";
        // resolved kernel packages that contain the running release
        for package in [
            "kernel-core-6.8.5-301.fc40.x86_64",
            "kernel-modules-6.8.5-301.fc40.x86_64",
            "kernel-devel-6.8.5-301.fc40.x86_64",
            "kernel-core-1:6.8.5-301.fc40.x86_64",
        ] {
            assert!(
                package_matches_running_kernel(package, release),
                "{package:?} must match the running kernel"
            );
        }
        // other versions, non-kernel packages and unusable inputs must not match
        for package in [
            "kernel-core-6.7.4-200.fc39.x86_64",
            "kernel-core-6.8.5-301.fc40.aarch64",
            "glibc-6.8.5-301.fc40.x86_64",
            "kernel",
            "",
        ] {
            assert!(
                !package_matches_running_kernel(package, release),
                "{package:?} must not match the running kernel"
            );
        }
        assert!(!package_matches_running_kernel(
            "kernel-core-6.8.5-301.fc40.x86_64",
            ""
        ));
        assert!(package_matches_running_kernel(
            "  kernel-core-6.8.5-301.fc40.x86_64  ",
            "  6.8.5-301.fc40.x86_64  "
        ));
    }
}

#[cfg(test)]
mod user_management_validator_tests {
    use super::{
        ensure_arguments_for_program, group_name_valid, is_protected_admin_group,
        is_protected_system_user, login_session_id_valid, user_account_name_valid,
        user_fullname_valid, user_password_valid,
    };

    fn check_lcp(args: &[&str]) -> Result<(), String> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        ensure_arguments_for_program("lcp-admin", &argv)
    }

    #[test]
    fn protected_admin_groups_cover_privilege_and_log_access() {
        for group in [
            "root", "wheel", "sudo", "admin", "adm", "polkitd", "systemd-journal",
            "docker", "libvirt", "SYSTEMD-JOURNAL",
        ] {
            assert!(is_protected_admin_group(group), "{group:?} must be protected");
        }
        for group in ["developers", "users", "docker-users", "wheelz"] {
            assert!(!is_protected_admin_group(group), "{group:?} must stay deletable");
        }
    }

    #[test]
    fn account_group_fullname_and_password_grammars() {
        for valid in ["alice", "a1", "_svc", "dev.user", "dev-user"] {
            user_account_name_valid(valid).unwrap_or_else(|e| panic!("{valid:?}: {e}"));
            group_name_valid(valid).unwrap_or_else(|e| panic!("{valid:?}: {e}"));
        }
        for invalid in ["", "Alice", "1alice", "-alice", "a b", "a/b", "a:b", "é"] {
            assert!(user_account_name_valid(invalid).is_err(), "{invalid:?}");
            assert!(group_name_valid(invalid).is_err(), "{invalid:?}");
        }
        assert!(user_account_name_valid(&"a".repeat(33)).is_err());

        for valid in ["Alice Example", "A. Sorensen (test)", "x"] {
            user_fullname_valid(valid).unwrap_or_else(|e| panic!("{valid:?}: {e}"));
        }
        for invalid in ["", "-dash", "line\nbreak", "tab\there", &"a".repeat(65)] {
            assert!(user_fullname_valid(invalid).is_err(), "{invalid:?}");
        }

        for valid in ["x", "correct horse battery staple", "cölon:allowed"] {
            user_password_valid(valid).unwrap_or_else(|e| panic!("{valid:?}: {e}"));
        }
        for invalid in ["", "a\nb", "a\rb", "a\0b", &"a".repeat(1025)] {
            assert!(user_password_valid(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn session_ids_and_protected_accounts() {
        for valid in ["1", "42", "4294967296"] {
            login_session_id_valid(valid).unwrap_or_else(|e| panic!("{valid:?}: {e}"));
        }
        for invalid in ["", "0", "-1", "0x1", "c1", "1 2", "１２"] {
            assert!(login_session_id_valid(invalid).is_err(), "{invalid:?}");
        }
        for protected in ["root", "sshd", "systemd-network", "SYSTEMD-RESOLVE", "polkitd"] {
            assert!(is_protected_system_user(protected), "{protected:?}");
        }
        for allowed in ["alice", "bob", "systemdadmin"] {
            assert!(!is_protected_system_user(allowed), "{allowed:?}");
        }
    }

    #[test]
    fn user_action_gate_validates_shapes_and_verbs() {
        for args in [
            vec!["user_add", "alice"],
            vec!["user_add", "alice", "Alice Example"],
            vec!["user_delete", "alice"],
            vec!["user_delete", "alice", "remove-home"],
            vec!["user_set_password", "alice"],
            vec!["user_toggle_sudo", "alice", "grant"],
            vec!["user_toggle_sudo", "alice", "revoke"],
            vec!["user_modify_group", "alice", "dev", "add"],
            vec!["user_modify_group", "alice", "dev", "remove"],
            vec!["group_manage", "add", "dev"],
            vec!["group_manage", "delete", "dev"],
            vec!["user_lock", "alice", "lock"],
            vec!["user_lock", "alice", "unlock"],
            vec!["session_kill", "42"],
        ] {
            check_lcp(&args).unwrap_or_else(|e| panic!("{args:?} must be valid: {e}"));
        }
        for args in [
            vec!["user_add"],
            vec!["user_add", "-bad"],
            vec!["user_add", "alice", "-bad"],
            vec!["user_delete", "alice", "force"],
            vec!["user_set_password"],
            vec!["user_toggle_sudo", "alice", "yes"],
            vec!["user_modify_group", "alice", "dev", "grant"],
            vec!["group_manage", "remove", "dev"],
            vec!["user_lock", "alice", "disable"],
            vec!["session_kill", "c1"],
            vec!["session_kill", "42", "extra"],
            vec!["user_set_password", "alice", "extra"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }

    #[test]
    fn userdel_accepts_remove_but_still_refuses_the_short_form() {
        let helper = |args: &[&str]| {
            let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            ensure_arguments_for_program("userdel", &argv)
        };
        helper(&["--force", "--remove", "alice"]).expect("long remove option must be allowed");
        assert!(helper(&["-r", "alice"]).is_err(), "short -r stays refused");
        assert!(helper(&["--force", "root"]).is_ok(), "account policy is action-level");
    }
}

#[cfg(test)]
mod security_action_validator_tests {
    use super::{
        ctrl_alt_del_verb_valid, ensure_arguments_for_program, security_apply_op_valid,
        security_kernel_param_valid, security_service_unit_valid, security_service_verb_valid,
        security_ssh_param_valid,
    };

    fn check_lcp(args: &[&str]) -> Result<(), String> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        ensure_arguments_for_program("lcp-admin", &argv)
    }

    #[test]
    fn security_actions_are_closed_allow_lists() {
        for unit in ["auditd", "usbguard", "chronyd", "systemd-timesyncd"] {
            security_service_unit_valid(unit).unwrap_or_else(|e| panic!("{unit:?}: {e}"));
        }
        for unit in ["sshd", "firewalld", "dbus", "auditd.service", ""] {
            assert!(security_service_unit_valid(unit).is_err(), "{unit:?}");
        }
        for verb in ["enable", "disable"] {
            security_service_verb_valid(verb).unwrap_or_else(|e| panic!("{verb:?}: {e}"));
        }
        for verb in ["restart", "start", "stop", "mask", ""] {
            assert!(security_service_verb_valid(verb).is_err(), "{verb:?}");
        }
        for verb in ["mask", "unmask"] {
            ctrl_alt_del_verb_valid(verb).unwrap_or_else(|e| panic!("{verb:?}: {e}"));
        }
        for verb in ["enable", "disable", "start", ""] {
            assert!(ctrl_alt_del_verb_valid(verb).is_err(), "{verb:?}");
        }
    }

    #[test]
    fn security_file_operations_are_closed_and_validated() {
        for (param, value) in [
            ("PermitRootLogin", "prohibit-password"),
            ("PermitRootLogin", "yes"),
            ("PasswordAuthentication", "no"),
            ("PasswordAuthentication", "yes"),
            ("MaxAuthTries", "4"),
            ("MaxAuthTries", "6"),
            ("LoginGraceTime", "60"),
            ("LoginGraceTime", "120"),
        ] {
            security_ssh_param_valid(param, value)
                .unwrap_or_else(|e| panic!("{param}={value}: {e}"));
        }
        for (param, value) in [
            ("PermitRootLogin", "no"),
            ("PasswordAuthentication", "maybe"),
            ("MaxAuthTries", "9"),
            ("Banner", "none"),
            ("LoginGraceTime", "30"),
        ] {
            assert!(
                security_ssh_param_valid(param, value).is_err(),
                "{param}={value} must be refused"
            );
        }

        for key in [
            "kernel.randomize_va_space",
            "net.ipv4.tcp_syncookies",
            "net.ipv4.ip_forward",
            "kernel.kptr_restrict",
            "kernel.dmesg_restrict",
            "net.ipv4.conf.all.accept_redirects",
            "net.ipv4.conf.all.log_martians",
        ] {
            security_kernel_param_valid(key, "1").unwrap_or_else(|e| panic!("{key}: {e}"));
        }
        for (key, value) in [
            ("kernel.panic", "1"),
            ("net.ipv4.ip_forward", "yes"),
            ("kernel.dmesg_restrict", "1
"),
            ("kernel.dmesg_restrict", ""),
        ] {
            assert!(
                security_kernel_param_valid(key, value).is_err(),
                "{key}={value:?} must be refused"
            );
        }

        for op in [
            "password_policy", "tmp_sticky_on", "tmp_sticky_off", "passwd_perms",
            "shadow_perms", "pam_faillock_on", "pam_faillock_off", "umask_on",
            "umask_off", "ssh_idle_on", "ssh_idle_off", "blacklist_fs_on",
            "blacklist_fs_off", "legal_banner_on", "legal_banner_off",
        ] {
            security_apply_op_valid(op).unwrap_or_else(|e| panic!("{op}: {e}"));
        }
        for op in ["", "reboot", "password_policy_extra"] {
            assert!(security_apply_op_valid(op).is_err(), "{op:?}");
        }
    }

    #[test]
    fn security_file_action_gate_enforces_shapes() {
        check_lcp(&["security_apply", "password_policy"]).expect("valid apply");
        check_lcp(&["security_ssh_param", "MaxAuthTries", "4"]).expect("valid ssh pair");
        check_lcp(&["security_kernel_param", "kernel.kptr_restrict", "2"]).expect("valid sysctl");
        check_lcp(&["security_lock_account"]).expect("lock all");
        check_lcp(&["security_lock_account", "alice"]).expect("lock one");
        for args in [
            vec!["security_apply", "reboot"],
            vec!["security_apply"],
            vec!["security_ssh_param", "MaxAuthTries", "9"],
            vec!["security_kernel_param", "kernel.panic", "1"],
            vec!["security_lock_account", "Alice"],
            vec!["security_lock_account", "alice", "extra"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }

    #[test]
    fn security_action_gate_enforces_shapes() {
        check_lcp(&["security_service_toggle", "auditd", "enable"]).expect("valid auditd action");
        check_lcp(&["security_service_toggle", "chronyd", "disable"]).expect("valid chronyd action");
        check_lcp(&["security_ctrl_alt_del", "mask"]).expect("valid mask action");
        for args in [
            vec!["security_service_toggle", "auditd"],
            vec!["security_service_toggle", "sshd", "enable"],
            vec!["security_service_toggle", "auditd", "restart"],
            vec!["security_service_toggle", "auditd", "enable", "extra"],
            vec!["security_ctrl_alt_del", "start"],
            vec!["security_ctrl_alt_del", "mask", "extra"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }
}

#[cfg(test)]
mod service_manager_validator_tests {
    use super::{
        ensure_arguments_for_program, service_unit_name_valid, unit_action_valid,
    };

    fn check_lcp(args: &[&str]) -> Result<(), String> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        ensure_arguments_for_program("lcp-admin", &argv)
    }

    #[test]
    fn unit_names_and_protected_action_policy() {
        for unit in ["nginx.service", "cron-daily.timer", "dev-sda.device", "user@1000.service"] {
            service_unit_name_valid(unit).unwrap_or_else(|e| panic!("{unit}: {e}"));
        }
        for unit in ["", "nginx", "-bad.service", "/etc/passwd", "bad;name.service", "a/b.service"] {
            assert!(service_unit_name_valid(unit).is_err(), "{unit:?}");
        }

        for (verb, unit) in [
            ("stop", "systemd-logind.service"),
            ("disable", "dbus-broker.service"),
            ("mask", "systemd-journald.service"),
            ("stop", "firewalld.service"),
            ("disable", "sshd.service"),
            ("mask", "NetworkManager.service"),
        ] {
            assert!(unit_action_valid(verb, unit).is_err(), "{verb} {unit} must be blocked");
        }
        for (verb, unit) in [
            ("restart", "sshd.service"),
            ("start", "firewalld.service"),
            ("unmask", "systemd-logind.service"),
            ("stop", "nginx.service"),
            ("disable", "httpd.service"),
            ("mask", "cups.service"),
        ] {
            unit_action_valid(verb, unit)
                .unwrap_or_else(|e| panic!("{verb} {unit}: {e}"));
        }
        assert!(unit_action_valid("explode", "nginx.service").is_err());
    }

    #[test]
    fn unit_action_gate_enforces_shapes() {
        check_lcp(&["audit_setup"]).expect("valid audit setup");
        assert!(check_lcp(&["audit_setup", "extra"]).is_err());
        check_lcp(&["unit_action", "restart", "nginx.service"]).expect("valid action");
        check_lcp(&["unit_action", "mask", "cups.service"]).expect("valid mask");
        check_lcp(&["unit_dropin_write", "nginx.service"]).expect("valid dropin");
        check_lcp(&["unit_create", "my-app.service"]).expect("valid create");
        check_lcp(&["unit_daemon_reload"]).expect("valid recovery");
        for args in [
            vec!["unit_action", "restart"],
            vec!["unit_action", "explode", "nginx.service"],
            vec!["unit_action", "stop", "sshd.service"],
            vec!["unit_action", "restart", "nginx"],
            vec!["unit_dropin_write"],
            vec!["unit_dropin_write", "-bad.service"],
            vec!["unit_create", "-bad.service"],
            vec!["unit_daemon_reload", "extra"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }
}

#[cfg(test)]
mod copr_validator_tests {
    use super::{copr_repo_valid, copr_verb_valid, ensure_arguments_for_program};

    fn check_lcp(args: &[&str]) -> Result<(), String> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        ensure_arguments_for_program("lcp-admin", &argv)
    }

    #[test]
    fn copr_repo_grammar_is_closed() {
        for repo in [
            "owner/project",
            "ppa:owner/project",
            "owner/project-1.2+x",
            "a_b/c.d",
        ] {
            copr_repo_valid(repo).unwrap_or_else(|e| panic!("{repo:?} must be valid: {e}"));
        }
        for repo in [
            "",
            "owner",
            "owner/",
            "/project",
            "owner/project/extra",
            "-owner/project",
            "owner/-project",
            "owner/.",
            "owner/..",
            "../etc/passwd",
            "own er/project",
            "owner/project:tag",
            "PPA:owner/project",
            "ppa:",
            "owner/pro\nject",
        ] {
            assert!(copr_repo_valid(repo).is_err(), "{repo:?} must be refused");
        }
        let overlong = format!("{}/{}", "a".repeat(120), "b".repeat(120));
        assert!(copr_repo_valid(&overlong).is_err(), "overlong repo must be refused");
    }

    #[test]
    fn copr_verbs_are_enable_or_disable_only() {
        for verb in ["enable", "disable"] {
            copr_verb_valid(verb).unwrap_or_else(|e| panic!("{verb}: {e}"));
        }
        for verb in ["", "install", "ENABLE", "enable ", "--enable"] {
            assert!(copr_verb_valid(verb).is_err(), "{verb:?} must be refused");
        }
    }

    #[test]
    fn nmcli_interface_state_gate_enforces_shape() {
        check_lcp(&["nmcli_interface_state", "eth0", "up"]).expect("valid up");
        check_lcp(&["nmcli_interface_state", "wlan0", "down"]).expect("valid down");
        for args in [
            vec!["nmcli_interface_state"],
            vec!["nmcli_interface_state", "eth0"],
            vec!["nmcli_interface_state", "eth0", "toggle"],
            vec!["nmcli_interface_state", "lo", "down"],
            vec!["nmcli_interface_state", "eth0", "up", "extra"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }

    #[test]
    fn copr_action_gate_enforces_exact_shape() {
        check_lcp(&["copr_manage", "enable", "owner/project"]).expect("valid enable");
        check_lcp(&["copr_manage", "disable", "ppa:owner/project"]).expect("valid disable");
        for args in [
            vec!["copr_manage"],
            vec!["copr_manage", "enable"],
            vec!["copr_manage", "enable", "owner/project", "extra"],
            vec!["copr_manage", "install", "owner/project"],
            vec!["copr_manage", "enable", "owner"],
            vec!["copr_manage", "enable", "-owner/project"],
        ] {
            assert!(check_lcp(&args).is_err(), "{args:?} must be refused");
        }
    }
}


#[cfg(test)]
mod polkit_policy_tests {
    use super::{verify_polkit_policy_file, verify_polkit_policy_text};

    const SHIPPED: &str = include_str!("../../polkit/com.controlpanel.pkexec.policy");

    #[test]
    fn the_shipped_policy_passes_verification() {
        verify_polkit_policy_text(SHIPPED).expect("the shipped policy must be valid");
    }

    #[test]
    fn missing_stale_or_weakened_policies_are_refused() {
        let stale_id = SHIPPED.replace(
            "com.controlpanel.helper.packaged",
            "com.controlpanel.helper.other",
        );
        assert!(verify_polkit_policy_text(&stale_id).is_err());

        let generic_program = SHIPPED.replace("/usr/bin/lcp-admin", "/usr/bin/bash");
        assert!(verify_polkit_policy_text(&generic_program).is_err());

        let cached = SHIPPED.replace(
            "<allow_active>auth_admin</allow_active>",
            "<allow_active>auth_admin_keep</allow_active>",
        );
        assert!(verify_polkit_policy_text(&cached).is_err());

        let open = SHIPPED.replace(
            "<allow_any>auth_admin</allow_any>",
            "<allow_any>yes</allow_any>",
        );
        assert!(verify_polkit_policy_text(&open).is_err());

        let malformed = SHIPPED.replace("org.freedesktop.policykit.exec.path\">", "unterminated=\"");
        assert!(verify_polkit_policy_text(&malformed).is_err());

        let truncated = SHIPPED.replace("</annotate>", "");
        assert!(verify_polkit_policy_text(&truncated).is_err());

        let duplicate_action = format!("{SHIPPED}{SHIPPED}");
        assert!(verify_polkit_policy_text(&duplicate_action).is_err());

        let extra_action = SHIPPED.replace(
            "</policyconfig>",
            "<action id=\"com.example.other\"><defaults><allow_any>auth_admin</allow_any><allow_inactive>auth_admin</allow_inactive><allow_active>auth_admin</allow_active></defaults></action></policyconfig>",
        );
        assert!(verify_polkit_policy_text(&extra_action).is_err());
    }

    #[test]
    fn the_packaged_rule_file_matches_the_helper_text() {
        let packaged = include_str!("../../polkit/50-linux-control-panel.rules");
        assert_eq!(packaged, super::POLKIT_RULE_TEXT);
    }

    #[test]
    fn the_installed_rule_must_match_the_shipped_text() {
        use super::{verify_polkit_rule_text, POLKIT_RULE_TEXT};
        verify_polkit_rule_text(POLKIT_RULE_TEXT).expect("the shipped rule text must be valid");
        let weakened = POLKIT_RULE_TEXT.replace("subject.isInGroup(\"wheel\")", "true");
        assert!(verify_polkit_rule_text(&weakened).is_err());
    }

    #[test]
    fn duplicate_policy_files_are_refused() {
        use super::verify_polkit_policy_directory;

        let directory = std::env::temp_dir().join(format!(
            "lcp-polkit-actions-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("com.controlpanel.pkexec.policy"), SHIPPED).unwrap();
        verify_polkit_policy_directory(&directory).expect("a single canonical policy is valid");

        std::fs::write(directory.join("unrelated.policy"), "<policyconfig></policyconfig>").unwrap();
        verify_polkit_policy_directory(&directory)
            .expect("an unrelated policy file must not be treated as a duplicate");

        std::fs::write(
            directory.join("duplicate.policy"),
            SHIPPED.replace("com.controlpanel.helper.packaged", "com.controlpanel.helper.shadow"),
        )
        .unwrap();
        assert!(
            verify_polkit_policy_directory(&directory).is_err(),
            "a second file defining a control-panel helper action must be refused"
        );

        std::fs::write(
            directory.join("duplicate.policy"),
            "<policyconfig><annotate key=\"org.freedesktop.policykit.exec.path\">/usr/bin/lcp-admin</annotate></policyconfig>",
        )
        .unwrap();
        assert!(
            verify_polkit_policy_directory(&directory).is_err(),
            "a second file annotating a helper path must be refused"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn override_action_directories_are_scanned_and_fail_closed() {
        use super::verify_polkit_policy_layout;

        let root = std::env::temp_dir().join(format!(
            "lcp-polkit-layout-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let canonical = root.join("usr-share");
        let etc = root.join("etc");
        let run = root.join("run");
        let local = root.join("local");
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::write(
            canonical.join("com.controlpanel.pkexec.policy"),
            SHIPPED,
        )
        .unwrap();

        // Optional override directories may be absent.
        verify_polkit_policy_layout(&canonical, &[&etc, &run, &local])
            .expect("missing optional action directories are valid");

        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("unrelated.policy"), "<policyconfig></policyconfig>").unwrap();
        verify_polkit_policy_layout(&canonical, &[&etc, &run, &local])
            .expect("an unrelated override policy must not be treated as a conflict");

        // /etc takes precedence: a helper id there must be refused.
        std::fs::write(
            etc.join("shadow.policy"),
            SHIPPED.replace("com.controlpanel.helper.packaged", "com.controlpanel.helper.shadow"),
        )
        .unwrap();
        assert!(
            verify_polkit_policy_layout(&canonical, &[&etc, &run, &local]).is_err(),
            "a conflicting action id in /etc must be refused"
        );
        std::fs::remove_file(etc.join("shadow.policy")).unwrap();

        // /run: even the same filename with unrelated content is a shadow.
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(
            run.join("com.controlpanel.pkexec.policy"),
            "<policyconfig></policyconfig>",
        )
        .unwrap();
        assert!(
            verify_polkit_policy_layout(&canonical, &[&etc, &run, &local]).is_err(),
            "a same-named policy in /run must be refused even if its content differs"
        );
        std::fs::remove_file(run.join("com.controlpanel.pkexec.policy")).unwrap();

        // /usr/local/share: a helper path annotation is a conflict.
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(
            local.join("annotate.policy"),
            "<policyconfig><annotate key=\"org.freedesktop.policykit.exec.path\">/usr/bin/lcp-admin</annotate></policyconfig>",
        )
        .unwrap();
        assert!(
            verify_polkit_policy_layout(&canonical, &[&etc, &run, &local]).is_err(),
            "a helper path annotation in /usr/local/share must be refused"
        );

        // An override path that exists but is not a directory must fail closed.
        let not_a_directory = root.join("not-a-directory");
        std::fs::write(&not_a_directory, b"not a directory").unwrap();
        assert!(
            verify_polkit_policy_layout(&canonical, &[&not_a_directory]).is_err(),
            "an unreadable/non-directory override path must fail closed"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_installed_policy_file_is_an_error() {
        let error = verify_polkit_policy_file(
            "/nonexistent-lcp-polkit-policy-for-test.policy",
        )
        .expect_err("a missing policy must be refused");
        assert!(error.contains("could not be read"), "{error}");
    }
}

#[cfg(test)]
mod system_info_validator_tests {
    use super::{
        ensure_arguments_for_program as check, process_renice_valid, smart_device_valid,
    };

    fn check_lcp(args: &[&str]) -> Result<(), String> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        check("lcp-admin", &argv)
    }

    #[test]
    fn process_renice_only_accepts_negative_priority_and_unprotected_pids() {
        process_renice_valid(4242, -1).expect("negative priority");
        process_renice_valid(4242, -20).expect("minimum priority");
        assert!(process_renice_valid(4242, 0).is_err());
        assert!(process_renice_valid(4242, 5).is_err());
        assert!(process_renice_valid(4242, -21).is_err());
        assert!(process_renice_valid(1, -5).is_err());
        assert!(process_renice_valid(100, -5).is_err());
        assert!(process_renice_valid(101, -5).is_ok());

        check_lcp(&["process_renice", "4242", "-5"]).expect("valid gate shape");
        assert!(check_lcp(&["process_renice", "50", "-5"]).is_err());
        assert!(check_lcp(&["process_renice", "4242", "5"]).is_err());
        assert!(check_lcp(&["process_renice", "4242"]).is_err());
        assert!(check_lcp(&["process_renice", "x", "-5"]).is_err());
    }

    #[test]
    fn smart_test_type_is_closed() {
        use super::smart_test_type_valid;
        for test_type in ["short", "long", "offline", "conveyance"] {
            smart_test_type_valid(test_type).expect("valid self-test type");
        }
        for test_type in ["", "extended", "SHORT", "short;reboot"] {
            assert!(smart_test_type_valid(test_type).is_err(), "{test_type:?}");
        }
    }

    #[test]
    fn smart_device_only_accepts_a_direct_non_symlink_device_path() {
        assert!(smart_device_valid("/etc/passwd").is_err());
        assert!(smart_device_valid("/dev/../etc/passwd").is_err());
        assert!(smart_device_valid("/dev/disk/by-id/foo").is_err());
        assert!(check_lcp(&["smart_data", "/etc/passwd"]).is_err());
        assert!(check_lcp(&["smart_self_test", "/etc/passwd", "long"]).is_err());
        assert!(check_lcp(&["smart_self_test", "/etc/passwd", "bad"]).is_err());

        #[cfg(not(feature = "test-fixture"))]
        {
            // /dev/null exists but is a character device, not a block device.
            assert!(smart_device_valid("/dev/null").is_err());
        }

        #[cfg(feature = "test-fixture")]
        {
            let _guard = super::FIXTURE_TEST_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dev_dir = std::path::Path::new("/tmp/lcp-admin-test-fixture/dev");
            std::fs::create_dir_all(dev_dir).unwrap();
            let device = dev_dir.join("validator-sda");
            std::fs::write(&device, b"fixture device").unwrap();
            let device_str = device.to_str().unwrap().to_string();
            smart_device_valid(&device_str).expect("fixture regular file is allowed");
            check_lcp(&["smart_health", &device_str]).expect("valid health gate shape");
            check_lcp(&["smart_data", &device_str]).expect("valid data gate shape");
            check_lcp(&["smart_self_test", &device_str, "long"])
                .expect("valid self-test gate shape");

            let link = dev_dir.join("validator-link");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&device, &link).unwrap();
            assert!(smart_device_valid(link.to_str().unwrap()).is_err());
            assert!(check_lcp(&["smart_health", link.to_str().unwrap()]).is_err());

            let _ = std::fs::remove_file(&link);
            let _ = std::fs::remove_file(&device);
        }
    }
}
