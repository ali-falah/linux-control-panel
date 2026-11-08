//! Fixture tests for the systemd unit actions. With `test-fixture`, systemctl
//! is a logging stub and the unit drop-in tree lives below
//! `/tmp/lcp-admin-test-fixture/systemd`; no host unit is touched.

#![cfg(feature = "test-fixture")]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const SYSTEMD_ROOT: &str = "/tmp/lcp-admin-test-fixture/systemd/system";
const SYSTEMCTL_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/systemctl";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
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
    run_helper_with_stdin(args, "")
}

fn run_helper_with_stdin(args: &[&str], stdin: &str) -> Run {
    use std::io::Write;
    let mut child = Command::new(HELPER)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"));
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
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
    std::fs::create_dir_all(SYSTEMD_ROOT).unwrap();
    const STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(SYSTEMCTL_BIN, STUB).unwrap();
    std::fs::set_permissions(SYSTEMCTL_BIN, std::fs::Permissions::from_mode(0o755)).unwrap();
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
fn unit_actions_reach_systemctl_with_the_exact_verb() {
    let _guard = fixture();
    for (verb, unit) in [
        ("start", "nginx.service"),
        ("stop", "nginx.service"),
        ("restart", "nginx.service"),
        ("reload", "nginx.service"),
        ("enable", "cron-daily.timer"),
        ("disable", "httpd.service"),
        ("mask", "cups.service"),
        ("unmask", "cups.service"),
    ] {
        let run = run_helper(&["unit_action", verb, unit]);
        assert_eq!(run.code, 0, "{verb} {unit} must succeed: {run:?}");
    }
    assert_eq!(
        calls(),
        vec![
            "start nginx.service", "stop nginx.service", "restart nginx.service",
            "reload nginx.service", "enable cron-daily.timer", "disable httpd.service",
            "mask cups.service", "unmask cups.service",
        ]
    );
}

#[test]
fn protected_units_are_refused_by_the_helper() {
    let _guard = fixture();
    for (verb, unit) in [
        ("stop", "systemd-logind.service"),
        ("disable", "dbus-broker.service"),
        ("mask", "systemd-journald.service"),
        ("stop", "firewalld.service"),
        ("disable", "sshd.service"),
        ("mask", "NetworkManager.service"),
    ] {
        let run = run_helper(&["unit_action", verb, unit]);
        assert_ne!(run.code, 0, "{verb} {unit} must be blocked: {run:?}");
        assert!(run.stderr.contains("cannot be"), "{run:?}");
    }
    // essential units may still be stopped/restarted in the approved policy
    let run = run_helper(&["unit_action", "stop", "firewalld.service"]);
    assert_ne!(run.code, 0);
    let run = run_helper(&["unit_action", "restart", "sshd.service"]);
    assert_eq!(run.code, 0, "restart remains allowed: {run:?}");
    assert!(calls().len() <= 1, "only the allowed action ran: {:?}", calls());
}

#[test]
fn invalid_units_and_verbs_are_refused_before_systemctl() {
    let _guard = fixture();
    for args in [
        vec!["unit_action", "start", "nginx"],
        vec!["unit_action", "start", "-nginx.service"],
        vec!["unit_action", "start", "nginx;id.service"],
        vec!["unit_action", "start", "/opt/evil.service"],
        vec!["unit_action", "explode", "nginx.service"],
        vec!["unit_action", "start"],
        vec!["unit_action", "start", "nginx.service", "extra"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid shape must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn dropin_write_creates_the_fixed_override_and_reloads() {
    let _guard = fixture();
    let dir = format!("{SYSTEMD_ROOT}/nginx.service.d");
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/override.conf");
    std::fs::write(&path, "[Service]\nEnvironment=OLD=1\n").unwrap();
    let run = run_helper_with_stdin(
        &["unit_dropin_write", "nginx.service"],
        "[Service]\nEnvironment=FIXTURE=1\n",
    );
    assert_eq!(run.code, 0, "drop-in write must succeed: {run:?}");
    assert_eq!(calls(), vec!["daemon-reload"]);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[Service]\nEnvironment=FIXTURE=1\n"
    );
    let backup = format!("{path}.control-panel.bak");
    assert_eq!(
        std::fs::read_to_string(&backup).unwrap(),
        "[Service]\nEnvironment=OLD=1\n",
        "the previous override must be preserved as a backup"
    );
    assert_eq!(
        std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o600,
        "backups must be owner-only"
    );
}

#[test]
fn dropin_write_refuses_invalid_units_and_empty_critical_overrides() {
    let _guard = fixture();
    for unit in ["nginx", "../../etc/passwd", "-evil.service"] {
        let run = run_helper_with_stdin(&["unit_dropin_write", unit], "[Service]\n");
        assert_eq!(run.code, 2, "unit {unit:?} must be refused: {run:?}");
    }
    let run = run_helper_with_stdin(&["unit_dropin_write", "systemd-logind.service"], "   \n");
    assert_ne!(run.code, 0, "empty critical override must be refused: {run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn dropin_write_refuses_symlinked_directories() {
    let _guard = fixture();
    use std::os::unix::fs::symlink;
    let outside = format!("{FIXTURE_ROOT}/outside");
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, format!("{SYSTEMD_ROOT}/nginx.service.d")).unwrap();
    let run = run_helper_with_stdin(&["unit_dropin_write", "nginx.service"], "[Service]\n");
    assert_ne!(run.code, 0, "symlinked drop-in dir must be refused: {run:?}");
    assert!(run.stderr.contains("symbolic link"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn daemon_reload_failure_rolls_back_and_is_reported() {
    let _guard = fixture();
    let dir = format!("{SYSTEMD_ROOT}/nginx.service.d");
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/override.conf");
    std::fs::write(&path, "[Service]\nEnvironment=OLD=1\n").unwrap();
    set_ctl("fail");
    let run = run_helper_with_stdin(
        &["unit_dropin_write", "nginx.service"],
        "[Service]\nEnvironment=NEW=1\n",
    );
    assert_ne!(run.code, 0, "reload failure must fail the action: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    assert!(run.stderr.contains("previous override was restored on disk"), "{run:?}");
    assert!(run.stderr.contains("systemd was NOT reloaded by the retry"), "{run:?}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[Service]\nEnvironment=OLD=1\n",
        "the previous override must be restored"
    );
}

#[test]
fn daemon_reload_failure_without_previous_override_removes_the_file() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper_with_stdin(&["unit_dropin_write", "nginx.service"], "[Service]\n");
    assert_ne!(run.code, 0, "reload failure must fail the action: {run:?}");
    assert!(
        !std::path::Path::new(&format!("{SYSTEMD_ROOT}/nginx.service.d/override.conf")).exists(),
        "a fresh override must be removed by the rollback"
    );
    assert!(run.stderr.contains("newly written override was removed from disk"), "{run:?}");
    assert!(run.stderr.contains("systemd was NOT reloaded by the retry"), "{run:?}");
}

#[test]
fn symlinked_backup_is_refused_before_overwriting() {
    let _guard = fixture();
    use std::os::unix::fs::symlink;
    let dir = format!("{SYSTEMD_ROOT}/nginx.service.d");
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/override.conf");
    std::fs::write(&path, "[Service]\nEnvironment=OLD=1\n").unwrap();
    let outside = format!("{FIXTURE_ROOT}/outside-backup");
    symlink(&outside, format!("{path}.control-panel.bak")).unwrap();
    let run = run_helper_with_stdin(&["unit_dropin_write", "nginx.service"], "[Service]\n");
    assert_ne!(run.code, 0, "a symlinked backup must be refused: {run:?}");
    assert!(run.stderr.contains("symlinked override backup"), "{run:?}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[Service]\nEnvironment=OLD=1\n",
        "the original override must be untouched"
    );
    assert!(!std::path::Path::new(&outside).exists());
}

#[test]
fn missing_systemctl_is_refused() {
    let _guard = fixture();
    std::fs::remove_file(SYSTEMCTL_BIN).ok();
    let run = run_helper(&["unit_action", "restart", "nginx.service"]);
    assert_ne!(run.code, 0, "missing systemctl must fail: {run:?}");
    assert!(run.stderr.contains("systemctl is not available"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn daemon_reload_recovery_succeeds_without_rewriting() {
    let _guard = fixture();
    set_ctl("fail");
    let failed = run_helper(&["unit_daemon_reload"]);
    assert_ne!(failed.code, 0, "missing reload must fail: {failed:?}");
    std::fs::remove_file(format!("{FIXTURE_ROOT}/ctl/fail")).ok();
    let run = run_helper(&["unit_daemon_reload"]);
    assert_eq!(run.code, 0, "recovery reload must succeed: {run:?}");
    assert_eq!(calls(), vec!["daemon-reload", "daemon-reload"]);
}

#[test]
fn daemon_reload_recovery_refuses_bad_shapes() {
    let _guard = fixture();
    let run = run_helper(&["unit_daemon_reload", "nginx.service"]);
    assert_eq!(run.code, 2, "recovery takes no arguments: {run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
    std::fs::remove_file(SYSTEMCTL_BIN).ok();
    let run = run_helper(&["unit_daemon_reload"]);
    assert_ne!(run.code, 0, "missing systemctl must fail: {run:?}");
    assert!(run.stderr.contains("systemctl is not available"), "{run:?}");
}
