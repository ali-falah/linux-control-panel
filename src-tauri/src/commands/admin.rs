//! Administrator rights for the Control Panel.
//!
//! The app never runs a program as root itself. It asks the desktop's
//! PolicyKit agent to authorise the small root owned helper `lcp-admin`,
//! and the agent runs that helper as root. Because the helper is the only
//! thing that is ever authorised, a cached authorisation can not be abused to
//! run `dnf`, `systemctl` or a shell as root by another program of the same
//! user.
//!
//! The helper ships with the package (the RPM/DEB installs the external
//! binary as `/usr/bin/lcp-admin`); the polkit rule is installed by the
//! helper's own `install_polkit_rule` action, and the pkexec policy
//! (`src-tauri/polkit/com.controlpanel.pkexec.policy`) is installed by hand
//! or by the packager — the polkit defaults carry no caching either way;
//! the five-minute `timestamp_timeout` keep comes only from the rules.d
//! rule, which is what the status bar shows.

use crate::utils::privilege::{run_privileged_output, verify_trusted_binary, RootAuth};

/// Where the root owned helper is installed when the app sets it up itself.
pub const HELPER_PATH: &str = "/usr/local/libexec/lcp-admin";

/// Where the helper lives when it was installed as part of the package.
pub const PACKAGED_HELPER_PATH: &str = "/usr/bin/lcp-admin";

/// Where the PolicyKit rule is installed. The single source of truth for the
/// path and the rule text is `root_policy.rs` — the helper carries the same
/// constants and refuses to write anything that does not match byte for byte.
pub const POLKIT_RULE_PATH: &str = crate::utils::root_policy::POLKIT_RULE_FILE;

/// Is the helper installed and trustworthy (root owned, not writable by others)?
///
/// The copy that came with the package counts, and so does one that was
/// placed by hand on a development checkout.
pub fn helper_is_installed() -> bool {
    verify_trusted_binary(HELPER_PATH).is_ok() || verify_trusted_binary(PACKAGED_HELPER_PATH).is_ok()
}

/// The path of the helper that may be used, if there is one.
pub fn installed_helper_path() -> Option<&'static str> {
    if verify_trusted_binary(HELPER_PATH).is_ok() {
        Some(HELPER_PATH)
    } else if verify_trusted_binary(PACKAGED_HELPER_PATH).is_ok() {
        Some(PACKAGED_HELPER_PATH)
    } else {
        None
    }
}

/// Report whether the helper is installed and trustworthy.
///
/// No root execution happens here: the helper (and its PolicyKit rule) ship
/// with the RPM, and a development checkout places the helper by hand.
#[tauri::command]
pub async fn admin_install_helper() -> Result<bool, String> {
    if helper_is_installed() {
        return Ok(true);
    }

    // The helper can not be installed by the app: no root copy of an
    // arbitrary file over its own helper is allowed, whatever template the
    // file claims. The helper (and its PolicyKit rule) ship with the RPM.
    // A development build is trusted exactly as long as it was installed by
    // root by hand — anything else is refused instead of silently installed.
    Err(format!(
        "The administrator helper is not installed. Install it with the \
         package: sudo dnf install linux-control-panel — or place the helper \
         manually: sudo install -m 0755 -o root -g root <helper> {HELPER_PATH}"
    ))
}

/// Report whether the root owned helper is installed and trustworthy.
///
/// This is deliberately not merged with `admin_mode_state`: "the helper is
/// on disk" is a persistent mechanical fact, while "PolicyKit currently
/// holds a temporary authorisation for this process" is per-session live
/// state. The install fact is needed before any PolicyKit call can work;
/// the live state only feeds the admin-mode toggle.
#[tauri::command]
pub async fn helper_installed() -> Result<bool, String> {
    Ok(helper_is_installed())
}

// ─── Admin mode: real PolicyKit state, no app-side stopwatch ─────────────────

/// The action ids `pkexec` authorises the helper under — the two custom
/// actions of the bundled `.policy` file, substituted by polkit for the
/// generic `org.freedesktop.policykit.exec` action because of the
/// `org.freedesktop.policykit.exec.path` annotations (see
/// `root_policy::HELPER_ACTION_IDS`). Any temporary authorization lives
/// under one of these ids, never under the generic one.
const PKEXEC_ACTION_IDS: &[&str] = &crate::utils::root_policy::HELPER_ACTION_IDS;

/// The real admin-mode state, read from PolicyKit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AdminModeState {
    /// A temporary authorization for this process and the pkexec action.
    pub on: bool,
    /// Unix seconds at which PolicyKit drops that authorization.
    pub expires_at: Option<i64>,
    /// Why the state could not be read or changed, when it could not be.
    pub error: Option<String>,
}

impl AdminModeState {
    fn off(error: Option<String>) -> Self {
        Self { on: false, expires_at: None, error }
    }

    fn enabled(expires_at: i64) -> Self {
        Self { on: true, expires_at: Some(expires_at), error: None }
    }
}

/// Resolve `pkcheck` the same way the privileged programs are resolved; it is
/// only ever run unprivileged here.
fn pkcheck_path() -> Result<String, String> {
    let path = crate::utils::privilege::resolve_root_program("pkcheck")
        .ok_or_else(|| "PolicyKit (pkcheck) was not found on this system.".to_string())?;
    crate::utils::privilege::verify_trusted_binary(&path)?;
    Ok(path)
}

/// Run one read-only `pkcheck` query. `LC_ALL=C` is deliberate: the expiry is
/// printed with `strftime("%c")`, and the C locale is the only one whose
/// shape we parse.
fn run_pkcheck(args: &[&str]) -> Result<String, String> {
    let pkcheck = pkcheck_path()?;
    let output = std::process::Command::new(pkcheck)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("PolicyKit (pkcheck) is not available: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("PolicyKit could not read the admin state (pkcheck exited with {}).", output.status)
        } else {
            format!("PolicyKit could not read the admin state: {stderr}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse the absolute expiry out of one `expires:` value,
/// e.g. `in 4 minutes (Thu Oct 29 12:39:00 2026)`.
fn parse_pkcheck_expiry(value: &str) -> Result<chrono::NaiveDateTime, String> {
    let (_, absolute) = value.rsplit_once('(').ok_or_else(|| {
        format!("PolicyKit reported an admin expiry without an absolute time: {value}")
    })?;
    let absolute = absolute.trim().trim_end_matches(')');
    // glibc's C locale pads a single-digit day with a space, and chrono's
    // `%d` wants it zero-padded; normalise both shapes explicitly.
    let mut parts = absolute.split_whitespace();
    let (Some(weekday), Some(month), Some(day), Some(time), Some(year), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(format!("PolicyKit reported an unreadable admin expiry: {value}"));
    };
    let day: u32 = day
        .parse()
        .map_err(|_| format!("PolicyKit reported an unreadable admin expiry: {value}"))?;
    let absolute = format!("{weekday} {month} {day:02} {time} {year}");
    chrono::NaiveDateTime::parse_from_str(&absolute, "%a %b %d %H:%M:%S %Y")
        .map_err(|_| format!("PolicyKit reported an unreadable admin expiry: {value}"))
}

/// Find *our* temporary authorization in `pkcheck --list-temp` output.
///
/// `pkcheck --list-temp` enumerates the current session, so the list can also
/// contain authorisations of other processes. Only an entry whose subject is
/// `unix-process:<our pid>:…` for the pkexec action counts — that is the
/// exact subject PolicyKit matches when the app's own `pkexec` calls run.
/// Empty output means "no authorisation" (OFF); anything unrecognised is an
/// error, never a guess.
fn parse_list_temp(output: &str, our_pid: u32) -> Result<Option<chrono::NaiveDateTime>, String> {
    if output.trim().is_empty() {
        return Ok(None);
    }
    let our_subject = format!("unix-process:{our_pid}:");
    let mut newest: Option<chrono::NaiveDateTime> = None;
    for block in output.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let field = |label: &str| {
            block
                .lines()
                .find_map(|line| line.strip_prefix(label).map(str::trim))
        };
        let subject = field("subject:").ok_or_else(|| {
            "PolicyKit returned an admin-state entry without a subject; refusing to guess.".to_string()
        })?;
        if !subject.starts_with(&our_subject) {
            continue;
        }
        let action = field("action:").ok_or_else(|| {
            "PolicyKit returned an admin-state entry without an action; refusing to guess.".to_string()
        })?;
        // The generic exec action is deliberately not accepted: the helper's
        // keep only ever exists under the annotated custom actions, and a
        // generic-exec temporary authorization would come from some third
        // party policy, not from this app's admin mode.
        if !PKEXEC_ACTION_IDS.contains(&action) {
            continue;
        }
        let expires = field("expires:").ok_or_else(|| {
            "PolicyKit returned an admin-state entry without an expiry; refusing to guess.".to_string()
        })?;
        let expiry = parse_pkcheck_expiry(expires)?;
        newest = Some(newest.map_or(expiry, |current| current.max(expiry)));
    }
    Ok(newest)
}

/// Read the real admin-mode state. Never prompts: `--list-temp` is read-only.
/// Every reading is recorded as the snapshot the privilege gate reuses.
pub(crate) fn read_admin_mode_state() -> AdminModeState {
    let state = match run_pkcheck(&["--list-temp"]) {
        Ok(output) => match parse_list_temp(&output, std::process::id()) {
            Ok(None) => AdminModeState::off(None),
            Ok(Some(expires)) => match expires.and_local_timezone(chrono::Local).earliest() {
                Some(expires) => AdminModeState::enabled(expires.timestamp()),
                None => AdminModeState::off(Some(
                    "PolicyKit reported an admin expiry that does not exist in the local time zone."
                        .to_string(),
                )),
            },
            Err(error) => AdminModeState::off(Some(error)),
        },
        Err(error) => AdminModeState::off(Some(error)),
    };
    crate::utils::privilege::record_admin_state(&state);
    state
}

/// Read-only state query for the status bar poll. Cannot prompt.
#[tauri::command]
pub async fn admin_mode_state() -> AdminModeState {
    read_admin_mode_state()
}

/// Check the installed PolicyKit action definitions at startup or on demand.
/// A missing, stale or weakened file is reported as an error; the helper and
/// the Polkit call path also refuse to proceed.
///
/// A rules.d file this user cannot read (Fedora ships `0750 root:polkitd`) is
/// not an error: only the helper, running as root, can see it. Reporting it as
/// broken would offer a "repair" that writes a correct rule and then fails the
/// very same check.
#[tauri::command]
pub async fn admin_policy_status() -> Result<(), String> {
    crate::utils::root_policy::verify_installed_polkit_policy()?;
    match crate::utils::root_policy::polkit_rule_check() {
        crate::utils::root_policy::PolkitRuleCheck::Verified
        | crate::utils::root_policy::PolkitRuleCheck::Unreadable(_) => Ok(()),
        crate::utils::root_policy::PolkitRuleCheck::Mismatch(message) => Err(message),
    }
}

/// Install (or repair) the rules.d rule that turns the helper's authorisation
/// into a *kept* one, then read it back.
///
/// A source checkout and an AppImage do not run the package scripts, so the
/// policy file can be present while the rule is missing. The consequence is
/// not cosmetic: `admin_mode_on` can never turn ON (PolicyKit keeps nothing
/// without the rule), and every privileged action would prompt again. This
/// command closes that dead end with the one authorisation the helper's
/// `install_polkit_rule` action needs.
///
/// The rule text comes from the app's own constant and the helper refuses
/// anything that is not byte-identical to it, so the strongest thing this can
/// grant is still exactly "the root owned helper may run as root".
#[tauri::command]
pub async fn admin_policy_repair() -> Result<String, String> {
    // The rule only grants a keep *for* the policy's action ids, so repairing
    // it without the policy would be meaningless (and the helper refuses to
    // run without the policy anyway).
    crate::utils::root_policy::verify_installed_polkit_policy()?;

    // Already installed byte for byte: never raise a needless prompt.
    if matches!(
        crate::utils::root_policy::polkit_rule_check(),
        crate::utils::root_policy::PolkitRuleCheck::Verified
    ) {
        return Ok("The administrator authorization rule is already installed.".to_string());
    }

    // The same named-action route every other helper call takes: no arguments,
    // one PolicyKit prompt, the rule text on stdin. It reports a helper that
    // predates the action, and the helper itself refuses any content that is
    // not byte-identical to its own constant.
    install_polkit_rule().await?;

    // The helper validated the exact bytes before writing them. Read the file
    // back when this host allows it; a rules.d directory this user cannot read
    // is not a failed repair.
    match crate::utils::root_policy::polkit_rule_check() {
        crate::utils::root_policy::PolkitRuleCheck::Verified => Ok(
            "Administrator authorization rule installed. Admin mode can be enabled now."
                .to_string(),
        ),
        crate::utils::root_policy::PolkitRuleCheck::Unreadable(_) => Ok(
            "Administrator authorization rule installed. This system does not let the panel \
             read the rule back; the helper verified the bytes it wrote."
                .to_string(),
        ),
        crate::utils::root_policy::PolkitRuleCheck::Mismatch(error) => Err(format!(
            "The authorization rule was written but does not verify: {error}"
        )),
    }
}

/// Turn admin mode ON by taking the one authorisation the panel caches.
///
/// The prompt is raised by the helper's dedicated `admin_keep` action: it does
/// nothing but exit 0, and the panel's polkit rule grants `AUTH_ADMIN_KEEP` for
/// the helper, which is what PolicyKit turns into a temporary authorization.
/// `date` cannot be used for this: its pkexec action falls back to
/// `auth_admin` (no keep), so a successful prompt would leave `--list-temp`
/// empty and ON could never be verified.
///
/// A missing or stale rule is repaired here rather than handed back to the
/// user: a readable mismatch is installed before the first prompt, and a host
/// that hides the rule from this user (`0750 root:polkitd`) is repaired and
/// retried once if the first keep does not stick.
/// Install the rules.d rule through the helper's one bootstrap action.
///
/// The helper validates the bytes itself; this is the only action that runs
/// before the rule exists, so it is the one that makes the keep possible.
async fn install_polkit_rule() -> Result<(), String> {
    crate::commands::service_manager::run_service_named_action(
        "install_polkit_rule",
        &[],
        crate::utils::root_policy::POLKIT_RULE_TEXT,
    )
    .await
}

/// Run the helper's `admin_keep` and report the state PolicyKit actually kept.
///
/// ON is never inferred from the exit code: the helper runs, then
/// `pkcheck --list-temp` is read back. A prompt that was dismissed, or one that
/// PolicyKit did not keep, comes back as an `Off` state with the reason.
async fn take_admin_keep(helper: &str) -> Result<AdminModeState, String> {
    let args = vec![String::from("admin_keep")];
    let output = run_privileged_output(helper, &args, &RootAuth::Polkit)
        .await
        .map_err(|error| format!("Admin mode was not enabled: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if output.status.code() == Some(126) || stderr.contains("dismissed") {
            "Admin mode was not enabled.".to_string()
        } else if stderr.is_empty() {
            "Admin mode was not enabled: PolicyKit did not authorise the request.".to_string()
        } else {
            format!("Admin mode was not enabled: {stderr}")
        };
        return Err(message);
    }
    Ok(read_admin_mode_state())
}

#[tauri::command]
pub async fn admin_mode_on() -> AdminModeState {
    if let Err(error) = crate::utils::root_policy::verify_installed_polkit_policy() {
        return AdminModeState::off(Some(format!(
            "The installed PolicyKit policy is missing or unsafe: {error}. \
             Reinstall src-tauri/polkit/com.controlpanel.pkexec.policy before enabling admin mode."
        )));
    }

    let Some(helper) = installed_helper_path() else {
        return AdminModeState::off(Some(
            "The administrator helper (lcp-admin) is not installed. Install the Control Panel \
             package, or place the bundled helper at /usr/bin/lcp-admin, then try again."
                .to_string(),
        ));
    };

    let before = read_admin_mode_state();
    if before.on {
        return before;
    }
    if before.error.is_some() {
        // We cannot verify the result of a prompt, so we must not raise one.
        return before;
    }

    // A readable, wrong rule is repaired first: prompting for a keep that
    // PolicyKit will not grant spends the password for nothing. A rule this
    // user cannot read (Fedora's `0750 root:polkitd` directory) proves
    // nothing either way — the keep check below decides.
    let mut rule_verified = matches!(
        crate::utils::root_policy::polkit_rule_check(),
        crate::utils::root_policy::PolkitRuleCheck::Verified
    );

    if !rule_verified
        && matches!(
            crate::utils::root_policy::polkit_rule_check(),
            crate::utils::root_policy::PolkitRuleCheck::Mismatch(_)
        )
    {
        if let Err(error) = install_polkit_rule().await {
            return AdminModeState::off(Some(error));
        }
        rule_verified = matches!(
            crate::utils::root_policy::polkit_rule_check(),
            crate::utils::root_policy::PolkitRuleCheck::Verified
        );
    }

    let first = match take_admin_keep(helper).await {
        Ok(state) => state,
        Err(message) => return AdminModeState::off(Some(message)),
    };
    if first.on {
        return first;
    }

    // Self-healing for hosts where the rule cannot be read: the prompt above
    // was wasted because the rule really is missing. Install it and retry the
    // keep once, so the user never has to find a Repair button.
    if !rule_verified {
        if let Err(error) = install_polkit_rule().await {
            let reason = first.error.unwrap_or_default();
            return AdminModeState::off(Some(format!("{reason} {error}").trim().to_string()));
        }
        return match take_admin_keep(helper).await {
            Ok(second) if second.on => second,
            Ok(second) => AdminModeState::off(second.error),
            Err(message) => AdminModeState::off(Some(message)),
        };
    }

    AdminModeState::off(first.error)
}

/// Turn admin mode OFF by revoking the real cached authorisation, then confirm
/// it is gone. Failing to confirm is reported, never papered over.
#[tauri::command]
pub async fn admin_mode_off() -> AdminModeState {
    let revoke = run_pkcheck(&["--revoke-temp"]);
    let state = read_admin_mode_state();
    match revoke {
        Err(error) => AdminModeState {
            on: state.on,
            expires_at: state.expires_at,
            error: Some(format!("Admin mode could not be turned off: {error}")),
        },
        Ok(_) if state.on => AdminModeState {
            on: true,
            expires_at: state.expires_at,
            error: Some(
                "PolicyKit still reports an active administrator authorization after revoking it."
                    .to_string(),
            ),
        },
        Ok(_) => state,
    }
}

#[cfg(test)]
mod admin_mode_tests {
    use super::*;
    use chrono::NaiveDate;

    fn entry(action: &str, pid: u32, expires: &str) -> String {
        format!(
            "authorization id: tmpauthz1\naction:           {action}\nsubject:          unix-process:{pid}:4242 (/usr/bin/linux-control-panel)\nobtained:         now (Thu Oct 29 12:34:56 2026)\nexpires:          {expires}\n\n"
        )
    }

    fn at(hour: u32, minute: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 29)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn an_empty_list_means_admin_mode_is_off() {
        assert_eq!(parse_list_temp("", 42).unwrap(), None);
        assert_eq!(parse_list_temp("  \n\n", 42).unwrap(), None);
    }

    #[test]
    fn the_expiry_is_read_from_the_c_locale_timestamp() {
        assert_eq!(
            parse_pkcheck_expiry("in 4 minutes (Thu Oct 29 12:39:00 2026)").unwrap(),
            at(12, 39)
        );
        // the C locale pads a single-digit day with a space
        assert_eq!(
            parse_pkcheck_expiry("in 4 minutes (Thu Oct  1 12:39:00 2026)").unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 1)
                .unwrap()
                .and_hms_opt(12, 39, 0)
                .unwrap()
        );
    }

    #[test]
    fn an_authorisation_for_this_process_carries_its_expiry() {
        let output = entry(
            super::PKEXEC_ACTION_IDS[0],
            42,
            "in 4 minutes (Thu Oct 29 12:39:00 2026)",
        );
        assert_eq!(parse_list_temp(&output, 42).unwrap(), Some(at(12, 39)));
    }

    /// Regression guard for the live finding that the rule (and this parser)
    /// looked for `org.freedesktop.policykit.exec` while pkexec authorises
    /// under the `.policy` action ids: both install paths must be recognised,
    /// and the generic exec action must never count as this app's admin mode.
    #[test]
    fn the_helper_is_recognised_under_the_annotated_policy_actions_not_the_generic_exec_action() {
        for id in super::PKEXEC_ACTION_IDS {
            assert_eq!(
                parse_list_temp(&entry(id, 42, "in 4 minutes (Thu Oct 29 12:39:00 2026)"), 42)
                    .unwrap(),
                Some(at(12, 39)),
                "a temporary authorization under {id} is the helper's admin mode"
            );
        }
        // The generic pkexec action never carries this app's keep, so even a
        // temporary authorization recorded under it must read as OFF.
        assert_eq!(
            parse_list_temp(
                &entry(
                    "org.freedesktop.policykit.exec",
                    42,
                    "in 4 minutes (Thu Oct 29 12:39:00 2026)"
                ),
                42
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn other_processes_and_other_actions_do_not_count() {
        let other_process = entry(
            super::PKEXEC_ACTION_IDS[1],
            43,
            "in 4 minutes (Thu Oct 29 12:39:00 2026)",
        );
        assert_eq!(parse_list_temp(&other_process, 42).unwrap(), None);

        let other_action =
            entry("org.example.other", 42, "in 4 minutes (Thu Oct 29 12:39:00 2026)");
        assert_eq!(parse_list_temp(&other_action, 42).unwrap(), None);

        let mixed = format!(
            "{}{}",
            entry("org.example.other", 42, "in 4 minutes (Thu Oct 29 12:39:00 2026)"),
            entry(
                super::PKEXEC_ACTION_IDS[0],
                42,
                "in 2 minutes (Thu Oct 29 12:37:00 2026)"
            )
        );
        assert_eq!(parse_list_temp(&mixed, 42).unwrap(), Some(at(12, 37)));
    }

    #[test]
    fn malformed_or_unverifiable_output_fails_closed() {
        assert!(parse_list_temp("garbage without fields", 42).is_err());
        assert!(
            parse_list_temp(
                &entry(
                    super::PKEXEC_ACTION_IDS[0],
                    42,
                    "in 4 minutes"
                ),
                42
            )
            .is_err()
        );
        assert!(
            parse_list_temp(
                &entry(
                    super::PKEXEC_ACTION_IDS[0],
                    42,
                    "in 4 minutes (not a date)"
                ),
                42
            )
            .is_err()
        );
    }
}
