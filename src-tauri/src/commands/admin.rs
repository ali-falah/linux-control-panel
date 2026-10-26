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

use crate::utils::privilege::{
    is_auth_failure, resolve_root_program, run_privileged_output, run_privileged_with_stdin,
    verify_trusted_binary, RootAuth,
};

/// Where the root owned helper is installed when the app sets it up itself.
pub const HELPER_PATH: &str = "/usr/local/libexec/lcp-admin";

/// Where the helper lives when it was installed as part of the package.
pub const PACKAGED_HELPER_PATH: &str = "/usr/bin/lcp-admin";

/// Where the PolicyKit rule is installed. The single source of truth for the
/// path and the rule text is `root_policy.rs` — the helper carries the same
/// constants and refuses to write anything that does not match byte for byte.
pub const POLKIT_RULE_PATH: &str = crate::utils::root_policy::POLKIT_RULE_FILE;

/// The PolicyKit rule the bootstrap installs; the helper's
/// `install_polkit_rule` action verifies the content against this constant.
use crate::utils::root_policy::POLKIT_RULE_TEXT as POLKIT_RULE;

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

    // The copy must start out trustworthy: root owned and not user writable.
    // The helper-side `install` template re-checks this, and a dev-built
    // helper in `target/` is refused here with a way out.
    if let Err(reason) = verify_trusted_binary(&source.to_string_lossy()) {
        return Err(format!(
            "The helper binary is not trusted as an installation source: {reason}. \
             In a development checkout the build is yours, not root's — install \
             it manually with: sudo install -m 0755 -o root -g root \
             '{src}' /usr/local/libexec/lcp-admin",
            src = source.to_string_lossy()
        ));
    }

    let source_path = source.to_string_lossy().to_string();
    let auth = RootAuth::Polkit;

    // 1. The target directories exist (both are trusted, fixed paths).
    let directories: [String; 2] = [
        "/usr/local/libexec".to_string(),
        "/etc/polkit-1/rules.d".to_string(),
    ];
    let mkdir_args: Vec<String> = std::iter::once("-p".to_string())
        .chain(directories.into_iter())
        .collect();
    let program = resolve_root_program("mkdir").ok_or_else(|| {
        "mkdir was not found — this is not a Fedora system the panel supports.".to_string()
    })?;
    let output = run_privileged_output(&program, &mkdir_args, &auth)
        .await
        .map_err(|e| format!("Could not prepare the helper directories: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_auth_failure(&stderr, "") {
            return Err(
                "Administrator authentication was cancelled — the helper was not installed."
                    .to_string(),
            );
        }
        return Err(format!("Could not prepare the helper directories: {stderr}"));
    }

    // 2. The helper itself, from the verified source, with the one fixed
    //    template the helper accepts (see check_install in root_policy.rs).
    let install_args: Vec<String> = [
        "-m", "0755", "-o", "root", "-g", "root", &source_path, HELPER_PATH,
    ]
    .iter()
    .map(|part| part.to_string())
    .collect();
    let program = resolve_root_program("install").ok_or_else(|| {
        "install was not found — this is not a Fedora system the panel supports.".to_string()
    })?;
    let output = run_privileged_output(&program, &install_args, &auth)
        .await
        .map_err(|e| format!("Could not install the administrator helper: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_auth_failure(&stderr, "") {
            return Err(
                "Administrator authentication was cancelled — the helper was not installed."
                    .to_string(),
            );
        }
        return Err(format!("Could not install the administrator helper: {stderr}"));
    }

    // 3. The PolicyKit rule, through the helper's own action: the content
    //    travels on stdin and only gets written when it matches the rule the
    //    helper knows byte for byte.
    let output = run_privileged_with_stdin(HELPER_PATH, &["install_polkit_rule".to_string()], &auth, POLKIT_RULE)
        .await
        .map_err(|e| format!("Could not install the PolicyKit rule: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_auth_failure(&stderr, "") {
            return Err(
                "Administrator authentication was cancelled — the rule was not installed."
                    .to_string(),
            );
        }
        return Err(format!("Could not install the PolicyKit rule: {stderr}"));
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
