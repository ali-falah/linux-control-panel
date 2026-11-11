//! Fixture tests for the `firewall_zone_rule` and `firewall_rich_rule`
//! named actions, in the same spirit as `fixture_dispatch.rs`: with the
//! compile-time `test-fixture` feature, the helper's firewall tool
//! constants are swapped to stub binaries below
//! `/tmp/lcp-admin-test-fixture/bin` that merely record the argv they
//! receive (one full command line per line in `log/calls.log`) and exit
//! according to control files under `ctl/`. No real firewall is touched.
//!
//! Because the stub path is a literal in the helper, these tests also pin
//! something the real strings cannot: *which binary the helper actually
//! executed and with what argv and in which order*.

#![cfg(feature = "test-fixture")]

use std::process::Command;

const FIXTURE_ROOT: &str = "/tmp/lcp-admin-test-fixture";
// The same value root_policy/main.rs concede to the fixture feature.
const FIREWALL_CMD_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/firewall-cmd";
const UFW_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/ufw";
const SYSTEMCTL_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/systemctl";
const LOG: &str = "/tmp/lcp-admin-test-fixture/log/calls.log";

const HELPER: &str = env!("CARGO_BIN_EXE_lcp-admin");
// Same value the helper concedes to the test-fixture feature.
const SS_BIN: &str = "/tmp/lcp-admin-test-fixture/bin/ss";



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

/// Take the fixture lock, wipe and rebuild the fixture tree (optionally
/// with the stub binaries so the helper "finds" a backend), then return
/// the guard that serializes every test against the shared root.
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
        const STUB: &str = "#!/bin/sh\n\
            LOG=\"/tmp/lcp-admin-test-fixture/log/calls.log\"\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            printf '%s\\n' \"$*\" >> \"$LOG\"\n\
            case \" $* \" in\n\
            *' --permanent '*) if [ -e \"$CTL/fail-permanent\" ]; then echo 'BY-STUB: injected permanent failure' >&2; exit 9; fi;;\n\
            *' --query-service=ssh'*) if [ -e \"$CTL/zone-no-ssh\" ]; then echo 'BY-STUB: zone does not permit ssh' >&2; exit 1; fi;;\n\
            *' --force enable '*) if [ -e \"$CTL/fail-ufw-enable\" ]; then echo 'BY-STUB: injected ufw enable failure' >&2; exit 9; fi;;\n\
            *) if [ -e \"$CTL/fail-runtime\" ]; then echo 'BY-STUB: injected runtime failure' >&2; exit 9; fi;;\n\
            esac\n";
        for bin in [FIREWALL_CMD_BIN, UFW_BIN] {
            std::fs::write(bin, STUB).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // `ss` stub for the helper's SSH-session detection. Default: no
        // active session. ctl/ss-session: one established SSH session.
        // ctl/ss-inconclusive: the probe fails (exit 1), which is an
        // inconclusive answer per the approved fail-open policy.
        // ctl/zone-no-ssh: the firewall-cmd --query-service=ssh probe
        // reports that the destination zone does not permit ssh.
        const SS_STUB: &str = "#!/bin/sh\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            printf 'ss %s\\n' \"$*\" >> \"/tmp/lcp-admin-test-fixture/log/ss.log\"\n\
            if [ -e \"$CTL/ss-inconclusive\" ]; then echo 'stub: probe unavailable' >&2; exit 1; fi\n\
            if [ -e \"$CTL/ss-session\" ]; then\n\
            if [ -e \"$CTL/ss-nomarker\" ]; then printf 'ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342\\n';\n\
            else printf 'ESTAB 0 0 192.168.0.103%%wlp1s0:22 10.0.0.5:53342\\n'; fi; fi\n";
        std::fs::write(SS_BIN, SS_STUB).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(SS_BIN, std::fs::Permissions::from_mode(0o755)).unwrap();
        // `systemctl` stub for the firewalld hardening action. Default: ok.
        // ctl/fail-systemctl: the unit cannot be enabled.
        const SYSTEMCTL_STUB: &str = "#!/bin/sh\n\
            LOG=\"/tmp/lcp-admin-test-fixture/log/calls.log\"\n\
            CTL=\"/tmp/lcp-admin-test-fixture/ctl\"\n\
            printf '%s\\n' \"$*\" >> \"$LOG\"\n\
            if [ -e \"$CTL/fail-systemctl\" ]; then echo 'BY-STUB: injected systemctl failure' >&2; exit 9; fi\n";
        std::fs::write(SYSTEMCTL_BIN, SYSTEMCTL_STUB).unwrap();
        std::fs::set_permissions(SYSTEMCTL_BIN, std::fs::Permissions::from_mode(0o755)).unwrap();
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

/// How many times the helper ran the real `ss` SSH-session probe during
/// this action. GROUP 4 requires exactly one probe per interface-zone
/// action: a second probe could disagree with the first and silently clear
/// a detected session.
fn ss_probes() -> usize {
    std::fs::read_to_string(format!("{FIXTURE_ROOT}/log/ss.log"))
        .unwrap_or_default()
        .lines()
        .count()
}

// ── firewall_zone_rule on firewalld ──────────────────────────────────────────

#[test]
fn zone_rule_firewalld_add_service_runs_runtime_then_permanent_with_zone_flag() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_zone_rule", "add", "public", "service", "http"]);
    assert_eq!(run.code, 0, "add must succeed: {run:?}");
    assert!(run.stderr.is_empty(), "success prints nothing: {run:?}");

    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=public --add-service=http",
            "--zone=public --add-service=http --permanent",
        ],
        "runtime first, then permanent; zone always as --zone=<value>: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_remove_port_runs_runtime_then_permanent_with_zone_flag() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_zone_rule", "remove", "FedoraWorkstation", "port", "8080/tcp"]);
    assert_eq!(run.code, 0, "remove must succeed: {run:?}");

    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=FedoraWorkstation --remove-port=8080/tcp",
            "--zone=FedoraWorkstation --remove-port=8080/tcp --permanent",
        ],
        "remove ports uses --remove-port in both phases: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_add_port_uses_add_port_flag() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_zone_rule", "add", "public", "port", "1025-65535/udp"]);
    assert_eq!(run.code, 0, "add port must succeed: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=public --add-port=1025-65535/udp",
            "--zone=public --add-port=1025-65535/udp --permanent",
        ],
        "port range passes through unchanged: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_remove_service_uses_remove_service_flag() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_zone_rule", "remove", "home", "service", "ssh"]);
    assert_eq!(run.code, 0, "remove service must succeed: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=home --remove-service=ssh",
            "--zone=home --remove-service=ssh --permanent",
        ],
        "service removal argv pinned: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_runtime_failure_stops_before_permanent() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");

    let run = run_helper(&["firewall_zone_rule", "add", "public", "service", "http"]);
    assert_ne!(run.code, 0, "a runtime failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "the tool's own stderr must reach the caller: {run:?}"
    );

    let calls = calls();
    assert_eq!(
        calls,
        vec!["--zone=public --add-service=http"],
        "the permanent step must never run after a runtime failure: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_permanent_failure_reports_the_partial_state() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_zone_rule", "add", "public", "service", "http"]);
    assert_ne!(run.code, 0, "a permanent failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected permanent failure"),
        "the permanent step's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("runtime"),
        "the partial result must say what succeeded at runtime: {run:?}"
    );

    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=public --add-service=http",
            "--zone=public --add-service=http --permanent",
        ],
        "both steps were attempted: {:?}", calls);
}

#[test]
fn zone_rule_firewalld_both_steps_fail_reports_only_the_runtime_attempt() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_zone_rule", "add", "public", "service", "http"]);
    assert_ne!(run.code, 0, "both failing must fail the action: {run:?}");
    assert!(
        run.stderr.contains("runtime failure"),
        "the runtime error is the one to report: {run:?}"
    );
    assert!(
        !run.stderr.contains("permanent failure"),
        "the permanent step was never attempted: {run:?}"
    );
}

#[test]
fn zone_rule_firewalld_remove_failure_does_not_claim_a_rollback() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "service", "http"]);
    assert_ne!(run.code, 0);
    // Rollback is out of scope: the wording must never promise one.
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
}

// ── firewall_zone_rule on ufw ────────────────────────────────────────────────

#[test]
fn zone_rule_ufw_add_refuses_a_named_zone_explicitly() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    // ufw has no firewalld-style zones: a named zone must never be silently
    // dropped or implied to have been modified.
    let run = run_helper(&["firewall_zone_rule", "add", "public", "port", "8080/tcp"]);
    assert_ne!(run.code, 0, "a named zone cannot be honored by ufw: {run:?}");
    assert!(
        run.stderr.contains("ufw has no firewalld-style zones"),
        "the refusal must explain the zone concept: {run:?}"
    );
    assert!(
        run.stderr.contains("No rule was changed"),
        "the refusal must state that nothing was changed: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "a refused zone request must not run any tool: {:?}",
        calls()
    );
}

#[test]
fn zone_rule_ufw_add_translates_to_allow_without_zones() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    let run = run_helper(&["firewall_zone_rule", "add", "", "port", "8080/tcp"]);
    assert_eq!(run.code, 0, "ufw add must succeed: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec!["allow 8080/tcp"],
        "ufw receives a single allow command: {:?}", calls);
}

#[test]
fn zone_rule_ufw_remove_translates_to_delete_allow() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN);

    let run = run_helper(&["firewall_zone_rule", "remove", "", "service", "ssh"]);
    assert_eq!(run.code, 0, "ufw remove must succeed: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec!["delete allow ssh"],
        "ufw removal is `delete allow <value>`: {:?}", calls);
}

#[test]
fn zone_rule_ufw_failure_preserves_stderr() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN);
    set_ctl("fail-runtime"); // ufw is treated as the runtime step

    let run = run_helper(&["firewall_zone_rule", "add", "", "port", "8080/tcp"]);
    assert_ne!(run.code, 0, "a ufw failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "ufw's stderr must be preserved: {run:?}"
    );
}

// ── validation refusals (nothing is executed) ────────────────────────────────

#[test]
fn zone_rule_rejects_an_invalid_zone_before_any_execution() {
    let _guard = fixture(true);

    for zone in ["../etc", "-oProxyCommand=evil", "", "a b", "hidden;" ] {
        let run = run_helper(&["firewall_zone_rule", "add", zone, "service", "http"]);
        assert_eq!(
            run.code, 2,
            "a bad zone must be refused with exit 2: {zone:?} — {run:?}"
        );
        assert!(
            run.stderr.starts_with("lcp-admin:"),
            "the refusal must come from the helper itself: {run:?}"
        );
    }
    assert!(
        calls().is_empty(),
        "a refused action must not run any tool: {:?}",
        calls()
    );
}

#[test]
fn zone_rule_rejects_an_invalid_service_and_port_before_any_execution() {
    let _guard = fixture(true);

    for value in ["a b", "-mfoo", "ssh;reboot", "x/y/z", "UPPER"] {
        let run = run_helper(&["firewall_zone_rule", "add", "public", "service", value]);
        assert_eq!(run.code, 2, "bad service must be refused: {value:?} — {run:?}");
    }
    // port values always carry the protocol part
    for value in ["not-a-port", "22/icmp", "99999/tcp", "" ] {
        let run = run_helper(&["firewall_zone_rule", "add", "public", "port", value]);
        assert_eq!(run.code, 2, "bad port must be refused: {value:?} — {run:?}");
    }
    assert!(
        calls().is_empty(),
        "no refused shape may reach a tool: {:?}",
        calls()
    );
}

#[test]
fn zone_rule_rejects_unknown_kind_verb_and_wrong_operand_count() {
    let _guard = fixture(true);

    let kind = run_helper(&["firewall_zone_rule", "add", "public", "rich-rule", "x"]);
    assert_eq!(kind.code, 2, "kind must be service or port: {kind:?}");

    let verb = run_helper(&["firewall_zone_rule", "toggle", "public", "service", "ssh"]);
    assert_eq!(verb.code, 2, "verb must be add or remove: {verb:?}");

    let count = run_helper(&["firewall_zone_rule", "add", "public", "service"]);
    assert_eq!(count.code, 2, "missing operand must be refused: {count:?}");

    let extra = run_helper(&["firewall_zone_rule", "add", "public", "service", "ssh", "extra"]);
    assert_eq!(extra.code, 2, "an extra operand must be refused: {extra:?}");

    assert!(
        calls().is_empty(),
        "shape refusals must not reach any tool: {:?}",
        calls()
    );
}

#[test]
fn zone_rule_option_injection_cannot_pass_as_a_value() {
    let _guard = fixture(true);

    // A value starting with '-' would become a firewall-cmd option if any
    // validator slipped; every variant must be refused before execution.
    for (kind, value) in [
        ("service", "--query-service=ssh"),
        ("port", "--permanent"),
        ("service", "--"),
    ] {
        let run = run_helper(&["firewall_zone_rule", "add", "public", kind, value]);
        assert_eq!(
            run.code, 2,
            "an option-like operand must be refused: {kind} {value:?} — {run:?}"
        );
    }
    assert!(
        calls().is_empty(),
        "option injection must never reach the tool: {:?}",
        calls()
    );
}

#[test]
fn zone_rule_reports_when_no_backend_is_available() {
    let _guard = fixture(false); // neither stub installed

    let run = run_helper(&["firewall_zone_rule", "add", "public", "service", "ssh"]);
    assert_ne!(run.code, 0, "no backend must fail: {run:?}");
    assert!(
        run.stderr.contains("neither firewalld nor ufw"),
        "the error must name both tried backends: {run:?}"
    );
    assert!(calls().is_empty(), "nothing ran: {:?}", calls());
}

#[test]
fn rich_rule_reports_when_no_backend_is_available() {
    let _guard = fixture(false);

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" accept"#,
    ]);
    assert_ne!(run.code, 0, "no backend must fail: {run:?}");
    assert!(
        run.stderr.contains("firewalld"),
        "the error must mention firewalld: {run:?}"
    );
    assert!(calls().is_empty(), "nothing ran: {:?}", calls());
}

// ── firewall_rich_rule on firewalld ─────────────────────────────────────────

fn rich(rule: &str) -> String {
    format!("--add-rich-rule={rule}")
}

#[test]
fn rich_rule_firewalld_service_rule_runs_runtime_then_permanent() {
    let _guard = fixture(true);

    let rule = r#"rule family="ipv4" service name="ssh" accept"#;
    let run = run_helper(&["firewall_rich_rule", "add", "public", rule]);
    assert_eq!(run.code, 0, "a valid rich rule must succeed: {run:?}");
    assert!(run.stderr.is_empty());

    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"ssh\" accept",
            &format!("--zone=public {} --permanent", rich(rule)),
        ],
        "the helper rebuilds the rule text from the parsed structure: {:?}", calls);
}

#[test]
fn rich_rule_firewalld_port_protocol_and_action_flags_all_valid() {
    let _guard = fixture(true);

    // port element
    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv6" port port="22" protocol="tcp" reject"#,
    ]);
    assert_eq!(run.code, 0, "port element: {run:?}");

    // protocol element + destination
    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" destination address="192.168.0.0/24" protocol value="1" drop"#,
    ]);
    assert_eq!(run.code, 0, "protocol element: {run:?}");

    // remove
    let run = run_helper(&[
        "firewall_rich_rule",
        "remove",
        "public",
        r#"rule family="ipv4" source address="10.0.0.0/8" service name="ssh" accept"#,
    ]);
    assert_eq!(run.code, 0, "remove: {run:?}");

    let calls = calls();
    assert_eq!(calls.len(), 6, "each add/remove runs runtime then permanent: {:?}", calls);
    for (index, line) in calls.iter().enumerate() {
        let permanent = line.contains("--permanent");
        assert_eq!(
            permanent, index % 2 == 1,
            "runtime rules first, permanent rules second: {:?}", calls);
    }
    assert!(
        calls.iter().any(|line| {
            line.contains("--add-rich-rule=rule family=\"ipv6\" port port=\"22\" protocol=\"tcp\" reject")
        }),
        "port element argv: {:?}", calls);
    assert!(
        calls.iter().any(|line| {
            line.contains("--add-rich-rule=rule family=\"ipv4\" destination address=\"192.168.0.0/24\" protocol value=\"1\" drop")
        }),
        "protocol element argv: {:?}", calls);
    assert!(
        calls.iter().any(|line| line.contains("--remove-rich-rule=")),
        "remove builds --remove-rich-rule: {:?}", calls);
}

#[test]
fn rich_rule_firewalld_accept_reject_and_drop_labels_differ() {
    let _guard = fixture(true);

    for (action, flag) in [("accept", "accept"), ("reject", "reject"), ("drop", "drop")] {
        let _unused = action;
        let rule = format!(r#"rule family="ipv4" service name="http" {flag}"#);
        let run = run_helper(&["firewall_rich_rule", "add", "public", &rule]);
        assert_eq!(run.code, 0, "{flag} must succeed: {run:?}");
    }

    let mut calls = calls();
    // reduce to the runtime lines only
    calls.retain(|line| !line.contains("--permanent"));
    assert_eq!(
        calls,
        vec![
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"http\" accept",
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"http\" reject",
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"http\" drop",
        ],
        "the rebuilt rule carries the requested action: {:?}", calls);
}

#[test]
fn rich_rule_permanent_failure_reports_the_partial_state() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" accept"#,
    ]);
    assert_ne!(run.code, 0, "a permanent failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected permanent failure"),
        "the permanent step's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("runtime"),
        "the partial result must say what succeeded at runtime: {run:?}"
    );
}

#[test]
fn rich_rule_runtime_failure_stops_before_permanent() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");

    let run = run_helper(&[
        "firewall_rich_rule",
        "remove",
        "public",
        r#"rule family="ipv4" service name="ssh" accept"#,
    ]);
    assert_ne!(run.code, 0);
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "the runtime error must be preserved: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec!["--zone=public --remove-rich-rule=rule family=\"ipv4\" service name=\"ssh\" accept"],
        "the permanent step must never run after a runtime failure: {:?}", calls);
}

#[test]
fn rich_rule_invalid_grammar_and_unsupported_features_never_reach_the_tool() {
    let _guard = fixture(true);

    for rule in [
        // raw unvalidated text
        "anything at all",
        // unsupported feature: log
        r#"rule family="ipv4" service name="ssh" log accept"#,
        // unsupported feature: limit
        r#"rule family="ipv4" service name="ssh" accept limit value="1/m""#,
        // unsupported feature: masquerade
        r#"rule family="ipv4" masquerade accept"#,
        // malformed family
        r#"rule service name="ssh" accept"#,
        // unsupported action
        r#"rule family="ipv4" service name="ssh" mark"#,
    ] {
        let run = run_helper(&["firewall_rich_rule", "add", "public", rule]);
        assert_eq!(
            run.code, 2,
            "a rule the validator rejects must be refused: {rule} — {run:?}"
        );
        assert!(
            run.stderr.starts_with("lcp-admin:"),
            "the refusal must come from the helper: {run:?}"
        );
    }
    assert!(
        calls().is_empty(),
        "no unvalidated string may run as root: {:?}",
        calls()
    );
}

#[test]
fn rich_rule_ufw_request_is_explicitly_refused() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" accept"#,
    ]);
    assert_ne!(run.code, 0, "ufw must not run rich rules: {run:?}");
    assert!(
        run.stderr.contains("firewalld only") || run.stderr.contains("ufw does not manage rich rules"),
        "the refusal must explain that rich rules are firewalld-only: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("allow"),
        "ufw must not silently receive an allow rule instead: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "ufw must never have been executed for a rich rule: {:?}",
        calls()
    );
}

// ── SSH lockout guard enforced inside the helper ────────────────────────────

#[test]
fn guard_remove_ssh_service_without_confirmation_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "service", "ssh"]);
    assert_ne!(run.code, 0, "an unconfirmed SSH removal must fail: {run:?}");
    assert!(
        run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "the helper itself must return the exact prefix: {run:?}"
    );
    assert!(
        run.stderr.contains("SSH session"),
        "the refusal must explain the lockout risk: {run:?}"
    );
    assert_eq!(
        calls(),
        Vec::<String>::new(),
        "no firewall command may run before confirmation"
    );
}

#[test]
fn guard_remove_port22_without_confirmation_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "port", "22/tcp"]);
    assert_ne!(run.code, 0);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert_eq!(
        calls(),
        Vec::<String>::new(),
        "no firewall command may run before confirmation"
    );
}

#[test]
fn guard_remove_port_range_covering_22_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "port", "20-25/tcp"]);
    assert_ne!(run.code, 0);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(calls().is_empty(), "no firewall command may run: {:?}", calls());
}

#[test]
fn guard_rich_rule_drop_for_ssh_without_confirmation_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" drop"#,
    ]);
    assert_ne!(run.code, 0);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(calls().is_empty(), "no firewall command may run: {:?}", calls());
}

#[test]
fn guard_rich_rule_reject_for_port22_without_confirmation_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" port port="22" protocol="tcp" reject"#,
    ]);
    assert_ne!(run.code, 0);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(calls().is_empty(), "no firewall command may run: {:?}", calls());
}

#[test]
fn guard_remove_ssh_with_explicit_confirmation_proceeds() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_zone_rule",
        "remove",
        "public",
        "service",
        "ssh",
        "confirm",
    ]);
    assert_eq!(run.code, 0, "explicit confirmation must proceed: {run:?}");
    assert!(run.stdout.is_empty(), "the firewall stub prints nothing: {run:?}");
    assert!(run.stderr.is_empty(), "a confirmed run warns about nothing: {run:?}");
    assert_eq!(
        calls(),
        vec![
            "--zone=public --remove-service=ssh",
            "--zone=public --remove-service=ssh --permanent",
        ],
        "the confirmed operation executes the full two-phase sequence"
    );
}

#[test]
fn guard_rich_rule_drop_with_explicit_confirmation_proceeds() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" drop"#,
        "confirm",
    ]);
    assert_eq!(run.code, 0, "explicit confirmation must proceed: {run:?}");
    assert_eq!(
        calls(),
        vec![
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"ssh\" drop",
            "--zone=public --add-rich-rule=rule family=\"ipv4\" service name=\"ssh\" drop --permanent",
        ]
    );
}

#[test]
fn guard_rich_rule_reject_for_ssh_with_explicit_confirmation_proceeds() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" reject"#,
        "confirm",
    ]);
    assert_eq!(run.code, 0, "explicit confirmation must proceed: {run:?}");
    assert_eq!(
        calls().len(),
        2,
        "runtime then permanent ran after confirmation"
    );
}

#[test]
fn guard_inconclusive_detection_fails_open_with_a_warning() {
    let _guard = fixture(true);
    set_ctl("ss-inconclusive");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "service", "ssh"]);
    assert_eq!(
        run.code, 0,
        "an inconclusive probe fails open per the approved policy: {run:?}"
    );
    assert!(
        run.stderr.contains("inconclusive"),
        "the warning must say detection was inconclusive: {run:?}"
    );
    assert_eq!(
        calls(),
        vec![
            "--zone=public --remove-service=ssh",
            "--zone=public --remove-service=ssh --permanent",
        ],
        "the operation proceeded, best-effort"
    );
}

#[test]
fn guard_non_ssh_removal_is_not_blocked() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_zone_rule", "remove", "public", "service", "https"]);
    assert_eq!(
        run.code, 0,
        "a non-SSH removal must not hit the guard: {run:?}"
    );
    assert_eq!(calls().len(), 2, "runtime then permanent ran");
}

#[test]
fn guard_rich_rule_accept_for_ssh_is_not_blocked() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        r#"rule family="ipv4" service name="ssh" accept"#,
    ]);
    assert_eq!(
        run.code, 0,
        "an accept rule never locks the administrator out: {run:?}"
    );
    assert_eq!(calls().len(), 2);
}

#[test]
fn guard_confirm_cannot_bypass_invalid_input() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    // An invalid zone with `confirm` must still be refused by validation,
    // not push through the confirmation channel.
    let run = run_helper(&[
        "firewall_zone_rule",
        "remove",
        "../etc",
        "service",
        "https",
        "confirm",
    ]);
    assert_eq!(run.code, 2, "validation outranks confirmation: {run:?}");
    assert!(calls().is_empty(), "invalid input must run nothing: {:?}", calls());

    // And an invalid rich rule too.
    let run = run_helper(&[
        "firewall_rich_rule",
        "add",
        "public",
        "drop everything now",
        "confirm",
    ]);
    assert_eq!(run.code, 2);
    assert!(calls().is_empty(), "invalid input must run nothing: {:?}", calls());
}

#[test]
fn guard_stray_trailing_argument_is_refused_not_treated_as_confirmation() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    for bogus in ["confirmed", "yes", "confirm!", "true"] {
        let run = run_helper(&[
            "firewall_zone_rule",
            "remove",
            "public",
            "service",
            "ssh",
            bogus,
        ]);
        assert_eq!(
            run.code, 2,
            "only the exact literal 'confirm' may authorize: {bogus} — {run:?}"
        );
        assert!(
            calls().is_empty(),
            "a bogus trailing argument must run nothing: {:?}",
            calls()
        );
    }
}

// ── firewall_panic (GROUP 3) ─────────────────────────────────────────────────

#[test]
fn panic_firewalld_enable_runs_runtime_then_permanent() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_eq!(run.code, 0, "enable must succeed: {run:?}");
    assert!(run.stderr.is_empty(), "success prints nothing: {run:?}");
    assert_eq!(
        calls(),
        vec![
            "--panic-on",
            "--panic-on --permanent",
        ],
        "runtime first, then permanent; no zone argument: {:?}", calls());
}

#[test]
fn panic_firewalld_disable_runs_runtime_then_permanent() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_panic", "disable"]);
    assert_eq!(run.code, 0, "disable must succeed: {run:?}");
    assert_eq!(
        calls(),
        vec![
            "--panic-off",
            "--panic-off --permanent",
        ],
        "disabling uses --panic-off in both phases: {:?}", calls());
}

#[test]
fn panic_runtime_failure_stops_before_permanent() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(run.code, 0, "a runtime failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "the tool's stderr must be preserved: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["--panic-on"],
        "the permanent step must never run after a runtime failure: {:?}", calls());
}

#[test]
fn panic_permanent_failure_reports_the_partial_state_and_no_rollback() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(run.code, 0, "a permanent failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected permanent failure"),
        "the permanent step's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("enabled at runtime, but the persistent configuration was not"),
        "the partial state must be explicit: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["--panic-on", "--panic-on --permanent"],
        "both steps were attempted: {:?}", calls());
}

#[test]
fn panic_ufw_request_is_refused_before_any_command_runs() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(run.code, 0, "ufw must not run panic: {run:?}");
    assert!(
        run.stderr.contains("firewalld only") && run.stderr.contains("no panic mode"),
        "the refusal must explain the unsupported operation: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "no firewall command may run for a refused panic request: {:?}",
        calls()
    );

    let run = run_helper(&["firewall_panic", "disable"]);
    assert_ne!(run.code, 0);
    assert!(calls().is_empty(), "disable is also refused: {:?}", calls());
}

#[test]
fn panic_enable_without_confirmation_with_an_active_session_is_blocked_and_runs_nothing() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(run.code, 0, "an unconfirmed panic enable must fail: {run:?}");
    assert!(
        run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "the helper must return the exact prefix: {run:?}"
    );
    assert!(
        run.stderr.contains("sever every connection"),
        "the confirmation must explain the lockout-like risk: {run:?}"
    );
    assert_eq!(
        calls(),
        Vec::<String>::new(),
        "no firewall command may run before confirmation"
    );
}

#[test]
fn panic_enable_with_explicit_confirmation_proceeds_during_a_session() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_panic", "enable", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits panic: {run:?}");
    assert_eq!(
        calls().len(),
        2,
        "runtime then permanent ran after confirmation"
    );
}

#[test]
fn panic_enable_without_a_session_needs_no_confirmation() {
    let _guard = fixture(true);
    // ss-session not set: the stub reports no session

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_eq!(run.code, 0, "no session: enable must proceed: {run:?}");
    assert_eq!(calls().len(), 2);
}

#[test]
fn panic_enable_on_spawn_failure_fails_closed() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(SS_BIN); // probe cannot run: spawn failure

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(run.code, 0, "inconclusive detection must block: {run:?}");
    assert!(
        run.stderr.to_lowercase().contains("fail"),
        "the error must say panic is blocked: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "an inconclusive probe must not enable panic: {:?}",
        calls()
    );

    // even explicit confirmation cannot override an inconclusive probe
    let run = run_helper(&["firewall_panic", "enable", "confirm"]);
    assert_ne!(
        run.code, 0,
        "confirmation cannot override an inconclusive probe: {run:?}"
    );
    assert!(calls().is_empty(), "still nothing ran: {:?}", calls());
}

#[test]
fn panic_enable_on_nonzero_probe_exit_fails_closed() {
    let _guard = fixture(true);
    set_ctl("ss-inconclusive"); // stub exits 1 (GROUP 2's fail-open knob)

    let run = run_helper(&["firewall_panic", "enable"]);
    assert_ne!(
        run.code, 0,
        "GROUP 3 does not reuse GROUP 2's fail-open policy: {run:?}"
    );
    assert!(calls().is_empty(), "nothing ran: {:?}", calls());
}

#[test]
fn panic_disable_does_not_need_confirmation_or_a_probe() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(SS_BIN); // probe unavailable during disable

    let run = run_helper(&["firewall_panic", "disable"]);
    assert_eq!(
        run.code, 0,
        "disabling panic mode must remain possible: {run:?}"
    );
    assert_eq!(calls().len(), 2);
}

#[test]
fn panic_malformed_and_stray_confirmation_arguments_are_refused() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    for bogus in ["confirmed", "yes", "confirm!", "true", "false"] {
        let run = run_helper(&["firewall_panic", "enable", bogus]);
        assert_eq!(
            run.code, 2,
            "only the exact literal 'confirm' may authorize: {bogus} — {run:?}"
        );
        assert!(
            calls().is_empty(),
            "a malformed confirmation must run nothing: {:?}",
            calls()
        );
    }

    let stray = run_helper(&["firewall_panic", "enable", "confirm", "extra"]);
    assert_eq!(stray.code, 2, "too many arguments: {stray:?}");
    let missing = run_helper(&["firewall_panic"]);
    assert_eq!(missing.code, 2, "missing verb: {missing:?}");
    let bad_verb = run_helper(&["firewall_panic", "toggle"]);
    assert_eq!(bad_verb.code, 2, "verb must be enable/disable: {bad_verb:?}");
    assert!(calls().is_empty(), "shape refusals run nothing: {:?}", calls());
}

#[test]
fn panic_missing_confirmation_argument_is_blocked_with_the_exact_prefix() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    // confirmed as an absent field at the app layer: nothing is appended,
    // so the helper sees no confirmation
    let run = run_helper(&["firewall_panic", "enable"]);
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert_eq!(calls(), Vec::<String>::new());
}

// ── firewall_interface_zone (GROUP 4) ────────────────────────────────────────

#[test]
fn interface_zone_probes_ssh_exactly_once_and_never_twice() {
    let _guard = fixture(true);
    set_ctl("ss-session"); // matching session on wlp1s0
    set_ctl("zone-no-ssh"); // destination zone does not permit ssh

    // Regression for the TOCTOU gap: guard computation and enforcement must
    // share ONE probe result. A second probe could disagree with the first
    // and silently clear the detected session.
    let run = run_helper(&["firewall_interface_zone", "work", "wlp1s0"]);
    assert_ne!(
        run.code, 0,
        "the detected matching session must still require confirmation: {run:?}"
    );
    assert_eq!(
        ss_probes(),
        1,
        "guard decision and enforcement must share exactly one SSH probe"
    );

    // the confirmed retry is a fresh action with its own single probe
    let run = run_helper(&["firewall_interface_zone", "work", "wlp1s0", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits the move: {run:?}");
    assert_eq!(ss_probes(), 2, "exactly one probe per action");
}

#[test]
fn interface_zone_firewalld_moves_runtime_then_permanent() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_eq!(run.code, 0, "a valid binding must succeed: {run:?}");
    assert!(run.stderr.is_empty(), "success prints nothing: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=work --change-interface=eth1",
            "--zone=work --change-interface=eth1 --permanent",
        ],
        "runtime first, then permanent: {:?}", calls);
}

#[test]
fn interface_zone_a_zone_is_required() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_interface_zone", "", "eth1"]);
    assert_ne!(run.code, 0, "firewalld always needs a zone: {run:?}");
    assert!(
        run.stderr.contains("zone is required"),
        "the refusal must say why: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "a refused request must run nothing: {:?}",
        calls()
    );
}

#[test]
fn interface_zone_invalid_zone_and_interface_operands_are_refused() {
    let _guard = fixture(true);

    for bad in [
        &["firewall_interface_zone", "../etc", "eth1"][..],
        &["firewall_interface_zone", "-oProxyCommand=x", "eth1"][..],
        &["firewall_interface_zone", "work", "../eth1"][..],
        &["firewall_interface_zone", "work", "eth 1"][..],
        &["firewall_interface_zone", "work", "-oProxyCommand=x"][..],
    ] {
        let run = run_helper(bad);
        assert_ne!(run.code, 0, "invalid operands must be refused: {run:?}");
    }
    assert!(
        calls().is_empty(),
        "validation must run before any command: {:?}",
        calls()
    );
}

#[test]
fn interface_zone_ufw_request_is_refused_before_any_command_runs() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_ne!(run.code, 0, "ufw must not move interfaces: {run:?}");
    assert!(
        run.stderr.contains("firewalld only") && run.stderr.contains("no firewalld-style zones"),
        "the refusal must explain the unsupported operation: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "no firewall command may run for a refused request: {:?}",
        calls()
    );

    // even with confirmation ufw stays refused
    let run = run_helper(&["firewall_interface_zone", "work", "eth1", "confirm"]);
    assert_ne!(run.code, 0, "confirmation does not make ufw support zones: {run:?}");
    assert!(calls().is_empty(), "still nothing ran: {:?}", calls());
}

#[test]
fn interface_zone_ssh_carrying_interface_without_confirmation_is_blocked() {
    let _guard = fixture(true);
    set_ctl("ss-session"); // session scope marker names wlp1s0
    set_ctl("zone-no-ssh"); // destination zone does not permit ssh

    let run = run_helper(&["firewall_interface_zone", "work", "wlp1s0"]);
    assert_ne!(run.code, 0, "an unconfirmed risky move must fail: {run:?}");
    assert!(
        run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "the helper must return the exact prefix: {run:?}"
    );
    assert!(
        run.stderr.contains("moving the interface carrying the active SSH session"),
        "the confirmation must describe the risk: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec!["--zone work --query-service=ssh"],
        "the read-only zone probe ran, but no change: {:?}", calls);
}

#[test]
fn interface_zone_confirmed_operation_proceeds_during_a_session() {
    let _guard = fixture(true);
    set_ctl("ss-session");
    set_ctl("zone-no-ssh");

    let run = run_helper(&["firewall_interface_zone", "work", "wlp1s0", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits the move: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone work --query-service=ssh",
            "--zone=work --change-interface=wlp1s0",
            "--zone=work --change-interface=wlp1s0 --permanent",
        ],
        "probe, runtime, permanent: {:?}", calls);
}

#[test]
fn interface_zone_zone_permitting_ssh_needs_no_confirmation() {
    let _guard = fixture(true);
    // ss-session without zone-no-ssh: the zone permits ssh
    set_ctl("ss-session");

    let run = run_helper(&["firewall_interface_zone", "work", "wlp1s0"]);
    assert_eq!(
        run.code, 0,
        "moving into an ssh-permitting zone is not an SSH risk: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone work --query-service=ssh",
            "--zone=work --change-interface=wlp1s0",
            "--zone=work --change-interface=wlp1s0 --permanent",
        ],
        "probe reports the zone permits ssh, so the move proceeds: {:?}", calls);
}

#[test]
fn interface_zone_session_provably_on_another_interface_needs_no_confirmation() {
    let _guard = fixture(true);
    set_ctl("ss-session"); // scope marker names wlp1s0
    set_ctl("zone-no-ssh");

    // moving an interface the session is provably NOT carried by
    let run = run_helper(&["firewall_interface_zone", "work", "eth0"]);
    assert_eq!(
        run.code, 0,
        "a session on another interface cannot be cut by this move: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone work --query-service=ssh",
            "--zone=work --change-interface=eth0",
            "--zone=work --change-interface=eth0 --permanent",
        ],
        "no confirmation needed when the session is provably elsewhere: {:?}", calls);
}

#[test]
fn interface_zone_session_without_interface_marker_is_treated_as_risk() {
    let _guard = fixture(true);
    // ctl/ss-nomarker: the stub prints a session whose local endpoint has
    // no %interface scope marker
    set_ctl("ss-session");
    set_ctl("ss-nomarker");
    set_ctl("zone-no-ssh");

    // no scope marker: the session cannot be proven to be elsewhere
    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_ne!(run.code, 0, "an unmarkable session must not be waved away: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");

    // confirmation still works for a markerless session
    let run = run_helper(&["firewall_interface_zone", "work", "eth1", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits the move: {run:?}");
}

#[test]
fn interface_zone_inconclusive_session_probe_proceeds_with_the_group2_warning() {
    let _guard = fixture(true);
    set_ctl("ss-inconclusive"); // ss exits 1: GROUP 2's fail-open knob
    set_ctl("zone-no-ssh");

    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_eq!(
        run.code, 0,
        "GROUP 2's approved inconclusive behavior is fail-open, not blocked: {run:?}"
    );
    assert!(
        run.stderr.contains("inconclusive"),
        "a warning must say detection was inconclusive: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone work --query-service=ssh",
            "--zone=work --change-interface=eth1",
            "--zone=work --change-interface=eth1 --permanent",
        ],
        "warning does not stop the move: {:?}", calls);
}

#[test]
fn interface_zone_runtime_failure_stops_before_permanent() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");

    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_ne!(run.code, 0, "a runtime failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "the tool's stderr must be preserved: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec!["--zone=work --change-interface=eth1"],
        "the permanent step must never run after a runtime failure: {:?}", calls);
}

#[test]
fn interface_zone_permanent_failure_reports_the_partial_state_and_no_rollback() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_interface_zone", "work", "eth1"]);
    assert_ne!(run.code, 0, "a permanent failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected permanent failure"),
        "the permanent step's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr
            .contains("was bound to zone work at runtime, but the persistent configuration was not"),
        "the partial state must be explicit: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "--zone=work --change-interface=eth1",
            "--zone=work --change-interface=eth1 --permanent",
        ],
        "both steps were attempted: {:?}", calls);
}

#[test]
fn interface_zone_malformed_stray_and_missing_arguments_are_refused() {
    let _guard = fixture(true);

    let missing_interface = run_helper(&["firewall_interface_zone", "work"]);
    assert_eq!(missing_interface.code, 2, "missing interface: {missing_interface:?}");

    let missing_all = run_helper(&["firewall_interface_zone"]);
    assert_eq!(missing_all.code, 2, "missing operands: {missing_all:?}");

    for bogus in ["confirmed", "yes", "confirm!", "true"] {
        let run = run_helper(&["firewall_interface_zone", "work", "eth1", bogus]);
        assert_eq!(
            run.code, 2,
            "only the exact literal 'confirm' may authorize: {bogus} — {run:?}"
        );
    }

    let stray = run_helper(&["firewall_interface_zone", "work", "eth1", "confirm", "extra"]);
    assert_eq!(stray.code, 2, "too many arguments: {stray:?}");

    assert!(
        calls().is_empty(),
        "shape refusals run nothing: {:?}",
        calls()
    );
}

// ── firewall_enable_hardened (Security Auditor fix) ─────────────────────────

#[test]
fn hardened_firewalld_enable_runs_systemctl_then_runtime_then_permanent() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_eq!(run.code, 0, "hardening must succeed: {run:?}");
    assert!(run.stderr.is_empty(), "success prints nothing: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec![
            "enable --now firewalld",
            "--set-default-zone=drop",
            "--set-default-zone=drop --permanent",
        ],
        "service first, then runtime, then permanent: {:?}", calls);
}

#[test]
fn hardened_ufw_enable_sets_deny_incoming_then_enables() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // ufw-only host

    let run = run_helper(&["firewall_enable_hardened", "ufw", "confirm"]);
    assert_eq!(run.code, 0, "hardening must succeed: {run:?}");
    let calls = calls();
    assert_eq!(
        calls,
        vec!["default deny incoming", "--force enable"],
        "deny policy before enabling: {:?}", calls);
}

#[test]
fn hardened_requires_confirmation_before_any_command() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_enable_hardened", "firewalld"]);
    assert_ne!(run.code, 0, "unconfirmed hardening must fail: {run:?}");
    assert!(
        run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "the exact prefix must reach the caller: {run:?}"
    );
    assert!(
        run.stderr.contains("No active SSH session was visible"),
        "the message must state the detection result: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "no command may run before confirmation: {:?}",
        calls()
    );
}

#[test]
fn hardened_confirmation_message_includes_a_detected_ssh_session() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_enable_hardened", "firewalld"]);
    assert_ne!(run.code, 0, "unconfirmed hardening must fail: {run:?}");
    assert!(run.stderr.starts_with("CONFIRMATION_REQUIRED:"), "{run:?}");
    assert!(
        run.stderr.contains("Active SSH session(s) detected")
            && run.stderr.contains("wlp1s0"),
        "the detected session must be named: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn hardened_with_a_session_and_confirmation_proceeds() {
    let _guard = fixture(true);
    set_ctl("ss-session");

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_eq!(run.code, 0, "explicit confirmation permits it: {run:?}");
    assert_eq!(calls().len(), 3);
}

#[test]
fn hardened_inconclusive_detection_fails_closed_even_with_confirmation() {
    let _guard = fixture(true);
    set_ctl("ss-inconclusive");

    let run = run_helper(&["firewall_enable_hardened", "firewalld"]);
    assert_ne!(run.code, 0, "an inconclusive probe must block: {run:?}");
    assert!(
        run.stderr.contains("blocked (fail closed)"),
        "the refusal must state the fail-closed policy: {run:?}"
    );
    assert!(
        !run.stderr.starts_with("CONFIRMATION_REQUIRED:"),
        "this is a hard stop, not a confirmable prompt: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());

    // confirmation cannot override an inconclusive probe
    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "confirmation must not override: {run:?}");
    assert!(
        run.stderr.contains("blocked (fail closed)"),
        "the hard stop must persist: {run:?}"
    );
    assert!(calls().is_empty(), "still nothing may run: {:?}", calls());

    // the ufw backend follows the same policy
    let run = run_helper(&["firewall_enable_hardened", "ufw", "confirm"]);
    assert_ne!(run.code, 0, "ufw must fail closed too: {run:?}");
    assert!(calls().is_empty(), "still nothing may run: {:?}", calls());
}

#[test]
fn hardened_rejects_an_unknown_backend_before_any_command() {
    let _guard = fixture(true);

    let run = run_helper(&["firewall_enable_hardened", "nftables", "confirm"]);
    assert_eq!(run.code, 2, "only firewalld or ufw: {run:?}");
    assert!(
        run.stderr.contains("must be firewalld or ufw"),
        "the refusal must name the contract: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn hardened_rejects_a_missing_requested_backend() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN); // only ufw installed

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "a missing backend must be refused: {run:?}");
    assert!(
        run.stderr.contains("firewalld is not installed") && run.stderr.contains("ufw"),
        "the refusal must explain the hosting backend: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn hardened_rejects_when_systemctl_is_unavailable() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(SYSTEMCTL_BIN);

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "firewalld cannot be enabled without systemctl: {run:?}");
    assert!(
        run.stderr.contains("systemctl is not available"),
        "the refusal must name the missing tool: {run:?}"
    );
    assert!(calls().is_empty(), "nothing may run: {:?}", calls());
}

#[test]
fn hardened_systemctl_failure_stops_every_firewall_change() {
    let _guard = fixture(true);
    set_ctl("fail-systemctl");

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "a service failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected systemctl failure"),
        "the tool's stderr must be preserved: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["enable --now firewalld"],
        "no firewall change may follow a failed service step: {:?}", calls());
}

#[test]
fn hardened_firewalld_runtime_failure_reports_the_partial_state_and_no_rollback() {
    let _guard = fixture(true);
    set_ctl("fail-runtime");

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "a runtime failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected runtime failure"),
        "the tool's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("firewalld was enabled and is running"),
        "the service change must be reported: {run:?}"
    );
    assert!(
        run.stderr.contains("default zone was not changed to drop")
            && run.stderr.contains("permanent configuration was not updated"),
        "the unapplied zone change must be reported: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["enable --now firewalld", "--set-default-zone=drop"],
        "the permanent step must never run after a runtime failure: {:?}", calls());
}

#[test]
fn hardened_firewalld_permanent_failure_reports_the_partial_state_and_no_rollback() {
    let _guard = fixture(true);
    set_ctl("fail-permanent");

    let run = run_helper(&["firewall_enable_hardened", "firewalld", "confirm"]);
    assert_ne!(run.code, 0, "a permanent failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected permanent failure"),
        "the permanent step's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("was set to drop at runtime, but the persistent configuration was not"),
        "the partial state must be explicit: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
    assert_eq!(
        calls(),
        vec![
            "enable --now firewalld",
            "--set-default-zone=drop",
            "--set-default-zone=drop --permanent",
        ],
        "both phases were attempted: {:?}", calls());
}

#[test]
fn hardened_ufw_failure_stops_before_enable() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN);
    set_ctl("fail-runtime");

    let run = run_helper(&["firewall_enable_hardened", "ufw", "confirm"]);
    assert_ne!(run.code, 0, "a policy failure must fail the action: {run:?}");
    assert_eq!(
        calls(),
        vec!["default deny incoming"],
        "enabling must not follow a failed policy step: {:?}", calls());
}

#[test]
fn hardened_ufw_partial_failure_reports_the_applied_policy() {
    let _guard = fixture(true);
    let _ = std::fs::remove_file(FIREWALL_CMD_BIN);
    set_ctl("fail-ufw-enable");

    let run = run_helper(&["firewall_enable_hardened", "ufw", "confirm"]);
    assert_ne!(run.code, 0, "an enable failure must fail the action: {run:?}");
    assert!(
        run.stderr.contains("BY-STUB: injected ufw enable failure"),
        "the tool's stderr must be preserved: {run:?}"
    );
    assert!(
        run.stderr.contains("was set to deny, but ufw was not enabled"),
        "the partial state must be explicit: {run:?}"
    );
    assert!(
        !run.stderr.to_lowercase().contains("rollback"),
        "no rollback may be promised: {run:?}"
    );
    assert_eq!(
        calls(),
        vec!["default deny incoming", "--force enable"],
        "both steps were attempted: {:?}", calls());
}

#[test]
fn hardened_malformed_and_stray_confirmation_arguments_are_refused() {
    let _guard = fixture(true);

    for bogus in ["confirmed", "yes", "confirm!", "true"] {
        let run = run_helper(&["firewall_enable_hardened", "firewalld", bogus]);
        assert_eq!(
            run.code, 2,
            "only the exact literal 'confirm' may authorize: {bogus} — {run:?}"
        );
    }
    let stray = run_helper(&["firewall_enable_hardened", "firewalld", "confirm", "extra"]);
    assert_eq!(stray.code, 2, "too many arguments: {stray:?}");
    let missing = run_helper(&["firewall_enable_hardened"]);
    assert_eq!(missing.code, 2, "missing backend: {missing:?}");
    assert!(calls().is_empty(), "shape refusals run nothing: {:?}", calls());
}

// ── generic channel closure (F1) ───────────────────────────────────────────

#[test]
fn generic_firewall_cmd_program_is_refused_before_execution() {
    let _guard = fixture(true);

    let run = run_helper(&[FIREWALL_CMD_BIN, "--panic-on"]);
    assert_eq!(run.code, 2, "the generic channel must refuse firewall-cmd: {run:?}");
    assert!(
        run.stderr.contains("not on the allow list"),
        "the refusal must name the allow list: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "no firewall command may be executed: {:?}",
        calls()
    );
}

#[test]
fn generic_ufw_program_is_refused_before_execution() {
    let _guard = fixture(true);

    let run = run_helper(&[UFW_BIN, "default", "deny", "outgoing"]);
    assert_eq!(run.code, 2, "the generic channel must refuse ufw: {run:?}");
    assert!(
        run.stderr.contains("not on the allow list"),
        "the refusal must name the allow list: {run:?}"
    );
    assert!(
        calls().is_empty(),
        "no firewall command may be executed: {:?}",
        calls()
    );
}
