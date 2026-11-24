//! Fixture tests for the `pkg_maintenance` named action. With the
//! compile-time `test-fixture` feature the helper's package-manager, df and
//! lock paths are swapped to stubs/control files below
//! `/tmp/lcp-admin-test-fixture`; no real package transaction, lock file or
//! disk is touched.
//!
//! The stubs record every argv line in `log/calls.log`, so the tests pin the
//! exact command sequence (preview before autoremove, never apt after dnf).

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
const DNF_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/dnf";
const APT_GET_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/apt-get";
const DF_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/df";
const UNAME_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/uname";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";

const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");



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

fn fixture(with_stubs: bool) -> std::fs::File {
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
    std::fs::create_dir_all(format!("{FIXTURE_ROOT}/ctl")).unwrap();
    if with_stubs {
        const DNF_STUB: &str = r#"#!/bin/sh
LOG="/tmp/lcp-admin-test-fixture/log/calls.log"
CTL="/tmp/lcp-admin-test-fixture/ctl"
printf '%s\n' "$*" >> "$LOG"
case " $* " in
*' history info '*)
  echo 'Transaction ID : 42'
  echo 'Packages Altered:'
  if [ -f "$CTL/history-packages" ]; then
    while IFS= read -r package; do echo "    Install $package @fedora"; done < "$CTL/history-packages"
  else
    echo '    Install nano-8.0-1.fc40.x86_64 @fedora'
  fi
  exit 0;;
*' autoremove --assumeno '*)
  if [ -e "$CTL/autoremove-protected" ]; then
    echo 'Removing unused dependencies:'
    echo ' kernel-core-6.8.5-301.fc40.x86_64 x86_64 1.0 M'
    echo 'Operation aborted.' >&2
    exit 1
  fi
  echo 'Nothing to do.'
  exit 0;;
esac
if [ -e "$CTL/fail" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi
exit 0
"#;
        const APT_STUB: &str = "#!/bin/sh\n\
            LOG=\"/tmp/lcp-admin-test-fixture/log/calls.log\"\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            printf '%s\\n' \"$*\" >> \"$LOG\"\n\
            case \" $* \" in\n\
            *' -s autoremove '*)\n\
              if [ -e \"$CTL/apt-autoremove-protected\" ]; then\n\
                echo 'Remv kernel-core [6.8.5-301.fc40.x86_64]'\n\
              fi\n\
              exit 0;;\n\
            esac\n\
            if [ -e \"$CTL/fail\" ]; then echo 'BY-STUB: injected failure' >&2; exit 9; fi\n\
            exit 0\n";
        const DF_STUB: &str = "#!/bin/sh\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            if [ -e \"$CTL/low-disk\" ]; then printf 'Avail\\n1024\\n'; else printf 'Avail\\n1073741824\\n'; fi\n\
            exit 0\n";
        const UNAME_STUB: &str = "#!/bin/sh\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            if [ -e \"$CTL/uname-slow\" ]; then sleep 6; fi\n\
            if [ -f \"$CTL/kernel-release\" ]; then cat \"$CTL/kernel-release\"; else echo '6.8.5-301.fc40.x86_64'; fi\n";
        use std::os::unix::fs::PermissionsExt;
        for (path, body) in [(DNF_BIN, DNF_STUB), (APT_GET_BIN, APT_STUB), (DF_BIN, DF_STUB), (UNAME_BIN, UNAME_STUB)] {
            std::fs::write(path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    lock_file
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
fn clean_dnf_uses_clean_all_only() {
    let _guard = fixture(true);
    let run = run_helper(&["pkg_maintenance", "clean"]);
    assert_eq!(run.code, 0, "clean must succeed: {run:?}");
    assert_eq!(calls(), vec!["clean all"], "dnf clean all only: {:?}", calls());
}

#[test]
fn makecache_dnf_uses_makecache_only() {
    let _guard = fixture(true);
    let run = run_helper(&["pkg_maintenance", "makecache"]);
    assert_eq!(run.code, 0, "makecache must succeed: {run:?}");
    assert_eq!(calls(), vec!["makecache"], "dnf makecache only: {:?}", calls());
}

#[test]
fn autoremove_dnf_previews_then_applies() {
    let _guard = fixture(true);
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_eq!(run.code, 0, "autoremove must succeed: {run:?}");
    assert_eq!(
        calls(),
        vec!["autoremove --assumeno", "autoremove -y"],
        "preview first, then the real transaction: {:?}",
        calls()
    );
}

#[test]
fn dnf_failure_is_not_retried_on_apt() {
    let _guard = fixture(true);
    set_ctl("fail");
    let run = run_helper(&["pkg_maintenance", "clean"]);
    assert_ne!(run.code, 0, "a dnf failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected failure"),
        "stderr must be preserved: {run:?}"
    );
    assert_eq!(calls(), vec!["clean all"], "apt must never follow dnf: {:?}", calls());
}

#[test]
fn apt_clean_used_when_dnf_absent() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    let run = run_helper(&["pkg_maintenance", "clean"]);
    assert_eq!(run.code, 0, "apt clean must succeed: {run:?}");
    assert_eq!(calls(), vec!["clean"], "apt-get clean: {:?}", calls());
}

#[test]
fn apt_makecache_uses_update_when_dnf_absent() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    let run = run_helper(&["pkg_maintenance", "makecache"]);
    assert_eq!(run.code, 0, "apt update must succeed: {run:?}");
    assert_eq!(calls(), vec!["update"], "apt-get update: {:?}", calls());
}

#[test]
fn apt_autoremove_previews_then_applies() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_eq!(run.code, 0, "apt autoremove must succeed: {run:?}");
    assert_eq!(
        calls(),
        vec!["-s autoremove", "autoremove -y"],
        "simulation first, then the real transaction: {:?}",
        calls()
    );
}

#[test]
fn no_package_manager_is_refused() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    let _ = std::fs::remove_file(APT_GET_BIN);
    let run = run_helper(&["pkg_maintenance", "clean"]);
    assert_ne!(run.code, 0, "no backend must be refused: {run:?}");
    assert!(run.stderr.contains("neither dnf nor apt-get"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn dnf_autoremove_protected_package_is_hard_blocked() {
    let _guard = fixture(true);
    set_ctl("autoremove-protected");
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_ne!(run.code, 0, "protected removals must block: {run:?}");
    assert!(
        run.stderr.contains("kernel-core") && run.stderr.contains("refusing"),
        "the refusal must name the protected package: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["autoremove --assumeno"],
        "the real autoremove must never run: {:?}",
        calls()
    );
}

#[test]
fn apt_autoremove_protected_package_is_hard_blocked() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    set_ctl("apt-autoremove-protected");
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_ne!(run.code, 0, "protected removals must block: {run:?}");
    assert!(
        run.stderr.contains("kernel-core") && run.stderr.contains("refusing"),
        "the refusal must name the protected package: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["-s autoremove"],
        "the real autoremove must never run: {:?}",
        calls()
    );
}

#[test]
fn autoremove_lock_refuses_before_any_command() {
    let _guard = fixture(true);
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/pkg-lock"), "1").unwrap();
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_ne!(run.code, 0, "a live lock must block: {run:?}");
    assert!(
        run.stderr.contains("locked") && run.stderr.contains("PID 1"),
        "the lock owner must be reported: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn autoremove_low_disk_refuses_before_any_command() {
    let _guard = fixture(true);
    set_ctl("low-disk");
    let run = run_helper(&["pkg_maintenance", "autoremove"]);
    assert_ne!(run.code, 0, "low disk must block: {run:?}");
    assert!(run.stderr.contains("insufficient disk space"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn invalid_and_wrong_shaped_operations_are_refused() {
    let _guard = fixture(true);
    for op in ["update", "Clean", "remove", ""] {
        let run = run_helper(&["pkg_maintenance", op]);
        assert_eq!(run.code, 2, "invalid op {op:?} must be refused: {run:?}");
    }
    let missing = run_helper(&["pkg_maintenance"]);
    assert_eq!(missing.code, 2, "missing op: {missing:?}");
    let extra = run_helper(&["pkg_maintenance", "clean", "extra"]);
    assert_eq!(extra.code, 2, "extra args: {extra:?}");
    assert!(calls().is_empty(), "shape refusals run nothing: {:?}", calls());
}

#[test]
fn non_zero_exit_propagates_for_all_operations() {
    let _guard = fixture(true);
    set_ctl("fail");
    for op in ["clean", "makecache", "autoremove"] {
        let run = run_helper(&["pkg_maintenance", op]);
        assert_ne!(run.code, 0, "{op} must fail on a non-zero exit: {run:?}");
        assert!(
            run.stderr.contains("BY-STUB: injected failure"),
            "{op} must preserve stderr: {run:?}"
        );
    }
}

fn set_ctl_content(name: &str, content: &str) {
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/{name}"), content).unwrap();
}

fn clear_calls() {
    let _ = std::fs::remove_file(LOG);
}

#[test]
fn undo_safe_transaction_previews_then_rolls_back_without_confirmation() {
    let _guard = fixture(true);
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_eq!(run.code, 0, "a safe rollback must succeed: {run:?}");
    assert_eq!(
        calls(),
        vec!["history info 42", "history undo 42 -y"],
        "read-only preview first, then the rollback: {:?}",
        calls()
    );
}

#[test]
fn undo_protected_package_requires_confirmation_and_lists_every_package() {
    let _guard = fixture(true);
    set_ctl_content("history-packages", "glibc-2.39-1.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "a protected rollback must ask first: {run:?}");
    assert!(
        run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "exact prefix required: {run:?}"
    );
    assert!(
        run.stderr.contains("glibc-2.39-1.fc40.x86_64")
            && run.stderr.contains("system-critical package"),
        "the full package list and the specific risk must be shown: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["history info 42"],
        "nothing destructive may run before confirmation: {:?}",
        calls()
    );
    clear_calls();
    let run = run_helper(&["dnf_history_undo", "42", "confirm"]);
    assert_eq!(run.code, 0, "confirmation permits the rollback: {run:?}");
    assert_eq!(
        calls(),
        vec!["history info 42", "history undo 42 -y"],
        "confirmed rollback runs both steps: {:?}",
        calls()
    );
}

#[test]
fn undo_running_kernel_rollback_requires_confirmation() {
    let _guard = fixture(true);
    set_ctl_content("history-packages", "kernel-core-6.8.5-301.fc40.x86_64\n");
    set_ctl_content("kernel-release", "6.8.5-301.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "a kernel rollback must ask first: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("kernel you are currently running")
            && run.stderr.contains("6.8.5-301.fc40.x86_64"),
        "the running kernel must be named: {run:?}"
    );
    assert_eq!(calls(), vec!["history info 42"]);
    clear_calls();
    let run = run_helper(&["dnf_history_undo", "42", "confirm"]);
    assert_eq!(run.code, 0, "confirmation permits the rollback: {run:?}");
}

#[test]
fn undo_kernel_and_protected_risks_are_listed_together() {
    let _guard = fixture(true);
    set_ctl_content(
        "history-packages",
        "kernel-core-6.8.5-301.fc40.x86_64\nglibc-2.39-1.fc40.x86_64\n",
    );
    set_ctl_content("kernel-release", "6.8.5-301.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("kernel you are currently running")
            && run.stderr.contains("glibc-2.39-1.fc40.x86_64")
            && run.stderr.contains("system-critical package"),
        "both risks and both package names must be listed: {run:?}"
    );
    assert_eq!(calls(), vec!["history info 42"]);
}

#[test]
fn undo_with_no_resolved_packages_is_refused() {
    let _guard = fixture(true);
    set_ctl_content("history-packages", "");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "an inconclusive preview must refuse: {run:?}");
    assert!(
        run.stderr.contains("could not determine the package set"),
        "the refusal must explain the missing preview: {run:?}"
    );
    assert_eq!(calls(), vec!["history info 42"]);
}

#[test]
fn undo_invalid_transaction_ids_are_refused() {
    let _guard = fixture(true);
    for id in ["0", "00", "abc", "1.5", "+1", "12345678901"] {
        let run = run_helper(&["dnf_history_undo", id]);
        assert_eq!(run.code, 2, "id {id:?} must be refused: {run:?}");
    }
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn undo_confirmation_shape_is_exact() {
    let _guard = fixture(true);
    for bogus in ["yes", "confirmed", "confirm!", "true"] {
        let run = run_helper(&["dnf_history_undo", "42", bogus]);
        assert_eq!(run.code, 2, "{bogus:?} must be refused: {run:?}");
    }
    let extra = run_helper(&["dnf_history_undo", "42", "confirm", "extra"]);
    assert_eq!(extra.code, 2, "extra arguments: {extra:?}");
    let missing = run_helper(&["dnf_history_undo"]);
    assert_eq!(missing.code, 2, "missing id: {missing:?}");
    assert!(calls().is_empty(), "shape refusals run nothing: {:?}", calls());
}

#[test]
fn undo_refuses_when_dnf_is_absent() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(DNF_BIN);
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "dnf-only action: {run:?}");
    assert!(run.stderr.contains("dnf is not available"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn undo_lock_refuses_before_any_command() {
    let _guard = fixture(true);
    std::fs::write(format!("{FIXTURE_ROOT}/ctl/pkg-lock"), "1").unwrap();
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "a live lock must block: {run:?}");
    assert!(run.stderr.contains("locked") && run.stderr.contains("PID 1"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn undo_low_disk_refuses_before_any_command() {
    let _guard = fixture(true);
    set_ctl("low-disk");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "low disk must block: {run:?}");
    assert!(run.stderr.contains("insufficient disk space"), "{run:?}");
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn undo_non_zero_exit_propagates() {
    let _guard = fixture(true);
    set_ctl("fail");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "a failed rollback must return an error: {run:?}");
    assert!(run.stderr.contains("BY-STUB: injected failure"), "{run:?}");
    assert_eq!(
        calls(),
        vec!["history info 42", "history undo 42 -y"],
        "preview and attempted rollback are both recorded: {:?}",
        calls()
    );
}

#[test]
fn undo_unknown_running_kernel_with_kernel_package_fails_closed() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(UNAME_BIN);
    set_ctl_content("history-packages", "kernel-core-6.8.5-301.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "unknown kernel identity must require confirmation: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("running kernel could not be determined"),
        "the unknown-kernel risk must be named: {run:?}"
    );
    assert_eq!(calls(), vec!["history info 42"]);
    clear_calls();
    let run = run_helper(&["dnf_history_undo", "42", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits it: {run:?}");
}

#[test]
fn undo_unknown_running_kernel_without_kernel_package_proceeds() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(UNAME_BIN);
    // default history package is nano — no kernel involvement
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_eq!(run.code, 0, "a non-kernel rollback needs no confirmation: {run:?}");
    assert!(
        run.stderr.contains("no kernel package is affected"),
        "the warning must be explicit: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["history info 42", "history undo 42 -y"],
        "the rollback still runs: {:?}",
        calls()
    );
}

#[test]
fn undo_empty_uname_output_with_kernel_package_fails_closed() {
    let _guard = fixture(true);
    set_ctl_content("kernel-release", "");
    set_ctl_content("history-packages", "kernel-core-6.8.5-301.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "empty uname output must fail closed: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("running kernel could not be determined"),
        "{run:?}"
    );
}

#[test]
fn undo_timed_out_uname_with_kernel_package_fails_closed() {
    let _guard = fixture(true);
    set_ctl("uname-slow");
    set_ctl_content("history-packages", "kernel-core-6.8.5-301.fc40.x86_64\n");
    let run = run_helper(&["dnf_history_undo", "42"]);
    assert_ne!(run.code, 0, "a timed-out uname must fail closed: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("running kernel could not be determined"),
        "{run:?}"
    );
    assert_eq!(calls(), vec!["history info 42"]);
}

#[test]
fn repo_add_named_action_still_uses_the_validated_package_manager_path() {
    let _guard = fixture(true);
    let run = run_helper(&["repo_add", "https://example.com/example.repo"]);
    assert_eq!(run.code, 0, "named repo_add must still work: {run:?}");
    let calls = calls();
    assert!(
        calls.iter().any(|call| {
            call.contains("config-manager") && call.contains("addrepo")
        }),
        "named repo_add must use the validated dnf path: {calls:?}"
    );
}
