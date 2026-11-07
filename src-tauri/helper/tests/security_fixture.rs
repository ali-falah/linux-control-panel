//! Fixture tests for the Security Auditor service/file actions. With
//! `test-fixture`, systemd tools, sshd, sysctl, authselect and passwd are
//! logging stubs and all managed files live under
//! `/tmp/lcp-admin-test-fixture/etc`; no host unit, account or config file is
//! touched.

#![cfg(feature = "test-fixture")]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const ETC: &str = "/tmp/lcp-admin-test-fixture/etc";
const BIN: &str = "/tmp/lcp-admin-test-fixture/bin";
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
    for dir in [
        format!("{FIXTURE_ROOT}/bin"),
        format!("{FIXTURE_ROOT}/log"),
        format!("{FIXTURE_ROOT}/ctl"),
        format!("{FIXTURE_ROOT}/tmp"),
        format!("{ETC}/ssh/sshd_config.d"),
        format!("{ETC}/sysctl.d"),
        format!("{ETC}/security/faillock.conf.d"),
        format!("{ETC}/modprobe.d"),
        format!("{ETC}/tmpfiles.d"),
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(
        format!("{ETC}/ssh/sshd_config"),
        "# test config\nPermitRootLogin yes\nPasswordAuthentication yes\n",
    )
    .unwrap();
    std::fs::write(
        format!("{ETC}/login.defs"),
        "# login defaults\nPASS_MAX_DAYS 99999\nPASS_MIN_LEN 5\nUMASK 022\n",
    )
    .unwrap();
    std::fs::write(format!("{ETC}/passwd"), "root:x:0:0::/root:/bin/sh\n").unwrap();
    std::fs::write(
        format!("{ETC}/shadow"),
        "root:$6$hash:19000:0:99999:7:::\nalice::19000:0:99999:7:::\n",
    )
    .unwrap();

    const TOOL_STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
if [ "$1" = "-T" ]; then
  printf 'permitrootlogin prohibit-password\nmaxauthtries 4\npasswordauthentication no\nlogingracetime 60\n'
  exit 0
fi
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    for tool in ["systemctl", "sshd", "sysctl", "authselect", "passwd"] {
        let path = format!("{BIN}/{tool}");
        std::fs::write(&path, TOOL_STUB).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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

fn reset_calls() {
    let _ = std::fs::remove_file(LOG);
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ETC}/{rel}")).unwrap_or_default()
}

fn set_ctl(name: &str) {
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/{name}"), "1").unwrap();
}

#[test]
fn ssh_param_is_validated_edited_and_reloaded() {
    let _guard = fixture();
    let run = run_helper(&["security_ssh_param", "PermitRootLogin", "no"]);
    // "no" is not one of the two allowed values for PermitRootLogin; use the
    // valid revert value in the successful case below.
    assert_eq!(run.code, 2, "invalid value must be refused: {run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());

    let cfg_path = format!("{ETC}/ssh/sshd_config");
    std::fs::set_permissions(&cfg_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let run = run_helper(&["security_ssh_param", "PermitRootLogin", "prohibit-password"]);
    assert_eq!(run.code, 0, "valid SSH parameter must succeed: {run:?}");
    assert_eq!(
        std::fs::metadata(&cfg_path).unwrap().permissions().mode() & 0o777,
        0o600,
        "the original file mode must be preserved"
    );
    let cfg = read("ssh/sshd_config");
    assert!(cfg.contains("PermitRootLogin prohibit-password"), "{cfg}");
    assert_eq!(
        calls(),
        vec![
            "-t -f /tmp/lcp-admin-test-fixture/etc/ssh/sshd_config".to_string(),
            "-T -f /tmp/lcp-admin-test-fixture/etc/ssh/sshd_config".to_string(),
            "is-active --quiet sshd".to_string(),
            "reload sshd".to_string(),
        ]
    );
}

#[test]
fn ssh_param_rejects_unknown_pairs_and_restores_on_invalid_config() {
    let _guard = fixture();
    for args in [
        vec!["security_ssh_param", "MaxAuthTries", "9"],
        vec!["security_ssh_param", "Banner", "none"],
        vec!["security_ssh_param", "PermitRootLogin"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid pair must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());

    set_ctl("fail");
    let before = read("ssh/sshd_config");
    let cfg_path = format!("{ETC}/ssh/sshd_config");
    let run = run_helper(&["security_ssh_param", "MaxAuthTries", "4"]);
    assert_ne!(run.code, 0, "sshd -t failure must fail: {run:?}");
    assert_eq!(read("ssh/sshd_config"), before, "the old config must be restored");
    assert_eq!(
        std::fs::read_to_string(format!("{cfg_path}.control-panel.bak")).unwrap(),
        before,
        "the pre-edit backup must survive the rollback"
    );
}

#[test]
fn managed_paths_cannot_be_redirected_through_symlinks() {
    let _guard = fixture();
    use std::os::unix::fs::symlink;
    let cfg = format!("{ETC}/ssh/sshd_config");
    let outside = format!("{FIXTURE_ROOT}/outside-sshd_config");
    std::fs::write(&outside, "PermitRootLogin yes\n").unwrap();
    std::fs::remove_file(&cfg).unwrap();
    symlink(&outside, &cfg).unwrap();
    let run = run_helper(&["security_ssh_param", "PermitRootLogin", "prohibit-password"]);
    assert_ne!(run.code, 0, "a symlinked managed file must be refused: {run:?}");
    assert!(run.stderr.contains("symbolic link"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "PermitRootLogin yes\n");
}

#[test]
fn kernel_param_persists_and_applies_the_closed_key() {
    let _guard = fixture();
    let run = run_helper(&["security_kernel_param", "net.ipv4.tcp_syncookies", "1"]);
    assert_eq!(run.code, 0, "valid sysctl must succeed: {run:?}");
    assert!(read("sysctl.d/99-hardening.conf").contains("net.ipv4.tcp_syncookies = 1"));
    assert_eq!(
        calls(),
        vec![
            "-w net.ipv4.tcp_syncookies=1".to_string(),
            "-p /tmp/lcp-admin-test-fixture/etc/sysctl.d/99-hardening.conf".to_string(),
        ]
    );
    for args in [
        vec!["security_kernel_param", "kernel.panic", "1"],
        vec!["security_kernel_param", "net.ipv4.ip_forward", "yes"],
        vec!["security_kernel_param", "net.ipv4.ip_forward"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid sysctl must be refused: {args:?} — {run:?}");
    }
}

#[test]
fn security_apply_handles_password_policy_and_umask() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "password_policy"]).code, 0);
    let defs = read("login.defs");
    assert!(defs.contains("PASS_MAX_DAYS 90"), "{defs}");
    assert!(defs.contains("PASS_MIN_LEN 12"), "{defs}");
    assert_eq!(run_helper(&["security_apply", "umask_on"]).code, 0);
    assert!(read("login.defs").contains("UMASK 027"));
    assert_eq!(run_helper(&["security_apply", "umask_off"]).code, 0);
    assert!(read("login.defs").contains("UMASK 022"));
}

#[test]
fn security_apply_tmp_sticky_writes_and_removes_persistence() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "tmp_sticky_on"]).code, 0);
    assert!(std::path::Path::new(&format!("{ETC}/tmpfiles.d/tmp-sticky.conf")).is_file());
    let mode = std::fs::metadata(format!("{FIXTURE_ROOT}/tmp")).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o1777, "sticky mode must be applied to the fixture tmp dir");
    assert_eq!(run_helper(&["security_apply", "tmp_sticky_off"]).code, 0);
    assert!(!std::path::Path::new(&format!("{ETC}/tmpfiles.d/tmp-sticky.conf")).exists());
}

#[test]
fn security_apply_fixes_password_database_permissions() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "passwd_perms"]).code, 0);
    let mode = std::fs::metadata(format!("{ETC}/passwd")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
    assert_eq!(run_helper(&["security_apply", "shadow_perms"]).code, 0);
    let mode = std::fs::metadata(format!("{ETC}/shadow")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o000);
}

#[test]
fn security_lock_account_targets_only_the_requested_account() {
    let _guard = fixture();
    let run = run_helper(&["security_lock_account", "alice"]);
    assert_eq!(run.code, 0, "locking must succeed: {run:?}");
    assert_eq!(calls(), vec!["-l -- alice"]);
    reset_calls();
    let run = run_helper(&["security_lock_account"]);
    assert_eq!(run.code, 0, "empty-password scan must succeed: {run:?}");
    assert_eq!(calls(), vec!["-l -- alice"]);
    let run = run_helper(&["security_lock_account", "Alice"]);
    assert_eq!(run.code, 2, "invalid account must be refused: {run:?}");
}

#[test]
fn security_lock_account_never_locks_protected_accounts() {
    let _guard = fixture();
    let run = run_helper(&["security_lock_account", "root"]);
    assert_ne!(run.code, 0, "explicit root lock must be refused: {run:?}");
    assert!(run.stderr.contains("vital system or administrative"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());

    std::fs::write(
        format!("{ETC}/shadow"),
        "root::19000:0:99999:7:::\nalice::19000:0:99999:7:::\n",
    )
    .unwrap();
    let run = run_helper(&["security_lock_account"]);
    assert_eq!(run.code, 0, "auto lock must succeed for the ordinary account: {run:?}");
    assert_eq!(calls(), vec!["-l -- alice"], "root must never be auto-locked");
}

#[test]
fn security_apply_faillock_writes_and_removes_the_policy() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "pam_faillock_on"]).code, 0);
    let policy = read("security/faillock.conf.d/50-hardening.conf");
    assert!(policy.contains("deny = 5"), "{policy}");
    assert!(calls().iter().any(|c| c == "enable-feature with-faillock"));
    reset_calls();
    assert_eq!(run_helper(&["security_apply", "pam_faillock_off"]).code, 0);
    assert!(!std::path::Path::new(&format!("{ETC}/security/faillock.conf.d/50-hardening.conf")).exists());
}

#[test]
fn security_apply_ssh_idle_and_banner_files() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "ssh_idle_on"]).code, 0);
    assert!(read("ssh/sshd_config.d/50-idle-timeout.conf").contains("ClientAliveInterval 300"));
    assert_eq!(run_helper(&["security_apply", "ssh_idle_off"]).code, 0);
    assert!(!std::path::Path::new(&format!("{ETC}/ssh/sshd_config.d/50-idle-timeout.conf")).exists());
    assert_eq!(run_helper(&["security_apply", "legal_banner_on"]).code, 0);
    assert!(read("issue").contains("Authorized uses only"));
    assert!(read("ssh/sshd_config.d/50-banner.conf").contains("Banner /etc/issue.net"));
    assert_eq!(run_helper(&["security_apply", "legal_banner_off"]).code, 0);
    assert!(!std::path::Path::new(&format!("{ETC}/ssh/sshd_config.d/50-banner.conf")).exists());
}

#[test]
fn security_apply_blacklist_and_unknown_operations() {
    let _guard = fixture();
    assert_eq!(run_helper(&["security_apply", "blacklist_fs_on"]).code, 0);
    assert!(read("modprobe.d/disable-unused-fs.conf").contains("install cramfs /bin/true"));
    assert_eq!(run_helper(&["security_apply", "blacklist_fs_off"]).code, 0);
    assert!(!std::path::Path::new(&format!("{ETC}/modprobe.d/disable-unused-fs.conf")).exists());
    for op in ["", "reboot", "password_policy_extra"] {
        let run = run_helper(&["security_apply", op]);
        assert_eq!(run.code, 2, "unknown op {op:?} must be refused: {run:?}");
    }
}

#[test]
fn security_operations_fail_closed_on_tool_failure() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper(&["security_kernel_param", "net.ipv4.tcp_syncookies", "1"]);
    assert_ne!(run.code, 0, "sysctl failure must propagate: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    assert!(
        !std::path::Path::new(&format!("{ETC}/sysctl.d/99-hardening.conf")).exists(),
        "a rejected runtime value must not be persisted"
    );
    let run = run_helper(&["security_ssh_param", "PermitRootLogin", "yes"]);
    assert_ne!(run.code, 0, "sshd failure must propagate: {run:?}");
    let run = run_helper(&["security_apply", "ssh_idle_on"]);
    assert_eq!(run.code, 0, "non-required reload failure is tolerated: {run:?}");
}

// ── Existing service-action tests (preserved from the first migration) ──────

#[test]
fn auditd_and_usbguard_toggle_exact_argv() {
    let _guard = fixture();
    let run = run_helper(&["security_service_toggle", "auditd", "enable"]);
    assert_eq!(run.code, 0, "auditd enable must succeed: {run:?}");
    assert_eq!(calls(), vec!["enable --now auditd"]);
    reset_calls();
    let run = run_helper(&["security_service_toggle", "usbguard", "disable"]);
    assert_eq!(run.code, 0, "usbguard disable must succeed: {run:?}");
    assert_eq!(calls(), vec!["disable --now usbguard"]);
}

#[test]
fn time_daemons_are_the_only_extra_units() {
    let _guard = fixture();
    for unit in ["chronyd", "systemd-timesyncd"] {
        let run = run_helper(&["security_service_toggle", unit, "enable"]);
        assert_eq!(run.code, 0, "{unit} must be accepted: {run:?}");
    }
    assert_eq!(
        calls(),
        vec!["enable --now chronyd", "enable --now systemd-timesyncd"]
    );
}

#[test]
fn ctrl_alt_del_masks_only_the_fixed_target() {
    let _guard = fixture();
    let run = run_helper(&["security_ctrl_alt_del", "mask"]);
    assert_eq!(run.code, 0, "mask must succeed: {run:?}");
    assert_eq!(calls(), vec!["mask ctrl-alt-del.target"]);
    reset_calls();
    let run = run_helper(&["security_ctrl_alt_del", "unmask"]);
    assert_eq!(run.code, 0, "unmask must succeed: {run:?}");
    assert_eq!(calls(), vec!["unmask ctrl-alt-del.target"]);
}

#[test]
fn arbitrary_units_and_verbs_are_refused_before_any_command() {
    let _guard = fixture();
    for args in [
        vec!["security_service_toggle", "sshd", "enable"],
        vec!["security_service_toggle", "auditd", "restart"],
        vec!["security_service_toggle", "auditd"],
        vec!["security_service_toggle", "auditd", "enable", "extra"],
        vec!["security_ctrl_alt_del", "enable"],
        vec!["security_ctrl_alt_del"],
        vec!["security_ctrl_alt_del", "mask", "extra"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid shape must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn missing_systemctl_is_refused() {
    let _guard = fixture();
    let _ = std::fs::remove_file(format!("{BIN}/systemctl"));
    let run = run_helper(&["security_service_toggle", "auditd", "enable"]);
    assert_ne!(run.code, 0, "missing systemctl must fail: {run:?}");
    assert!(run.stderr.contains("systemctl is not available"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn systemctl_failure_propagates() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper(&["security_service_toggle", "auditd", "enable"]);
    assert_ne!(run.code, 0, "systemctl failure must fail: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    let run = run_helper(&["security_ctrl_alt_del", "mask"]);
    assert_ne!(run.code, 0, "systemctl failure must fail: {run:?}");
}

#[test]
fn faillock_authselect_failure_is_not_reported_as_success() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper(&["security_apply", "pam_faillock_on"]);
    assert_ne!(run.code, 0, "authselect failure must fail the action: {run:?}");
    assert!(run.stderr.contains("authselect could not enable"), "{run:?}");
    assert!(
        std::path::Path::new(&format!("{ETC}/security/faillock.conf.d/50-hardening.conf")).is_file(),
        "the partial file state must still be visible"
    );
}
