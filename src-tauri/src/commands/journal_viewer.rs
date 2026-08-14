

#[tauri::command]
pub async fn get_journal_logs(
    unit_filter: Option<String>,
    priority: Option<u8>,
    since_filter: Option<String>,
    until_filter: Option<String>,
) -> Result<Vec<String>, String> {
    let mut cmd = crate::utils::privilege::tokio::Command::new("journalctl");

    if since_filter.is_some() || until_filter.is_some() {
        cmd.arg("-n").arg("2000");
    } else {
        cmd.arg("-n").arg("100");
    }
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

    if let Some(since) = since_filter {
        if !since.is_empty() {
            cmd.arg("--since").arg(since);
        }
    }

    if let Some(until) = until_filter {
        if !until.is_empty() {
            cmd.arg("--until").arg(until);
        }
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
