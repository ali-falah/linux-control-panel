//! Integration tests: the freshly built helper binary must refuse the
//! argument shapes the hardening phase 2 report lists, with exit code 2 and
//! an `lcp-admin:` message on stderr, while the app's real call shapes still
//! run (as the invoking user here — without pkexec the helper just execs it).
//!
//! The same policy rules are also unit-tested inside `root_policy.rs`, but
//! only these tests prove the helper binary actually enforces them.

use std::process::Command;

/// Helper paths under `target/{debug,release}` are outside this crate, so the
/// checkout-relative path to the debug artifact is used.
const HELPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/debug/lcp-admin"
);

#[derive(Debug)]
struct Run {
    code: i32,
    stderr: String,
}

/// Run the helper; returns the exit data on success.
fn run(program: &str, args: &[&str]) -> std::io::Result<Run> {
    Command::new(HELPER)
        .arg(program)
        .args(args)
        .output()
        .map(|out| Run {
            code: out.status.code().unwrap_or(101),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
}

fn refused(program: &str, args: &[&str], because: &str) {
    let run = run(program, args)
        .unwrap_or_else(|err| panic!("{program} {args:?}: unexpected spawn error {err}"));
    assert_eq!(
        run.code, 2,
        "expected refusal (exit 2): {program} {args:?} — {run:?} ({because})"
    );
    assert!(
        run.stderr.starts_with("lcp-admin:"),
        "refusal must come from the helper itself: {:?}",
        run.stderr
    );
}

fn allowed(program: &str, args: &[&str]) {
    let run = run(program, args)
        .unwrap_or_else(|err| panic!("{program} {args:?}: unexpected spawn error {err}"));
    assert_eq!(run.code, 0, "expected success: {program} {args:?} — {run:?}");
}

#[test]
fn exec_capable_programs_are_never_started() {
    refused("bash", &["-c", "id"], "shell");
    refused("sh", &["-c", "id"], "shell");
    refused("awk", &["'BEGIN {}'"], "interpreter");
    refused("gawk", &["'BEGIN {}'"], "interpreter");
    refused("find", &["/tmp", "-exec", "id", ";"], "exec-capable");
}

#[test]
fn dash_arguments_are_refused() {
    refused("mkdir", &["-rf", "/tmp/never"], "unknown/multi flag; also no such flag on the allow list");
    refused("ufw", &["--force", "enable"], "ufw has no flag list");
    refused("journalctl", &["--no-pager"], "no privileged journalctl flags");
}

#[test]
fn package_manager_refuses_setopt_and_local_files() {
    refused("dnf", &["--setopt=x", "install", "mesa-libGL"], "setopt outside the timeout shape");
    refused("dnf", &["--setopt=reposdir=/tmp/evil", "install", "mesa-libGL"], "setopt redirect");
    refused("dnf", &["install", "/tmp/payload.rpm"], "local package file");
    refused("rpm", &["-i", "payload.rpm"], "local package file");
    refused("dnf", &["--installroot=/srv/evil", "install", "mesa-libGL"], "foreign root");
    refused("dnf", &["install", "sub/dir"], "path operand");
}

#[test]
fn relative_paths_are_refused_where_absolute_is_required() {
    refused("cat", &["../etc/shadow"], "relative path");
    refused("cat", &["etc/shadow"], "relative path");
    refused("cat", &["/etc/../../shadow"], "relative parts");
    refused("semodule", &["-i", "somefile.pp"], "policy file must be absolute");
    refused("mkdir", &["-p", "relative/dir"], "directory must be absolute");
    refused("tail", &["-n", "5", "log.tmp"], "log file must be absolute");
    refused("useradd", &["-c", "ok", "/abs/name"], "names have no slash");
}

#[test]
fn empty_and_control_character_arguments_are_refused() {
    refused("cat", &[""], "empty argument");
    refused("cat", &["a\nb"], "newline");
    refused("systemctl", &["start", "x\n.service"], "newline");
    // a NUL inside an argument can not even reach the helper's argv — the OS
    // refuses it, so the helper never gets a chance to run anything
    assert!(
        Command::new(HELPER).arg("cat").arg("a\0b").output().is_err(),
        "NUL argument must not spawn"
    );
}

#[test]
fn systemctl_only_runs_the_documented_verbs() {
    refused("systemctl", &["link", "somewhere.service"], "link loads unit files");
    refused("systemctl", &["edit", "nginx"], "editor");
    refused("systemctl", &["import-environment", "DISPLAY"], "environment injection");
    refused("systemctl", &["start", "/opt/evil.service"], "unit file path");
    refused("systemctl", &["sense", "nginx"], "undocumented verb");
}

#[test]
fn user_tools_reject_system_ids_and_duplicate_ids() {
    refused("useradd", &["-o", "-u", "0", "x"], "root id");
    refused("useradd", &["-u", "999", "x"], "system id");
    refused("groupadd", &["--non-unique"], "duplicate ids");
    refused("usermod", &["-o", "-u", "500", "x"], "system id");
}

#[test]
fn ip_never_reaches_netns_or_batch_and_certbot_never_sees_hooks() {
    refused("ip", &["netns", "exec", "something"], "namespace exec");
    refused("ip", &["-batch", "/tmp/script"], "batch script");
    refused("certbot", &["renew", "--post-hook", "anything"], "hook");
}

#[test]
fn valid_calls_still_work() {
    allowed("cat", &["/etc/hostname"]);
    allowed("ls", &["/var/log"]);
    allowed("ip", &["link", "show"]);
    allowed("date", &[]);
}

// ── named actions: `lcp-admin <action>` ──────────────────────────────────────/// Run the helper action with content on stdin; never touches rules.d as the
/// invoking user here — anything that would reach the write already needs
/// root, so these tests only pin down the refusals.
fn run_action_with_stdin(args: &[&str], input: &[u8]) -> std::io::Result<Run> {
    use std::io::Write;
    let mut child = Command::new(HELPER).args(args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
    child.stdin.take().unwrap().write_all(input)?;
    child.wait_with_output().map(|out| Run {
        code: out.status.code().unwrap_or(101),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

#[test]
fn install_polkit_rule_rejects_wrong_content_and_extra_arguments() {    // mismatching content — the one thing the action must never write
    let run = run_action_with_stdin(&["install_polkit_rule"], b"polkit.addRule(function() {});")
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"));
    assert_eq!(run.code, 2, "wrong content must be refused: {run:?}");
    assert!(
        run.stderr.starts_with("lcp-admin:"),
        "refusal must come from the helper itself: {:?}",
        run.stderr
    );

    // empty stdin is also a mismatch
    let run = run_action_with_stdin(&["install_polkit_rule"], b"")
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"));
    assert_eq!(run.code, 2, "empty content must be refused: {run:?}");
    assert!(run.stderr.starts_with("lcp-admin:"));

    // an action takes no parameters at all
    refused("install_polkit_rule", &["extra"], "the action shape is exactly one argument");
    refused("install_polkit_rule", &["--content", "x"], "the action shape is exactly one argument");
}

// ── named actions: user ssh keys (`read/write_user_ssh_keys <account>`) ─────

#[test]
fn ssh_key_actions_reject_bad_account_names_and_wrong_shapes() {
    // traversal and path fragments never reach a home directory
    refused("read_user_ssh_keys", &["../root"], "dots and slashes are not account names");
    refused("write_user_ssh_keys", &["/etc/shadow"], "absolute paths are not account names");
    refused("read_user_ssh_keys", &["marvin;reboot"], "no shell metacharacters");
    refused("read_user_ssh_keys", &["mar vin"], "no spaces");
    // exactly one account name
    refused("read_user_ssh_keys", &[], "missing account name");
    refused("write_user_ssh_keys", &["marvin", "extra"], "too many arguments");
    // unknown actions stay unknown
    refused("write_ssh_keys", &["marvin"], "no such action");
}

#[test]
fn ssh_key_actions_reject_accounts_that_do_not_exist() {
    refused(
        "read_user_ssh_keys",
        &["lcp_no_such_user_xyz"],
        "no local account with that name",
    );
    refused(
        "write_user_ssh_keys",
        &["lcp_no_such_user_xyz"],
        "no local account with that name",
    );
}

#[test]
fn unknown_actions_and_program_shaped_names_are_refused() {
    refused("some_other_action", &[], "no such action, no such program");
    refused("install", &["-m", "0644", "src", "dst"], "wrong install template (see root_policy tests)");
    refused("lcp-admin", &["install_polkit_rule"], "the helper may not call itself");
}

// ── helper hardening: no self-overwrite, no secret reads ────────────────────

#[test]
fn the_helper_is_never_copied_over_itself() {
    // every shape the old install template allowed is now refused outright
    refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/usr/bin/cat", "/usr/local/libexec/lcp-admin"],
            "the helper must never copy an arbitrary root-owned file over itself");
    refused("install", &["-m", "0755", "-o", "root", "-g", "root", "/bin/true", "/usr/local/libexec/lcp-admin"],
            "the helper must never copy an arbitrary root-owned file over itself");
    refused("install", &["-d", "/srv/evil"], "no install verbs at all");
}

#[test]
fn credential_files_are_never_readable_through_the_helper() {
    refused("cat", &["/etc/shadow"], "credentials");
    refused("cat", &["/etc/shadow-"], "credentials");
    refused("grep", &["-i", "x", "/etc/sudoers"], "privilege policy");
    refused("cat", &["/etc/sudoers.d/50-panel"], "privilege policy");
    refused("cat", &["/root/.ssh/id_rsa"], "private key");
    refused("tail", &["-n", "5", "/home/.ssh/id_rsa"], "private key");
    refused("cat", &["/etc/ssl/private/server.key"], "private key");
    refused("cat", &["/etc/gshadow"], "group credentials");
}

#[test]
fn readers_still_see_the_everyday_files() {
    allowed("cat", &["/etc/hostname"]);
    allowed("cat", &["/etc/hosts"]);
    // semodule -l is policy-allowed (unit-test ok("semodule", &["-l"])) but
    // as the *invoking user* here it can not open the SELinux store — so it
    // is only asserted at policy level
    allowed("systemctl", &["is-active", "dbus"]);
}

#[test]
fn symlinks_are_refused_everywhere_the_readers_may_look() {
    // a pre-existing symlink inside the allowed /var/log directory (installed
    // by the system itself) must not be readable — the symlink rules apply
    // inside the allowlist as well
    refused("cat", &["/var/log/README"], "a symlink may not be read even in /var/log");
}

#[test]
fn symlink_escape_is_refused_in_the_panel_log_dir() {
    // the panel log directory is allow-listed; a symlink placed there may not
    // point at /etc/shadow (the test creates and removes it in the caller's
    // own config directory)
    let home = std::env::var("HOME").expect("HOME is set for test processes");
    let log_dir = std::path::PathBuf::from(home).join(".config/control-panel/logs");
    // the app creates this directory itself when it starts; the test may
    // create it, too
    std::fs::create_dir_all(&log_dir).expect("panel log directory must be create-able");
    let escape = log_dir.join("refusals-test-escape.log");
    std::os::unix::fs::symlink("/etc/shadow", &escape).expect("symlink creation in own dir");
    let path = escape.to_string_lossy().into_owned();
    let out = run("cat", &[&path])
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"));
    std::fs::remove_file(&escape).expect("cleanup of the test symlink");
    assert_eq!(
        out.code, 2,
        "a symlink inside an allowed directory may not escape it: {out:?}"
    );
    assert!(
        out.stderr.starts_with("lcp-admin:"),
        "refusal must come from the helper itself: {:?}",
        out.stderr
    );
}

// ── step 3A: named-action dispatch exit codes (F-5) and write_profile_sh ────
//
// As the invoking user (no pkexec, no root), a success path of
// install_polkit_rule / write_profile_sh would touch the real host, so those
// tests only prove failure shapes *and exit codes*: exit 2 with the helper's
// own message, never a fall-through to a bogus "takes exactly one account
// name" (the F-5 regression), and exit 2 usage errors for the account
// actions.

#[test]
fn named_action_exit_codes_stay_two_for_invalid_argv() {
    // (F-5) the argv-shaped refusals must keep the helper's own prefix —
    // the old code fell through to an unrelated "takes exactly one account
    // name" *after a successful action*; these are their failure twins
    refused("install_polkit_rule", &["extra"], "no params allowed");
    refused("read_user_ssh_keys", &[], "missing account");
    refused("read_user_ssh_keys", &["a", "b"], "too many params");
    refused("write_profile_sh", &[], "missing path");
    refused("write_profile_sh", &["/etc/profile.d/x.sh", "extra"], "too many params");
    // an account that exists but whose argv shape is wrong still exits 2
    // without touching anything
    refused("write_user_ssh_keys", &[], "missing account");
}

#[test]
fn write_profile_sh_path_policy_is_enforced_by_the_helper_itself() {
    // the action's one destination class: a regular <name>.sh directly
    // inside /etc/profile.d — everything else is refused before stdin is
    // even read
    refused("write_profile_sh", &["/etc/shadow"], "not profile.d, not .sh");
    refused("write_profile_sh", &["/etc/profile.d/../../etc/shadow.sh"], "traversal");
    refused("write_profile_sh", &["/etc/profile.d/sub/x.sh"], "extra directory component");
    refused("write_profile_sh", &["/etc/profile.d/x.txt"], "wrong extension");
    refused("write_profile_sh", &["/etc/profile.d/.hidden.sh"], "dot file");
    refused("write_profile_sh", &["/etc/profile.d/bad*name.sh"], "non-name characters");
    refused("write_profile_sh", &["relative/x.sh"], "relative path");
    refused("write_profile_sh", &["/etc/profile.d/x.sh/../y.sh"], "relative parts");
}

// ── admin mode: the prompt-raising no-op action ─────────────────────────────

#[test]
fn admin_keep_does_nothing_but_exit_zero() {
    // The status bar's admin-mode toggle raises its one polkit prompt with
    // this action. It must be inert: no arguments, no output and no side
    // effects — only the successful exit that lets PolicyKit keep the
    // authorisation for the invoking process.
    let out = Command::new(HELPER)
        .arg("admin_keep")
        .output()
        .expect("admin_keep: unexpected spawn error");
    assert_eq!(out.status.code(), Some(0), "admin_keep must exit 0: {out:?}");
    assert!(out.stdout.is_empty(), "admin_keep must not print: {:?}", out.stdout);
    assert!(out.stderr.is_empty(), "admin_keep must not print: {:?}", out.stderr);

    refused("admin_keep", &["extra"], "the action takes no further arguments");
}
