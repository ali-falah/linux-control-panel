//! Scope A of the STEP 3B plan: binary-level *success-dispatch* tests.
//!
//! The refusals tests (`refusals.rs`) pin the failure shapes down as the
//! invoking user — where a success path would touch the real host. This file
//! closes that gap in the opposite, safe direction: when this crate is built
//! with the compile-time `test-fixture` feature, the helper's privileged
//! destination constants (polkit rule, /etc/profile.d, /etc/passwd, the
//! repository directories) are swapped to fixture twins below
//! `/tmp/lcp-admin-test-fixture` by the compiler itself — no environment
//! variable and no runtime argument is involved, the feature is not a
//! default feature, and a release build refuses it outright via a
//! `compile_error!` in root_policy.rs.
//!
//! Everything here therefore exercises the helper binary end-to-end:
//! dispatch → validators → descriptor-pinned write core → exit code 0, with
//! the writes landing in this file's own fixture, never in the real home
//! directory, /etc, polkit rules, SSH configuration or repositories.

#![cfg(feature = "test-fixture")]

#[path = "../../src/utils/root_policy.rs"]
#[allow(dead_code)]
mod root_policy;

use root_policy::{POLKIT_RULE_FILE, POLKIT_RULE_TEXT, PROFILE_D_DIR};

use std::process::Command;
use std::sync::{Mutex, MutexGuard};

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";

/// The helper binary for this package, built with the same feature set that
/// activated this file (Cargo guarantees that for CARGO_BIN_EXE_*).
const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");

/// One compiler-fixed fixture directory, shared by all the tests here.
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> std::io::Result<Run> {
    use std::io::Write;
    let mut child = Command::new(HELPER)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(input)?;
    child.wait_with_output().map(|out| Run {
        code: out.status.code().unwrap_or(101),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// Build the whole fixture tree the helper's constants point at: the test
/// user owns the directories, so `privileged_uid` (the invoking user in a
/// test-fixture build) matches.
fn build_fixture() {
    let _ = std::fs::remove_dir_all(FIXTURE_ROOT);
    let etc = std::path::Path::new(FIXTURE_ROOT).join("etc");

    let profile_d = etc.join("profile.d");
    std::fs::create_dir_all(&profile_d).unwrap();
    // 0700 so the pin check ('no group/world write bits') passes
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&profile_d, std::fs::Permissions::from_mode(0o700)).unwrap();

    for dir in [
        etc.join("polkit-1/rules.d"),
        etc.join("yum.repos.d"),
        etc.join("dnf"),
        etc.join("apt/sources.list.d"),
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }

    // a fixture account: tfuser, the uid/gid of the test process, home is a
    // fixture directory (the write action creates .ssh inside it)
    let home = std::path::Path::new(FIXTURE_ROOT).join("home/tfuser");
    std::fs::create_dir_all(&home).unwrap();
    let uid = rustix::process::getuid().as_raw();
    let gid = rustix::process::getgid().as_raw();
    std::fs::write(
        etc.join("passwd"),
        format!(
            "tfuser:x:{uid}:{gid}:Fixture Account:{}:/bin/false\n",
            home.display()
        ),
    )
    .unwrap();
}

/// Take the fixture lock, wipe and rebuild the tree; hold the guard for the
/// rest of the test.
fn fixture() -> MutexGuard<'static, ()> {
    let guard = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    build_fixture();
    guard
}

/// Run the helper binary against the current fixture state.
fn run_helper(args: &[&str], content: &str) -> Run {
    run_with_stdin(args, content.as_bytes())
        .unwrap_or_else(|err| panic!("unexpected spawn error {err}"))
}

/// Assert the helper refuses (exit 2, its own stderr prefix).
fn refusing(args: &[&str], content: &str, because: &str) {
    let run = run_helper(args, content);
    assert_eq!(
        run.code, 2,
        "expected refusal (exit 2): {args:?} — {run:?} ({because})"
    );
    assert!(
        run.stderr.starts_with("lcp-admin:"),
        "the refusal must come from the helper itself: {run:?}"
    );
}

// ── write_profile_sh ─────────────────────────────────────────────────────────

#[test]
fn write_profile_sh_success_dispatch_writes_the_file_and_exits_zero() {
    let _guard = fixture();
    let dest = format!("{PROFILE_D_DIR}/10-fixture.sh");
    let content = "export FIXTURE_ONLY=\"yes\"\n";

    let run = run_helper(&["write_profile_sh", &dest], content);
    assert_eq!(run.code, 0, "success must exit 0: {run:?}");
    assert!(
        run.stderr.is_empty(),
        "a success leaves nothing on stderr: {run:?}"
    );

    let written = std::fs::read_to_string(&dest).unwrap();
    assert_eq!(written, content);
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(&dest).unwrap();
    assert_eq!(meta.mode() & 0o777, 0o644, "a new profile file must be 0644");
    assert_eq!(
        meta.uid(),
        rustix::process::getuid().as_raw(),
        "the fixture file must be owned by the privileged identity"
    );
    // unique temp names: nothing temporary may be left behind
    let profile_dir = std::path::Path::new(PROFILE_D_DIR);
    let no_temp_left = std::fs::read_dir(profile_dir)
        .unwrap()
        .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains(".tmp"));
    assert!(no_temp_left, "no temp files may be left behind");
}

#[test]
fn write_profile_sh_refusals_exit_two_and_write_nothing() {
    let _guard = fixture();
    let dest = format!("{PROFILE_D_DIR}/10-never.sh");

    // shell-code content — the F-1 policy gate inside the helper
    refusing(
        &["write_profile_sh", &dest],
        "rm -rf /\n",
        "free shell text must be refused",
    );
    assert!(
        !std::path::Path::new(&dest).exists(),
        "a refused action must not create the destination"
    );

    // a destination outside the confined directory fails the path validator
    // before stdin is even read
    refusing(
        &["write_profile_sh", "/etc/shadow"],
        "export IGNORABLE=\"x\"\n",
        "outside PROFILE_D_DIR",
    );
    // wrong extension
    refusing(
        &["write_profile_sh", &format!("{PROFILE_D_DIR}/x.conf")],
        "",
        "wrong extension",
    );
    // relative parts in the path
    refusing(
        &["write_profile_sh", &format!("{PROFILE_D_DIR}/../hosts")],
        "",
        "relative parts",
    );
    // substitution characters inside a quoted value
    refusing(
        &["write_profile_sh", &dest],
        "export X=\"$(touch /tmp/pwned)\"\n",
        "command substitution",
    );
    assert!(
        !std::path::Path::new(&dest).exists(),
        "nothing may be written by any refused shape"
    );
}

// ── read/write_user_ssh_keys ─────────────────────────────────────────────────

const TEST_KEY: &str = "ssh-ed25519 AAAAAAAAFIXTUREKEY fixture@example.com\n";

#[test]
fn ssh_key_actions_roundtrip_through_the_fixture_passwd() {
    let _guard = fixture();
    let home = std::path::Path::new(FIXTURE_ROOT).join("home/tfuser");

    // reading an account without any keys is an empty list, not an error
    let fresh = run_helper(&["read_user_ssh_keys", "tfuser"], "");
    assert_eq!(fresh.code, 0, "a fresh read must exit 0: {fresh:?}");
    assert_eq!(fresh.stdout, "", "a fresh account has no keys");
    assert!(!home.join(".ssh").exists(), "a read must not create .ssh");

    // write: exit 0, authorized_keys installed with the fixed shape
    let write_run = run_helper(&["write_user_ssh_keys", "tfuser"], TEST_KEY);
    assert_eq!(write_run.code, 0, "the write must exit 0: {write_run:?}");
    assert!(write_run.stderr.is_empty());

    use std::os::unix::fs::MetadataExt;
    let auth = home.join(".ssh/authorized_keys");
    let dir_meta = std::fs::metadata(home.join(".ssh")).unwrap();
    assert_eq!(dir_meta.mode() & 0o777, 0o700, ".ssh must be 0700");
    assert_eq!(
        dir_meta.uid(),
        rustix::process::getuid().as_raw(),
        ".ssh must be owned by the account's uid from passwd"
    );
    let auth_meta = std::fs::metadata(&auth).unwrap();
    assert_eq!(auth_meta.mode() & 0o777, 0o600, "authorized_keys must be 0600");

    // read: exit 0 and the very same bytes on stdout
    let read_run = run_helper(&["read_user_ssh_keys", "tfuser"], "");
    assert_eq!(read_run.code, 0, "reading back must exit 0: {read_run:?}");
    assert_eq!(read_run.stdout, TEST_KEY, "the roundtrip must return the write");
}

#[test]
fn ssh_key_write_refuses_unknown_accounts_and_bad_shapes() {
    let _guard = fixture();
    // not in the fixture passwd → refused
    refusing(
        &["write_user_ssh_keys", "lcp_no_such_user"],
        TEST_KEY,
        "unknown account",
    );
    // an account name that never reaches a home path
    refusing(
        &["write_user_ssh_keys", "../root"],
        TEST_KEY,
        "traversal in the account name",
    );
    // wrong argv shape (one stray extra parameter)
    refusing(&["read_user_ssh_keys", "tfuser", "extra"], "", "too many parameters");
}

// ── install_polkit_rule ──────────────────────────────────────────────────────

#[test]
fn install_polkit_rule_success_writes_the_exact_rule_text() {
    let _guard = fixture();
    let run = run_helper(&["install_polkit_rule"], POLKIT_RULE_TEXT);
    assert_eq!(run.code, 0, "success must exit 0: {run:?}");
    assert!(
        run.stderr.is_empty(),
        "a success leaves nothing on stderr: {run:?}"
    );
    let written = std::fs::read_to_string(POLKIT_RULE_FILE).unwrap();
    assert_eq!(
        written, POLKIT_RULE_TEXT,
        "only the byte-identical rule may be installed"
    );
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(POLKIT_RULE_FILE).unwrap();
    assert_eq!(meta.mode() & 0o777, 0o644, "the rule must be 0644");
}

#[test]
fn install_polkit_rule_refuses_any_other_content() {
    let _guard = fixture();
    let run = run_helper(&["install_polkit_rule"], "polkit.addRule(function() {});");
    assert_eq!(run.code, 2, "mismatching content must be refused: {run:?}");
    assert!(
        !std::path::Path::new(POLKIT_RULE_FILE).exists(),
        "wrong content must not touch the destination"
    );
}

// ── write_repo_file: deliberate fail-closed dispatch ─────────────────────────

#[test]
fn write_repo_file_stays_unregistered_and_fails_closed() {
    // The action is fully implemented for a future, explicitly authorised
    // step (scope C) but deliberately not registered: a valid fixture path
    // and valid content — still exit 2, still no file.
    let dest = format!("{FIXTURE_ROOT}/etc/yum.repos.d/fixture.repo");
    let content = "[fixture]\n\
         name=Fixture\n\
         baseurl=https://mirror.example.org/fedora/$releasever/os/\n\
         gpgcheck=1\n\
         enabled=1\n";
    let _guard = fixture();
    let run = run_helper(&["write_repo_file", &dest], content);
    assert_eq!(
        run.code, 2,
        "an unregistered action must fail closed: {run:?}"
    );
    assert!(
        run.stderr.starts_with("lcp-admin:"),
        "the refusal must come from the helper itself: {run:?}"
    );
    assert!(
        !std::path::Path::new(&dest).exists(),
        "no repository file may be written while the action is unregistered"
    );
}
