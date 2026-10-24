use std::ffi::OsStr;
use std::process::{Child, Output, Stdio};

// ─── Centralised privilege escalation ─────────────────────────────────────────
//
// Every privileged step of the app goes through this module. The rules are:
//
//  * the app NEVER handles a password unless the user explicitly typed it into
//    the app's own prompt (PolicyKit keeps the password in the desktop agent),
//  * a password is only ever piped into `sudo`'s stdin — never an argument,
//    never an environment variable, never a log line,
//  * only programs from a fixed allow list may run as root, so the frontend
//    can never turn the app into "run this as root",
//  * the program is verified to be a root owned binary that no other user can
//    modify, so a user writable copy can never be executed with root rights,
//  * no shell is involved: a privileged call is always a program plus a
//    validated argument vector.

pub use crate::utils::root_policy::{
    ensure_no_shell_escape, ensure_program_allowed, ensure_repo_file_path, ensure_safe_arguments,
    ensure_safe_operand, resolve_root_program, verify_trusted_binary, ALLOWED_ROOT_PROGRAMS,
};

/// When the last privileged call succeeded — PolicyKit keeps the
/// authorisation for a few minutes after that, which the status bar shows.
static LAST_AUTHORISED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// How long PolicyKit keeps an authorisation by default
/// (`timestamp_timeout`, five minutes).
pub const AUTHORISATION_LIFETIME_SECS: u64 = 300;

/// Seconds the authorisation is still valid, `None` when it is not (or no
/// longer) valid.
pub fn authorisation_remaining() -> Option<u64> {
    let guard = LAST_AUTHORISED.lock().ok()?;
    let last = (*guard)?;
    let elapsed = last.elapsed().as_secs();
    if elapsed >= AUTHORISATION_LIFETIME_SECS {
        return None;
    }
    Some(AUTHORISATION_LIFETIME_SECS - elapsed)
}

/// Write a privileged call to the application log — the exact command that
/// ran is needed when something goes wrong.
fn log_privileged_call(args: &[String]) {
    crate::log_to_file("INFO", &format!("privileged call: {}", args.join(" ")));
}

fn note_authorised() {
    if let Ok(mut guard) = LAST_AUTHORISED.lock() {
        *guard = Some(std::time::Instant::now());
    }
}

/// How a privileged step authenticates the user.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RootAuth {
    /// PolicyKit agent (`pkexec`). The desktop agent collects the password and
    /// hands it to the privileged program directly — the app never sees it.
    Polkit,
    /// `sudo` with a password the user typed into the app. The password is
    /// piped into sudo's stdin, never passed as an argument.
    SudoPassword(String),
}

impl RootAuth {
    /// The password, if this authentication method uses one.
    pub fn password(&self) -> Option<&str> {
        match self {
            RootAuth::SudoPassword(password) => Some(password.as_str()),
            RootAuth::Polkit => None,
        }
    }
}

/// Build the privileged command for `program`.
///
/// * `RootAuth::Polkit` → `pkexec -- <program> <args>`
/// * `RootAuth::SudoPassword` → `sudo -S -p "" -- <program> <args>`
///
/// The command is returned together with the password that has to be written
/// to its stdin (if any). The password is never part of the argument list.
pub fn build_privileged_command(
    program: &str,
    args: &[String],
    auth: &RootAuth,
) -> Result<(std::process::Command, Option<String>), String> {
    ensure_program_allowed(program)?;
    ensure_no_shell_escape(program, args)?;
    ensure_safe_arguments(args)?;
    verify_trusted_binary(program)?;

    let argv = privileged_argv(program, args, auth)?;
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    Ok((cmd, auth.password().map(|password| password.to_string())))
}

/// The absolute path of pkexec.
fn pkexec_path() -> String {
    resolve_root_program("pkexec").unwrap_or_else(|| String::from("/usr/bin/pkexec"))
}

/// The full argument vector for a privileged call.
///
/// * `RootAuth::Polkit` -> `pkexec -- <program> <args>`
/// * `RootAuth::SudoPassword` -> `sudo -S -p "" -- <program> <args>`
pub fn privileged_argv(
    program: &str,
    args: &[String],
    auth: &RootAuth,
) -> Result<Vec<String>, String> {
    ensure_program_allowed(program)?;
    ensure_no_shell_escape(program, args)?;
    ensure_safe_arguments(args)?;
    verify_trusted_binary(program)?;

    let mut argv: Vec<String> = Vec::new();
    match auth {
        RootAuth::Polkit => {
            // NOTE: pkexec does not understand the "--" separator — it would
            // try to execute a program called "--". The program is passed as an
            // absolute path, so there is nothing to disambiguate.
            // pkexec itself is addressed by its absolute path as well, so the
            // call does not depend on the PATH of the desktop session.
            argv.push(pkexec_path());
            // When the root owned helper is installed the request goes through
            // it. PolicyKit then only ever authorises the helper — a cached
            // authorisation can not be used to run dnf, systemctl or a shell
            // as root by another program of the same user.
            if let Some(helper) = crate::commands::admin::installed_helper_path() {
                argv.push(String::from(helper));
            }
        }
        RootAuth::SudoPassword(_) => {
            // `-S` reads the password from stdin, an empty prompt suppresses
            // sudo's own prompt, `--` ends sudo's option parsing.
            argv.push(String::from("sudo"));
            argv.push(String::from("-S"));
            argv.push(String::from("-p"));
            argv.push(String::new());
            argv.push(String::from("--"));
        }
    }
    argv.push(program.to_string());
    argv.extend(args.iter().cloned());
    Ok(argv)
}

/// Build the privileged command for `program` (async variant).
pub fn build_privileged_tokio_command(
    program: &str,
    args: &[String],
    auth: &RootAuth,
) -> Result<(::tokio::process::Command, Option<String>), String> {
    let argv = privileged_argv(program, args, auth)?;
    let mut cmd = ::tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    Ok((cmd, auth.password().map(|password| password.to_string())))
}

/// Run a privileged program and capture its output.
///
/// The password (if any) is written to the program's stdin and the stdin is
/// closed afterwards, so it can never be read by the program itself.
pub async fn run_privileged_output(
    program: &str,
    args: &[String],
    auth: &RootAuth,
) -> Result<std::process::Output, String> {
    use ::tokio::io::AsyncWriteExt;

    let (mut cmd, password) = build_privileged_tokio_command(program, args, auth)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if password.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {program} with administrator rights: {e}"))?;

    if let Some(password) = password {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(password.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            let _ = stdin.shutdown().await;
        }
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to run {program} with administrator rights: {e}"))?;
    // the agent authorised the request, so the authorisation is valid now
    note_authorised();
    Ok(output)
}

/// Run a privileged program, write `input` to its stdin and capture the output.
///
/// This is how data (a crontab, a configuration file) is handed over to a
/// privileged program without ever passing it through a shell.
pub async fn run_privileged_with_stdin(
    program: &str,
    args: &[String],
    auth: &RootAuth,
    input: &str,
) -> Result<std::process::Output, String> {
    use ::tokio::io::AsyncWriteExt;

    if auth.password().is_some() {
        return Err(
            "A program that reads its input from stdin cannot be combined with a sudo password."
                .to_string(),
        );
    }
    let (mut cmd, password) = build_privileged_tokio_command(program, args, auth)?;
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {program} with administrator rights: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        if let Some(password) = password {
            let _ = stdin.write_all(password.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
        }
        let _ = stdin.write_all(input.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to run {program} with administrator rights: {e}"))?;
    // the agent authorised the request, so the authorisation is valid now
    note_authorised();
    Ok(output)
}

/// Run one of the app's own fixed maintenance scripts with administrator
/// rights.
///
/// The script **must** be a compile time literal of the app. Values that come
/// from the user are never interpolated into the script — they are passed as
/// positional parameters (`$1`, `$2`, …) and have to be quoted inside the
/// script.
pub async fn run_privileged_script(script: &str) -> std::io::Result<std::process::Output> {
    run_privileged_script_with_args(script, &[]).await
}

/// The argument vector for one of the app's own fixed maintenance scripts.
///
/// `pkexec <shell> -c <script> [args…]` — note that pkexec does not
/// understand the "--" separator.
pub fn privileged_script_argv(shell: &str, script: &str, args: &[String]) -> Vec<String> {
    let mut argv = vec![pkexec_path(), shell.to_string(), String::from("-c"), script.to_string()];
    argv.extend(args.iter().cloned());
    argv
}

/// Run one of the app's own fixed maintenance scripts with administrator
/// rights, passing the given positional parameters.
pub async fn run_privileged_script_with_args(
    script: &str,
    args: &[String],
) -> std::io::Result<std::process::Output> {
    let deny = |message: String| std::io::Error::new(std::io::ErrorKind::PermissionDenied, message);
    ensure_safe_arguments(args).map_err(deny)?;

    let shell = resolve_root_program("bash").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "bash was not found on this system.",
        )
    })?;
    ensure_program_allowed(&shell).map_err(deny)?;
    verify_trusted_binary(&shell).map_err(deny)?;

    let argv = privileged_script_argv(&shell, script, args);
    let mut cmd = ::tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    cmd.output()
        .await
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to run the administrator script: {e}")))
}

/// Drop the cached sudo credential again, so a password typed for one action
/// does not stay usable for the rest of the session.
pub fn invalidate_cached_credentials() {
    let _ = std::process::Command::new("sudo")
        .arg("-k")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Detect an authentication failure in the output of a privileged program.
pub fn is_auth_failure(stderr: &str, stdout: &str) -> bool {
    let combined = format!("{stderr} {stdout}").to_lowercase();
    combined.contains("not authorized")
        || combined.contains("polkit-error-quark")
        || combined.contains("authentication failed")
        || combined.contains("incorrect password")
        || combined.contains("sorry, try again")
        || combined.contains("no password was provided")
        || combined.contains("a password is required")
        || combined.contains("action matches no rule")
        || combined.contains("dismissed")
        || combined.contains("canceled")
        || combined.contains("cancelled")
}

fn allowed_root_config_path(path: &std::path::Path) -> bool {
    use std::path::Component;
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir | Component::CurDir)) {
        return false;
    }

    let value = path.to_string_lossy();
    let fixed = [
        "/etc/hosts", "/etc/environment", "/etc/default/grub", "/etc/default/grub.bak",
        "/etc/selinux/config", "/etc/selinux/config.bak", "/etc/apt/sources.list",
        "/etc/profile", "/etc/bash.bashrc", "/etc/nginx/nginx.conf",
    ];
    if fixed.contains(&value.as_ref()) {
        return true;
    }

    let allowed_file = |prefix: &str, extensions: &[&str]| {
        value.starts_with(prefix)
            && path.parent() == Some(std::path::Path::new(prefix.trim_end_matches('/')))
            && path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                !name.is_empty() && !name.starts_with('.') && extensions.iter().any(|ext| name.ends_with(ext))
            })
    };

    allowed_file("/etc/profile.d/", &[".sh"])
        || allowed_file("/etc/yum.repos.d/", &[".repo"])
        || allowed_file("/etc/apt/sources.list.d/", &[".list", ".sources"])
        || (value.starts_with("/etc/nginx/conf.d/") && (value.ends_with(".conf") || value.ends_with(".conf.tmp_nginx_test")))
        || (value.starts_with("/etc/nginx/sites-available/") && path.parent() == Some(std::path::Path::new("/etc/nginx/sites-available")))
        || (value.starts_with("/etc/nginx/sites-enabled/") && path.parent() == Some(std::path::Path::new("/etc/nginx/sites-enabled")))
        || (path.strip_prefix("/etc/systemd/system").ok().is_some_and(|relative| {
            let mut parts = relative.components();
            matches!(parts.next(), Some(Component::Normal(unit_dir)) if unit_dir.to_string_lossy().ends_with(".d"))
                && matches!(parts.next(), Some(Component::Normal(file)) if file == "override.conf")
                && parts.next().is_none()
        }))
}

// A wrapper that mimics std::process::Command and tokio::process::Command
pub struct Command {
    program: String,
    args: Vec<String>,
}

/// The argument vector of a privileged call, after it has been validated.
///
/// A command created with "pkexec" is rewritten to
/// `pkexec -- <absolute program> <args>`: the program is resolved to an
/// absolute path, checked against the allow list and verified to be a root
/// owned binary. A program that is not allowed never reaches the kernel with
/// root rights.
fn validated_privileged_argv(
    program: &str,
    args: &[String],
) -> Result<Option<Vec<String>>, std::io::Error> {
    if program != "pkexec" {
        return Ok(None);
    }
    let (first, rest) = args.split_first().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "No program given for the administrator operation.",
        )
    })?;

    let resolved = resolve_root_program(first).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Refused to run {first} with administrator rights: the program was not found."),
        )
    })?;

    // The argument vector is built in exactly one place (`privileged_argv`) so
    // that the call the app makes and the call that is verified can never
    // drift apart again.
    let argv = privileged_argv(&resolved, rest, &RootAuth::Polkit).map_err(|message| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, message)
    })?;
    Ok(Some(argv))
}

impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            program: program.as_ref().to_string_lossy().to_string(),
            args: Vec::new(),
        }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.as_ref().to_string_lossy().to_string());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.args.push(arg.as_ref().to_string_lossy().to_string());
        }
        self
    }

    pub fn output(&mut self) -> std::io::Result<Output> {
        let mut cmd = std::process::Command::new(&self.program);
        let privileged = self.program == "pkexec" || self.program == "sudo";
        match validated_privileged_argv(&self.program, &self.args)? {
            Some(argv) => {
                cmd = std::process::Command::new(&argv[0]);
                cmd.args(&argv[1..]);
            }
            None => {
                cmd.args(&self.args);
            }
        }
        let output = cmd.output()?;
        if privileged && output.status.success() {
            note_authorised();
        }
        Ok(output)
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        let mut cmd = std::process::Command::new(&self.program);
        match validated_privileged_argv(&self.program, &self.args)? {
            Some(argv) => {
                cmd = std::process::Command::new(&argv[0]);
                cmd.args(&argv[1..]);
            }
            None => {
                cmd.args(&self.args);
            }
        }
        cmd.spawn()
    }
}

pub mod tokio {
    use super::*;
    use ::tokio::process::Child as TokioChild;
    use std::path::{Path, PathBuf};

    pub struct Command {
        program: String,
        args: Vec<String>,
        capture_stdout: bool,
        capture_stderr: bool,
        current_dir: Option<PathBuf>,
    }

    impl Command {
        pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
            Self {
                program: program.as_ref().to_string_lossy().to_string(),
                args: Vec::new(),
                capture_stdout: false,
                capture_stderr: false,
                current_dir: None,
            }
        }

        pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
            self.args.push(arg.as_ref().to_string_lossy().to_string());
            self
        }

        pub fn args<I, S>(&mut self, args: I) -> &mut Self
        where
            I: IntoIterator<Item = S>,
            S: AsRef<OsStr>,
        {
            for arg in args {
                self.args.push(arg.as_ref().to_string_lossy().to_string());
            }
            self
        }

        pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
            self.current_dir = Some(dir.as_ref().to_path_buf());
            self
        }

        pub fn stdin<T: Into<Stdio>>(&mut self, _cfg: T) -> &mut Self {
            self
        }

        pub fn stdout<T: Into<Stdio>>(&mut self, _cfg: T) -> &mut Self {
            self.capture_stdout = true;
            self
        }

        pub fn stderr<T: Into<Stdio>>(&mut self, _cfg: T) -> &mut Self {
            self.capture_stderr = true;
            self
        }

        pub async fn output(&mut self) -> std::io::Result<Output> {
            let mut cmd = ::tokio::process::Command::new(&self.program);
            let privileged = self.program == "pkexec" || self.program == "sudo";
            if let Some(ref path) = self.current_dir {
                cmd.current_dir(path);
            }
            match validated_privileged_argv(&self.program, &self.args)? {
                Some(argv) => {
                    cmd = ::tokio::process::Command::new(&argv[0]);
                    if let Some(ref path) = self.current_dir {
                        cmd.current_dir(path);
                    }
                    cmd.args(&argv[1..]);
                }
                None => {
                cmd.args(&self.args);
            }
            }
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            let output = cmd.output().await.map_err(|e| {
                // Name the command in the error — "No such file or directory"
                // on its own says nothing about what could not be started.
                let command = format!(
                    "{} {}",
                    cmd.as_std().get_program().to_string_lossy(),
                    self.args.join(" ")
                );
                crate::log_to_file("ERROR", &format!("privileged command failed to start: {command}: {e}"));
                std::io::Error::new(e.kind(), format!("{command}: {e}"))
            })?;
            if privileged {
                log_privileged_call(&self.args);
            }
            if privileged && output.status.success() {
                note_authorised();
            }
            Ok(output)
        }

        pub fn spawn(&mut self) -> std::io::Result<TokioChild> {
            let mut cmd = ::tokio::process::Command::new(&self.program);
            if let Some(ref path) = self.current_dir {
                cmd.current_dir(path);
            }
            match validated_privileged_argv(&self.program, &self.args)? {
                Some(argv) => {
                    cmd = ::tokio::process::Command::new(&argv[0]);
                    if let Some(ref path) = self.current_dir {
                        cmd.current_dir(path);
                    }
                    cmd.args(&argv[1..]);
                }
                None => {
                cmd.args(&self.args);
            }
            }
            if self.capture_stdout { cmd.stdout(Stdio::piped()); }
            if self.capture_stderr { cmd.stderr(Stdio::piped()); }
            cmd.spawn()
        }
    }
}

/// Write file contents to a root-owned path safely via base64 decoding.
/// Automatically creates a timestamped backup (.bak.<timestamp>) and (.bak) before overwriting.
pub async fn write_file_as_root(path: &str, content: &str) -> Result<(), String> {
    let target = std::path::Path::new(path);
    if !allowed_root_config_path(target) {
        return Err(format!("Blocked write to unsupported privileged path: {path}"));
    }
    if target.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("Blocked write through a symbolic link: {path}"));
    }
    let mut component_path = std::path::PathBuf::new();
    for component in target.components() {
        component_path.push(component.as_os_str());
        if component_path.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(format!("Blocked privileged path containing a symbolic link: {path}"));
        }
    }
    if let Some(parent) = target.parent() {
        let canonical_parent = parent.canonicalize().map_err(|e| format!("Cannot validate privileged path {path}: {e}"))?;
        if !canonical_parent.starts_with("/etc") {
            return Err(format!("Blocked privileged path outside /etc: {path}"));
        }
    }
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let encoded = BASE64.encode(content.as_bytes());
    
    // Back up first, then atomically replace the file so a failed write cannot truncate it.
    // The path is passed as a positional parameter, the content is base64
    // encoded — nothing the user controls is ever part of the script itself.
    let script = String::from(
        r#"set -e
target="$1"
if [ -f "$target" ]; then
    TS=$(date +%Y%m%d_%H%M%S)
    cp -p "$target" "$target.bak."$TS
    cp -p "$target" "$target.bak"
fi
tmp=$(mktemp "$target.tmp.XXXXXX")
trap 'rm -f "$tmp"' EXIT
echo -n "$2" | base64 -d > "$tmp"
if [ -e "$target" ]; then
    chown --reference="$target" "$tmp"
    chmod --reference="$target" "$tmp"
fi
mv -f "$tmp" "$target"
trap - EXIT"#,
    );
    
    let output = run_privileged_script_with_args(&script, &[path.to_string(), encoded.clone()])
        .await
        .map_err(|e| format!("Failed to execute root write: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        let out = String::from_utf8_lossy(&output.stdout).to_string();
        let combined = if err.is_empty() { out } else { err };
        return Err(format!("Failed to write {path}: {combined}"));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_outside_the_allow_list_is_refused() {
        assert!(ensure_program_allowed("/usr/bin/dnf").is_ok());
        assert!(ensure_program_allowed("/usr/bin/systemctl").is_ok());
        // the frontend can never pick an arbitrary program
        assert!(ensure_program_allowed("/usr/bin/curl").is_err());
        assert!(ensure_program_allowed("/bin/sh").is_ok());
    }

    #[test]
    fn a_shell_escape_is_refused() {
        let args = vec!["-c".to_string(), "id".to_string()];
        assert!(ensure_no_shell_escape("/bin/bash", &args).is_err());
        let plain = vec!["restart".to_string(), "nginx".to_string()];
        assert!(ensure_no_shell_escape("/bin/bash", &plain).is_ok());
    }

    #[test]
    fn control_characters_and_empty_arguments_are_refused() {
        assert!(ensure_safe_arguments(&["ok".to_string()]).is_ok());
        assert!(ensure_safe_arguments(&["".to_string()]).is_err());
        assert!(ensure_safe_arguments(&["a\nb".to_string()]).is_err());
        assert!(ensure_safe_arguments(&["a\0b".to_string()]).is_err());
    }

    #[test]
    fn a_binary_that_is_not_root_owned_is_refused() {
        // /tmp is writable by every user
        assert!(verify_trusted_binary("/tmp").is_err());
    }

    #[test]
    fn an_operand_that_would_be_read_as_an_option_is_refused() {
        assert!(ensure_safe_operand("mesa-libGL", "package").is_ok());
        assert!(ensure_safe_operand("--setopt=reposdir=/tmp/evil", "package").is_err());
        assert!(ensure_safe_operand("-rf", "path").is_err());
        assert!(ensure_safe_operand("", "package").is_err());
        assert!(ensure_safe_operand("a\nb", "package").is_err());
    }

    #[test]
    fn only_repository_files_of_the_package_manager_are_allowed() {
        assert!(ensure_repo_file_path("/etc/yum.repos.d/example.repo").is_ok());
        assert!(ensure_repo_file_path("/etc/apt/sources.list.d/example.list").is_ok());
        // an arbitrary file must never be touched with root rights
        assert!(ensure_repo_file_path("/etc/shadow").is_err());
        assert!(ensure_repo_file_path("/etc/passwd").is_err());
        assert!(ensure_repo_file_path("/etc/yum.repos.d/../../shadow").is_err());
        assert!(ensure_repo_file_path("/tmp/whatever.repo").is_err());
        assert!(ensure_repo_file_path("relative/path.repo").is_err());
    }

    #[test]
    fn the_privileged_command_does_not_contain_a_separator_pkexec_does_not_understand() {
        // this is the path the app really takes: the wrapper turns
        // `pkexec systemctl restart nginx.service` into the final argv
        let call = vec![
            String::from("systemctl"),
            String::from("restart"),
            String::from("nginx.service"),
        ];
        let argv = validated_privileged_argv("pkexec", &call).unwrap().unwrap();
        // pkexec is addressed by its absolute path, so the call does not
        // depend on the PATH of the desktop session
        assert_eq!(argv[0], "/usr/bin/pkexec");
        // pkexec would try to run a program called "--"
        assert!(!argv.iter().any(|arg| arg == "--"));
        assert_eq!(argv[1], "/usr/bin/systemctl");
        assert_eq!(argv[2], "restart");
        assert_eq!(argv[3], "nginx.service");
    }

    #[test]
    fn no_privileged_command_ever_contains_a_separator_pkexec_does_not_understand() {
        // the wrapper (used by every module)
        let call = vec![
            String::from("systemctl"),
            String::from("stop"),
            String::from("nginx.service"),
        ];
        let argv = validated_privileged_argv("pkexec", &call).unwrap().unwrap();
        assert!(!argv.iter().any(|arg| arg == "--"), "{argv:?}");

        // the dnf path
        let args = vec![String::from("upgrade"), String::from("--best")];
        let argv = privileged_argv("/usr/bin/dnf", &args, &RootAuth::Polkit).unwrap();
        assert!(!argv.iter().any(|arg| arg == "--"), "{argv:?}");

        // the fixed script path (SELinux, audit, GRUB, cron)
        let argv = privileged_script_argv("/bin/bash", "set -e", &[]);
        assert!(!argv.iter().any(|arg| arg == "--"), "{argv:?}");
        assert_eq!(argv[0], "/usr/bin/pkexec");
        assert_eq!(argv[1], "/bin/bash");
        assert_eq!(argv[2], "-c");
    }

    #[test]
    fn an_authentication_failure_is_detected() {
        assert!(is_auth_failure("Error executing command as another user: Not authorized", ""));
        assert!(is_auth_failure("Sorry, try again.", ""));
        assert!(!is_auth_failure("Complete!", ""));
    }
}
