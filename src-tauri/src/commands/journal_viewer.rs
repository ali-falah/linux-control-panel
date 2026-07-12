use tokio::process::Command;
use tokio::io::AsyncReadExt;
use std::process::Stdio;

#[tauri::command]
pub async fn get_journal_logs(
    unit_filter: Option<String>,
    priority: Option<u8>,
) -> Result<Vec<String>, String> {
    let mut cmd = crate::utils::privilege::tokio::Command::new("journalctl");

    cmd.arg("-n").arg("100"); // Last 100 lines
    cmd.arg("-o").arg("json"); // JSON output
    cmd.arg("--no-pager");

    if let Some(unit) = unit_filter {
        if !unit.is_empty() {
            cmd.arg("-u").arg(unit);
        }
    }

    if let Some(prio) = priority {
        cmd.arg("-p").arg(prio.to_string());
    }

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => return Err(format!("Failed to run journalctl: {}", e)),
    };

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    
    // Each line is a JSON object
    let lines: Vec<String> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(lines)
}
