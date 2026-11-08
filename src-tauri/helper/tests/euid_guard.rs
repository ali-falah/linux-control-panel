//! Production helper identity regression: a direct invocation that is not
//! root must be refused before any action runs. The fixture build is the
//! documented exception and runs as the test user by design.

#![cfg(not(feature = "test-fixture"))]

use std::process::Command;

const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");

#[test]
fn a_non_root_direct_invocation_is_refused_before_any_action() {
    // A test suite running as root cannot exercise the refusal; the pure
    // predicate is covered separately in the helper unit tests.
    if rustix::process::geteuid().as_raw() == 0 {
        return;
    }
    let output = Command::new(HELPER)
        .arg("admin_keep")
        .output()
        .expect("the helper binary must be runnable");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a non-root helper invocation must exit 2: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("must run as root"),
        "the refusal must explain the identity requirement: {stderr}"
    );
}
