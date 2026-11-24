use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::utils::privilege::tokio::Command;
use crate::utils::privilege::{run_privileged_output, RootAuth};

/// One scheduled command from a crontab file.
///
/// `source` separates the user's own crontab (editable) from the read-only
/// system files (`/etc/crontab` and `/etc/cron.d/*`). The panel never writes
/// a system file: a crontab written as root is delayed root code execution,
/// so the privilege layer refuses `crontab` for that direction on purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub schedule: String,
    pub command: String,
    pub user: String,
    pub source: String,
    pub file: String,
    pub raw: String,
    pub editable: bool,
    /// Position among identical `raw` lines, so deleting one duplicate can
    /// never delete its twin.
    pub occurrence: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CronState {
    pub user: String,
    pub jobs: Vec<CronJob>,
    pub errors: Vec<String>,
}

/// The shortcuts cron itself accepts.
const NAMED_SCHEDULES: [&str; 8] = [
    "@reboot",
    "@yearly",
    "@annually",
    "@monthly",
    "@weekly",
    "@daily",
    "@midnight",
    "@hourly",
];

const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

const DAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

// ─── Parsing ─────────────────────────────────────────────────────────────────

/// Environment assignments (`PATH=/usr/bin`) are settings, not jobs.
fn is_env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split one crontab line into `(schedule, owning user, command)`.
///
/// System files carry a user field between the schedule and the command
/// (`0 3 * * * root /usr/local/bin/job`); a user crontab does not. Named
/// schedules (`@reboot`) follow the same rule.
fn parse_cron_line(line: &str, system: bool) -> Option<(String, Option<String>, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let first = *tokens.first()?;
    if is_env_assignment(first) {
        return None;
    }

    if first.starts_with('@') {
        return if system {
            if tokens.len() < 3 {
                None
            } else {
                Some((
                    first.to_string(),
                    Some(tokens[1].to_string()),
                    tokens[2..].join(" "),
                ))
            }
        } else if tokens.len() < 2 {
            None
        } else {
            Some((first.to_string(), None, tokens[1..].join(" ")))
        };
    }

    if tokens.len() < 6 {
        return None;
    }
    let schedule = tokens[0..5].join(" ");
    if system {
        if tokens.len() < 7 {
            return None;
        }
        Some((schedule, Some(tokens[5].to_string()), tokens[6..].join(" ")))
    } else {
        Some((schedule, None, tokens[5..].join(" ")))
    }
}

fn parse_crontab(content: &str, system: bool, file: &str, owner: &str) -> Vec<CronJob> {
    let mut jobs = Vec::new();
    let mut occurrences: HashMap<String, usize> = HashMap::new();

    for (index, line) in content.lines().enumerate() {
        let Some((schedule, line_user, command)) = parse_cron_line(line, system) else {
            continue;
        };
        let raw = line.trim().to_string();
        let occurrence = occurrences.entry(raw.clone()).or_insert(0);
        let current = *occurrence;
        *occurrence += 1;

        jobs.push(CronJob {
            id: format!("{file}:{index}"),
            schedule,
            command,
            user: line_user.unwrap_or_else(|| owner.to_string()),
            source: if system { "system" } else { "user" }.to_string(),
            file: file.to_string(),
            raw,
            editable: !system,
            occurrence: current,
        });
    }

    jobs
}

/// The system crontabs the panel may read without privileges. They are
/// world-readable configuration; the panel shows them read-only.
fn system_cron_files() -> Vec<String> {
    let mut files = Vec::new();
    if Path::new("/etc/crontab").is_file() {
        files.push(String::from("/etc/crontab"));
    }

    if let Ok(entries) = std::fs::read_dir("/etc/cron.d") {
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return None;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.ends_with('~') {
                    return None;
                }
                if [".rpmsave", ".rpmnew", ".rpmorig", ".bak", ".disabled"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
                {
                    return None;
                }
                Some(path.to_string_lossy().to_string())
            })
            .collect();
        names.sort();
        files.extend(names);
    }

    files
}

fn read_system_crontabs() -> (Vec<CronJob>, Vec<String>) {
    let mut jobs = Vec::new();
    let mut errors = Vec::new();

    for file in system_cron_files() {
        match std::fs::read_to_string(&file) {
            Ok(content) => jobs.extend(parse_crontab(&content, true, &file, "root")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => errors.push(format!("{file} could not be read: {error}")),
        }
    }

    (jobs, errors)
}

async fn read_user_crontab() -> Result<String, String> {
    let output = Command::new("crontab")
        .args(["-l"])
        .output()
        .await
        .map_err(|e| format!("Could not run crontab: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // "no crontab for <user>" is an empty crontab, not a failure.
        if stderr.to_lowercase().contains("no crontab") {
            return Ok(String::new());
        }
        return Err(if stderr.is_empty() {
            String::from("crontab -l failed".to_string())
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn write_user_crontab(content: &str) -> Result<(), String> {
    use ::tokio::io::AsyncWriteExt;

    let mut command = tokio::process::Command::new("crontab");
    command
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not write the crontab: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Could not write the crontab: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            String::from("crontab - failed")
        } else {
            stderr
        });
    }

    Ok(())
}

// ─── Validation ──────────────────────────────────────────────────────────────

fn parse_cron_value(
    value: &str,
    min: u32,
    max: u32,
    names: Option<&[&str]>,
    label: &str,
) -> Result<u32, String> {
    let lower = value.to_ascii_lowercase();
    if let Some(names) = names {
        if let Some(index) = names.iter().position(|name| *name == lower) {
            // month names are 1-based, weekday names are 0-based
            return Ok(min + index as u32);
        }
    }
    let number: u32 = lower
        .parse()
        .map_err(|_| format!("The {label} field has an invalid value \"{value}\"."))?;
    if !(min..=max).contains(&number) {
        return Err(format!(
            "The {label} field value \"{value}\" must be between {min} and {max}."
        ));
    }
    Ok(number)
}

fn validate_cron_field(
    field: &str,
    label: &str,
    min: u32,
    max: u32,
    names: Option<&[&str]>,
) -> Result<(), String> {
    for part in field.split(',') {
        if part.is_empty() {
            return Err(format!("The {label} field has an empty list item."));
        }

        let (base, step) = match part.split_once('/') {
            Some((base, step)) => {
                let step: u32 = step
                    .parse()
                    .map_err(|_| format!("The {label} step \"{step}\" is not a number."))?;
                if step == 0 {
                    return Err(format!("The {label} step must be at least 1."));
                }
                (base, Some(step))
            }
            None => (part, None),
        };

        if base == "*" {
            if let Some(step) = step {
                if step > max - min + 1 {
                    return Err(format!(
                        "The {label} step must be between 1 and {}.",
                        max - min + 1
                    ));
                }
            }
            continue;
        }

        let (start, end) = match base.split_once('-') {
            Some((from, to)) => (
                parse_cron_value(from, min, max, names, label)?,
                parse_cron_value(to, min, max, names, label)?,
            ),
            None => {
                let single = parse_cron_value(base, min, max, names, label)?;
                (single, if step.is_some() { max } else { single })
            }
        };

        if start > end {
            return Err(format!("The {label} range \"{base}\" is reversed."));
        }
        if let Some(step) = step {
            if step > end - start + 1 {
                return Err(format!(
                    "The {label} step must be between 1 and {}.",
                    end - start + 1
                ));
            }
        }
    }

    Ok(())
}

pub fn validate_cron_schedule(schedule: &str) -> Result<(), String> {
    let schedule = schedule.trim();
    if schedule.is_empty() {
        return Err(String::from("A schedule is required."));
    }

    if schedule.starts_with('@') {
        let lower = schedule.to_ascii_lowercase();
        if NAMED_SCHEDULES.contains(&lower.as_str()) {
            return Ok(());
        }
        return Err(format!(
            "Unknown shortcut \"{schedule}\". Use @reboot, @hourly, @daily, @weekly, @monthly or @yearly."
        ));
    }

    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(String::from(
            "A cron schedule needs five fields: minute, hour, day of month, month, weekday.",
        ));
    }

    let specs: [(&str, u32, u32, Option<&[&str]>); 5] = [
        ("minute", 0, 59, None),
        ("hour", 0, 23, None),
        ("day of month", 1, 31, None),
        ("month", 1, 12, Some(&MONTH_NAMES)),
        ("weekday", 0, 7, Some(&DAY_NAMES)),
    ];

    for (field, (label, min, max, names)) in fields.iter().zip(specs.iter()) {
        validate_cron_field(field, label, *min, *max, *names)?;
    }

    Ok(())
}

fn validate_cron_command(command: &str) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty() {
        return Err(String::from("A command is required."));
    }
    if command.contains('\n') || command.contains('\r') || command.contains('\0') {
        return Err(String::from("The command must be a single line."));
    }
    if command.len() > 4096 {
        return Err(String::from(
            "The command is too long (4096 characters maximum).",
        ));
    }
    Ok(())
}

/// Remove exactly one line: the `occurrence`-th line equal to `raw`.
/// Matching is trimmed, but the ordinal keeps duplicate jobs distinct.
fn remove_cron_occurrence(content: &str, raw: &str, occurrence: usize) -> Result<String, String> {
    let target = raw.trim();
    let mut seen = 0usize;
    let mut removed = false;
    let mut kept: Vec<&str> = Vec::new();

    for line in content.lines() {
        if !removed && line.trim() == target {
            if seen == occurrence {
                removed = true;
                continue;
            }
            seen += 1;
        }
        kept.push(line);
    }

    if !removed {
        return Err(String::from(
            "That job is no longer in your crontab — reload the list and try again.",
        ));
    }

    let mut updated = kept.join("\n");
    if !updated.is_empty() {
        updated.push('\n');
    }
    Ok(updated)
}

// ─── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_cron_jobs() -> Result<CronState, String> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| String::from("user"));

    let mut jobs = Vec::new();
    let mut errors = Vec::new();

    match read_user_crontab().await {
        Ok(content) => jobs.extend(parse_crontab(&content, false, "crontab", &user)),
        Err(error) => errors.push(format!("Your crontab could not be read: {error}")),
    }

    let (system_jobs, system_errors) = read_system_crontabs();
    jobs.extend(system_jobs);
    errors.extend(system_errors);

    Ok(CronState { user, jobs, errors })
}

#[tauri::command]
pub async fn add_cron_job(schedule: String, command: String) -> Result<String, String> {
    let schedule = schedule.trim().to_string();
    let command = command.trim().to_string();
    validate_cron_schedule(&schedule)?;
    validate_cron_command(&command)?;

    let entry = format!("{schedule} {command}");
    let current = read_user_crontab().await?;
    if current.lines().any(|line| line.trim() == entry) {
        return Err(String::from("This exact job is already in your crontab."));
    }

    // Read, extend and write back through `crontab -`: no shell is involved,
    // so a command can never be interpreted while it is installed.
    let mut content = current;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&entry);
    content.push('\n');

    write_user_crontab(&content).await?;
    Ok(String::from("Cron job added to your crontab"))
}

#[tauri::command]
pub async fn delete_cron_job(raw: String, occurrence: usize) -> Result<String, String> {
    let current = read_user_crontab().await?;
    let updated = remove_cron_occurrence(&current, &raw, occurrence)?;
    write_user_crontab(&updated).await?;
    Ok(String::from("Cron job deleted"))
}

// ─── Systemd timers ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdTimer {
    pub unit: String,
    pub activates: String,
    pub description: String,
    /// Enabled at boot (independent of whether it is running right now).
    pub enabled: bool,
    /// Loaded and active right now.
    pub active: bool,
    /// The systemd SUB state, e.g. `waiting` or `running`.
    pub state: String,
    pub status: String,
    /// Unix seconds; `None` when systemd has no scheduled elapse.
    pub next_run: Option<i64>,
    pub last_run: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TimerEntry {
    #[serde(default)]
    next: Option<i64>,
    #[serde(default)]
    last: Option<i64>,
    unit: String,
    #[serde(default)]
    activates: String,
}

async fn systemctl_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("systemctl {} failed: {e}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("systemctl {} failed", args.join(" "))
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_timer_json(json: &str) -> Result<Vec<TimerEntry>, String> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("Could not read the timer list: {e}"))
}

type UnitRow = (String, String, String);

/// `UNIT LOAD ACTIVE SUB DESCRIPTION` — the leading bullet of a failed unit
/// shifts every column, so it is dropped before splitting.
fn parse_timer_units(text: &str) -> HashMap<String, UnitRow> {
    let mut units = HashMap::new();

    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let offset = usize::from(parts.first() == Some(&"●"));
        if parts.len() < offset + 5 {
            continue;
        }
        let unit = parts[offset].to_string();
        let active = parts[offset + 2].to_string();
        let sub = parts[offset + 3].to_string();
        let description = parts[offset + 4..].join(" ");
        units.insert(unit, (active, sub, description));
    }

    units
}

fn parse_timer_unit_files(text: &str) -> HashMap<String, bool> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let unit = parts.next()?.to_string();
            let state = parts.next()?;
            Some((unit, state.starts_with("enabled")))
        })
        .collect()
}

fn service_of(unit: &str) -> String {
    match unit.strip_suffix(".timer") {
        Some(base) => format!("{base}.service"),
        None => unit.to_string(),
    }
}

#[tauri::command]
pub async fn cron_list_timers() -> Result<Vec<SystemdTimer>, String> {
    let entries_result = systemctl_output(&[
        "list-timers",
        "--all",
        "--no-legend",
        "--no-pager",
        "--output=json",
    ])
    .await;
    let units_result = systemctl_output(&[
        "list-units",
        "--type=timer",
        "--all",
        "--no-legend",
        "--no-pager",
        "--plain",
    ])
    .await;
    let files_text =
        systemctl_output(&["list-unit-files", "--type=timer", "--no-legend", "--plain"])
            .await
            .unwrap_or_default();

    let entries = match entries_result {
        Ok(json) => parse_timer_json(&json)?,
        Err(error) => {
            if units_result.is_err() {
                return Err(error);
            }
            Vec::new()
        }
    };
    let units = match units_result {
        Ok(text) => parse_timer_units(&text),
        Err(error) => {
            if entries.is_empty() {
                return Err(error);
            }
            HashMap::new()
        }
    };
    let enabled_units = parse_timer_unit_files(&files_text);

    let mut timers = Vec::new();
    let mut seen = HashSet::new();

    for entry in &entries {
        seen.insert(entry.unit.clone());
        let (active, sub, description) = units.get(&entry.unit).cloned().unwrap_or_else(|| {
            (
                String::from("inactive"),
                String::from("dead"),
                String::new(),
            )
        });
        let activates = if entry.activates.is_empty() {
            service_of(&entry.unit)
        } else {
            entry.activates.clone()
        };
        timers.push(SystemdTimer {
            enabled: enabled_units.get(&entry.unit).copied().unwrap_or(false),
            active: active == "active",
            status: format!("{active} ({sub})"),
            state: sub,
            next_run: entry
                .next
                .filter(|value| *value > 0)
                .map(|value| value / 1_000_000),
            last_run: entry
                .last
                .filter(|value| *value > 0)
                .map(|value| value / 1_000_000),
            activates,
            description,
            unit: entry.unit.clone(),
        });
    }

    for (unit, (active, sub, description)) in units {
        if seen.contains(&unit) {
            continue;
        }
        timers.push(SystemdTimer {
            enabled: enabled_units.get(&unit).copied().unwrap_or(false),
            active: active == "active",
            status: format!("{active} ({sub})"),
            state: sub,
            next_run: None,
            last_run: None,
            activates: service_of(&unit),
            description,
            unit,
        });
    }

    timers.sort_by(|a, b| a.unit.cmp(&b.unit));
    Ok(timers)
}

fn is_valid_timer_unit(unit: &str) -> bool {
    if unit.is_empty() || unit.len() > 255 || !unit.ends_with(".timer") {
        return false;
    }
    if unit.starts_with('-') || unit.starts_with('.') {
        return false;
    }
    unit.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '@' | '-'))
}

#[tauri::command]
pub async fn cron_toggle_timer(unit: String, enable: bool) -> Result<String, String> {
    if !is_valid_timer_unit(&unit) {
        return Err(format!("\"{unit}\" is not a valid systemd timer unit."));
    }

    // Goes through the central privilege layer: admin mode must be on, the
    // helper re-validates the unit and refuses protected units.
    let verb = if enable { "enable" } else { "disable" };
    let args = vec![verb.to_string(), String::from("--now"), unit.clone()];
    let output = run_privileged_output("/usr/bin/systemctl", &args, &RootAuth::Polkit).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("systemctl {verb} {unit} failed")
        } else {
            stderr
        });
    }

    Ok(format!(
        "Timer {unit} {}",
        if enable {
            "enabled and started"
        } else {
            "disabled and stopped"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_cron_lines() {
        let content = "PATH=/usr/bin\n# a comment\n\n0 3 * * * /usr/bin/backup --full\n*/15 * * * * /usr/bin/poll\n";
        let jobs = parse_crontab(content, false, "crontab", "ali");

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].schedule, "0 3 * * *");
        assert_eq!(jobs[0].command, "/usr/bin/backup --full");
        assert_eq!(jobs[0].user, "ali");
        assert_eq!(jobs[0].source, "user");
        assert!(jobs[0].editable);
        assert_eq!(jobs[1].schedule, "*/15 * * * *");
    }

    #[test]
    fn parses_named_schedules() {
        let content = "@reboot /usr/local/bin/warmup\n@daily /usr/bin/report --mail\n";
        let jobs = parse_crontab(content, false, "crontab", "ali");

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].schedule, "@reboot");
        assert_eq!(jobs[0].command, "/usr/local/bin/warmup");
        assert_eq!(jobs[1].schedule, "@daily");
    }

    #[test]
    fn system_files_carry_a_user_field() {
        let content = "SHELL=/bin/bash\n30 2 * * * root /usr/local/bin/rotate\n@hourly root /usr/local/bin/tick\n";
        let jobs = parse_crontab(content, true, "/etc/crontab", "root");

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].user, "root");
        assert_eq!(jobs[0].command, "/usr/local/bin/rotate");
        assert_eq!(jobs[0].schedule, "30 2 * * *");
        assert!(!jobs[0].editable);
        assert_eq!(jobs[1].schedule, "@hourly");
        assert_eq!(jobs[1].command, "/usr/local/bin/tick");
    }

    #[test]
    fn duplicates_get_distinct_occurrences() {
        let content = "0 3 * * * /bin/backup\n0 3 * * * /bin/backup\n";
        let jobs = parse_crontab(content, false, "crontab", "ali");

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].occurrence, 0);
        assert_eq!(jobs[1].occurrence, 1);
        assert_ne!(jobs[0].id, jobs[1].id);
    }

    #[test]
    fn short_lines_are_ignored() {
        let content = "* * * *\nnot a cron line\n";
        assert!(parse_crontab(content, false, "crontab", "ali").is_empty());
    }

    #[test]
    fn accepts_valid_schedules() {
        for schedule in [
            "* * * * *",
            "*/5 * * * *",
            "0 3 * * 1-5",
            "0 0 1 jan *",
            "30 4 * * sun",
            "5/15 * * * *",
            "@reboot",
            "@daily",
        ] {
            assert!(
                validate_cron_schedule(schedule).is_ok(),
                "{schedule} should be valid"
            );
        }
    }

    #[test]
    fn rejects_invalid_schedules() {
        for schedule in [
            "",
            "* * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "10-5 * * * *",
            "* * * * mon-",
            "@sometimes",
            "1,,2 * * * *",
        ] {
            assert!(
                validate_cron_schedule(schedule).is_err(),
                "{schedule} should be invalid"
            );
        }
    }

    #[test]
    fn commands_must_be_one_line() {
        assert!(validate_cron_command("/bin/true").is_ok());
        assert!(validate_cron_command("  ").is_err());
        assert!(validate_cron_command("a\nb").is_err());
    }

    #[test]
    fn deletes_only_the_selected_duplicate() {
        let content = "0 3 * * * /bin/backup\n0 3 * * * /bin/backup\n0 4 * * * /bin/other\n";

        let first = remove_cron_occurrence(content, "0 3 * * * /bin/backup", 0).unwrap();
        assert_eq!(first, "0 3 * * * /bin/backup\n0 4 * * * /bin/other\n");

        let second = remove_cron_occurrence(content, "0 3 * * * /bin/backup", 1).unwrap();
        assert_eq!(second, "0 3 * * * /bin/backup\n0 4 * * * /bin/other\n");

        assert!(remove_cron_occurrence(content, "0 3 * * * /bin/backup", 2).is_err());
        assert!(remove_cron_occurrence(content, "9 9 * * * /bin/missing", 0).is_err());
    }

    #[test]
    fn removing_the_last_line_leaves_no_stray_blank_line() {
        let content = "0 3 * * * /bin/backup\n";
        assert_eq!(
            remove_cron_occurrence(content, "0 3 * * * /bin/backup", 0).unwrap(),
            ""
        );
    }

    #[test]
    fn timer_units_are_validated() {
        assert!(is_valid_timer_unit("logrotate.timer"));
        assert!(is_valid_timer_unit("dnf-makecache.timer"));
        assert!(is_valid_timer_unit("user@1000.timer"));
        assert!(!is_valid_timer_unit("logrotate.service"));
        assert!(!is_valid_timer_unit("--now.timer"));
        assert!(!is_valid_timer_unit("../etc/passwd.timer"));
        assert!(!is_valid_timer_unit("bad timer.timer"));
        assert!(!is_valid_timer_unit(""));
    }

    #[test]
    fn parses_timer_json() {
        let json = r#"[
            {"next":1795422603762707,"last":1795419003759987,"unit":"snapper-cleanup.timer","activates":"snapper-cleanup.service"},
            {"next":0,"last":0,"unit":"idle.timer","activates":""}
        ]"#;
        let entries = parse_timer_json(json).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].unit, "snapper-cleanup.timer");
        assert_eq!(entries[0].next.unwrap() / 1_000_000, 1795422603);
        assert!(entries[1].activates.is_empty());
        assert!(parse_timer_json("[]").unwrap().is_empty());
    }

    #[test]
    fn parses_timer_units_and_drops_the_bullet() {
        let text = "● broken.timer loaded failed failed Broken timer\nlogrotate.timer loaded active waiting Daily rotation\n";
        let units = parse_timer_units(text);

        assert_eq!(units.get("broken.timer").unwrap().0, "failed");
        assert_eq!(units.get("broken.timer").unwrap().2, "Broken timer");
        assert_eq!(units.get("logrotate.timer").unwrap().1, "waiting");
        assert_eq!(units.get("logrotate.timer").unwrap().2, "Daily rotation");
    }

    #[test]
    fn parses_enabled_state() {
        let text = "logrotate.timer enabled enabled\nbtrfs-trim.timer disabled disabled\nalias.timer alias -\n";
        let states = parse_timer_unit_files(text);

        assert!(states["logrotate.timer"]);
        assert!(!states["btrfs-trim.timer"]);
        assert!(!states["alias.timer"]);
    }

    #[test]
    fn service_name_follows_the_unit() {
        assert_eq!(service_of("logrotate.timer"), "logrotate.service");
        assert_eq!(service_of("odd-unit"), "odd-unit");
    }
}
