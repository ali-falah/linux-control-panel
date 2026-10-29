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
    verify_trusted_binary, is_valid_account_name, validate_profile_sh_content,
    validate_profile_sh_path, POLKIT_RULE_FILE, POLKIT_RULE_TEXT, REPO_CONTENT_MAX,
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

/// Named actions the helper performs itself, instead of executing a root
/// program. Their arguments are validated by `check_helper_action` in
/// root_policy.rs, so the helper only ever `exec`s programs for the generic
/// allow-listed calls.
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
    "install_polkit_rule", "read_user_ssh_keys", "write_user_ssh_keys",
    "write_profile_sh",
];

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
        let outcome = if requested == "install_polkit_rule" {
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
                // policy errors arrive without the helper's own prefix
                let line = message
                    .strip_prefix("lcp-admin: ")
                    .map(|rest| format!("lcp-admin: {rest}"))
                    .unwrap_or_else(|| format!("lcp-admin: {message}"));
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
    install_root_owned_file(path, &content, "profile file")
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
    install_root_owned_file(path, &content, "repository file")
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
/// * the content lands in a uniquely named `O_CREAT|O_EXCL|O_NOFOLLOW` temp
///   sibling, then one `renameat` publishes it (replacing whatever sits at
///   the destination name instead of following it),
/// * owner/mode transfer happens on the temp descriptor *before* the
///   rename, so there is no path-based `chmod`/`chown` race window,
/// * an existing file keeps its owner and mode; a new file gets 0644
///   `privileged:privileged`.
fn install_root_owned_file(path: &str, content: &str, what: &str) -> Result<(), String> {
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
            Some((stat.st_uid, stat.st_gid, stat.st_mode & 0o7777))
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

    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::from(fd);
        let (target_uid, target_gid, target_mode) = existing
            .unwrap_or((dir_stat.st_uid, dir_stat.st_gid, 0o644));
        rustix::fs::fchmod(&file, Mode::from_raw_mode(target_mode))
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
        rustix::fs::renameat(
            dir.as_fd(),
            temp_name.as_str(),
            dir.as_fd(),
            name.as_str(),
        )
        .map_err(|_| format!("lcp-admin: could not publish the {what}"))
    })();

    match result {
        Ok(()) => Ok(()),
        Err(message) => {
            let _ = rustix::fs::unlinkat(&dir, temp_name.as_str(), AtFlags::empty());
            Err(message)
        }
    }
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
