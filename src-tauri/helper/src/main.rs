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
//! Usage: `lcp-admin <program> [args…]`

#[path = "../../src/utils/root_policy.rs"]
#[allow(dead_code)]
mod root_policy;

use root_policy::{
    ensure_no_shell_escape, ensure_program_allowed, ensure_safe_arguments, resolve_root_program,
    verify_trusted_binary,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(requested) = args.first() else {
        eprintln!("lcp-admin: no program given.");
        std::process::exit(2);
    };

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
