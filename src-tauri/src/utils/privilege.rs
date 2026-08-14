use std::ffi::OsStr;
use std::process::{Child, Output, Stdio};
use std::sync::Mutex;
use std::io::Write;

pub static SUDO_PASSWORD: Mutex<Option<String>> = Mutex::new(None);

#[tauri::command]
pub async fn set_sudo_password(password: String) -> Result<(), String> {
    let mut child = ::tokio::process::Command::new("sudo")
        .arg("-k")
        .arg("-S")
        .arg("-v")
        .arg("--prompt=")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(mut stdin) = child.stdin.take() {
        use ::tokio::io::AsyncWriteExt;
        let mut pw = password.clone();
        pw.push('\n');
        let _ = stdin.write_all(pw.as_bytes()).await;
    }

    let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if output.status.success() {
        let mut guard = SUDO_PASSWORD.lock().unwrap();
        *guard = Some(password);
        Ok(())
    } else {
        Err("Incorrect sudo password".to_string())
    }
}

#[tauri::command]
pub fn clear_sudo_password() {
    let mut guard = SUDO_PASSWORD.lock().unwrap();
    *guard = None;
}

#[tauri::command]
pub fn check_sudo_status() -> bool {
    let guard = SUDO_PASSWORD.lock().unwrap();
    guard.is_some()
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

    fn build_std(&self) -> std::io::Result<(std::process::Command, Option<String>)> {
        let guard = SUDO_PASSWORD.lock().unwrap();
        if self.program == "pkexec" && guard.is_some() {
            let pw = guard.clone().unwrap();
            let mut cmd = std::process::Command::new("sudo");
            cmd.arg("-S").arg("--prompt=");
            // pkexec arguments are actually the command and its arguments!
            // e.g., pkexec systemctl restart nginx
            for arg in &self.args {
                cmd.arg(arg);
            }
            Ok((cmd, Some(pw)))
        } else if self.program == "pkexec" && guard.is_none() {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Root privileges are required. Please enable Root in the Control Panel.",
            ))
        } else {
            let mut cmd = std::process::Command::new(&self.program);
            cmd.args(&self.args);
            Ok((cmd, None))
        }
    }
    


    pub fn output(&mut self) -> std::io::Result<Output> {
        let (mut cmd, pw_opt) = self.build_std()?;
        if let Some(pw) = pw_opt {
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            let mut child = cmd.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                let mut p = pw.clone();
                p.push('\n');
                let _ = stdin.write_all(p.as_bytes());
            }
            child.wait_with_output()
        } else {
            cmd.output()
        }
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        let (mut cmd, pw_opt) = self.build_std()?;
        if let Some(pw) = pw_opt {
            cmd.stdin(Stdio::piped());
            let mut child = cmd.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                let mut p = pw.clone();
                p.push('\n');
                let _ = stdin.write_all(p.as_bytes());
            }
            Ok(child)
        } else {
            cmd.spawn()
        }
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
            let (is_pkexec, has_pw, pw_opt) = {
                let guard = SUDO_PASSWORD.lock().unwrap();
                (
                    self.program == "pkexec",
                    guard.is_some(),
                    if self.program == "pkexec" && guard.is_some() { Some(guard.clone().unwrap()) } else { None }
                )
            };

            if is_pkexec && !has_pw {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Root privileges are required. Please enable Root in the Control Panel.",
                ));
            }

            if let Some(pw) = pw_opt {
                let mut cmd = ::tokio::process::Command::new("sudo");
                if let Some(ref path) = self.current_dir {
                    cmd.current_dir(path);
                }
                cmd.arg("-S").arg("--prompt=");
                for arg in &self.args {
                    cmd.arg(arg);
                }
                cmd.stdin(Stdio::piped());
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());
                let mut child = cmd.spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    use ::tokio::io::AsyncWriteExt;
                    let mut p = pw.clone();
                    p.push('\n');
                    let _ = stdin.write_all(p.as_bytes()).await;
                }
                child.wait_with_output().await
            } else {
                let mut cmd = ::tokio::process::Command::new(&self.program);
                if let Some(ref path) = self.current_dir {
                    cmd.current_dir(path);
                }
                cmd.args(&self.args);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());
                cmd.output().await
            }
        }

        pub fn spawn(&mut self) -> std::io::Result<TokioChild> {
            let (is_pkexec, has_pw, pw_opt) = {
                let guard = SUDO_PASSWORD.lock().unwrap();
                (
                    self.program == "pkexec",
                    guard.is_some(),
                    if self.program == "pkexec" && guard.is_some() { Some(guard.clone().unwrap()) } else { None }
                )
            };

            if is_pkexec && !has_pw {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Root privileges are required. Please enable Root in the Control Panel.",
                ));
            }

            if let Some(pw) = pw_opt {
                let mut cmd = ::tokio::process::Command::new("sudo");
                if let Some(ref path) = self.current_dir {
                    cmd.current_dir(path);
                }
                cmd.arg("-S").arg("--prompt=");
                for arg in &self.args {
                    cmd.arg(arg);
                }
                cmd.stdin(Stdio::piped());
                if self.capture_stdout { cmd.stdout(Stdio::piped()); }
                if self.capture_stderr { cmd.stderr(Stdio::piped()); }
                let mut child = cmd.spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    use ::tokio::io::AsyncWriteExt;
                    let mut p = pw.clone();
                    p.push('\n');
                    ::tokio::spawn(async move {
                        let _ = stdin.write_all(p.as_bytes()).await;
                    });
                }
                Ok(child)
            } else {
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
}

/// Write file contents to a root-owned path safely via base64 decoding.
/// Automatically creates a timestamped backup (.bak.<timestamp>) and (.bak) before overwriting.
pub async fn write_file_as_root(path: &str, content: &str) -> Result<(), String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let encoded = BASE64.encode(content.as_bytes());
    let escaped_path = path.replace('\'', "'\\''");
    
    // Script performs safety backup then decodes and writes the file
    let script = format!(
        r#"if [ -f '{escaped_path}' ]; then
    TS=$(date +%Y%m%d_%H%M%S)
    cp -p '{escaped_path}' '{escaped_path}.bak.'$TS 2>/dev/null || true
    cp -p '{escaped_path}' '{escaped_path}.bak' 2>/dev/null || true
fi
echo -n '{encoded}' | base64 -d > '{escaped_path}'"#
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
