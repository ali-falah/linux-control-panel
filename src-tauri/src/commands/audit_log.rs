use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use crate::utils::privilege::tokio::Command;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthEvent {
    pub timestamp: String,
    pub user: String,
    pub event_type: String,
    pub source_ip: String,
    pub result: String,
    pub details: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditLogEvent {
    pub timestamp: String,
    pub user: String,
    pub command: String,
    pub cwd: String,
    pub result: String,
    pub key: String,
    pub auid: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeThreat {
    pub id: String,
    pub timestamp: String,
    pub title: String,
    pub description: String,
    pub severity: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditdStatus {
    pub installed: bool,
    pub running: bool,
    pub rules_configured: bool,
}

#[tauri::command]
pub async fn get_auth_events(
    since_days: Option<u32>,
    custom_start: Option<String>,
    custom_end: Option<String>
) -> Result<Vec<AuthEvent>, String> {
    // Try reading /var/log/secure or /var/log/auth.log via pkexec (needs root)
    // If root is not available, fall back to journalctl which works without root
    let script = r#"
    if [ -r /var/log/secure ]; then
        tail -n 2000 /var/log/secure
    elif [ -r /var/log/auth.log ]; then
        tail -n 2000 /var/log/auth.log
    else
        echo "__FALLBACK__"
    fi
    "#;

    let output = Command::new("pkexec")
        .args(["bash", "-c", script])
        .stderr(Stdio::piped())
        .output()
        .await;

    let raw_events = match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            if s.trim() == "__FALLBACK__" || s.trim().is_empty() {
                // Fall back to journalctl (no root needed)
                return get_auth_events_journalctl(since_days, custom_start, custom_end).await;
            } else {
                parse_auth_log_text(&s)?
            }
        }
        _ => {
            // Root not available or command failed — use journalctl fallback
            return get_auth_events_journalctl(since_days, custom_start, custom_end).await;
        }
    };
    Ok(raw_events)
}

async fn get_auth_events_journalctl(
    since_days: Option<u32>,
    custom_start: Option<String>,
    custom_end: Option<String>
) -> Result<Vec<AuthEvent>, String> {
    // Use JSON format — MESSAGE field is always complete (no line-wrapping),
    // and _UID gives us the kernel-verified real UID for accurate user resolution.
    let mut cmd = ::tokio::process::Command::new("journalctl");
    cmd.args(["-n", "2000", "--no-pager", "-o", "json",
              "_COMM=sshd", "_COMM=sudo", "_COMM=su", "_COMM=su-l"]);

    if let Some(days) = since_days {
        cmd.arg("--since").arg(format!("{} days ago", days));
    } else {
        if let Some(ref start) = custom_start {
            cmd.arg("--since").arg(start);
        }
        if let Some(ref end) = custom_end {
            cmd.arg("--until").arg(end);
        }
    }

    let output = cmd.output().await.map_err(|e| e.to_string())?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut events: Vec<AuthEvent> = Vec::new();

    // Process from newest to oldest (lines are oldest to newest)
    for raw_line in text.lines().rev() {
        if raw_line.trim().is_empty() { continue; }
        let entry: serde_json::Value = match serde_json::from_str(raw_line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg   = entry["MESSAGE"].as_str().unwrap_or("").to_string();
        let comm  = entry["_COMM"].as_str().unwrap_or("").to_string();
        let uid   = entry["_UID"].as_str().unwrap_or("");
        // Timestamp from _SOURCE_REALTIME_TIMESTAMP or __REALTIME_TIMESTAMP (microseconds since epoch)
        let ts_us = entry["__REALTIME_TIMESTAMP"].as_str()
            .or_else(|| entry["_SOURCE_REALTIME_TIMESTAMP"].as_str())
            .unwrap_or("0");
        let timestamp = parse_microsecond_timestamp(ts_us);
        // Username: prefer resolved UID (kernel-verified), fall back to message extraction
        let uid_user = resolve_uid(uid);

        if comm == "sshd" || msg.contains("sshd") {
            if msg.contains("Accepted ") {
                let user = extract_between(&msg, "for ", " from ").unwrap_or_else(|| uid_user.clone());
                let ip   = extract_between(&msg, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent { timestamp, user, event_type: "SSH Login".to_string(), source_ip: ip, result: "Success".to_string(), details: "SSH session established".to_string() });
            } else if msg.contains("Failed ") {
                let user = extract_between(&msg, "for ", " from ").unwrap_or_else(|| uid_user.clone());
                let ip   = extract_between(&msg, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent { timestamp, user, event_type: "SSH Login".to_string(), source_ip: ip, result: "Failure".to_string(), details: "SSH authentication failed".to_string() });
            } else if msg.contains("Invalid user") || msg.contains("invalid user") {
                let user = extract_between(&msg, "Invalid user ", " from ")
                    .or_else(|| extract_between(&msg, "invalid user ", " from "))
                    .unwrap_or_else(|| "unknown".to_string());
                let ip = extract_between(&msg, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent { timestamp, user, event_type: "SSH Login".to_string(), source_ip: ip, result: "Failure".to_string(), details: "Invalid user attempted SSH login".to_string() });
            }
        } else if comm == "sudo" {
            if msg.contains("COMMAND=") {
                // msg format: "   ali : TTY=pts/0 ; PWD=/... ; USER=root ; COMMAND=/..."
                let user = msg.trim().split(':').next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| uid_user.clone());
                let target = extract_between(&msg, "USER=", " ;").unwrap_or_else(|| "root".to_string());
                let cmd    = msg.split("COMMAND=").nth(1).unwrap_or("").trim().to_string();
                events.push(AuthEvent { timestamp, user, event_type: "Sudo Run".to_string(), source_ip: "local".to_string(), result: "Success".to_string(), details: format!("Run as {}: {}", target, cmd) });
            } else if msg.contains("authentication failure") || msg.contains("conversation failed") {
                // PAM auth failure message ends with "...ruser=ali rhost=  user=ali"
                // Search for " user=" with leading space to avoid matching "ruser="
                let user = msg.split(" user=").last()
                    .map(|s| s.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| uid_user.clone());
                events.push(AuthEvent { timestamp, user, event_type: "Sudo Failure".to_string(), source_ip: "local".to_string(), result: "Failure".to_string(), details: "Sudo authentication failed".to_string() });
            }
        } else if comm == "su" || comm == "su-l" {
            if msg.contains("session opened") {
                let user = extract_between(&msg, "for user ", " by ").unwrap_or_else(|| uid_user.clone());
                let by   = extract_between(&msg, "by ", "(").unwrap_or_else(|| "unknown".to_string());
                events.push(AuthEvent { timestamp, user: format!("{} -> {}", by, user), event_type: "Su Attempt".to_string(), source_ip: "local".to_string(), result: "Success".to_string(), details: "Switch user session opened".to_string() });
            } else if msg.contains("authentication failure") {
                let user = msg.split(" user=").last()
                    .map(|s| s.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| uid_user.clone());
                events.push(AuthEvent { timestamp, user, event_type: "Su Attempt".to_string(), source_ip: "local".to_string(), result: "Failure".to_string(), details: "Su authentication failed".to_string() });
            }
        }

        if events.len() >= 200 { break; }
    }

    Ok(events)
}


fn parse_auth_log_text(text: &str) -> Result<Vec<AuthEvent>, String> {
    let mut events = Vec::new();

    for line in text.lines().rev() {
        if line.trim().is_empty() {
            continue;
        }

        // Extract timestamp (first 3-4 fields, e.g. "Jul 20 10:15:30" or ISO "2026-07-20T10:15:30...")
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let timestamp = if parts[0].contains('T') || parts[0].contains('-') {
            // ISO format from journalctl
            parts[0].trim_end_matches('+').to_string()
        } else if parts.len() >= 3 {
            format!("{} {} {}", parts[0], parts[1], parts[2])
        } else {
            "Unknown".to_string()
        };

        // 1. SSH Logins
        if line.contains("sshd[") || line.contains("sshd:") {
            if line.contains("Accepted ") {
                let user = extract_between(line, "for ", " from ").unwrap_or_else(|| "unknown".to_string());
                let ip = extract_between(line, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "SSH Login".to_string(),
                    source_ip: ip,
                    result: "Success".to_string(),
                    details: "SSH session established".to_string(),
                });
            } else if line.contains("Failed ") {
                let user = extract_between(line, "for ", " from ").unwrap_or_else(|| "unknown".to_string());
                let ip = extract_between(line, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "SSH Login".to_string(),
                    source_ip: ip,
                    result: "Failure".to_string(),
                    details: "SSH authentication failed".to_string(),
                });
            } else if line.contains("Invalid user") || line.contains("invalid user") {
                let user = extract_between(line, "Invalid user ", " from ")
                    .or_else(|| extract_between(line, "invalid user ", " from "))
                    .unwrap_or_else(|| "unknown".to_string());
                let ip = extract_between(line, "from ", " port").unwrap_or_else(|| "local".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "SSH Login".to_string(),
                    source_ip: ip,
                    result: "Failure".to_string(),
                    details: "Invalid user attempted SSH login".to_string(),
                });
            }
        }
        // 2. Sudo usage
        else if line.contains("sudo:") || line.contains("sudo[") {
            if line.contains("COMMAND=") {
                // Extract username — handles multiple log formats:
                // /var/log/secure:  "hostname sudo: USER : TTY=..."
                // journalctl:       "2026-... host sudo[1234]:    USER : TTY=..."
                let user = if let Some(after_bracket) = line.split("]:").nth(1) {
                    // journalctl format: ]:   USERNAME :
                    after_bracket.trim().split(':').next()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "unknown".to_string())
                } else {
                    // /var/log/secure format: "sudo: USERNAME :"
                    extract_between(line, "sudo: ", " :")
                        .or_else(|| parts.get(4).map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown".to_string())
                };
                let target_user = extract_between(line, "USER=", " ;").unwrap_or_else(|| "root".to_string());
                let cmd = line.split("COMMAND=").nth(1).unwrap_or("").trim().to_string();
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "Sudo Run".to_string(),
                    source_ip: "local".to_string(),
                    result: "Success".to_string(),
                    details: format!("Run as {}: {}", target_user, cmd),
                });
            } else if line.contains("authentication failure") || line.contains("conversation failed") {
                // user= may appear as "user=ali " or "user=ali\n" or end of line
                let user = extract_between(line, "user=", " ")
                    .or_else(|| {
                        line.split("user=").nth(1).map(|s| s.split_whitespace().next().unwrap_or("unknown").to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "Sudo Failure".to_string(),
                    source_ip: "local".to_string(),
                    result: "Failure".to_string(),
                    details: "Sudo authentication failed".to_string(),
                });
            }
        }
        // 3. Su attempts
        else if line.contains("su:") || line.contains("su-l:") || line.contains("su[") {
            if line.contains("session opened") {
                let user = extract_between(line, "for user ", " by ").unwrap_or_else(|| "unknown".to_string());
                let by = extract_between(line, "by ", "(").unwrap_or_else(|| "unknown".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user: format!("{} -> {}", by, user),
                    event_type: "Su Attempt".to_string(),
                    source_ip: "local".to_string(),
                    result: "Success".to_string(),
                    details: "Switch user session opened".to_string(),
                });
            } else if line.contains("authentication failure") {
                let user = extract_between(line, "user=", " ").unwrap_or_else(|| "unknown".to_string());
                events.push(AuthEvent {
                    timestamp: timestamp.clone(),
                    user,
                    event_type: "Su Attempt".to_string(),
                    source_ip: "local".to_string(),
                    result: "Failure".to_string(),
                    details: "Su authentication failed".to_string(),
                });
            }
        }
        // 4. PAM failures
        else if line.contains("pam_unix") && line.contains("authentication failure") {
            let user = extract_between(line, "user=", " ").unwrap_or_else(|| "unknown".to_string());
            let rhost = extract_between(line, "rhost=", " ").unwrap_or_else(|| "local".to_string());
            events.push(AuthEvent {
                timestamp: timestamp.clone(),
                user,
                event_type: "PAM Auth".to_string(),
                source_ip: rhost,
                result: "Failure".to_string(),
                details: "PAM authentication failure".to_string(),
            });
        }

        if events.len() >= 200 {
            break;
        }
    }

    Ok(events)
}

#[tauri::command]
pub async fn check_auditd_status() -> Result<AuditdStatus, String> {
    let installed = Command::new("which")
        .arg("auditctl")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "auditd"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    // /etc/audit/rules.d/ is root-owned (mode 750), so Path::exists() fails for non-root.
    // Use pkexec to check the file via elevated access.
    let rules_configured = Command::new("pkexec")
        .args(["ls", "/etc/audit/rules.d/99-control-panel.rules"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok(AuditdStatus {
        installed,
        running,
        rules_configured,
    })
}

#[tauri::command]
pub async fn setup_auditd_rules() -> Result<String, String> {
    let script = r#"set -e
if ! command -v auditctl >/dev/null 2>&1; then
  if command -v dnf >/dev/null 2>&1; then
    dnf install -y audit || true
  elif command -v apt-get >/dev/null 2>&1; then
    apt-get update && apt-get install -y auditd || true
  fi
fi
mkdir -p /etc/audit/rules.d
cat << 'EOF' > /etc/audit/rules.d/99-control-panel.rules
-a always,exit -F arch=b64 -S execve -k cmd_audit
-w /etc/passwd -p wa -k identity
-w /etc/sudoers -p wa -k sudoers_change
-w /etc/sysctl.d/ -p wa -k sysctl_tamper
EOF
if command -v augenrules >/dev/null 2>&1; then
  augenrules --load || true
fi
systemctl enable --now auditd || systemctl restart auditd || true
"#;

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok("Auditd rules successfully installed and reloaded.".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let combined = if stderr.is_empty() { stdout } else { stderr };
        Err(format!("Failed to configure auditd rules: {}", combined.trim()))
    }
}

#[tauri::command]
pub async fn get_command_audit_logs(
    since_days: Option<u32>,
    custom_start: Option<String>,
    custom_end: Option<String>
) -> Result<Vec<AuditLogEvent>, String> {
    let script = "cat /var/log/audit/audit.log 2>/dev/null | tail -n 1000";
    let output = Command::new("pkexec")
        .args(["bash", "-c", script])
        .stderr(Stdio::piped())
        .output()
        .await;

    let out = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()), // Silence errors/empty if not root or missing file
    };

    let cutoff_start = if let Some(days) = since_days {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Some(now - (days as i64) * 86400)
    } else {
        custom_start.as_ref().and_then(|s| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
                .ok()
                .map(|dt| dt.and_utc().timestamp())
        })
    };

    let cutoff_end = custom_end.as_ref().and_then(|s| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(23, 59, 59).unwrap()))
            .ok()
            .map(|dt| dt.and_utc().timestamp())
    });

    let stdout_str = String::from_utf8_lossy(&out.stdout);
    let mut msg_map: HashMap<String, (String, String, String, String, String, String)> = HashMap::new();
    // msg_id -> (timestamp, uid, auid, exe/key, cwd, result)

    for line in stdout_str.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Extract msg ID, e.g., msg=audit(1626789000.123:456):
        let msg_id = match extract_between(line, "msg=audit(", "):") {
            Some(id) => id,
            None => continue,
        };

        if let Some(sec_str) = msg_id.split('.').next() {
            if let Ok(sec) = sec_str.parse::<i64>() {
                if let Some(cutoff_time) = cutoff_start {
                    if sec < cutoff_time {
                        continue;
                    }
                }
                if let Some(cutoff_time) = cutoff_end {
                    if sec > cutoff_time {
                        continue;
                    }
                }
            }
        }

        let entry = msg_map.entry(msg_id.clone()).or_insert_with(|| {
            let ts = parse_audit_timestamp(&msg_id);
            (ts, "unknown".to_string(), "unset".to_string(), "".to_string(), "/".to_string(), "Success".to_string())
        });

        if line.contains("type=SYSCALL") {
            if let Some(uid) = extract_between(line, "uid=", " ") {
                entry.1 = resolve_uid(&uid);
            }
            if let Some(auid) = extract_between(line, "auid=", " ") {
                entry.2 = auid;
            }
            if let Some(exe) = extract_between(line, "exe=\"", "\"") {
                if entry.3.is_empty() {
                    entry.3 = exe;
                }
            }
            if let Some(key) = extract_between(line, "key=\"", "\"") {
                if !key.is_empty() {
                    entry.3 = format!("{} [{}]", entry.3, key);
                }
            }
            if line.contains("success=no") || line.contains("exit=-") {
                entry.5 = "Failure".to_string();
            }
        } else if line.contains("type=EXECVE") {
            let mut args = Vec::new();
            let mut i = 0;
            while let Some(arg) = extract_between(line, &format!("a{}=\"", i), "\"") {
                args.push(arg);
                i += 1;
            }
            if !args.is_empty() {
                entry.3 = args.join(" ");
            }
        } else if line.contains("type=CWD") {
            if let Some(cwd) = extract_between(line, "cwd=\"", "\"") {
                entry.4 = cwd;
            }
        }
    }

    let mut events: Vec<AuditLogEvent> = msg_map
        .into_iter()
        .map(|(_, (ts, uid, auid, cmd, cwd, res))| AuditLogEvent {
            timestamp: ts,
            user: uid,
            command: if cmd.is_empty() { "file_access".to_string() } else { cmd },
            cwd,
            result: res,
            key: "".to_string(),
            auid,
        })
        .collect();

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events.truncate(200);

    Ok(events)
}

#[tauri::command]
pub async fn get_runtime_threats(
    since_days: Option<u32>,
    custom_start: Option<String>,
    custom_end: Option<String>
) -> Result<Vec<RuntimeThreat>, String> {
    let mut threats = Vec::new();

    let auth_events = get_auth_events(since_days, custom_start.clone(), custom_end.clone()).await.unwrap_or_default();
    let audit_events = get_command_audit_logs(since_days, custom_start, custom_end).await.unwrap_or_default();

    // 1. Repeated Sudo Failures (3+)
    let mut user_sudo_fails: HashMap<String, u32> = HashMap::new();
    for ev in &auth_events {
        if ev.event_type == "Sudo Failure" || (ev.event_type.contains("Sudo") && ev.result == "Failure") {
            let count = user_sudo_fails.entry(ev.user.clone()).or_insert(0);
            *count += 1;
        }
    }
    for (user, count) in user_sudo_fails {
        if count >= 3 {
            threats.push(RuntimeThreat {
                id: format!("threat_sudo_fail_{}", user),
                timestamp: "Recent".to_string(),
                title: "Repeated Sudo Authentication Failures".to_string(),
                description: format!("User '{}' failed sudo authentication {} times in recent logs.", user, count),
                severity: "Warning".to_string(),
                category: "Runtime Threats".to_string(),
            });
        }
    }

    // 2. setenforce 0 runs
    for ev in &audit_events {
        if ev.command.contains("setenforce 0") || ev.command.contains("setenforce Permissive") {
            threats.push(RuntimeThreat {
                id: format!("threat_setenforce_{}", ev.timestamp),
                timestamp: ev.timestamp.clone(),
                title: "SELinux Enforcement Disabled".to_string(),
                description: format!("User '{}' executed 'setenforce 0' in working directory '{}'.", ev.user, ev.cwd),
                severity: "Critical".to_string(),
                category: "Runtime Threats".to_string(),
            });
        }

        // 3. iptables -F runs
        if ev.command.contains("iptables -F") || ev.command.contains("iptables --flush") || ev.command.contains("nft flush ruleset") {
            threats.push(RuntimeThreat {
                id: format!("threat_iptables_flush_{}", ev.timestamp),
                timestamp: ev.timestamp.clone(),
                title: "Firewall Rules Flushed".to_string(),
                description: format!("User '{}' executed firewall flush command '{}'.", ev.user, ev.command),
                severity: "Critical".to_string(),
                category: "Runtime Threats".to_string(),
            });
        }

        // 4. sysctl writes to hardening config
        if ev.command.contains("sysctl_tamper") || (ev.command.contains("/etc/sysctl.d/") && ev.command.contains("sysctl")) {
            threats.push(RuntimeThreat {
                id: format!("threat_sysctl_tamper_{}", ev.timestamp),
                timestamp: ev.timestamp.clone(),
                title: "Kernel Sysctl Configuration Modification".to_string(),
                description: format!("File modification in /etc/sysctl.d/ detected by user '{}'.", ev.user),
                severity: "Warning".to_string(),
                category: "Runtime Threats".to_string(),
            });
        }

        // 5. Root commands bypassing sudo
        if ev.user == "root" && ev.auid != "0" && ev.auid != "4294967295" && ev.auid != "unset" {
            let cmd_base = ev.command.split_whitespace().next().unwrap_or("");
            if !cmd_base.ends_with("sudo") && !cmd_base.ends_with("pkexec") && !cmd_base.ends_with("systemd") {
                threats.push(RuntimeThreat {
                    id: format!("threat_root_bypass_{}", ev.timestamp),
                    timestamp: ev.timestamp.clone(),
                    title: "Root Execution Bypassing Sudo".to_string(),
                    description: format!("User (auid={}) executed root command '{}' without standard sudo invocation.", ev.auid, ev.command),
                    severity: "Critical".to_string(),
                    category: "Runtime Threats".to_string(),
                });
            }
        }
    }

    Ok(threats)
}

fn extract_between(s: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = s.find(start)? + start.len();
    let rest = &s[start_idx..];
    let end_idx = rest.find(end)?;
    Some(rest[..end_idx].trim().to_string())
}

fn parse_audit_timestamp(id: &str) -> String {
    if let Some(sec_str) = id.split('.').next() {
        if let Ok(sec) = sec_str.parse::<i64>() {
            use chrono::TimeZone;
            if let Some(dt) = chrono::Local.timestamp_opt(sec, 0).single() {
                return dt.format("%b %d %H:%M:%S").to_string();
            }
        }
    }
    id.to_string()
}

/// Parse a journalctl __REALTIME_TIMESTAMP (microseconds since epoch) into a human-readable string.
fn parse_microsecond_timestamp(us_str: &str) -> String {
    if let Ok(us) = us_str.parse::<i64>() {
        let secs = us / 1_000_000;
        use chrono::TimeZone;
        if let Some(dt) = chrono::Local.timestamp_opt(secs, 0).single() {
            return dt.format("%b %d %H:%M:%S").to_string();
        }
    }
    us_str.to_string()
}

fn resolve_uid(uid_str: &str) -> String {
    // Try to look up username from /etc/passwd dynamically
    if let Ok(contents) = std::fs::read_to_string("/etc/passwd") {
        for line in contents.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 3 && fields[2] == uid_str {
                return fields[0].to_string();
            }
        }
    }
    // Fallback: known system UIDs
    match uid_str {
        "0" => "root".to_string(),
        "65534" | "4294967295" => "nobody".to_string(),
        _ => format!("uid:{}", uid_str),
    }
}
