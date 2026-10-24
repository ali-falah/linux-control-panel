//! Administrator rights for the Control Panel.
//!
//! The app never runs a program as root itself. It asks the desktop's
//! PolicyKit agent to authorise the small root owned helper `lcp-admin`,
//! and the agent runs that helper as root. Because the helper is the only
//! thing that is ever authorised, a cached authorisation can not be abused to
//! run `dnf`, `systemctl` or a shell as root by another program of the same
//! user.
//!
//! The helper and the PolicyKit rule are installed once (that first
//! installation asks for the password), afterwards PolicyKit keeps the
//! authorisation for the default `timestamp_timeout` of five minutes — which
//! is what the status bar shows.

use std::path::{Path, PathBuf};

use crate::utils::privilege::{is_auth_failure, run_privileged_script_with_args, verify_trusted_binary};

/// Where the root owned helper is installed when the app sets it up itself.
pub const HELPER_PATH: &str = "/usr/local/libexec/lcp-admin";

/// Where the helper lives when it was installed as part of the package.
pub const PACKAGED_HELPER_PATH: &str = "/usr/bin/lcp-admin";

/// Where the PolicyKit rule is installed.
pub const POLKIT_RULE_PATH: &str = "/etc/polkit-1/rules.d/50-linux-control-panel.rules";

/// The PolicyKit rule: the helper may be run as root for an *active* local
/// session, and the authorisation is kept for the default five minutes
/// (`auth_admin_keep`) instead of asking for a password every time.
const POLKIT_RULE: &str = r#"// Installed by the Linux Control Panel.
// Grants the root owned helper (and only the helper) the right to run with
// administrator rights. The authorisation is kept for the default
// timestamp_timeout (5 minutes), so a multi step action asks only once.
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.policykit.exec" &&
        subject.active && subject.local &&
        subject.isInGroup("wheel")) {
        var key = action.lookup("program");
        if (key == "/usr/local/libexec/lcp-admin") {
            return polkit.Result.AUTH_ADMIN_KEEP;
        }
    }
});
"#;

/// Is the helper installed and trustworthy (root owned, not writable by others)?
///
/// Either the copy the app installed itself or the one that came with the
/// package counts.
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

/// Find the helper that ships with the app.
///
/// In a bundle the helper sits next to the application binary, in a
/// development build it is in the target directory of the crate.
fn find_bundled_helper() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("lcp-admin"));
            candidates.push(dir.join("bin").join("lcp-admin"));
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // the helper crate in a development build
    candidates.push(manifest.join("helper").join("target").join("debug").join("lcp-admin"));
    // the prepared sidecar
    if let Ok(triple) = std::env::var("TAURI_ENV_TARGET_TRIPLE") {
        candidates.push(
            manifest
                .join("binaries")
                .join(format!("lcp-admin-{triple}")),
        );
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Install the helper and the PolicyKit rule.
///
/// This is the only step that asks for the password directly — afterwards
/// PolicyKit keeps the authorisation for five minutes.
#[tauri::command]
pub async fn admin_install_helper() -> Result<bool, String> {
    if helper_is_installed() {
        return Ok(true);
    }

    let source = find_bundled_helper().ok_or_else(|| {
        "The administrator helper was not found next to the application. \
         Reinstall the application to set it up."
            .to_string()
    })?;

    // The rule is passed base64 encoded so that nothing needs to be escaped.
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let encoded_rule = BASE64.encode(POLKIT_RULE.as_bytes());

    // A fixed script of the app — the paths are passed as positional
    // parameters, they are never interpolated into the script.
    let script = r#"set -eu
install -d -m 0755 -o root -g root /usr/local/libexec
install -d -m 0755 -o root -g root /etc/polkit-1/rules.d
install -m 0755 -o root -g root "$1" /usr/local/libexec/lcp-admin
printf '%s' "$2" | base64 -d > /etc/polkit-1/rules.d/50-linux-control-panel.rules.tmp
chown root:root /etc/polkit-1/rules.d/50-linux-control-panel.rules.tmp
chmod 0644 /etc/polkit-1/rules.d/50-linux-control-panel.rules.tmp
mv -f /etc/polkit-1/rules.d/50-linux-control-panel.rules.tmp /etc/polkit-1/rules.d/50-linux-control-panel.rules
"#;

    let output = run_privileged_script_with_args(
        script,
        &[
            source.to_string_lossy().to_string(),
            encoded_rule,
        ],
    )
    .await
    .map_err(|e| format!("Could not install the administrator helper: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_auth_failure(&stderr, "") {
            return Err(
                "Administrator authentication was cancelled — the helper was not installed.".to_string(),
            );
        }
        return Err(format!("Could not install the administrator helper: {stderr}"));
    }

    Ok(helper_is_installed())
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
