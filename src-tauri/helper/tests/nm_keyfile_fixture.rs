//! Fixture tests for `nm_store_keyfile_secret`. The keyfile lives under
//! `/tmp/lcp-admin-test-fixture/etc/NetworkManager/system-connections` and the
//! `nmcli` stub only logs argv, so no real profile or reload is touched.

#![cfg(feature = "test-fixture")]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const BIN_DIR: &str = "/tmp/lcp-admin-test-fixture/bin";
const KEYFILE_DIR: &str = "/tmp/lcp-admin-test-fixture/etc/NetworkManager/system-connections";
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");

const KEYFILE: &str = "[connection]\nid=Work VPN\nuuid=1111\ntype=vpn\n\n\
[vpn]\nservice-type=org.freedesktop.NetworkManager.openvpn\nremote=vpn.example.com\n\n\
[ipv4]\nmethod=auto\n";

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn fixture() -> std::fs::File {
    let lock_file = std::fs::File::create(format!("{FIXTURE_ROOT}.lock"))
        .expect("fixture lock file could not be created");
    rustix::fs::flock(&lock_file, rustix::fs::FlockOperation::LockExclusive)
        .expect("the fixture lock could not be taken");
    let _ = std::fs::remove_dir_all(FIXTURE_ROOT);
    for directory in [BIN_DIR, KEYFILE_DIR, &format!("{FIXTURE_ROOT}/log")] {
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

fn write_keyfile(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(KEYFILE_DIR).join(name);
    std::fs::write(&path, content).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn run_helper(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(HELPER)
        .args(args)
        .env_remove("PKEXEC_UID")
        .env_remove("SUDO_UID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the helper could not be started");
    if let Some(mut pipe) = child.stdin.take() {
        let _ = pipe.write_all(stdin.as_bytes());
    }
    let output = child.wait_with_output().expect("the helper could not finish");
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
fn vpn_secret_is_written_to_the_keyfile_and_reloaded_without_argv_exposure() {
    let _guard = fixture();
    let path = write_keyfile("work.nmconnection", KEYFILE);

    let run = run_helper(
        &["nm_store_keyfile_secret", "Work VPN", "vpn-password"],
        "s3cret\n",
    );
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("password-flags=0"), "{updated}");
    assert!(updated.contains("[vpn-secrets]\npassword=s3cret"), "{updated}");
    assert!(updated.contains("remote=vpn.example.com"), "{updated}");
    assert!(updated.contains("[ipv4]\nmethod=auto"), "{updated}");

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the keyfile must stay root-only");

    // The secret travels on stdin; the stub only ever sees the reload argv.
    let calls = calls();
    assert_eq!(calls, vec!["connection reload".to_string()]);
    assert!(
        !calls.join("\n").contains("s3cret"),
        "the secret must never reach argv: {calls:?}"
    );
}

#[test]
fn wifi_psk_updates_only_the_wifi_security_section() {
    let _guard = fixture();
    let wifi = "[connection]\nid=Home Wi-Fi\n\n\
[wifi-security]\nkey-mgmt=wpa-psk\n\n\
[ipv4]\nmethod=auto\n";
    let path = write_keyfile("home.nmconnection", wifi);

    let run = run_helper(
        &["nm_store_keyfile_secret", "Home Wi-Fi", "wifi-psk"],
        "pa\\ss\n",
    );
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("psk-flags=0"), "{updated}");
    // A backslash in the passphrase is escaped once for the keyfile format.
    assert!(updated.contains("psk=pa\\\\ss"), "{updated}");
    assert!(updated.contains("key-mgmt=wpa-psk"), "{updated}");
}

#[test]
fn unknown_connection_and_kind_are_refused_without_writing() {
    let _guard = fixture();
    let path = write_keyfile("work.nmconnection", KEYFILE);

    let missing = run_helper(
        &["nm_store_keyfile_secret", "Other VPN", "vpn-password"],
        "s3cret",
    );
    assert_ne!(missing.code, 0);
    assert!(missing.stderr.contains("no NetworkManager keyfile"), "{}", missing.stderr);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), KEYFILE);

    let bad_kind = run_helper(
        &["nm_store_keyfile_secret", "Work VPN", "wifi-psk"],
        "s3cret",
    );
    assert_ne!(bad_kind.code, 0);
    assert!(bad_kind.stderr.contains("no [wifi-security] section"), "{}", bad_kind.stderr);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), KEYFILE);
    assert!(calls().is_empty(), "a refused request must not reload nmcli");
}

#[test]
fn a_symlinked_keyfile_is_not_followed() {
    let _guard = fixture();
    let outside = format!("{FIXTURE_ROOT}/outside.nmconnection");
    std::fs::write(&outside, KEYFILE).unwrap();
    let link = std::path::Path::new(KEYFILE_DIR).join("link.nmconnection");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    let run = run_helper(
        &["nm_store_keyfile_secret", "Work VPN", "vpn-password"],
        "s3cret",
    );
    assert_ne!(run.code, 0);
    assert!(run.stderr.contains("no NetworkManager keyfile"), "{}", run.stderr);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), KEYFILE);
}

#[test]
fn multiline_secrets_are_refused() {
    let _guard = fixture();
    let path = write_keyfile("work.nmconnection", KEYFILE);

    let run = run_helper(
        &["nm_store_keyfile_secret", "Work VPN", "vpn-password"],
        "first\nsecond\n",
    );
    assert_ne!(run.code, 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), KEYFILE);
}
