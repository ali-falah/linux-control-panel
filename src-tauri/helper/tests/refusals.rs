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
    allowed("ls", &["/tmp"]);
    allowed("ip", &["link", "show"]);
    allowed("date", &[]);
}

// ── named actions: `lcp-admin <action>` ──────────────────────────────────────

/// Run the helper action with content on stdin; never touches rules.d as the
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
fn install_polkit_rule_rejects_wrong_content_and_extra_arguments() {
    // mismatching content — the one thing the action must never write
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

#[test]
fn unknown_actions_and_program_shaped_names_are_refused() {
    refused("some_other_action", &[], "no such action, no such program");
    refused("install", &["-m", "0644", "src", "dst"], "wrong install template (see root_policy tests)");
    refused("lcp-admin", &["install_polkit_rule"], "the helper may not call itself");
}
