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
    let var = std::path::Path::new(FIXTURE_ROOT).join("var");

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
        // the nginx tree the config actions pin
        etc.join("nginx"),
        etc.join("nginx/conf.d"),
        etc.join("nginx/sites-available"),
        etc.join("nginx/sites-enabled"),
        etc.join("nginx/backups"),
        // the web root the www verbs pin (0700 for the same pin check)
        var.join("www"),
        var.join("www/html"),
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let www_root = var.join("www");
    std::fs::set_permissions(&www_root, std::fs::Permissions::from_mode(0o700)).unwrap();

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

#[test]
fn write_profile_sh_keeps_a_backup_and_prints_its_path() {
    let _guard = fixture();
    let dest = format!("{PROFILE_D_DIR}/20-backup.sh");
    let first = "export BACKUP_SOURCE=\"one\"\n";
    let second = "export BACKUP_SOURCE=\"two\"\n";

    // a brand-new file has nothing to back up and prints nothing
    let created = run_helper(&["write_profile_sh", &dest], first);
    assert_eq!(created.code, 0, "the first write must succeed: {created:?}");
    assert_eq!(
        created.stdout, "",
        "a new file has no previous revision to report"
    );

    let rewritten = run_helper(&["write_profile_sh", &dest], second);
    assert_eq!(rewritten.code, 0, "the rewrite must succeed: {rewritten:?}");
    assert!(rewritten.stderr.is_empty());

    let backup = rewritten.stdout.trim();
    assert!(
        backup.starts_with(&format!("{PROFILE_D_DIR}/")) && backup.ends_with(".bak"),
        "the helper must print the backup path it made: {rewritten:?}"
    );
    assert_eq!(
        std::fs::read_to_string(backup).unwrap(),
        first,
        "the backup must hold the revision that was replaced"
    );
    assert_eq!(
        std::fs::read_to_string(&dest).unwrap(),
        second,
        "the destination must hold the new revision"
    );
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        std::fs::metadata(backup).unwrap().mode() & 0o777,
        0o644,
        "the backup is a normal readable profile file"
    );
}

#[test]
fn write_profile_sh_accepts_the_exact_path_manager_content() {
    let _guard = fixture();
    let dest = format!("{PROFILE_D_DIR}/50-path.sh");

    // exactly what shell_add_path_entry writes for an /etc/profile.d target
    let one = "export FOO=\"bar\"\nexport PATH=\"$PATH:/opt/bin\"\n";
    let two = format!("{one}export PATH=\"$PATH:/opt/bin:/usr/local/foo\"\n");
    let created = run_helper(&["write_profile_sh", &dest], one);
    assert_eq!(created.code, 0, "PATH content must now pass: {created:?}");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), one);

    let appended = run_helper(&["write_profile_sh", &dest], &two);
    assert_eq!(
        appended.code, 0,
        "a second PATH entry must pass: {appended:?}"
    );
    let backup = appended.stdout.trim();
    assert!(
        backup.starts_with(&format!("{PROFILE_D_DIR}/")) && backup.ends_with(".bak"),
        "the rewrite must still report its backup: {appended:?}"
    );
    assert_eq!(std::fs::read_to_string(backup).unwrap(), one);

    // exactly what shell_remove_path_entry writes: the /opt/bin line is gone
    let removed = "export FOO=\"bar\"\nexport PATH=\"$PATH:/usr/local/foo\"\n";
    let rewritten = run_helper(&["write_profile_sh", &dest], removed);
    assert_eq!(
        rewritten.code, 0,
        "the removal shape must pass: {rewritten:?}"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), removed);

    // the exception is only the one grammar: shell syntax stays refused, and
    // the previously written file is left untouched
    refusing(
        &["write_profile_sh", &dest],
        "export PATH=\"$PATH:/opt/bin;rm -rf /\"\n",
        "$PATH followed by shell syntax",
    );
    refusing(
        &["write_profile_sh", &dest],
        "export PATH=\"$(touch /tmp/x)\"\n",
        "$PATH command substitution",
    );
    refusing(
        &["write_profile_sh", &dest],
        "export PATH=\"$HOME/bin\"\n",
        "another variable",
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), removed);
}

#[test]
fn write_profile_sh_forces_0644_on_an_existing_file() {
    let _guard = fixture();
    let dest = format!("{PROFILE_D_DIR}/30-mode.sh");
    std::fs::write(&dest, "export MODE=\"old\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o666)).unwrap();

    let run = run_helper(&["write_profile_sh", &dest], "export MODE=\"new\"\n");
    assert_eq!(run.code, 0, "the write must succeed: {run:?}");
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        std::fs::metadata(&dest).unwrap().mode() & 0o777,
        0o644,
        "an existing file must not keep its group/world-writable mode"
    );
}

#[test]
fn write_profile_sh_refuses_a_symlinked_profile_directory() {
    let _guard = fixture();
    let profile_dir = std::path::Path::new(PROFILE_D_DIR);
    std::fs::remove_dir_all(profile_dir).unwrap();
    let elsewhere = std::path::Path::new(FIXTURE_ROOT).join("symlinked-profile.d");
    std::fs::create_dir_all(&elsewhere).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, profile_dir).unwrap();

    let dest = format!("{PROFILE_D_DIR}/40-symlink.sh");
    let run = run_helper(&["write_profile_sh", &dest], "export SYMLINK=\"no\"\n");
    assert_eq!(
        run.code, 2,
        "a symlinked profile directory must be refused: {run:?}"
    );
    assert!(
        run.stderr.starts_with("lcp-admin:"),
        "the refusal must come from the helper: {run:?}"
    );
    assert!(
        !elsewhere.join("40-symlink.sh").exists(),
        "nothing may be written through the symlink"
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

// ── restore_profile_sh ───────────────────────────────────────────────────────

#[cfg(feature = "test-fixture")]
mod restore_profile_sh_dispatch {
    use super::*;

    /// Write the destination through the validated write action and return
    /// the revision backup path the write core reports.
    fn write_and_capture_backup(dest: &str, content: &str) -> String {
        let run = run_helper(&["write_profile_sh", dest], content);
        assert_eq!(run.code, 0, "the setup write must succeed: {run:?}");
        run.stdout.trim().to_string()
    }

    fn read(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    }

    /// 11: write a file, change it, restore the first revision — the content
    /// is exactly restored and the restore itself keeps the intermediate
    /// state as a fresh revision.
    #[test]
    fn restore_profile_sh_roundtrip_restores_the_exact_content() {
        let _guard = fixture();
        let dest = format!("{PROFILE_D_DIR}/10-restore.sh");
        // brand-new file: no revision exists yet for the *first* write
        assert_eq!(
            run_helper(&["write_profile_sh", &dest], "export ORIGINAL=\"yes\"\n").code,
            0
        );
        let first = "export ORIGINAL=\"yes\"\n";
        // a second write keeps the first state as its revision backup
        let backup = write_and_capture_backup(&dest, "export CHANGED=\"interim\"\n");
        assert!(
            backup.starts_with(&format!("{PROFILE_D_DIR}/10-restore.sh.")) && backup.ends_with(".bak"),
            "the backup must be a revision sibling of the destination: {backup}"
        );
        assert_eq!(read(&backup), first);

        // intermediate state: the destination has already moved on
        assert_eq!(
            run_helper(&["write_profile_sh", &dest], "export SECOND=\"no\"\n").code,
            0
        );
        let run = run_helper(&["restore_profile_sh", &dest, &backup], "");
        assert_eq!(run.code, 0, "the restore must succeed: {run:?}");
        assert_eq!(run.stderr, "", "a success leaves nothing on stderr: {run:?}");
        assert_eq!(read(&dest), first, "the content must be byte-exact");

        // the restore kept the intermediate content as its own fresh revision
        let fresh = run.stdout.trim();
        assert!(fresh.ends_with(".bak") && fresh != backup, "{fresh:?}");
        assert_eq!(read(fresh), "export SECOND=\"no\"\n");
    }

    /// 7: unknown or malformed destinations — the destination policy applies
    /// to restores exactly as it does to writes, before anything is read.
    #[test]
    fn restore_profile_sh_refuses_unknown_or_program_shaped_paths() {
        let _guard = fixture();
        let dest = format!("{PROFILE_D_DIR}/10-restore.sh");
        let backup = format!("{dest}.1700000000_1.2.bak");
        refusing(&["restore_profile_sh"], "", "no arguments");
        refusing(
            &["restore_profile_sh", &dest],
            "",
            "the backup argument is missing",
        );
        refusing(
            &["restore_profile_sh", "/etc/shadow", &backup],
            "",
            "the destination must be in the confined profile directory",
        );
        refusing(
            &["restore_profile_sh", &format!("{PROFILE_D_DIR}/10-x.conf"), &backup],
            "",
            "only .sh destinations may be restored",
        );
        refusing(
            &["restore_profile_sh", backup.as_str(), backup.as_str()],
            "",
            "a backup path is not a destination; the destinations must be .sh files",
        );
    }

    /// 8: every refused backup shape — foreign directory, foreign name,
    /// cross-destination name, missing revision shape, hostile suffix,
    /// well-shaped but nonexistent sibling.
    #[test]
    fn restore_profile_sh_refuses_a_backup_that_is_not_its_own_sibling() {
        let _guard = fixture();
        let dest = format!("{PROFILE_D_DIR}/10-restore.sh");
        write_and_capture_backup(&dest, "export CURRENT=\"v1\"\n");

        refusing(
            &["restore_profile_sh", &dest, "/tmp/hostile/target.bak"],
            "",
            "the backup must live in the confined profile directory",
        );
        let foreign = format!("{PROFILE_D_DIR}/foreign.txt");
        std::fs::write(&foreign, "export BORROWED=\"1\"\n").unwrap();
        refusing(
            &["restore_profile_sh", &dest, &foreign],
            "",
            "a plain file is not a revision sibling",
        );
        let cross = format!("{PROFILE_D_DIR}/19-other.sh.1700000000_1.2.bak");
        std::fs::write(&cross, "export WRONG_OWNER=\"1\"\n").unwrap();
        refusing(
            &["restore_profile_sh", &dest, &cross],
            "",
            "a sibling of another destination is not this destination's backup",
        );
        let shapeless = format!("{PROFILE_D_DIR}/10-restore.sh.bak");
        std::fs::write(&shapeless, "export SHAPELESS=\"1\"\n").unwrap();
        refusing(
            &["restore_profile_sh", &dest, &shapeless],
            "",
            "without a numeric revision this is not a backup naming",
        );
        let hostile = format!("{PROFILE_D_DIR}/10-restore.sh.17000000;.txt.bak");
        std::fs::write(&hostile, "x\n").unwrap();
        refusing(
            &["restore_profile_sh", &dest, &hostile],
            "",
            "the revision suffix must be alphanumeric only",
        );
        refusing(
            &[
                "restore_profile_sh",
                &dest,
                &format!("{dest}.9999999999_1.2.bak"),
            ],
            "",
            "a well-shaped but missing sibling must be refused",
        );
        assert_eq!(
            read(&dest),
            "export CURRENT=\"v1\"\n",
            "nothing may be written for a refused backup"
        );
    }

    /// 9: symlinked backups are refused instead of followed, and a symlinked
    /// destination is never written through.
    #[test]
    fn restore_profile_sh_refuses_symlinks_in_backup_or_destination() {
        let _guard = fixture();
        let dest = format!("{PROFILE_D_DIR}/10-restore.sh");
        write_and_capture_backup(&dest, "export CURRENT=\"v1\"\n");

        let real = format!("{PROFILE_D_DIR}/real-target.bak");
        std::fs::write(&real, "export STOLEN=\"1\"\n").unwrap();
        let link = format!("{dest}.1700000000_1.2.bak");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        refusing(
            &["restore_profile_sh", &dest, &link],
            "",
            "a symlinked backup must not be followed",
        );
        assert_eq!(
            read(&dest),
            "export CURRENT=\"v1\"\n",
            "the destination stays untouched"
        );

        let victim = format!("{PROFILE_D_DIR}/victim.sh");
        std::fs::write(&victim, "original\n").unwrap();
        let dest_link = format!("{PROFILE_D_DIR}/10-symlinked.sh");
        std::os::unix::fs::symlink(&victim, &dest_link).unwrap();
        let backup = format!("{PROFILE_D_DIR}/10-symlinked.sh.1700000000_1.2.bak");
        std::fs::write(&backup, "export OK=\"1\"\n").unwrap();
        refusing(
            &["restore_profile_sh", &dest_link, &backup],
            "",
            "a symlinked destination is never replaced",
        );
        assert_eq!(
            read(&victim),
            "original\n",
            "the symlink's target must stay untouched"
        );
    }

    /// 10: the historical bytes are re-validated — a backup that predates
    /// the current content policy is refused, destination untouched.
    #[test]
    fn restore_profile_sh_rejects_content_that_fails_the_profile_policy() {
        let _guard = fixture();
        let dest = format!("{PROFILE_D_DIR}/10-restore.sh");
        write_and_capture_backup(&dest, "export CURRENT=\"v1\"\n");

        let legacy = format!("{dest}.1700000000_7.7.bak");
        std::fs::write(&legacy, "rm -rf /\n").unwrap();
        let run = run_helper(&["restore_profile_sh", &dest, &legacy], "");
        assert_eq!(run.code, 2, "shell-code backup content must be refused: {run:?}");
        assert!(
            run.stderr.contains("predates the current content policy"),
            "the refusal must tell the user to restore it manually: {run:?}"
        );
        assert_eq!(read(&dest), "export CURRENT=\"v1\"\n");

        let binary = format!("{dest}.1700000000_8.8.bak");
        std::fs::write(&binary, [0xff, 0xfe]).unwrap();
        let run = run_helper(&["restore_profile_sh", &dest, &binary], "");
        assert_eq!(run.code, 2, "non-UTF-8 backup content must be refused: {run:?}");
        assert!(run.stderr.contains("predates"), "{run:?}");
        assert_eq!(read(&dest), "export CURRENT=\"v1\"\n");

        let huge = format!("{dest}.1700000000_9.9.bak");
        std::fs::write(&huge, format!("export BIG=\"{}\"\n", "x".repeat(256 * 1024))).unwrap();
        let run = run_helper(&["restore_profile_sh", &dest, &huge], "");
        assert_eq!(run.code, 2, "an oversized backup must be refused: {run:?}");
        assert_eq!(read(&dest), "export CURRENT=\"v1\"\n");
    }
}

// ── nginx config actions ─────────────────────────────────────────────────────
//
// GROUP 2 of the nginx batch: `nginx_config_install`, `nginx_config_backup`
// and `nginx_restore_backup`, exercised end-to-end through the helper binary
// against the fixture's own nginx tree. The install writes go through the
// same root-owned install core as the profile files (root:root 0644, atomic
// rename, backup-on-write), the backups live in the fixture backups
// directory as `<basename>.<stamp>.bak` revision siblings, and every refusal
// leaves the destination byte-identical.

#[cfg(feature = "test-fixture")]
mod nginx_config_dispatch {
    use root_policy::{
        NGINX_BACKUPS_DIR, NGINX_CONF_D, NGINX_NGINX_CONF, NGINX_SITES_AVAILABLE,
        NGINX_SITES_ENABLED,
    };

    use super::*;

    fn read(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    }

    /// Run the install action and return what it printed — the revision
    /// backup path of the replaced content, "" for a first creation.
    fn install(dest: &str, content: &str) -> String {
        let run = run_helper(&["nginx_config_install", dest], content);
        assert_eq!(run.code, 0, "the install must succeed: {run:?}");
        assert!(run.stderr.is_empty(), "a success leaves nothing on stderr: {run:?}");
        run.stdout.trim().to_string()
    }

    /// 1: create a site file, rewrite it, and exercise the exact nginx.conf —
    /// every write is root:root 0644 and keeps the replaced content as a
    /// fresh revision sibling next to the destination.
    #[test]
    fn nginx_config_install_creates_rewrites_and_prints_revision_backups() {
        let _guard = fixture();
        use std::os::unix::fs::MetadataExt;

        let dest = format!("{NGINX_SITES_AVAILABLE}/example.com");
        let first = "# the first site definition\nserver {\n    listen 80;\n}\n";
        // a brand-new file prints nothing: there is no previous revision
        assert_eq!(install(&dest, first), "", "a first install has no backup to report");
        assert_eq!(read(&dest), first, "the content must be byte-exact");
        let meta = std::fs::metadata(&dest).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o644, "an installed config must be 0644");
        assert_eq!(
            meta.uid(),
            rustix::process::getuid().as_raw(),
            "the fixture file must be owned by the privileged identity"
        );

        let second = "server {\n    listen 8080;\n}\n";
        let backup = install(&dest, second);
        assert!(
            backup.starts_with(&format!("{NGINX_SITES_AVAILABLE}/example.com.")) && backup.ends_with(".bak"),
            "the rewrite must report the replaced content's revision sibling: {backup:?}"
        );
        assert_eq!(read(&backup), first, "the backup must hold the replaced revision");
        assert_eq!(read(&dest), second);

        // the exact main configuration file is installable like any other
        // destination
        let conf = "# fixture nginx.conf\nevents {}\n";
        assert_eq!(install(NGINX_NGINX_CONF, conf), "");
        assert_eq!(read(NGINX_NGINX_CONF), conf);
        assert_eq!(
            std::fs::metadata(NGINX_NGINX_CONF).unwrap().mode() & 0o777,
            0o644
        );
    }

    /// 2: the destination confinement and the structural content gate hold
    /// helper-side too — nothing is written for any refusal, and a symlinked
    /// destination is refused instead of written through.
    #[test]
    fn nginx_config_install_refuses_bad_destinations_and_content() {
        let _guard = fixture();
        let dest = format!("{NGINX_CONF_D}/10-gate.conf");
        let honest = "# an honest fixture config\n";
        install(&dest, honest);

        // destinations outside the pinned tree (the arg policy refused these
        // before dispatch too; here the helper proves the same rule again)
        for hostile in [
            "/etc/shadow",
            &format!("{NGINX_CONF_D}/../shadow"),
            "/etc/nginx/backups/nope.conf",
        ] {
            refusing(
                &["nginx_config_install", hostile],
                honest,
                "the destination must stay inside the nginx tree",
            );
        }

        // structural content gate: oversized, NUL, stray control characters
        let oversized = format!("# {}\n", "x".repeat(256 * 1024 + 1));
        refusing(
            &["nginx_config_install", &dest],
            &oversized,
            "the content must not exceed the size cap",
        );
        refusing(
            &["nginx_config_install", &dest],
            "# nul\0byte\n",
            "NUL bytes are never honest config content",
        );
        refusing(
            &["nginx_config_install", &dest],
            "# bell\x07byte\n",
            "control characters beyond newlines and tabs are refused",
        );

        // a symlinked destination is never replaced
        let victim = format!("{NGINX_CONF_D}/victim.conf");
        std::fs::write(&victim, "the victim\n").unwrap();
        let link = format!("{NGINX_CONF_D}/symlinked.conf");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        refusing(
            &["nginx_config_install", &link],
            "# through the link\n",
            "a symlinked destination must be refused, not followed",
        );
        assert_eq!(read(&victim), "the victim\n", "the link's target stays untouched");

        assert_eq!(read(&dest), honest, "a refused install leaves the destination untouched");
    }

    /// 3: the backup action copies the current content into the backups
    /// directory under one fresh `<basename>.<stamp>.bak` name, byte-exact,
    /// and reports that path as its only output.
    #[test]
    fn nginx_config_backup_copies_into_the_backups_dir() {
        let _guard = fixture();
        use std::os::unix::fs::MetadataExt;

        let dest = format!("{NGINX_SITES_AVAILABLE}/backed-up");
        let content = "server {\n    listen 443;\n}\n";
        install(&dest, content);
        let before = std::fs::metadata(&dest).unwrap().mtime();

        let run = run_helper(&["nginx_config_backup", &dest], "");
        assert_eq!(run.code, 0, "the backup must succeed: {run:?}");
        assert!(run.stderr.is_empty());
        let backup = run.stdout.trim().to_string();
        let backups_prefix = format!("{NGINX_BACKUPS_DIR}/backed-up.");
        assert!(
            backup.starts_with(&backups_prefix) && backup.ends_with(".bak"),
            "the backup must be a revision sibling of the destination in the backups directory: {backup:?}"
        );
        // the `<name>.<stamp>.bak` shape nginx_list_backups parses: exactly
        // one dot-separated stamp between the name and the extension
        let remainder = backup
            .strip_prefix(&backups_prefix)
            .unwrap()
            .strip_suffix(".bak")
            .unwrap();
        let (stamp, time_run) = remainder.split_once('_').unwrap_or_else(|| {
            panic!("the stamp must have a '<date>_<time>' shape: {backup:?}")
        });
        assert!(
            !time_run.contains('_'),
            "no extra revision part may appear: {backup:?}"
        );
        assert!(
            stamp.len() == 8 && stamp.bytes().all(|b| b.is_ascii_digit()),
            "a YYYYmmdd date stamp must be 8 digits: {backup:?}"
        );
        assert_eq!(read(&backup), content, "the backup must hold the current content");
        assert_eq!(
            std::fs::metadata(&backup).unwrap().mode() & 0o777,
            0o644,
            "the backup is a normal readable file"
        );
        assert_eq!(
            std::fs::metadata(&dest).unwrap().mtime(),
            before,
            "backing up must not touch the destination"
        );
    }

    /// 4: the backup action refuses anything that is not one plain regular
    /// file in the pinned tree — missing, a directory, symlinked.
    #[test]
    fn nginx_config_backup_refuses_missing_nonregular_or_symlinked_sources() {
        let _guard = fixture();

        refusing(
            &["nginx_config_backup", &format!("{NGINX_SITES_AVAILABLE}/absent")],
            "",
            "a missing destination cannot be backed up",
        );
        refusing(
            &["nginx_config_backup", NGINX_CONF_D],
            "",
            "a directory is not a file to back up",
        );

        let victim = format!("{NGINX_SITES_AVAILABLE}/still-honest.conf");
        std::fs::write(&victim, "untouched\n").unwrap();
        let link = format!("{NGINX_SITES_AVAILABLE}/linked");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        refusing(
            &["nginx_config_backup", &link],
            "",
            "a symlinked source must be refused, not read through",
        );
        assert_eq!(read(&victim), "untouched\n");
    }

    /// 11': write a config, change it, restore the backup action's revision —
    /// the content is byte-exact and the restore itself keeps the
    /// intermediate state as a fresh revision (so it is reversible).
    #[test]
    fn nginx_restore_backup_roundtrip_restores_the_exact_content() {
        let _guard = fixture();
        let dest = format!("{NGINX_CONF_D}/restored.conf");
        let first = "server {\n    listen 80;\n}\n";
        install(&dest, first);

        // the backup action captures the current revision
        let capture = run_helper(&["nginx_config_backup", &dest], "");
        assert_eq!(capture.code, 0, "the capture backup must succeed: {capture:?}");
        let backup = capture.stdout.trim().to_string();

        // the destination moves on
        let second = "server {\n    listen 8081;\n}\n";
        install(&dest, second);

        let run = run_helper(&["nginx_restore_backup", &dest, &backup], "");
        assert_eq!(run.code, 0, "the restore must succeed: {run:?}");
        assert!(run.stderr.is_empty());
        assert_eq!(read(&dest), first, "the restore must restore byte-exact");

        // the restore's own backup-on-write kept the intermediate revision
        let fresh = run.stdout.trim().to_string();
        assert!(fresh.ends_with(".bak") && fresh != backup, "{fresh:?}");
        assert_eq!(read(&fresh), second);
        // and it lives next to the destination, not in the backups directory
        assert!(
            fresh.starts_with(&format!("{NGINX_CONF_D}/restored.conf.")),
            "the restore's auto-backup is a sibling of the destination: {fresh:?}"
        );
    }

    /// 8': every refused backup — foreign directory, foreign name,
    /// cross-destination name, missing revision shape, well-shaped but
    /// missing.
    #[test]
    fn nginx_restore_backup_refuses_foreign_or_malformed_backups() {
        let _guard = fixture();
        let dest = format!("{NGINX_CONF_D}/restored.conf");
        install(&dest, "server {\n    listen 80;\n}\n");
        let honest = read(&dest);

        let well_named = format!(
            "{NGINX_BACKUPS_DIR}/restored.conf.20240102_030405.bak"
        );
        std::fs::write(&well_named, "ok\n").unwrap();

        refusing(
            &["nginx_restore_backup", &dest, "/tmp/hostile/target.bak"],
            "",
            "the backup must live in the confined backups directory",
        );
        let foreign = format!("{NGINX_CONF_D}/restored.conf.20240102_030406.bak");
        std::fs::write(&foreign, "borowed\n").unwrap();
        refusing(
            &["nginx_restore_backup", &dest, &foreign],
            "",
            "a sibling in the config directory is not a backups-directory revision",
        );
        let cross = format!("{NGINX_BACKUPS_DIR}/other.conf.20240102_030407.bak");
        std::fs::write(&cross, "wrong owner\n").unwrap();
        refusing(
            &["nginx_restore_backup", &dest, &cross],
            "",
            "a sibling of another destination is not this destination's backup",
        );
        let shapeless = format!("{NGINX_BACKUPS_DIR}/restored.conf.bak");
        std::fs::write(&shapeless, "no revision\n").unwrap();
        refusing(
            &["nginx_restore_backup", &dest, &shapeless],
            "",
            "without a numeric revision this is not a backup naming",
        );
        let hostile = format!("{NGINX_BACKUPS_DIR}/restored.conf.2024;0102_030408.bak");
        std::fs::write(&hostile, "hostile\n").unwrap();
        refusing(
            &["nginx_restore_backup", &dest, &hostile],
            "",
            "the revision stamp must be digits only",
        );
        refusing(
            &[
                "nginx_restore_backup",
                &dest,
                &format!("{NGINX_BACKUPS_DIR}/restored.conf.20240102_999999.bak"),
            ],
            "",
            "a well-shaped but missing sibling must be refused",
        );
        assert_eq!(read(&dest), honest, "nothing may be written for a refused backup");
    }

    /// 9': symlinked backups are refused instead of followed, and a symlinked
    /// destination is never written through.
    #[test]
    fn nginx_restore_backup_refuses_symlinked_backup_or_destination() {
        let _guard = fixture();
        let dest = format!("{NGINX_CONF_D}/restored.conf");
        install(&dest, "server {\n    listen 80;\n}\n");
        let honest = read(&dest);

        let real = format!("{NGINX_BACKUPS_DIR}/real-target.bak");
        std::fs::write(&real, "stolen\n").unwrap();
        let link = format!("{NGINX_BACKUPS_DIR}/restored.conf.20240102_030405.bak");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        refusing(
            &["nginx_restore_backup", &dest, &link],
            "",
            "a symlinked backup must not be followed",
        );
        assert_eq!(read(&dest), honest, "the destination stays untouched");

        let victim = format!("{NGINX_CONF_D}/victim-target.conf");
        std::fs::write(&victim, "original\n").unwrap();
        let dest_link = format!("{NGINX_SITES_ENABLED}/symlinked-site");
        std::os::unix::fs::symlink(&victim, &dest_link).unwrap();
        let backup = format!("{NGINX_BACKUPS_DIR}/symlinked-site.20240102_030409.bak");
        std::fs::write(&backup, "ok\n").unwrap();
        refusing(
            &["nginx_restore_backup", &dest_link, &backup],
            "",
            "a symlinked destination is never replaced",
        );
        assert_eq!(read(&victim), "original\n", "the symlink's target stays untouched");
    }

    /// 10': the historical bytes are re-validated against the structural
    /// gate — a backup that predates the current content policy is refused,
    /// the destination untouched.
    #[test]
    fn nginx_restore_backup_rejects_content_failing_the_config_gate() {
        let _guard = fixture();
        let dest = format!("{NGINX_CONF_D}/restored.conf");
        install(&dest, "server {\n    listen 80;\n}\n");
        let honest = read(&dest);

        let legacy = format!("{NGINX_BACKUPS_DIR}/restored.conf.20240102_030405.bak");
        std::fs::write(&legacy, "client_body_temp_path \x07;\n").unwrap();
        let run = run_helper(&["nginx_restore_backup", &dest, &legacy], "");
        assert_eq!(run.code, 2, "a control-character backup must be refused: {run:?}");
        assert!(
            run.stderr.contains("predates the current content policy"),
            "the refusal must tell the user to restore it manually: {run:?}"
        );
        assert_eq!(read(&dest), honest);

        let binary = format!("{NGINX_BACKUPS_DIR}/restored.conf.20240102_030406.bak");
        std::fs::write(&binary, [0xff, 0xfe]).unwrap();
        let run = run_helper(&["nginx_restore_backup", &dest, &binary], "");
        assert_eq!(run.code, 2, "non-UTF-8 backup content must be refused: {run:?}");
        assert!(run.stderr.contains("predates"), "{run:?}");
        assert_eq!(read(&dest), honest);

        let huge = format!("{NGINX_BACKUPS_DIR}/restored.conf.20240102_030407.bak");
        std::fs::write(&huge, format!("# {}\n", "x".repeat(256 * 1024))).unwrap();
        let run = run_helper(&["nginx_restore_backup", &dest, &huge], "");
        assert_eq!(run.code, 2, "an oversized backup must be refused: {run:?}");
        assert_eq!(read(&dest), honest);
    }
}

// ── nginx link / remove actions ──────────────────────────────────────────────
//
// GROUP 3 of the nginx batch: `nginx_link_site` (one same-named symlink from
// sites-available into sites-enabled) and `nginx_config_remove` (exactly one
// regular file in the tree, `nginx.conf` excluded, no -r/rm -f semantics).

#[cfg(feature = "test-fixture")]
mod nginx_link_remove_dispatch {
    use root_policy::{NGINX_CONF_D, NGINX_NGINX_CONF, NGINX_SITES_AVAILABLE, NGINX_SITES_ENABLED};

    use super::*;

    fn read(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    }

    fn install(dest: &str, content: &str) {
        let run = run_helper(&["nginx_config_install", dest], content);
        assert_eq!(run.code, 0, "the setup install must succeed: {run:?}");
    }

    /// 1: the one same-named symlink — creation, correctness (it enables
    /// exactly the selected site), and every refused shape leaving the
    /// enabled directory untouched.
    #[test]
    fn nginx_link_site_creates_the_one_enabled_symlink_or_refuses() {
        let _guard = fixture();
        let site = format!("{NGINX_SITES_AVAILABLE}/example.com");
        let link = format!("{NGINX_SITES_ENABLED}/example.com");
        install(&site, "server {\n    listen 80;\n    server_name example.com;\n}\n");

        let run = run_helper(&["nginx_link_site", &site, &link], "");
        assert_eq!(run.code, 0, "the link must succeed: {run:?}");
        assert!(run.stderr.is_empty(), "a success leaves nothing on stderr: {run:?}");
        assert!(run.stdout.is_empty(), "the link prints nothing: {run:?}");
        let meta = std::fs::symlink_metadata(&link).unwrap();
        assert!(meta.file_type().is_symlink(), "the enabled entry must be a symlink");
        assert_eq!(
            std::fs::read_link(&link).unwrap().to_string_lossy(),
            site,
            "the symlink must point at the selected site"
        );
        assert_eq!(read(&link), "server {\n    listen 80;\n    server_name example.com;\n}\n");

        // a second link over the existing entry is refused, existing untouched
        let before = std::fs::read_link(&link).unwrap();
        refusing(
            &["nginx_link_site", &site, &link],
            "",
            "an existing enabled entry must not be re-linked over",
        );
        assert_eq!(std::fs::read_link(&link).unwrap(), before);

        // basename mismatch: never a rename-in-disguise
        let other = format!("{NGINX_SITES_ENABLED}/other-site");
        refusing(
            &["nginx_link_site", &site, &other],
            "",
            "the link must enable the site it is named after",
        );
        assert!(
            !std::path::Path::new(&other).exists(),
            "a refused link must not create anything"
        );

        // endpoints outside their one directory, and a symlinked source
        refusing(
            &["nginx_link_site", &format!("{NGINX_CONF_D}/example.com"), &link],
            "",
            "the source must live in sites-available",
        );
        std::fs::remove_file(&site).unwrap();
        std::fs::write(&format!("{NGINX_SITES_AVAILABLE}/real-victim"), "victim\n").unwrap();
        let linked_site = format!("{NGINX_SITES_AVAILABLE}/linked");
        std::os::unix::fs::symlink(&format!("{NGINX_SITES_AVAILABLE}/real-victim"), &linked_site)
            .unwrap();
        let linked_entry = format!("{NGINX_SITES_ENABLED}/linked");
        refusing(
            &["nginx_link_site", &linked_site, &linked_entry],
            "",
            "a symlinked source must be refused, not read through",
        );
        assert!(
            !std::path::Path::new(&linked_entry).exists(),
            "the refused link must not exist"
        );

        // a missing source
        refusing(
            &["nginx_link_site", &format!("{NGINX_SITES_AVAILABLE}/absent"), &link],
            "",
            "a missing site cannot be linked",
        );
    }

    /// 2: exactly one regular file is removed — nginx.conf is the protected
    /// core, directories and symlinks and missing paths are refused, and
    /// everything else is byte-identical afterwards.
    #[test]
    fn nginx_config_remove_deletes_exactly_one_regular_file() {
        let _guard = fixture();
        let config = format!("{NGINX_CONF_D}/10-tmp.conf");
        install(&config, "server {\n    listen 8080;\n}\n");
        assert!(std::path::Path::new(&config).exists());

        let run = run_helper(&["nginx_config_remove", &config], "");
        assert_eq!(run.code, 0, "the removal must succeed: {run:?}");
        assert!(run.stderr.is_empty(), "a success leaves nothing on stderr: {run:?}");
        assert!(run.stdout.is_empty(), "the removal prints nothing: {run:?}");
        assert!(
            !std::path::Path::new(&config).exists(),
            "the removed file must be gone"
        );

        // the protected core is never a removal target — even a working
        // removal elsewhere proved the code path reached it above
        refusing(
            &["nginx_config_remove", NGINX_NGINX_CONF],
            "",
            "the exact nginx.conf is the protected core",
        );

        // no -r semantics, no rm -f semantics
        let subdir = format!("{NGINX_SITES_AVAILABLE}/subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        refusing(
            &["nginx_config_remove", &subdir],
            "",
            "a directory is not a removable target",
        );
        assert!(std::path::Path::new(&subdir).exists(), "the directory must stay");
        refusing(
            &["nginx_config_remove", &format!("{NGINX_SITES_AVAILABLE}/absent")],
            "",
            "a missing file is a refusal, not a silent success",
        );

        // a symlinked target is refused, its victim untouched
        let victim = format!("{NGINX_SITES_AVAILABLE}/still-here");
        std::fs::write(&victim, "untouched\n").unwrap();
        let link = format!("{NGINX_SITES_AVAILABLE}/removable-via-link");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        refusing(
            &["nginx_config_remove", &link],
            "",
            "a symlinked target must be refused, not removed through",
        );
        assert_eq!(read(&victim), "untouched\n");
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "the refused removal must not unlink anything"
        );
    }

    #[test]
    fn nginx_config_remove_keeps_other_files() {
        let _guard = fixture();
        let first = format!("{NGINX_SITES_AVAILABLE}/first");
        let second = format!("{NGINX_SITES_AVAILABLE}/second");
        install(&first, "# first\n");
        install(&second, "# second\n");

        let run = run_helper(&["nginx_config_remove", &first], "");
        assert_eq!(run.code, 0, "the targeted removal must succeed: {run:?}");
        assert!(!std::path::Path::new(&first).exists());
        assert_eq!(read(&second), "# second\n", "the neighbor must stay untouched");
    }

    /// 3: only one of this panel's own enabling links is unlinked — the
    /// inverse of `nginx_link_site`.
    #[test]
    fn nginx_unlink_site_removes_only_its_own_link() {
        let _guard = fixture();
        let site = format!("{NGINX_SITES_AVAILABLE}/example.com");
        let link = format!("{NGINX_SITES_ENABLED}/example.com");
        install(&site, "server {\n    listen 80;\n    server_name example.com;\n}\n");

        let run_link = run_helper(&["nginx_link_site", &site, &link], "");
        assert_eq!(run_link.code, 0, "the setup link must succeed: {run_link:?}");
        assert!(std::fs::symlink_metadata(&link).is_ok());

        let run = run_helper(&["nginx_unlink_site", &link], "");
        assert_eq!(run.code, 0, "the unlink must succeed: {run:?}");
        assert!(run.stderr.is_empty(), "a success leaves nothing on stderr: {run:?}");
        assert!(run.stdout.is_empty(), "the unlink prints nothing: {run:?}");
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the unlinked entry must be gone"
        );
        assert_eq!(read(&site), "server {\n    listen 80;\n    server_name example.com;\n}\n");

        // a missing entry is a refusal (strict — see the GROUP 3 report)
        refusing(
            &["nginx_unlink_site", &link],
            "",
            "a missing enabled entry cannot be unlinked",
        );

        // a regular file in sites-enabled is never unlinked as a toggle
        let as_file = format!("{NGINX_SITES_ENABLED}/as-file.conf");
        install(&as_file, "# a regular file, not a link\n");
        refusing(
            &["nginx_unlink_site", &as_file],
            "",
            "a regular file cannot be disabled by unlinking",
        );
        assert_eq!(read(&as_file), "# a regular file, not a link\n");

        // a same-named link pointing away from sites-available is refused —
        // the victim file keeps its content
        let victim = format!("{NGINX_CONF_D}/victim.conf");
        install(&victim, "server { listen 8080; }\n");
        let hijack = format!("{NGINX_SITES_ENABLED}/victim.conf");
        std::os::unix::fs::symlink(&victim, &hijack).unwrap();
        refusing(
            &["nginx_unlink_site", &hijack],
            "",
            "a link that does not point at its own sites-available site is foreign",
        );
        assert!(
            std::fs::symlink_metadata(&hijack).is_ok(),
            "the refused unlink must not remove the link"
        );
        assert_eq!(read(&victim), "server { listen 8080; }\n");

        // a renamed link into sites-available is also foreign (it must point
        // at exactly <same name> in sites-available)
        let other = format!("{NGINX_SITES_AVAILABLE}/other");
        std::fs::write(other.as_str(), "# other\n").unwrap();
        let renamed = format!("{NGINX_SITES_ENABLED}/example.com");
        std::os::unix::fs::symlink(&other, &renamed).unwrap();
        refusing(
            &["nginx_unlink_site", &renamed],
            "",
            "a link to another site's definition is not this link",
        );
        assert!(std::fs::symlink_metadata(&renamed).is_ok());
    }
}

// ── nginx web-root actions ───────────────────────────────────────────────────
//
// GROUP 4 of the nginx batch: the `nginx_www_fs` verbs (mkdir, remove,
// rename) and `nginx_www_install` — every path strictly below the pinned web
// root, per-component no-follow walking, protected names, and a byte-exact
// binary-tolerant install with its size cap and forced mode/owner.

#[cfg(feature = "test-fixture")]
mod nginx_www_dispatch {
    use root_policy::{NGINX_WWW_INSTALL_MAX, VAR_WWW_DIR};

    use super::*;

    /// Run the www_fs action: `bin(&["mkdir", path])`.
    fn www(parts: &[&str]) -> Run {
        let mut whole = vec!["nginx_www_fs"];
        whole.extend_from_slice(parts);
        run_helper(&whole, "")
    }

    fn ok(parts: &[&str]) {
        let run = www(parts);
        assert_eq!(run.code, 0, "{parts:?} must succeed: {run:?}");
        assert!(run.stderr.is_empty(), "a success leaves nothing on stderr: {run:?}");
        assert!(run.stdout.is_empty(), "no www verb prints anything: {run:?}");
    }

    fn read(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    }

    fn read_bytes(path: &str) -> Vec<u8> {
        std::fs::read(path).unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    }

    fn mode_of(path: &str) -> u32 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).unwrap().mode() & 0o7777
    }

    /// 1: mkdir creates a hierarchy at 0755, refuses nothing lightly —
    /// symlinked and non-directory components are walked around-by-refusal.
    #[test]
    fn nginx_www_fs_mkdir_creates_a_hierarchy_deep() {
        let _guard = fixture();
        let app = format!("{VAR_WWW_DIR}/app");
        let nested = format!("{app}/modules/deep");

        ok(&["mkdir", &nested]);
        use std::os::unix::fs::MetadataExt;
        assert!(
            std::path::Path::new(&nested).is_dir()
                && deep_dirs(&app, &["modules", "deep"]),
            "the whole nested hierarchy must exist"
        );
        assert_eq!(
            std::fs::metadata(&nested).unwrap().mode() & 0o777,
            0o755,
            "the created directory is 0755"
        );
        assert_eq!(
            std::fs::metadata(&nested).unwrap().uid(),
            rustix::process::getuid().as_raw(),
            "the fixture dirs must be owned by the privileged identity"
        );

        // creating again over the existing tree succeeds — mkdir of an
        // existing directory is a no-op success
        ok(&["mkdir", &nested]);

        // a component that exists as a file blocks the deeper dirs
        std::fs::write(&format!("{VAR_WWW_DIR}/blocker"), "x\n").unwrap();
        refusing(
            &["nginx_www_fs", "mkdir", &format!("{VAR_WWW_DIR}/blocker/into")],
            "",
            "a file cannot be a path component",
        );

        // a symlinked component refuses, the victim stays
        let victim = format!("{VAR_WWW_DIR}/app/real-dir");
        std::fs::create_dir_all(&victim).unwrap();
        std::os::unix::fs::symlink(&victim, &format!("{VAR_WWW_DIR}/app/link")).unwrap();
        refusing(
            &["nginx_www_fs", "mkdir", &format!("{VAR_WWW_DIR}/app/link/into")],
            "",
            "a symlinked component must be refused, not followed",
        );
        assert!(
            std::fs::read_dir(&victim).unwrap().next().is_none(),
            "the victim directory must stay empty"
        );
    }

    fn deep_dirs(root: &str, names: &[&str]) -> bool {
        let mut current = std::path::PathBuf::from(root);
        for name in names {
            current = current.join(name);
        }
        current.is_dir()
    }

    /// 2: install carries binary content byte-exact with its forced mode
    /// and owner, refuses oversized content, symlinked and non-regular
    /// destinations, and never receives a source path.
    #[test]
    fn nginx_www_install_writes_binary_content_exactly() {
        let _guard = fixture();
        let inside_html = format!("{VAR_WWW_DIR}/html/logo.png");
        // real binary-ish payload: NUL, high bytes, a long run
        let mut payload = vec![0u8, 0x89, b'P', b'N', b'G', 0xff, 0xfe];
        payload.extend(std::iter::repeat_n(0xa5u8, 4096));
        let mut child = run_with_stdin_bytes(
            &["nginx_www_install", inside_html.as_str()],
            &payload,
        )
        .unwrap_or_else(|error| panic!("unexpected spawn error {error}"));
        assert_eq!(child.code, 0, "the install must succeed: {child:?}");
        assert!(child.stderr.is_empty(), "a success leaves nothing on stderr: {child:?}");
        assert!(child.stdout.is_empty(), "the install prints nothing: {child:?}");
        assert_eq!(read_bytes(&inside_html), payload, "binary content must be byte-exact");
        use std::os::unix::fs::MetadataExt;
        assert_eq!(mode_of(&inside_html) & 0o777, 0o644, "the web file must be 0644");
        assert_eq!(
            std::fs::metadata(&inside_html).unwrap().uid(),
            rustix::process::getuid().as_raw(),
            "the fixture owner must be the privileged identity"
        );

        // rewriting replaces the file, again byte-exact, again 0644
        let second = vec![1u8, 2, 3];
        child = run_with_stdin_bytes(&["nginx_www_install", inside_html.as_str()], &second)
            .unwrap();
        assert_eq!(child.code, 0);
        assert_eq!(read_bytes(&inside_html), second);
        assert_eq!(mode_of(&inside_html) & 0o777, 0o644);

        // an oversized payload is capped (limit + 1 byte through stdin)
        let huge = vec![b'x'; NGINX_WWW_INSTALL_MAX + 1];
        child = run_with_stdin_bytes(&["nginx_www_install", inside_html.as_str()], &huge).unwrap();
        assert_eq!(child.code, 2, "oversized content must be refused: {child:?}");
        assert!(
            child.stderr.starts_with("lcp-admin:"),
            "the refusal must come from the helper itself: {child:?}"
        );
        assert_eq!(read_bytes(&inside_html), second, "a refused install leaves the file untouched");

        // exactly-at-cap content still passes
        let exact = vec![b'y'; NGINX_WWW_INSTALL_MAX];
        child = run_with_stdin_bytes(&["nginx_www_install", inside_html.as_str()], &exact).unwrap();
        assert_eq!(child.code, 0, "content exactly at the cap must pass: {child:?}");

        // a symlinked destination refuses, never written through
        ok(&["mkdir", &format!("{VAR_WWW_DIR}/app")]);
        let victim = format!("{VAR_WWW_DIR}/app/real.png");
        std::fs::write(&victim, b"real bytes\n").unwrap();
        let link = format!("{VAR_WWW_DIR}/app/linked.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        child = run_with_stdin_bytes(&["nginx_www_install", link.as_str()], b"evil").unwrap();
        assert_eq!(child.code, 2, "a symlinked destination must be refused: {child:?}");
        assert_eq!(read_bytes(&victim), b"real bytes\n".to_vec());

        // a destination in the directory position refuses
        let app_dir = format!("{VAR_WWW_DIR}/app");
        child = run_with_stdin_bytes(&["nginx_www_install", app_dir.as_str()], b"evil").unwrap();
        assert_eq!(child.code, 2, "a directory destination must be refused: {child:?}");

        // a missing parent directory refuses (no auto-mkdir on install)
        child =
            run_with_stdin_bytes(&["nginx_www_install", "/nowhere-under-root/x"], b"x").unwrap();
        assert_eq!(child.code, 2, "an invalid parent must be refused: {child:?}");
        let missing_parent = format!("{VAR_WWW_DIR}/absent-parent/file.txt");
        child =
            run_with_stdin_bytes(&["nginx_www_install", missing_parent.as_str()], b"x").unwrap();
        assert_eq!(child.code, 2, "a missing parent must be refused: {child:?}");
        assert!(!std::path::Path::new(&missing_parent).exists());
    }

    /// 3: remove deletes files and whole directories but never the
    /// protected names, and refuses symlinked components anywhere.
    #[test]
    fn nginx_www_fs_remove_deletes_files_and_directories() {
        let _guard = fixture();
        ok(&["mkdir", &format!("{VAR_WWW_DIR}/app")]);

        // a plain file
        let file = format!("{VAR_WWW_DIR}/app/temp.txt");
        std::fs::write(&file, "delete me\n").unwrap();
        ok(&["remove", &file]);
        assert!(!std::path::Path::new(&file).exists());

        // a whole directory tree, deep
        let tree = format!("{VAR_WWW_DIR}/app/nested/deeper");
        construct_tree(&tree, &["img", "css"], &["index.html", "style.css"]);
        ok(&["remove", &format!("{VAR_WWW_DIR}/app/nested")]);
        assert!(!std::path::Path::new(&format!("{VAR_WWW_DIR}/app/nested")).exists());

        // the protected names refuse — a file may be removed *inside* html,
        // but html itself and the web root are never removal targets
        let inside = format!("{VAR_WWW_DIR}/html/logo.png");
        let seeded = run_with_stdin_bytes(&["nginx_www_install", inside.as_str()], b"inside\n")
            .unwrap_or_else(|error| panic!("unexpected spawn error {error}"));
        assert_eq!(seeded.code, 0, "install inside html must succeed: {seeded:?}");
        refusing(
            &["nginx_www_fs", "remove", &format!("{VAR_WWW_DIR}/html")],
            "",
            "the distro-managed html tree cannot be removed",
        );
        assert!(std::path::Path::new(&format!("{VAR_WWW_DIR}/html")).is_dir());
        refusing(
            &["nginx_www_fs", "remove", &format!("{VAR_WWW_DIR}/html/")],
            "",
            "the root itself cannot be removed",
        );
        // yet files *inside* it are ordinary removable content
        ok(&["remove", &inside]);
        assert!(!std::path::Path::new(&inside).exists());

        // a symlinked target or symlinked component refuses, victim keeps
        let victim = format!("{VAR_WWW_DIR}/app/real.png");
        std::fs::write(&victim, "keep\n").unwrap();
        let link = format!("{VAR_WWW_DIR}/app/linked.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        refusing(
            &["nginx_www_fs", "remove", &link],
            "",
            "a symlinked target refuses instead of being unlinked",
        );
        assert_eq!(read(&victim), "keep\n");

        let walk_victim = format!("{VAR_WWW_DIR}/app/target-dir");
        std::fs::create_dir_all(&walk_victim).unwrap();
        std::os::unix::fs::symlink(&walk_victim, &format!("{VAR_WWW_DIR}/app/walker")).unwrap();
        refusing(
            &["nginx_www_fs", "remove", &format!("{VAR_WWW_DIR}/app/walker/child")],
            "",
            "a symlinked walk component refuses",
        );
        assert!(std::path::Path::new(&walk_victim).is_dir(), "the walked victim stays empty");

        // a missing path refuses (no rm -f semantics)
        refusing(
            &["nginx_www_fs", "remove", &format!("{VAR_WWW_DIR}/app/absent")],
            "",
            "a missing path cannot be removed",
        );
    }

    /// 4: rename moves files and directories inside the web root, when both
    /// endpoints are ordinary names and the destination is free.
    #[test]
    fn nginx_www_fs_rename_moves_inside_the_web_root() {
        let _guard = fixture();
        ok(&["mkdir", &format!("{VAR_WWW_DIR}/app")]);

        // a plain file within one directory
        let from = format!("{VAR_WWW_DIR}/app/old.txt");
        std::fs::write(&from, "content\n").unwrap();
        let to = format!("{VAR_WWW_DIR}/app/new.txt");
        ok(&["rename", &from, &to]);
        assert!(!std::path::Path::new(&from).exists());
        assert_eq!(read(&to), "content\n");

        // across directories (two different pinned parents)
        let other = format!("{VAR_WWW_DIR}/other");
        std::fs::create_dir_all(&other).unwrap();
        let far = format!("{other}/arrived.txt");
        ok(&["rename", &to, &far]);
        assert!(!std::path::Path::new(&to).exists());
        assert_eq!(read(&far), "content\n");

        // a directory rename keeps its children
        let dir_from = format!("{VAR_WWW_DIR}/app/site");
        std::fs::create_dir_all(&format!("{dir_from}/assets")).unwrap();
        std::fs::write(&format!("{dir_from}/assets/a.css"), "css\n").unwrap();
        let dir_to = format!("{VAR_WWW_DIR}/other/renamed");
        ok(&["rename", &dir_from, &dir_to]);
        assert_eq!(
            read(&format!("{dir_to}/assets/a.css")),
            "css\n",
            "the renamed directory keeps its children"
        );

        // refusals: over an existing destination, protected endpoints,
        // symlinked components, missing source
        refusing(
            &["nginx_www_fs", "rename", &far, &far],
            "",
            "the destination exists",
        );
        refusing(
            &["nginx_www_fs", "rename", &format!("{VAR_WWW_DIR}/other"), &format!("{VAR_WWW_DIR}/html")],
            "",
            "rename into the html tree is protected",
        );
        refusing(
            &["nginx_www_fs", "rename", &format!("{VAR_WWW_DIR}/html"), &format!("{VAR_WWW_DIR}/moved")],
            "",
            "rename away from the html tree is protected",
        );
        let victim = format!("{VAR_WWW_DIR}/app/victim.png");
        std::fs::write(&victim, "real\n").unwrap();
        let link = format!("{VAR_WWW_DIR}/app/link.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        refusing(
            &["nginx_www_fs", "rename", &link, &format!("{VAR_WWW_DIR}/other/x.png")],
            "",
            "a symlinked source refuses",
        );
        refusing(
            &["nginx_www_fs", "rename", &victim, &link],
            "",
            "a symlinked destination refuses",
        );
        assert_eq!(read(&victim), "real\n");
        refusing(
            &["nginx_www_fs", "rename", &format!("{VAR_WWW_DIR}/app/absent"), &format!("{VAR_WWW_DIR}/other/y")],
            "",
            "a missing source cannot be renamed",
        );
        refusing(
            &["nginx_www_fs", "rename", &far, &format!("{VAR_WWW_DIR}/app/absent-parent/z")],
            "",
            "a missing destination parent cannot receive a rename",
        );
    }

    fn construct_tree(root: &str, dirs: &[&str], files: &[&str]) {
        for dir in dirs {
            std::fs::create_dir_all(format!("{root}/{dir}")).unwrap();
        }
        for file in files {
            let parent = std::path::Path::new(root)
                .join(file)
                .parent()
                .unwrap()
                .to_path_buf();
            std::fs::create_dir_all(parent).unwrap();
            std::fs::write(format!("{root}/{file}"), format!("content of {file}\n")).unwrap();
        }
    }

    /// Byte-exact stdin runner for the install verb's payload runs.
    fn run_with_stdin_bytes(args: &[&str], input: &[u8]) -> std::io::Result<Run> {
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
}
