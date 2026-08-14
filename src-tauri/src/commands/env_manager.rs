use serde::{Deserialize, Serialize};
use std::fs;

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

    // Write safely via base64 decoding as root
    crate::utils::privilege::write_file_as_root("/etc/environment", &final_content).await?;

    Ok("Successfully updated /etc/environment".to_string())
}
