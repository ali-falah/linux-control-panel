//! Fixture tests for the account/group/session named actions. With the
//! compile-time `test-fixture` feature every user tool (useradd, userdel,
//! usermod, gpasswd, passwd, chpasswd, groupadd, groupdel, loginctl) is a
//! stub below `/tmp/lcp-admin-test-fixture/bin` that records its argv; no
//! account, password, group or session on the host is touched.

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const BIN_DIR: &str = "/tmp/lcp-admin-test-fixture/bin";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";
const PASSWORD_STDIN: &str = "/tmp/lcp-admin-test-fixture/ctl/password-stdin";

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
        .env("PKEXEC_UID", "1000")
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
    std::fs::create_dir_all(BIN_DIR).unwrap();
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/log")).unwrap();
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/ctl")).unwrap();

    const STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    const CHPASSWD_STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
cat > "$CTL/password-stdin"
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    use std::os::unix::fs::PermissionsExt;
    for tool in [
        "useradd", "userdel", "usermod", "gpasswd", "passwd", "groupadd", "groupdel",
    ] {
        let path = format!("{BIN_DIR}/{tool}");
        std::fs::write(&path, STUB).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    const LOGINCTL_STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s
' "$*" >> "$LOG"
if [ "$1" = "show-session" ]; then
  if [ -f "$CTL/session-owner" ]; then cat "$CTL/session-owner"; else echo '2000'; fi
  exit 0
fi
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
    let loginctl = format!("{BIN_DIR}/loginctl");
    std::fs::write(&loginctl, LOGINCTL_STUB).unwrap();
    std::fs::set_permissions(&loginctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let chpasswd = format!("{BIN_DIR}/chpasswd");
    std::fs::write(&chpasswd, CHPASSWD_STUB).unwrap();
    std::fs::set_permissions(&chpasswd, std::fs::Permissions::from_mode(0o755)).unwrap();
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
fn user_add_uses_useradd_with_validated_arguments() {
    let _guard = fixture();
    let run = run_helper(&["user_add", "alice", "Alice Example"]);
    assert_eq!(run.code, 0, "user_add must succeed: {run:?}");
    assert_eq!(calls(), vec!["-m -c Alice Example alice"]);
}

#[test]
fn user_add_without_fullname_uses_only_minus_m() {
    let _guard = fixture();
    let run = run_helper(&["user_add", "bob"]);
    assert_eq!(run.code, 0, "user_add must succeed: {run:?}");
    assert_eq!(calls(), vec!["-m bob"]);
}

#[test]
fn user_add_rejects_invalid_account_and_fullname() {
    let _guard = fixture();
    for args in [
        vec!["user_add", "-oops"],
        vec!["user_add", "Alice"],
        vec!["user_add", "a b"],
        vec!["user_add", "alice", "-dash"],
        vec!["user_add", "alice", "bad\nname"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid add must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn user_delete_passes_force_and_optional_remove_home() {
    let _guard = fixture();
    let run = run_helper(&["user_delete", "alice"]);
    assert_eq!(run.code, 0, "delete must succeed: {run:?}");
    assert_eq!(calls(), vec!["--force alice"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["user_delete", "alice", "remove-home"]);
    assert_eq!(run.code, 0, "delete with home must succeed: {run:?}");
    assert_eq!(calls(), vec!["--force --remove alice"]);
}

#[test]
fn user_delete_protected_account_is_refused() {
    let _guard = fixture();
    for user in ["root", "systemd-timesync", "sshd"] {
        let run = run_helper(&["user_delete", user]);
        assert_ne!(run.code, 0, "{user} must be protected: {run:?}");
        assert!(run.stderr.contains("vital system account"), "{run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn user_delete_rejects_bad_shapes() {
    let _guard = fixture();
    for args in [
        vec!["user_delete"],
        vec!["user_delete", "alice", "force"],
        vec!["user_delete", "alice", "remove-home", "extra"],
        vec!["user_delete", "Alice"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid delete must be refused: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn user_set_password_sends_the_password_on_stdin_only() {
    let _guard = fixture();
    let run = run_helper_with_stdin(&["user_set_password", "alice"], "s3cret:pass");
    assert_eq!(run.code, 0, "password change must succeed: {run:?}");
    assert_eq!(calls(), vec![""], "chpasswd takes no argv: {:?}", calls());
    assert_eq!(
        std::fs::read_to_string(PASSWORD_STDIN).unwrap(),
        "alice:s3cret:pass\n",
        "the helper rebuilds the record from the validated account"
    );
    assert!(
        !calls().iter().any(|line| line.contains("s3cret")),
        "the password must never reach argv"
    );
}

#[test]
fn user_set_password_rejects_injection_and_empty_values() {
    let _guard = fixture();
    for password in ["", "a\nb", "a\rb", "a\0b"] {
        let run = run_helper_with_stdin(&["user_set_password", "alice"], password);
        assert_ne!(run.code, 0, "password {password:?} must be refused: {run:?}");
    }
    let run = run_helper_with_stdin(&["user_set_password", "Alice"], "secret");
    assert_eq!(run.code, 2, "invalid user must be refused: {run:?}");
    assert!(calls().is_empty(), "chpasswd must not run: {:?}", calls());
}

#[test]
fn sudo_toggle_uses_usermod_and_gpasswd_with_the_detected_group() {
    let _guard = fixture();
    let run = run_helper(&["user_toggle_sudo", "alice", "grant"]);
    assert_eq!(run.code, 0, "grant must succeed: {run:?}");
    let call = calls().pop().unwrap();
    assert!(call.starts_with("-aG "), "usermod argv: {call}");
    assert!(call.ends_with(" alice"), "usermod argv: {call}");
    let run = run_helper(&["user_toggle_sudo", "alice", "revoke"]);
    assert_eq!(run.code, 0, "revoke must succeed: {run:?}");
    let call = calls().pop().unwrap();
    assert!(call.starts_with("-d alice "), "gpasswd argv: {call}");
}

#[test]
fn sudo_toggle_rejects_unknown_verbs_and_names() {
    let _guard = fixture();
    for args in [
        vec!["user_toggle_sudo", "alice", "yes"],
        vec!["user_toggle_sudo", "Alice", "grant"],
        vec!["user_toggle_sudo", "alice"],
        vec!["user_toggle_sudo", "alice", "grant", "extra"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid sudo toggle: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn modify_user_group_uses_the_right_tool_for_verb() {
    let _guard = fixture();
    let run = run_helper(&["user_modify_group", "alice", "developers", "add"]);
    assert_eq!(run.code, 0, "add must succeed: {run:?}");
    assert_eq!(calls(), vec!["-aG developers alice"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["user_modify_group", "alice", "developers", "remove"]);
    assert_eq!(run.code, 0, "remove must succeed: {run:?}");
    assert_eq!(calls(), vec!["-d alice developers"]);
}

#[test]
fn modify_user_group_rejects_invalid_shapes() {
    let _guard = fixture();
    for args in [
        vec!["user_modify_group", "alice", "dev", "grant"],
        vec!["user_modify_group", "Alice", "dev", "add"],
        vec!["user_modify_group", "alice", "-dev", "add"],
        vec!["user_modify_group", "alice", "dev"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid membership: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn group_manage_adds_and_deletes_only_valid_groups() {
    let _guard = fixture();
    let run = run_helper(&["group_manage", "add", "developers"]);
    assert_eq!(run.code, 0, "group add must succeed: {run:?}");
    assert_eq!(calls(), vec!["developers"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["group_manage", "delete", "developers"]);
    assert_eq!(run.code, 0, "group delete must succeed: {run:?}");
    assert_eq!(calls(), vec!["developers"]);
    for args in [
        vec!["group_manage", "remove", "developers"],
        vec!["group_manage", "add", "-bad"],
        vec!["group_manage", "add"],
        vec!["group_manage", "add", "dev", "extra"],
    ] {
        let run = run_helper(&args);
        assert_eq!(run.code, 2, "invalid group op: {args:?} — {run:?}");
    }
}

#[test]
fn user_lock_uses_passwd_flags_and_protects_system_accounts() {
    let _guard = fixture();
    let run = run_helper(&["user_lock", "alice", "lock"]);
    assert_eq!(run.code, 0, "lock must succeed: {run:?}");
    assert_eq!(calls(), vec!["-l alice"]);
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["user_lock", "alice", "unlock"]);
    assert_eq!(run.code, 0, "unlock must succeed: {run:?}");
    assert_eq!(calls(), vec!["-u alice"]);
    std::fs::remove_file(LOG).ok();
    for args in [
        vec!["user_lock", "root", "lock"],
        vec!["user_lock", "alice", "disable"],
        vec!["user_lock", "Alice", "lock"],
    ] {
        let run = run_helper(&args);
        assert_ne!(run.code, 0, "invalid lock: {args:?} — {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn session_kill_validates_the_numeric_id() {
    let _guard = fixture();
    let run = run_helper(&["session_kill", "42"]);
    assert_eq!(run.code, 0, "session kill must succeed: {run:?}");
    assert_eq!(
        calls(),
        vec!["show-session 42 -p User --value", "kill-session 42"],
        "owner lookup precedes the kill: {:?}",
        calls()
    );
    for id in ["", "0x1", "-1", "1 2", "c1", "999999999999999999999"] {
        let run = run_helper(&["session_kill", id]);
        assert_eq!(run.code, 2, "invalid session id {id:?}: {run:?}");
    }
}

#[test]
fn tool_failure_and_missing_tool_are_errors() {
    let _guard = fixture();
    set_ctl("fail");
    let run = run_helper(&["user_add", "alice"]);
    assert_ne!(run.code, 0, "tool failure must propagate: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    std::fs::remove_file(format!("{BIN_DIR}/useradd")).ok();
    std::fs::remove_file(LOG).ok();
    let run = run_helper(&["user_add", "alice"]);
    assert_ne!(run.code, 0, "missing useradd must be refused: {run:?}");
    assert!(run.stderr.contains("useradd is not available"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn protected_admin_groups_cannot_be_deleted() {
    let _guard = fixture();
    for group in ["wheel", "sudo", "root", "admin", "systemd-journal", "docker"] {
        let run = run_helper(&["group_manage", "delete", group]);
        assert_ne!(run.code, 0, "{group} deletion must be blocked: {run:?}");
        assert!(
            run.stderr.contains("essential system-administration group"),
            "{run:?}"
        );
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
    // a normal group is still deletable
    let run = run_helper(&["group_manage", "delete", "developers"]);
    assert_eq!(run.code, 0, "ordinary groups stay deletable: {run:?}");
}

#[test]
fn protected_account_passwords_cannot_be_changed() {
    let _guard = fixture();
    for user in ["root", "sshd", "systemd-timesync"] {
        let run = run_helper_with_stdin(&["user_set_password", user], "secret");
        assert_ne!(run.code, 0, "{user} password must be blocked: {run:?}");
        assert!(run.stderr.contains("vital system account"), "{run:?}");
    }
    assert!(calls().is_empty(), "chpasswd must not run: {:?}", calls());
    assert!(
        !std::path::Path::new(PASSWORD_STDIN).exists(),
        "the password must not be consumed for a refused account"
    );
}

#[test]
fn session_kill_refuses_the_callers_own_session() {
    let _guard = fixture();
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/session-owner"), "1000").unwrap();
    let run = run_helper(&["session_kill", "42"]);
    assert_ne!(run.code, 0, "own session must be protected: {run:?}");
    assert!(
        run.stderr.contains("belongs to the invoking user"),
        "the refusal must explain the ownership check: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["show-session 42 -p User --value"],
        "only the owner lookup may run: {:?}",
        calls()
    );
}

#[test]
fn session_kill_refuses_when_the_caller_is_unknown() {
    let _guard = fixture();
    use std::io::Write;
    let mut child = Command::new(HELPER)
        .args(["session_kill", "42"])
        .env_remove("PKEXEC_UID")
        .env_remove("SUDO_UID")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut pipe) = child.stdin.take() {
        let _ = pipe.write_all(b"");
    }
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(output.status.code(), Some(0), "unknown caller must fail closed: {stderr}");
    assert!(stderr.contains("invoking user could not be determined"), "{stderr}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn session_kill_fails_closed_on_unparseable_owner() {
    let _guard = fixture();
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/session-owner"), "").unwrap();
    let run = run_helper(&["session_kill", "42"]);
    assert_ne!(run.code, 0, "unparseable owner must fail closed: {run:?}");
    assert!(run.stderr.contains("could not be determined"), "{run:?}");
    assert_eq!(
        calls(),
        vec!["show-session 42 -p User --value"],
        "the kill must not run: {:?}",
        calls()
    );
}
