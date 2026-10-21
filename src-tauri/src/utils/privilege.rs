use std::ffi::OsStr;
use std::process::{Child, Output, Stdio};

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
        cmd.args(&self.args);
        cmd.output()
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        let mut cmd = std::process::Command::new(&self.program);
        cmd.args(&self.args);
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
            if let Some(ref path) = self.current_dir {
                cmd.current_dir(path);
            }
            cmd.args(&self.args);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            cmd.output().await
        }

        pub fn spawn(&mut self) -> std::io::Result<TokioChild> {
            let mut cmd = ::tokio::process::Command::new(&self.program);
            if let Some(ref path) = self.current_dir {
                cmd.current_dir(path);
            }
            cmd.args(&self.args);
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
    let escaped_path = path.replace('\'', "'\\''");
    
    // Back up first, then atomically replace the file so a failed write cannot truncate it.
    let script = format!(
        r#"set -e
if [ -f '{escaped_path}' ]; then
    TS=$(date +%Y%m%d_%H%M%S)
    cp -p '{escaped_path}' '{escaped_path}.bak.'$TS
    cp -p '{escaped_path}' '{escaped_path}.bak'
fi
tmp=$(mktemp '{escaped_path}.tmp.XXXXXX')
trap 'rm -f "$tmp"' EXIT
echo -n '{encoded}' | base64 -d > "$tmp"
if [ -e '{escaped_path}' ]; then
    chown --reference='{escaped_path}' "$tmp"
    chmod --reference='{escaped_path}' "$tmp"
fi
mv -f "$tmp" '{escaped_path}'
trap - EXIT"#
    );
    
    let mut cmd = tokio::Command::new("pkexec");
    cmd.args(["bash", "-c", &script]);
    
    let output = cmd.output().await.map_err(|e| format!("Failed to execute root write: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        let out = String::from_utf8_lossy(&output.stdout).to_string();
        let combined = if err.is_empty() { out } else { err };
        return Err(format!("Failed to write {path}: {combined}"));
    }
    
    Ok(())
}
