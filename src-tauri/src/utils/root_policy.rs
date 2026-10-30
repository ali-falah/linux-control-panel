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
];

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

/// Upper bound for a profile.d file's name (including the `.sh` extension).
const PROFILE_NAME_MAX: usize = 64;

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
    // The one directory policy packages may be installed from. It is owned by
    // root and not writable by other users, so the same user who calls the
    // helper can not swap the package after it passed validation (and
    // `ensure_trusted_path` additionally refuses symlinked paths).
    const POLICY_DIR: &str = "/var/lib/linux-control-panel/policy";
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
                    if !value.starts_with(&format!("{POLICY_DIR}/")) {
                        return Err(operand_not_allowed(
                            PROGRAM,
                            &value,
                            "policy packages must be staged in /var/lib/linux-control-panel/policy (a root-owned directory), not in a user-writable location",
                        ));
                    }
                    // the package is installed by root: verify the reachable
                    // target is root owned and not user writable
                    match std::fs::canonicalize(&value) {
                        Ok(canonical) => {
                            verify_trusted_binary(&canonical.to_string_lossy())?;
                        }
                        Err(_) => {
                            return Err(operand_not_allowed(
                                PROGRAM,
                                &value,
                                "the policy package must exist and be reachable",
                            ));
                        }
                    }
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
const HELPER_ACTIONS: &[&str] = &[
    "admin_keep", "install_polkit_rule", "read_user_ssh_keys", "write_user_ssh_keys",
    "write_profile_sh", "restore_profile_sh",
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

/// A helper revision backup must be the exact shape `create_backup_sibling`
/// produces for `dest`: the same confined parent directory, the name derived
/// from the destination (`<dest>.<unix-seconds>_<pid>.<nanos>.bak`). This is
/// enforced both by the app (before the privileged call exists) and again
/// inside the helper — both halves hold independently.
pub fn restore_profile_sh_backup_shape(dest: &str, backup: &str) -> Result<(), String> {
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
    if candidate.parent() != Some(Path::new(PROFILE_D_DIR)) {
        return Err(format!(
            "The backup must be a revision sibling of the destination in {PROFILE_D_DIR}, not {backup}"
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
            "The backup file name {backup_name:?} must start with the destination name {:?} and carry a '.<timestamp>_<suffix>.bak' revision.",
            dest_name
        ));
    };
    // <unix-seconds>_<pid>.<nanos> — the revision `create_backup_sibling`
    // writes; a free-form name would let any file pose as "a backup".
    let Some((secs, suffix)) = remainder.split_once('_') else {
        return Err(format!(
            "The backup revision {remainder:?} must be <unix-seconds>_<suffix>.bak."
        ));
    };
    if secs.is_empty() || secs.len() > 20 || !secs.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "The backup revision {remainder:?} must start with unix-seconds digits."
        ));
    }
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        return Err(format!(
            "The backup revision suffix {suffix:?} may only contain letters, digits and dots."
        ));
    }
    Ok(())
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

        ok("semodule", &["-l"]);
        // installs are only allowed from the panel's root-owned policy dir;
        // on dev machines that file does not exist → also refused
        refused("semodule", &["-i", "/tmp/audit2allow/mod.pp"]);
        refused("semodule", &["-i", "/var/lib/linux-control-panel/policy/mod.pp"]);
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

    // ── the helper action ─────────────────────────────────────────────────────

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
    use super::ensure_repo_file_content;

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
}
