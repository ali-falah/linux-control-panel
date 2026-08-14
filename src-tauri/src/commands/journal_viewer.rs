use tauri::{AppHandle, Emitter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

static LIVE_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);

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
        cmd.arg("-n").arg("150");
    }
    cmd.arg("-o").arg("json"); // JSON output
    cmd.arg("--no-pager");

    if let Some(unit) = unit_filter {
        let clean = unit.trim();
        if !clean.is_empty() {
            cmd.arg("-u").arg(clean);
        }
    }

    if let Some(prio) = priority {
        cmd.arg("-p").arg(prio.to_string());
    }

    if let Some(since) = since_filter {
        let clean = since.trim();
        if !clean.is_empty() {
            cmd.arg("--since").arg(clean);
        }
    }

    if let Some(until) = until_filter {
        let clean = until.trim();
        if !clean.is_empty() {
            cmd.arg("--until").arg(clean);
        }
    }

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => return Err(format!("Failed to run journalctl: {e}")),
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

#[tauri::command]
pub async fn start_journal_live_stream(
    app: AppHandle,
    unit_filter: Option<String>,
    priority: Option<u8>,
) -> Result<(), String> {
    if LIVE_STREAM_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(()); // Stream already running
    }

    tokio::spawn(async move {
        let mut cmd = tokio::process::Command::new("journalctl");
        cmd.args(["-f", "-n", "0", "-o", "json", "--no-pager"]);

        if let Some(unit) = unit_filter {
            let clean = unit.trim().to_string();
            if !clean.is_empty() {
                cmd.args(["-u", &clean]);
            }
        }

        if let Some(prio) = priority {
            cmd.args(["-p", &prio.to_string()]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::null());

        if let Ok(mut child) = cmd.spawn() {
            if let Some(stdout) = child.stdout.take() {
                let mut reader = BufReader::new(stdout).lines();
                while LIVE_STREAM_ACTIVE.load(Ordering::SeqCst) {
                    tokio::select! {
                        line_res = reader.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    if !line.trim().is_empty() {
                                        let _ = app.emit("journal-live-log", line);
                                    }
                                }
                                _ => break,
                            }
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}
                    }
                }
            }
            let _ = child.kill().await;
        }
        LIVE_STREAM_ACTIVE.store(false, Ordering::SeqCst);
    });

    Ok(())
}

#[tauri::command]
pub fn stop_journal_live_stream() {
    LIVE_STREAM_ACTIVE.store(false, Ordering::SeqCst);
}
