//! Fixture tests for the NetworkManager named actions and the closed generic
//! `nmcli`/`ip` channels. Stubs under `/tmp/lcp-admin-test-fixture/bin` log
//! every argv; no real interface or connection is touched.

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const IP_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/ip";
const NMCLI_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/nmcli";
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");
const UUID: &str = "8a3b2c1d-4e5f-6a7b-8c9d-0e1f2a3b4c5d";


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
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/bin")).unwrap();
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/log")).unwrap();
    const STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "/tmp/lcp-admin-test-fixture/log/calls.log"
exit 0
"#;
    use std::os::unix::fs::PermissionsExt;
    for path in [IP_BIN, NMCLI_BIN] {
        std::fs::write(path, STUB).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    lock_file
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

fn calls() -> Vec<String> {
    std::fs::read_to_string(LOG)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn interface_state_uses_the_fixed_ip_argv() {
    let _guard = fixture();
    let run = run_helper(&["nmcli_interface_state", "eth0", "down"]);
    assert_eq!(run.code, 0, "a valid interface state must succeed: {run:?}");
    assert_eq!(calls(), vec!["link set dev eth0 down"]);
}

#[test]
fn interface_state_blocks_loopback_before_any_command() {
    let _guard = fixture();
    let run = run_helper(&["nmcli_interface_state", "lo", "down"]);
    assert_eq!(run.code, 2, "loopback down must be refused: {run:?}");
    assert!(run.stderr.contains("loopback"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn named_nmcli_save_and_delete_use_the_exact_helper_argv() {
    let _guard = fixture();
    let save = run_helper(&[
        "nmcli_connection_save",
        "modify",
        "corp",
        "ipv4.method",
        "manual",
    ]);
    assert_eq!(save.code, 0, "named save must succeed: {save:?}");
    assert_eq!(calls(), vec!["connection modify corp ipv4.method manual"]);

    std::fs::remove_file(LOG).ok();
    let delete = run_helper(&["nmcli_connection_delete", UUID]);
    assert_eq!(delete.code, 0, "named delete must succeed: {delete:?}");
    assert_eq!(calls(), vec![format!("connection delete {UUID}")]);
}

#[test]
fn generic_nmcli_writes_are_refused_before_execution() {
    let _guard = fixture();
    for args in [
        vec!["nmcli", "connection", "add", "type", "vpn"],
        vec!["nmcli", "connection", "modify", "corp", "ipv4.method", "manual"],
        vec!["nmcli", "connection", "delete", UUID],
        vec!["nmcli", "connection", "up", UUID],
        vec!["nmcli", "connection", "down", UUID],
        vec!["nmcli", "general", "reload"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "generic nmcli write must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "no write may reach nmcli: {:?}", calls());
}

#[test]
fn generic_ip_writes_are_refused_before_execution() {
    let _guard = fixture();
    for args in [
        vec!["ip", "link", "set", "dev", "eth0", "down"],
        vec!["ip", "addr", "add", "192.0.2.1/24", "dev", "eth0"],
        vec!["ip", "route", "flush"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "generic ip write must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "no write may reach ip: {:?}", calls());
}
