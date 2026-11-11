//! Fixture tests for the System Info named actions. Stubs under
//! `/tmp/lcp-admin-test-fixture/bin` log argv; the fixture device is a regular
//! file under `/tmp/lcp-admin-test-fixture/dev`, so no real disk or process is
//! touched.

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const RENICE_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/renice";
const SMARTCTL_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/smartctl";
const DEVICE_DIR: &str = "/tmp/lcp-admin-test-fixture/dev";
const DEVICE: &str = "/tmp/lcp-admin-test-fixture/dev/sda";
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");


#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn fixture() -> std::fs::File {
    // The fixture directory is shared by every test binary, and each binary is
    // its own process — an in-process Mutex cannot serialise them. The flock is
    // released by the kernel even if this test panics or the process dies.
    let lock_file = std::fs::File::create(format!("{FIXTURE_ROOT}.lock"))
        .expect("fixture lock file could not be created");
    rustix::fs::flock(&lock_file, rustix::fs::FlockOperation::LockExclusive)
        .expect("the fixture lock could not be taken");
    let _ = std::fs::remove_dir_all(FIXTURE_ROOT);
    for directory in [
        format!("{FIXTURE_ROOT}/bin"),
        format!("{FIXTURE_ROOT}/log"),
        DEVICE_DIR.to_string(),
    ] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const RENICE_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "/tmp/lcp-admin-test-fixture/log/calls.log"
exit 0
"#;
    const SMARTCTL_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "/tmp/lcp-admin-test-fixture/log/calls.log"
echo 'Device Model: Test Disk'
echo 'SMART overall-health self-assessment test result: PASSED'
exit 0
"#;
    use std::os::unix::fs::PermissionsExt;
    for (path, body) in [(RENICE_BIN, RENICE_STUB), (SMARTCTL_BIN, SMARTCTL_STUB)] {
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(DEVICE, b"fixture block device\n").unwrap();
    lock_file
}

fn run_helper(args: &[&str]) -> Run {
    run_helper_with_uid(args, None)
}

fn run_helper_with_uid(args: &[&str], uid: Option<u32>) -> Run {
    let mut command = Command::new(HELPER);
    command.args(args);
    if let Some(uid) = uid {
        command.env("PKEXEC_UID", uid.to_string());
    } else {
        command.env_remove("PKEXEC_UID").env_remove("SUDO_UID");
    }
    let output = command
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

fn calls() -> Vec<String> {
    std::fs::read_to_string(LOG)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn process_renice_uses_the_exact_argv() {
    let _guard = fixture();
    let pid = std::process::id();
    let uid = rustix::process::getuid().as_raw();
    assert!(pid > 100, "the test process pid must be outside the protected range");
    let run = run_helper_with_uid(&["process_renice", &pid.to_string(), "-5"], Some(uid));
    assert_eq!(run.code, 0, "valid negative renice must succeed: {run:?}");
    assert_eq!(calls(), vec![format!("-n -5 -p {pid}")]);
}

#[test]
fn process_renice_refuses_protected_pids_and_non_negative_priorities() {
    let _guard = fixture();
    for args in [
        vec!["process_renice", "50", "-5"],
        vec!["process_renice", "1", "-5"],
        vec!["process_renice", &std::process::id().to_string(), "5"],
        vec!["process_renice", &std::process::id().to_string(), "0"],
        vec!["process_renice", &std::process::id().to_string(), "-21"],
        vec!["process_renice", "not-a-pid", "-5"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid renice must be refused: {args:?} — {run:?}");
        assert!(run.stderr.starts_with("lcp-admin:"), "{run:?}");
    }
    assert!(calls().is_empty(), "no refused renice may run: {:?}", calls());
}

#[test]
fn smart_health_returns_stdout_and_uses_the_exact_argv() {
    let _guard = fixture();
    let run = run_helper(&["smart_health", DEVICE]);
    assert_eq!(run.code, 0, "valid smart health must succeed: {run:?}");
    assert!(run.stdout.contains("Device Model: Test Disk"), "{run:?}");
    assert!(run.stdout.contains("PASSED"), "{run:?}");
    assert_eq!(calls(), vec![format!("-H -i {DEVICE}")]);
}

#[test]
fn smart_health_refuses_unsafe_paths_and_symlinks() {
    let _guard = fixture();
    for device in ["/etc/passwd", "/dev/sda", "/tmp/lcp-admin-test-fixture/dev/../etc/passwd"] {
        let run = run_helper(&["smart_health", device]);
        assert_eq!(run.code, 2, "unsafe device must be refused: {device} — {run:?}");
        assert!(run.stderr.starts_with("lcp-admin:"), "{run:?}");
    }

    let link = format!("{DEVICE_DIR}/link");
    std::os::unix::fs::symlink(DEVICE, &link).unwrap();
    let run = run_helper(&["smart_health", &link]);
    assert_eq!(run.code, 2, "a symlink device must be refused: {run:?}");
    assert!(run.stderr.starts_with("lcp-admin:"), "{run:?}");
    assert!(calls().is_empty(), "no refused SMART action may run: {:?}", calls());
}

#[test]
fn process_renice_fails_closed_without_an_invoking_uid_or_a_live_process() {
    let _guard = fixture();
    let pid = std::process::id();
    let run = run_helper(&["process_renice", &pid.to_string(), "-5"]);
    assert_eq!(run.code, 2, "missing invoking UID must fail closed: {run:?}");
    assert!(run.stderr.contains("invoking user could not be determined"), "{run:?}");

    let uid = rustix::process::getuid().as_raw();
    let missing = run_helper_with_uid(&["process_renice", "999999", "-5"], Some(uid));
    assert_eq!(missing.code, 2, "missing process must fail closed: {missing:?}");
    assert!(missing.stderr.contains("no longer exists"), "{missing:?}");
    assert!(calls().is_empty(), "no refused renice may run: {:?}", calls());
}

#[test]
fn smart_data_uses_the_exact_argv_and_returns_stdout() {
    let _guard = fixture();
    let run = run_helper(&["smart_data", DEVICE]);
    assert_eq!(run.code, 0, "valid smart data must succeed: {run:?}");
    assert!(run.stdout.contains("Device Model: Test Disk"), "{run:?}");
    assert_eq!(calls(), vec![format!("-a -j {DEVICE}")]);
}

#[test]
fn smart_self_test_uses_the_exact_argv_and_refuses_bad_types() {
    let _guard = fixture();
    let run = run_helper(&["smart_self_test", DEVICE, "long"]);
    assert_eq!(run.code, 0, "valid self-test must succeed: {run:?}");
    assert_eq!(calls(), vec![format!("-t long {DEVICE}")]);

    std::fs::remove_file(LOG).ok();
    for args in [
        vec!["smart_self_test", DEVICE, "extended"],
        vec!["smart_self_test", "/etc/passwd", "long"],
        vec!["smart_self_test", DEVICE],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid self-test must be refused: {args:?} — {run:?}");
        assert!(run.stderr.starts_with("lcp-admin:"), "{run:?}");
    }
    assert!(calls().is_empty(), "no refused self-test may run: {:?}", calls());
}
