use serde::{Deserialize, Serialize};
use std::fs;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    pub raw: String,
}

#[tauri::command]
pub async fn read_env_vars() -> Result<Vec<EnvVar>, String> {
    let content = fs::read_to_string("/etc/environment").unwrap_or_default();
    let mut vars = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            // remove surrounding quotes
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();

            vars.push(EnvVar {
                key: key.to_string(),
                value,
                raw: line.to_string(),
            });
        }
    }

    Ok(vars)
}

#[tauri::command]
pub async fn write_env_vars(vars: Vec<EnvVar>) -> Result<String, String> {
    let mut lines = Vec::new();
    lines.push("# System-wide environment variables".to_string());
    lines.push("# Managed by Control Panel".to_string());

    for var in vars {
        lines.push(format!("{}=\"{}\"", var.key, var.value));
    }

    let final_content = lines.join("\n") + "\n";

    let mut child = Command::new("pkexec")
        .args(["bash", "-c", "cat > /etc/environment"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pkexec: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(final_content.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }

    let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok("Successfully updated /etc/environment".to_string())
}
