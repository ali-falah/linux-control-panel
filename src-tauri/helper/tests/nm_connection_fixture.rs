//! Fixture tests for the `nmcli` argv that `nmcli_connection_save` builds.
//! The stub only records argv; no connection is created.

#![cfg(feature = "test-fixture")]

use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const BIN_DIR: &str = "/tmp/lcp-admin-test-fixture/bin";
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");

struct Run {
    code: i32,
    stderr: String,
}

fn fixture() -> std::fs::File {
    let lock_file = std::fs::File::create(format!("{FIXTURE_ROOT}.lock"))
        .expect("fixture lock file could not be created");
    rustix::fs::flock(&lock_file, rustix::fs::FlockOperation::LockExclusive)
        .expect("the fixture lock could not be taken");
    let _ = std::fs::remove_dir_all(FIXTURE_ROOT);
    for directory in [BIN_DIR, &format!("{FIXTURE_ROOT}/log")] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const NMCLI_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "/tmp/lcp-admin-test-fixture/log/calls.log"
exit 0
"#;
    let nmcli = format!("{BIN_DIR}/nmcli");
    std::fs::write(&nmcli, NMCLI_STUB).unwrap();
    std::fs::set_permissions(&nmcli, std::fs::Permissions::from_mode(0o755)).unwrap();
    lock_file
}

fn run_helper(args: &[&str]) -> Run {
    let output = Command::new(HELPER)
        .args(args)
        .env_remove("PKEXEC_UID")
        .env_remove("SUDO_UID")
        .stdin(Stdio::null())
        .output()
        .expect("the helper could not be started");
    Run {
        code: output.status.code().unwrap_or(101),
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
fn add_translates_connection_properties_to_nmcli_options() {
    let _guard = fixture();
    let run = run_helper(&[
        "nmcli_connection_save",
        "add",
        "Office LAN",
        "connection.type",
        "802-3-ethernet",
        "connection.id",
        "Office LAN",
        "connection.interface-name",
        "eth0",
        "connection.autoconnect",
        "yes",
        "ipv4.method",
        "manual",
    ]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert_eq!(
        calls(),
        vec![
            "connection add type 802-3-ethernet con-name Office LAN ifname eth0 \
             connection.autoconnect yes ipv4.method manual"
                .to_string()
        ]
    );
}

#[test]
fn add_maps_bridge_master_and_slave_type_options() {
    let _guard = fixture();
    let run = run_helper(&[
        "nmcli_connection_save",
        "add",
        "br0 port",
        "connection.type",
        "802-3-ethernet",
        "connection.id",
        "br0 port",
        "connection.interface-name",
        "eth1",
        "connection.master",
        "br0",
        "connection.slave-type",
        "bridge",
        "ipv4.method",
        "disabled",
    ]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert_eq!(
        calls(),
        vec![
            "connection add type 802-3-ethernet con-name br0 port ifname eth1 \
             master br0 slave-type bridge ipv4.method disabled"
                .to_string()
        ]
    );
}

#[test]
fn modify_keeps_the_connection_argument_and_translates_the_clear_sentinel() {
    let _guard = fixture();
    let run = run_helper(&[
        "nmcli_connection_save",
        "modify",
        "uuid-1",
        "connection.autoconnect",
        "no",
        "ipv4.addresses",
        "__LCP_CLEAR__",
        "ipv4.method",
        "auto",
    ]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    // An empty string is nmcli's clear instruction and stays an empty argv
    // element; the sentinel itself must never reach nmcli.
    assert_eq!(
        calls(),
        vec!["connection modify uuid-1 connection.autoconnect no ipv4.addresses  ipv4.method auto".to_string()]
    );
}
