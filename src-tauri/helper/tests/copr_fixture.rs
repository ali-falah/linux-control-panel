//! Fixture tests for the `copr_manage` action. With `test-fixture`, dnf and
//! add-apt-repository are logging stubs; no repository is added or removed.

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const DNF_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/dnf";
const APT_REPO_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/add-apt-repository";
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());
use std::sync::{Mutex, MutexGuard};

#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_helper(args: &[&str]) -> Run {
    let output = Command::new(HELPER)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"));
    Run {
        code: output.status.code().unwrap_or(101),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn fixture() -> MutexGuard<'static, ()> {
    let guard = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = std::fs::remove_dir_all(FIXTURE_ROOT);
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/bin")).unwrap();
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/log")).unwrap();
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/ctl")).unwrap();
    const STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    use std::os::unix::fs::PermissionsExt;
    for path in [DNF_BIN, APT_REPO_BIN] {
        std::fs::write(path, STUB).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    guard
}

fn calls() -> Vec<String> {
    std::fs::read_to_string(LOG)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn set_ctl(name: &str) {
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/{name}"), "1").unwrap();
}

#[test]
fn dnf_enable_and_disable_use_exact_argv() {
    let _guard = fixture();
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_eq!(run.code, 0, "enable must succeed: {run:?}");
    assert!(run.stdout.is_empty(), "the stub prints nothing: {run:?}");
    assert_eq!(calls(), vec!["copr enable owner/project -y"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["copr_manage", "disable", "owner/project"]);
    assert_eq!(run.code, 0, "disable must succeed: {run:?}");
    assert_eq!(calls(), vec!["copr disable owner/project -y"]);
}

#[test]
fn dnf_failure_is_not_retried_on_apt() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_ne!(run.code, 0, "dnf failure must fail the action: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    assert_eq!(calls(), vec!["copr enable owner/project -y"]);
}

#[test]
fn apt_enable_and_disable_normalize_the_ppa_prefix() {
    let _guard = fixture();
    std::fs::remove_file(DNF_BIN).ok();
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_eq!(run.code, 0, "apt enable must succeed: {run:?}");
    assert_eq!(calls(), vec!["-y ppa:owner/project"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["copr_manage", "disable", "ppa:owner/project"]);
    assert_eq!(run.code, 0, "apt disable must succeed: {run:?}");
    assert_eq!(calls(), vec!["--remove -y ppa:owner/project"]);
}

#[test]
fn ppa_repo_with_dnf_is_refused() {
    let _guard = fixture();
    let run = run_helper(&["copr_manage", "enable", "ppa:owner/project"]);
    assert_ne!(run.code, 0, "dnf cannot manage PPA repos: {run:?}");
    assert!(run.stderr.contains("add-apt-repository"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn invalid_repos_and_verbs_are_refused_before_any_command() {
    let _guard = fixture();
    for args in [
        vec!["copr_manage", "enable", "owner"],
        vec!["copr_manage", "enable", "owner/project/extra"],
        vec!["copr_manage", "enable", "-owner/project"],
        vec!["copr_manage", "enable", "owner/-project"],
        vec!["copr_manage", "enable", "owner/."],
        vec!["copr_manage", "enable", "owner/.."],
        vec!["copr_manage", "enable", "../etc/passwd"],
        vec!["copr_manage", "enable", "own er/project"],
        vec!["copr_manage", "install", "owner/project"],
        vec!["copr_manage", "enable"],
        vec!["copr_manage", "enable", "owner/project", "extra"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid shape must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn no_package_manager_is_refused() {
    let _guard = fixture();
    std::fs::remove_file(DNF_BIN).ok();
    std::fs::remove_file(APT_REPO_BIN).ok();
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_ne!(run.code, 0, "no backend must be refused: {run:?}");
    assert!(run.stderr.contains("neither dnf nor add-apt-repository"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn a_world_writable_backend_is_refused_before_execution() {
    let _guard = fixture();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(DNF_BIN, std::fs::Permissions::from_mode(0o777)).unwrap();
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_ne!(run.code, 0, "an untrusted backend must fail closed: {run:?}");
    assert!(
        run.stderr.contains("writable by other users")
            || run.stderr.contains("privileged identity"),
        "the refusal must name the trust failure: {run:?}"
    );
    assert!(calls().is_empty(), "the untrusted backend must not run: {:?}", calls());
}

#[test]
fn a_symlink_to_an_untrusted_backend_is_refused() {
    let _guard = fixture();
    use std::os::unix::fs::PermissionsExt;
    let target = format!("{FIXTURE_ROOT}/bin/untrusted-dnf");
    std::fs::write(&target, "#!/bin/sh\necho untrusted\n").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::remove_file(DNF_BIN).unwrap();
    std::os::unix::fs::symlink(&target, DNF_BIN).unwrap();
    let run = run_helper(&["copr_manage", "enable", "owner/project"]);
    assert_ne!(run.code, 0, "a symlink to an untrusted backend must fail closed: {run:?}");
    assert!(
        run.stderr.contains("writable by other users")
            || run.stderr.contains("privileged identity"),
        "the refusal must name the trust failure: {run:?}"
    );
    assert!(calls().is_empty(), "the untrusted backend must not run: {:?}", calls());
}
