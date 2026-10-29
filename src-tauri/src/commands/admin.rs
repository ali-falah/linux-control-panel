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

use crate::utils::privilege::verify_trusted_binary;

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

/// State of the administrator authorisation, for the status bar.
#[derive(serde::Serialize)]
pub struct AdminAuthState {
    /// The root owned helper is installed and can be authorised.
    pub helper_installed: bool,
    /// Seconds the current authorisation is still valid for.
    pub authorised_for: Option<u64>,
    /// How long PolicyKit keeps an authorisation by default.
    pub lifetime_secs: u64,
}

/// Report whether the helper is ready and how long the authorisation is valid.
#[tauri::command]
pub async fn admin_auth_state() -> Result<AdminAuthState, String> {
    Ok(AdminAuthState {
        helper_installed: helper_is_installed(),
        authorised_for: crate::utils::privilege::authorisation_remaining(),
        lifetime_secs: crate::utils::privilege::AUTHORISATION_LIFETIME_SECS,
    })
}
