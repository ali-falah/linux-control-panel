use tauri::{AppHandle, Emitter};
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

static LIVE_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Default number of entries returned for an explicit time range.
/// Old behaviour was a hard 150 (no range) / 2000 (with range) — far too small
/// to inspect a busy week.
const DEFAULT_MAX_ENTRIES: usize = 20_000;
/// Hard ceiling so a single request can never exhaust memory.
const ABSOLUTE_MAX_ENTRIES: usize = 200_000;

/// A journal entry with only the fields the UI renders.
///
/// The field names match journald's own so the frontend keeps working
/// unchanged, but the payload is ~4x smaller than the raw journalctl JSON
/// (which also carries __CURSOR, __MONOTONIC_TIMESTAMP, _BOOT_ID, …).
#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    pub ts: String,
    #[serde(rename = "PRIORITY")]
    pub prio: u8,
    #[serde(rename = "MESSAGE")]
    pub message: String,
    #[serde(rename = "_SYSTEMD_UNIT", skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(rename = "SYSLOG_IDENTIFIER", skip_serializing_if = "String::is_empty")]
    pub ident: String,
    #[serde(rename = "_PID", skip_serializing_if = "String::is_empty")]
    pub pid: String,
    #[serde(rename = "_COMM", skip_serializing_if = "String::is_empty")]
    pub comm: String,
    #[serde(rename = "_EXE", skip_serializing_if = "String::is_empty")]
    pub exe: String,
    #[serde(rename = "_CMDLINE", skip_serializing_if = "String::is_empty")]
    pub cmdline: String,
    #[serde(rename = "_SYSTEMD_CGROUP", skip_serializing_if = "String::is_empty")]
    pub cgroup: String,
    #[serde(rename = "_TRANSPORT", skip_serializing_if = "String::is_empty")]
    pub transport: String,
    #[serde(rename = "_UID", skip_serializing_if = "String::is_empty")]
    pub uid: String,
    #[serde(rename = "_GID", skip_serializing_if = "String::is_empty")]
    pub gid: String,
    #[serde(rename = "_HOSTNAME", skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(rename = "_BOOT_ID", skip_serializing_if = "String::is_empty")]
    pub boot_id: String,
    #[serde(rename = "_MACHINE_ID", skip_serializing_if = "String::is_empty")]
    pub machine_id: String,
}

/// One page of journal entries plus the metadata the UI needs to page on.
#[derive(Debug, Serialize)]
pub struct JournalPage {
    /// newest first
    pub entries: Vec<JournalEntry>,
    /// number of entries in this page
    pub returned: usize,
    /// true when the range holds more entries than this page
    pub truncated: bool,
    /// timestamp (µs) of the oldest entry in this page — used to fetch older entries
    pub oldest_ts: Option<String>,
    /// wall-clock time the fetch took
    pub elapsed_ms: u64,
}

/// Parse one `journalctl -o json` line into a compact entry.
fn parse_journal_line(line: &str) -> Option<JournalEntry> {
    let value: Value = serde_json::from_str(line).ok()?;
    let text = |key: &str| -> String {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };

    // MESSAGE may be a string or an array of lines
    let message = match value.get("MESSAGE") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    };

    Some(JournalEntry {
        ts: text("__REALTIME_TIMESTAMP"),
        // journald emits PRIORITY as a string ("6"), not a number
        prio: value
            .get("PRIORITY")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
            .unwrap_or(6) as u8,
        message,
        unit: text("_SYSTEMD_UNIT"),
        ident: text("SYSLOG_IDENTIFIER"),
        pid: text("_PID"),
        comm: text("_COMM"),
        exe: text("_EXE"),
        cmdline: text("_CMDLINE"),
        cgroup: text("_SYSTEMD_CGROUP"),
        transport: text("_TRANSPORT"),
        uid: text("_UID"),
        gid: text("_GID"),
        hostname: text("_HOSTNAME"),
        boot_id: text("_BOOT_ID"),
        machine_id: text("_MACHINE_ID"),
    })
}

#[tauri::command]
pub async fn get_journal_logs(
    unit_filter: Option<String>,
    priority: Option<u8>,
    since_filter: Option<String>,
    until_filter: Option<String>,
    max_entries: Option<usize>,
) -> Result<JournalPage, String> {
    let started = std::time::Instant::now();
    let limit = max_entries
        .unwrap_or(DEFAULT_MAX_ENTRIES)
        .clamp(100, ABSOLUTE_MAX_ENTRIES);

    let mut cmd = crate::utils::privilege::tokio::Command::new("journalctl");

    // Ask for one extra entry so we can tell the user the range was truncated
    cmd.arg("-n").arg((limit + 1).to_string());
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

    cmd.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run journalctl: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "journalctl produced no stdout".to_string())?;

    // Stream the output instead of buffering the whole thing in one String
    let mut reader = BufReader::new(stdout).lines();
    let mut entries: Vec<JournalEntry> = Vec::with_capacity(limit.min(4096));
    let mut truncated = false;

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| format!("Failed to read journalctl output: {e}"))?
    {
        if entries.len() >= limit {
            truncated = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(entry) = parse_journal_line(&line) {
            entries.push(entry);
        }
    }

    let _ = child.kill().await;

    // journalctl emits oldest first — the UI wants newest first
    entries.reverse();

    let oldest_ts = entries.last().map(|e| e.ts.clone());
    let returned = entries.len();

    Ok(JournalPage {
        entries,
        returned,
        truncated,
        oldest_ts,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Count every entry in the selected range without transferring them.
///
/// Runs separately from the page fetch (and in parallel with it) so the UI
/// can show "2,501 of 87,203 entries" without blocking the table. The `-o cat`
/// output is used because it is far lighter than the full JSON.
#[tauri::command]
pub async fn count_journal_entries(
    unit_filter: Option<String>,
    priority: Option<u8>,
    since_filter: Option<String>,
    until_filter: Option<String>,
) -> Result<u64, String> {
    let mut cmd = crate::utils::privilege::tokio::Command::new("journalctl");
    cmd.arg("-o").arg("cat");
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

    cmd.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run journalctl: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "journalctl produced no stdout".to_string())?;

    let mut reader = BufReader::new(stdout).lines();
    let mut count: u64 = 0;

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| format!("Failed to read journalctl output: {e}"))?
    {
        if !line.trim().is_empty() {
            count += 1;
        }
    }

    let _ = child.kill().await;
    Ok(count)
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
                                        // emit the same compact shape the page fetch returns
                                        if let Some(entry) = parse_journal_line(&line) {
                                            let _ = app.emit("journal-live-log", entry);
                                        }
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

#[tauri::command]
pub async fn export_journal_logs_to_file(filename: String, content: String) -> Result<String, String> {
    let download_dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));

    let target_path = download_dir.join(&filename);
    tokio::fs::write(&target_path, content.as_bytes())
        .await
        .map_err(|e| format!("Failed to write export file: {e}"))?;

    Ok(target_path.to_string_lossy().to_string())
}

