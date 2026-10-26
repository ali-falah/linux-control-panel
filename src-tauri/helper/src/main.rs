//! `lcp-admin` — the small, root owned helper that performs privileged
//! actions for the Control Panel.
//!
//! The app never runs a program with root rights itself. It asks the desktop's
//! PolicyKit agent to authorise *this* helper, and the agent runs it as root.
//! Because the helper is the only thing that is ever authorised, a cached
//! authorisation can not be used to run `dnf`, `systemctl` or a shell as root
//! by some other program of the same user.
//!
//! The helper re-validates everything it is asked to do — the program has to
//! be on the allow list and the arguments have to be free of control
//! characters. A shell is never started with `-c`.
//!
//! Usage: `lcp-admin <program> [args…]`; named actions:
//! `lcp-admin <action> [params]` (see `ROOT_ACTIONS`).

#[path = "../../src/utils/root_policy.rs"]
#[allow(dead_code)]
mod root_policy;

use root_policy::{
    ensure_arguments_for_program, ensure_no_shell_escape, ensure_program_allowed,
    ensure_safe_arguments, resolve_root_program, verify_trusted_binary, POLKIT_RULE_FILE,
    POLKIT_RULE_TEXT,
};

/// Named actions the helper performs itself, instead of executing a root
/// program. Their arguments are validated by `check_helper_action` in
/// root_policy.rs, so the helper only ever `exec`s programs for the generic
/// allow-listed calls.
const ROOT_ACTIONS: &[&str] = &["install_polkit_rule"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(requested) = args.first() else {
        eprintln!("lcp-admin: no program given.");
        std::process::exit(2);
    };

    // Named actions are dispatched before any program resolution: their first
    // argument is an action name, not a program.
    if ROOT_ACTIONS.contains(&requested.as_str()) {
        if args.len() != 1 {
            eprintln!("lcp-admin: the action '{requested}' takes no further arguments.");
            std::process::exit(2);
        }
        if requested == "install_polkit_rule" {
            install_polkit_rule_action();
        }
        // unreachable: ROOT_ACTIONS contains only known actions
        std::process::exit(2);
    }

    // The program has to be resolved to an absolute path before it is checked
    // — otherwise a relative name could pick up a program from the PATH.
    let Some(program) = resolve_root_program(requested) else {
        eprintln!("lcp-admin: the program '{requested}' was not found.");
        std::process::exit(2);
    };
    let rest: Vec<String> = args[1..].to_vec();

    if let Err(message) = ensure_program_allowed(&program)
        .and_then(|_| ensure_no_shell_escape(&program, &rest))
        .and_then(|_| ensure_safe_arguments(&rest))
        .and_then(|_| ensure_arguments_for_program(&program, &rest))
        .and_then(|_| verify_trusted_binary(&program))
    {
        eprintln!("lcp-admin: {message}");
        std::process::exit(2);
    }

    // `exec` replaces the helper with the program, so the exit status and the
    // output of the program are the ones the app sees.
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(&program).args(&rest).exec();
    eprintln!("lcp-admin: could not start {program}: {error}");
    std::process::exit(2);
}

/// The `install_polkit_rule` action: writes the PolicyKit rule to its one,
/// fixed location so the bootstrap does not need a shell, a temp file in a
/// user-writable directory or base64 in a script.
///
/// The rule text arrives on stdin and is written to `POLKIT_RULE_FILE` only
/// when it is byte-identical to the shared `POLKIT_RULE_TEXT` constant the
/// helper carries itself. The app cannot smuggle a different rule through —
/// even if it were compromised, the strongest it can grant is still exactly
/// "the helper may run as root".
fn install_polkit_rule_action() {
    // The rule is small; refuse anything larger before reading it.
    const MAX_RULE_BYTES: u32 = 128 * 1024;

    let mut content = String::new();
    match std::io::BufReader::new(std::io::stdin())
        .take(u64::from(MAX_RULE_BYTES) + 1)
        .read_to_string(&mut content)
    {
        Ok(_) if content.len() as u64 > u64::from(MAX_RULE_BYTES) => {
            eprintln!("lcp-admin: the rule content is too large.");
            std::process::exit(2);
        }
        Ok(_) => (),
        Err(_) => {
            eprintln!("lcp-admin: the rule content is not valid UTF-8 (or unreadable).");
            std::process::exit(2);
        }
    }

    if content != POLKIT_RULE_TEXT {
        eprintln!(
            "lcp-admin: the rule content does not match the rule this helper installs; refusing to write '{POLKIT_RULE_FILE}'."
        );
        std::process::exit(2);
    }

    // Atomic replacement: write the sibling temp file, then rename over the
    // destination. rename(2) keeps the previous rule until the very last
    // moment and never leaves a half-written rules.d entry (an unreadable
    // rules.d entry makes polkit ignore *all* rules). The temp name is fixed
    // but that cannot race into a mixed file: every run that reaches the
    // write carries the same byte-identical content, and rename(2) itself is
    // atomic. The file ends up root:root implicitly — the helper runs as root.
    use std::fs::Permissions;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let temp = std::path::Path::new(POLKIT_RULE_FILE).with_extension("rules.tmp");
    let _ = std::fs::remove_file(&temp);
    if let Err(error) = std::fs::write(&temp, content.as_bytes())
        .and_then(|_| std::fs::set_permissions(&temp, Permissions::from_mode(0o644)))
        .and_then(|_| std::fs::rename(&temp, POLKIT_RULE_FILE))
    {
        let _ = std::fs::remove_file(&temp);
        eprintln!("lcp-admin: could not install the rule: {error}");
        std::process::exit(2);
    }
}
