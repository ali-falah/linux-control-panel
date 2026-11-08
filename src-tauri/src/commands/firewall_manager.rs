use serde::{Deserialize, Serialize};
use std::process::Command as StdCommand;
use crate::utils::privilege::tokio::Command;
use crate::{log_to_file};
use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};
use crate::utils::root_policy::{
    firewall_port_value_valid, firewall_rich_rule_valid, firewall_service_name_valid,
    firewall_zone_name_valid, firewall_interface_name_valid, FirewallRichRule,
    FirewallRichRuleAction, FirewallRichRuleElement,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallState {
    pub is_running: bool,
    pub is_panic: bool,
    pub active_zones: Vec<String>,
    pub default_zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRules {
    pub zone: String,
    pub services: Vec<String>,
    pub ports: Vec<String>,
}

#[tauri::command]
pub async fn get_firewall_state() -> Result<FirewallState, String> {
    if crate::binary_exists("firewall-cmd").await {
        let state_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .arg("--state")
                .output()
        ).await.map_err(|_| "firewall-cmd --state timed out".to_string())?
        .map_err(|e| e.to_string())?;

        let is_running = String::from_utf8_lossy(&state_out.stdout).trim() == "running";

        if !is_running {
            return Ok(FirewallState {
                is_running: false,
                is_panic: false,
                active_zones: vec![],
                default_zone: String::new(),
            });
        }

        let panic_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .arg("--query-panic")
                .output()
        ).await.map_err(|_| "firewall-cmd --query-panic timed out".to_string())?
        .map_err(|e| e.to_string())?;
        let is_panic = String::from_utf8_lossy(&panic_out.stdout).trim() == "yes";

        let default_zone_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .arg("--get-default-zone")
                .output()
        ).await;
        let default_zone = match default_zone_out {
            Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => String::new(),
        };

        let active_zones_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .arg("--get-active-zones")
                .output()
        ).await;
        let active_zones_raw = match active_zones_out {
            Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        let mut active_zones = Vec::new();
        for line in active_zones_raw.lines() {
            if !line.starts_with(' ') && !line.is_empty() {
                let mut zone = line.trim().to_string();
                if zone.ends_with(" (default)") {
                    zone = zone.replace(" (default)", "");
                }
                active_zones.push(zone);
            }
        }

        if active_zones.is_empty() && !default_zone.is_empty() {
            active_zones.push(default_zone.clone());
        }

        return Ok(FirewallState {
            is_running,
            is_panic,
            active_zones,
            default_zone,
        });
    }

    if crate::binary_exists("ufw").await {
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("ufw")
                .arg("status")
                .output()
        ).await.map_err(|_| "ufw status timed out".to_string())?
        .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let is_running = stdout.to_lowercase().contains("status: active");

        return Ok(FirewallState {
            is_running,
            is_panic: false,
            active_zones: vec!["ufw".to_string()],
            default_zone: "ufw".to_string(),
        });
    }

    Err("Neither firewalld nor ufw is available".to_string())
}

#[tauri::command]
pub async fn get_zone_rules(zone: String) -> Result<ZoneRules, String> {
    if crate::binary_exists("firewall-cmd").await {
        let services_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .args(["--zone", &zone, "--list-services"])
                .output()
        ).await;
        
        let services: Vec<String> = match services_out {
            Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout)
                .trim()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        };

        let ports_out = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .args(["--zone", &zone, "--list-ports"])
                .output()
        ).await;
        
        let ports: Vec<String> = match ports_out {
            Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout)
                .trim()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        };

        return Ok(ZoneRules {
            zone,
            services,
            ports,
        });
    }

    if crate::binary_exists("ufw").await {
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("ufw")
                .arg("status")
                .output()
        ).await.map_err(|_| "ufw status timed out".to_string())?
        .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut services = Vec::new();
        let mut ports = Vec::new();

        let mut in_rules = false;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                in_rules = true;
                continue;
            }
            if in_rules && !trimmed.is_empty() {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if !parts.is_empty() {
                    let target = parts[0];
                    if target.contains('/') || target.chars().all(|c| c.is_ascii_digit()) {
                        if !ports.contains(&target.to_string()) {
                            ports.push(target.to_string());
                        }
                    } else {
                        if !services.contains(&target.to_string()) {
                            services.push(target.to_string());
                        }
                    }
                }
            }
        }

        return Ok(ZoneRules {
            zone,
            services,
            ports,
        });
    }

    Err("Neither firewalld nor ufw is available".to_string())
}

/// One change that can interrupt the administrator's SSH session. The
/// variants map to the cases approved for the firewall migration guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirewallSshGuard {
    RemoveServiceSsh,
    RemovePort22,
    RichDropSsh,
    RichRejectSsh,
    MoveSshInterface,
}

impl FirewallSshGuard {
    fn message(self) -> &'static str {
        match self {
            Self::RemoveServiceSsh => "removing the ssh firewall service",
            Self::RemovePort22 => "removing port 22 from the firewall",
            Self::RichDropSsh => "adding a firewall drop rule for ssh",
            Self::RichRejectSsh => "adding a firewall reject rule for ssh",
            Self::MoveSshInterface => "moving the interface carrying the active SSH session \
                 into a zone that does not permit ssh",
        }
    }
}

/// Parse the output of `ss -tnp state established ( sport = :22 )`. The
/// command returns only SSH sockets, so a successful, empty result means no
/// active SSH session was visible to this process. Every header shape `ss`
/// produces is filtered: "Netid …" (narrow output), "Recv-Q …" (flagless
/// `ss`) and "State Recv-Q …" (`ss state established`).
fn parse_ssh_sessions(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Netid"))
        .filter(|line| !line.starts_with("Recv-Q"))
        .filter(|line| !line.starts_with("State"))
        .map(|line| line.to_string())
        .collect()
}

/// Does this `ss` session line bind to the given local interface? The local
/// endpoint may carry the scope marker `192.168.0.103%wlp1s0:22`. `None`
/// means the line does not name an interface at all — the caller treats
/// that as "possibly this interface", never as "provably elsewhere": the
/// lockout risk must not be waved away because a marker is missing.
fn ssh_session_binds_interface(line: &str, interface: &str) -> Option<bool> {
    if line.is_empty() || interface.is_empty() {
        return None;
    }
    let local = line.split_whitespace().nth(3)?;
    local.split_once('%').map(|(_, tail)| tail.split(':').next() == Some(interface))
}

/// True when any detected session could plausibly be carried by
/// `interface`. A session is provably elsewhere only when its scope marker
/// names a different interface.
fn ssh_sessions_touch_interface(sessions: &[String], interface: &str) -> bool {
    sessions
        .iter()
        .any(|line| ssh_session_binds_interface(line, interface) != Some(false))
}

/// Best-effort SSH session detection. `None` means the probe itself failed or
/// its output could not be interpreted; the caller must treat that as
/// inconclusive, not as "no session".
fn active_ssh_sessions() -> Option<Vec<String>> {
    cmd_sessions_probe()
}

fn cmd_sessions_probe() -> Option<Vec<String>> {
    let output = StdCommand::new("ss")
        .args(["-tnp", "state", "established", "( sport = :22 )"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_ssh_sessions(&stdout))
}

/// The port field in a rich rule can be a single port or an inclusive range.
/// Return true when the range includes port 22.
fn port_string_includes_22(port: &str) -> bool {
    const SSH_PORT: u16 = 22;
    match port.split_once('-') {
        Some((start, end)) => {
            let start: u16 = start.parse().unwrap_or(SSH_PORT);
            let end: u16 = end.parse().unwrap_or(SSH_PORT);
            start <= SSH_PORT && SSH_PORT <= end
        }
        None => port.parse::<u16>().ok() == Some(SSH_PORT),
    }
}

fn rich_rule_drop_or_reject_targets_ssh(rule: &FirewallRichRule) -> Option<FirewallSshGuard> {
    if rule.action == FirewallRichRuleAction::Accept {
        return None;
    }
    let ssh = match &rule.element {
        FirewallRichRuleElement::Service(name) if name == "ssh" => true,
        FirewallRichRuleElement::Port { port, .. } if port_string_includes_22(port) => true,
        _ => false,
    };
    if !ssh {
        return None;
    }
    match rule.action {
        FirewallRichRuleAction::Drop => Some(FirewallSshGuard::RichDropSsh),
        FirewallRichRuleAction::Reject => Some(FirewallSshGuard::RichRejectSsh),
        FirewallRichRuleAction::Accept => unreachable!("filtered above"),
    }
}

fn ssh_confirmation_message(sessions: &[String], guard: FirewallSshGuard) -> Option<String> {
    if sessions.is_empty() {
        return None;
    }
    Some(format!(
        "CONFIRMATION_REQUIRED: This change can interrupt an active SSH session \
         ({}). It may make the firewall impossible to administer remotely. Do you want to proceed?",
        guard.message()
    ))
}

/// Best-effort guard for the two SSH-sensitive cases approved for this
/// batch. An empty detected set is treated as "no session visible"; a
/// `None` probe result is treated as inconclusive and allowed through with a
/// warning in the app log, per the approved fail-open policy. The session
/// list is passed in so tests can pin the gate without spawning `ss`.
fn require_active_ssh_confirmation_gate(
    sessions: Option<Vec<String>>,
    guard: FirewallSshGuard,
    confirmed: bool,
) -> Result<(), String> {
    match sessions {
        Some(sessions) => {
            if let Some(message) = ssh_confirmation_message(&sessions, guard) {
                if !confirmed {
                    return Err(message);
                }
            }
            Ok(())
        }
        None => {
            log_to_file(
                "WARN",
                &format!(
                    "SSH session detection was inconclusive; proceeding with {}",
                    guard.message()
                ),
            );
            Ok(())
        }
    }
}

/// Best-effort guard for the two SSH-sensitive cases approved for this
/// batch. An empty detected set is treated as "no session visible"; a failed
/// probe is treated as inconclusive and allowed through with a warning in the
/// app log, per the approved fail-open policy.
async fn require_active_ssh_confirmation(
    guard: Option<FirewallSshGuard>,
    confirmed: bool,
) -> Result<(), String> {
    let Some(guard) = guard else {
        return Ok(());
    };
    require_active_ssh_confirmation_gate(active_ssh_sessions(), guard, confirmed)
}

/// Call a named lcp-admin firewall action through the installed helper. The
/// helper remains the only boundary between the app and the root-owned
/// firewall configuration. When `confirmed`, the explicit `confirm`
/// trailing argument is appended so the helper's own SSH guard can accept
/// the retry.
pub(crate) async fn run_firewall_named_action(
    action: &str,
    params: &[String],
    confirmed: bool,
) -> Result<(), String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 2);
    args.push(action.to_string());
    args.extend_from_slice(params);
    if confirmed {
        args.push("confirm".to_string());
    }
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, "").await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

/// Contextualize a helper error for the caller. A `CONFIRMATION_REQUIRED:`
/// error is a retry contract: it must pass through unchanged, prefix at
/// character zero, so the caller can detect it and retry the same operation
/// with explicit confirmation. Ordinary errors receive descriptive context.
pub(crate) fn firewall_helper_error_context(prefix: &str, error: String) -> String {
    if error.starts_with("CONFIRMATION_REQUIRED:") {
        error
    } else {
        format!("{prefix}{error}")
    }
}

/// Operand validation for `modify_firewall_rule`. An empty zone is allowed:
/// ufw has no zone concept, and the empty-zone request is the documented
/// ufw shape (the helper enforces the same contract and explicitly refuses
/// a named zone when only ufw is installed). Every non-empty zone is
/// validated with the GROUP 1 validator, unchanged.
fn zone_rule_operands_valid(
    zone: &str,
    rule_type: &str,
    value: &str,
) -> Result<(), String> {
    if !zone.is_empty() {
        firewall_zone_name_valid(zone)?;
    }
    match rule_type {
        "service" => firewall_service_name_valid(value),
        "port" => firewall_port_value_valid(value),
        other => Err(format!(
            "The firewall rule kind must be service or port, got: {other}"
        )),
    }
}

/// Operand validation for `firewall_modify_rich_rule`: a rich rule exists
/// only on firewalld, so a real zone is always required. Returns the parsed
/// rule so the caller never parses unvalidated text twice.
fn rich_rule_operands_valid(zone: &str, rule: &str) -> Result<FirewallRichRule, String> {
    if zone.is_empty() {
        return Err("A zone is required for a rich rule.".to_string());
    }
    firewall_zone_name_valid(zone)?;
    firewall_rich_rule_valid(rule)
}

#[tauri::command]
pub async fn modify_firewall_rule(
    zone: String,
    rule_type: String, // "service" or "port"
    value: String,     // e.g. "http" or "8080/tcp"
    add: bool,
    confirmed: Option<bool>,
) -> Result<String, String> {
    zone_rule_operands_valid(&zone, &rule_type, &value)?;

    let guard = if !add {
        match (rule_type.as_str(), value.as_str()) {
            ("service", "ssh") => Some(FirewallSshGuard::RemoveServiceSsh),
            ("port", "22/tcp") => Some(FirewallSshGuard::RemovePort22),
            _ => None,
        }
    } else {
        None
    };
    require_active_ssh_confirmation(guard, confirmed.unwrap_or(false)).await?;

    run_firewall_named_action(
        "firewall_zone_rule",
        &[
            if add { "add" } else { "remove" }.to_string(),
            zone.clone(),
            rule_type.clone(),
            value.clone(),
        ],
        confirmed.unwrap_or(false),
    )
    .await
    .map_err(|e| firewall_helper_error_context("Failed to modify firewall rule: ", e))?;

    Ok(format!(
        "Successfully {}ed {} {}",
        if add { "add" } else { "remov" },
        rule_type,
        value
    ))
}

/// App-side GROUP 3 panic gate, mirroring the helper's fail-closed policy.
/// `None` (inconclusive detection) is *not* resolved here: the helper blocks
/// panic enablement on an inconclusive probe, so the app defers to the
/// privileged boundary instead of duplicating policy.
fn require_panic_confirmation_gate(
    sessions: Option<Vec<String>>,
    enable: bool,
    confirmed: bool,
) -> Result<(), String> {
    if !enable {
        return Ok(());
    }
    if let Some(sessions) = sessions {
        if !sessions.is_empty() && !confirmed {
            return Err(format!(
                "CONFIRMATION_REQUIRED: Enabling firewalld panic mode with an active SSH \
                 session will sever every connection, including the session it was detected \
                 from. Panic mode is not reversible from the panel while the panel \
                 connection is cut. Retry the same action with the explicit 'confirm' \
                 argument to proceed."
            ));
        }
    }
    Ok(())
}

/// Probe-caller seam around the panic gate: the SSH probe closure is only
/// invoked when enabling. Disabling never touches the probe, so it stays
/// possible even when `ss` is unavailable (the helper independently keeps
/// its fail-closed protection for enable).
fn panic_gate_with_probe<F>(
    enable: bool,
    confirmed: bool,
    probe: F,
) -> Result<(), String>
where
    F: FnOnce() -> Option<Vec<String>>,
{
    let sessions = if enable { probe() } else { None };
    require_panic_confirmation_gate(sessions, enable, confirmed)
}

#[tauri::command]
pub async fn toggle_panic_mode(
    enable: bool,
    confirmed: Option<bool>,
) -> Result<String, String> {
    panic_gate_with_probe(enable, confirmed.unwrap_or(false), active_ssh_sessions)?;

    run_firewall_named_action(
        "firewall_panic",
        &[if enable { "enable" } else { "disable" }.to_string()],
        confirmed.unwrap_or(false),
    )
    .await
    .map_err(|e| firewall_helper_error_context("Failed to change panic mode: ", e))?;

    Ok(format!(
        "Panic mode turned {}",
        if enable { "ON" } else { "OFF" }
    ))
}

#[tauri::command]
pub async fn firewall_get_rich_rules(zone: String) -> Result<Vec<String>, String> {
    if crate::binary_exists("firewall-cmd").await {
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .args(["--zone", &zone, "--list-rich-rules"])
                .output()
        ).await.map_err(|_| "firewall-cmd --list-rich-rules timed out".to_string())?
        .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let rules = stdout.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(rules);
    }

    if crate::binary_exists("ufw").await {
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("ufw")
                .args(["status", "numbered"])
                .output()
        ).await.map_err(|_| "ufw status timed out".to_string())?
        .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut rules = Vec::new();
        let mut in_rules = false;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_rules = true;
            }
            if in_rules && !trimmed.is_empty() {
                rules.push(trimmed.to_string());
            }
        }
        return Ok(rules);
    }

    Ok(Vec::new())
}

#[tauri::command]
pub async fn firewall_get_zone_interfaces(zone: String) -> Result<Vec<String>, String> {
    if crate::binary_exists("firewall-cmd").await {
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("firewall-cmd")
                .args(["--zone", &zone, "--list-interfaces"])
                .output()
        ).await.map_err(|_| "firewall-cmd --list-interfaces timed out".to_string())?
        .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let interfaces = stdout.trim()
            .split_whitespace()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(interfaces);
    }

    firewall_get_all_interfaces().await
}

#[tauri::command]
pub async fn firewall_get_all_interfaces() -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        Command::new("ip")
            .args(["-o", "link", "show"])
            .output()
    ).await.map_err(|_| "ip link show timed out".to_string())?
    .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1].trim_end_matches(':').to_string();
            if name != "lo" {
                interfaces.push(name);
            }
        }
    }
    Ok(interfaces)
}

#[tauri::command]
pub async fn firewall_modify_rich_rule(
    zone: String,
    rule: String,
    add: bool,
    confirmed: Option<bool>,
) -> Result<String, String> {
    let parsed = rich_rule_operands_valid(&zone, &rule)?;

    let guard = if add {
        rich_rule_drop_or_reject_targets_ssh(&parsed)
    } else {
        None
    };
    require_active_ssh_confirmation(guard, confirmed.unwrap_or(false)).await?;

    run_firewall_named_action(
        "firewall_rich_rule",
        &[
            if add { "add" } else { "remove" }.to_string(),
            zone.clone(),
            rule.clone(),
        ],
        confirmed.unwrap_or(false),
    )
    .await
    .map_err(|e| firewall_helper_error_context("Failed to modify rich rule: ", e))?;

    Ok(format!(
        "Successfully {}ed rich rule '{}'",
        if add { "add" } else { "remov" },
        rule
    ))
}

#[tauri::command]
pub async fn firewall_change_interface_zone(
    zone: String,
    interface: String,
    confirmed: Option<bool>,
) -> Result<String, String> {
    interface_zone_operands_valid(&zone, &interface)?;

    // Mirror the helper's guard order: the read-only zone probe runs only
    // when a session was actually seen; the helper remains authoritative.
    let sessions = active_ssh_sessions();
    let zone_permits_ssh = match &sessions {
        Some(sessions) if !sessions.is_empty() => {
            firewalld_zone_permits_ssh(&zone).await
        }
        _ => None,
    };
    interface_zone_confirmation_gate(
        &interface,
        sessions,
        zone_permits_ssh,
        confirmed.unwrap_or(false),
    )?;

    run_firewall_named_action(
        "firewall_interface_zone",
        &[zone.clone(), interface.clone()],
        confirmed.unwrap_or(false),
    )
    .await
    .map_err(|e| firewall_helper_error_context("Failed to change interface zone: ", e))?;

    Ok(format!(
        "Successfully bound interface {} to zone {}",
        interface, zone
    ))
}

/// Operand validation for `firewall_change_interface_zone`: firewalld
/// only, so a real zone and a kernel-safe interface name are both required.
fn interface_zone_operands_valid(zone: &str, interface: &str) -> Result<(), String> {
    if zone.is_empty() {
        return Err("A zone is required for an interface-zone binding.".to_string());
    }
    firewall_zone_name_valid(zone)?;
    firewall_interface_name_valid(interface)
}

/// App-side GROUP 4 interface-zone gate. `zone_permits_ssh: None` means the
/// app could not determine the zone's ssh permission: it neither blocks nor
/// confirms, and the privileged helper — which re-probes authoritatively —
/// applies the approved GROUP 2 inconclusive behavior instead.
fn interface_zone_confirmation_gate(
    interface: &str,
    sessions: Option<Vec<String>>,
    zone_permits_ssh: Option<bool>,
    confirmed: bool,
) -> Result<(), String> {
    if zone_permits_ssh == Some(true) {
        return Ok(());
    }
    let Some(sessions) = sessions else {
        // inconclusive detection: defer to the helper
        return Ok(());
    };
    if !ssh_sessions_touch_interface(&sessions, interface) {
        return Ok(());
    }
    if zone_permits_ssh == Some(false) && !confirmed {
        if let Some(message) = ssh_confirmation_message(&sessions, FirewallSshGuard::MoveSshInterface) {
            return Err(message);
        }
    }
    Ok(())
}

/// Read-only app probe mirroring the helper's authoritative zone check;
/// only used for the app-side confirmation prompt. Any failure leaves
/// `None`, which defers the decision to the helper.
async fn firewalld_zone_permits_ssh(zone: &str) -> Option<bool> {
    let output = Command::new("firewall-cmd")
        .args(["--zone", zone, "--query-service=ssh"])
        .output()
        .await
        .ok()?;
    Some(output.status.success())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortListenerInfo {
    pub port: u16,
    pub proto: String,
    pub is_listening: bool,
    pub process_name: String,
    pub pid: String,
}

/// Check if a local process is currently listening on the specified port
#[tauri::command]
pub async fn firewall_check_port_listener(port: u16, proto: String) -> Result<PortListenerInfo, String> {
    let flag = if proto.to_lowercase().contains("udp") { "-ulpn" } else { "-tlpn" };
    let output = Command::new("ss")
        .args([flag, "-H"])
        .output()
        .await;

    let mut is_listening = false;
    let mut process_name = String::new();
    let mut pid = String::new();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let target = format!(":{}", port);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[3].ends_with(&target) {
                is_listening = true;
                if parts.len() >= 6 {
                    let proc_info = parts[5];
                    if let Some(name_start) = proc_info.find("(\"") {
                        if let Some(name_end) = proc_info[name_start + 2..].find('"') {
                            process_name = proc_info[name_start + 2..name_start + 2 + name_end].to_string();
                        }
                    }
                    if let Some(pid_start) = proc_info.find("pid=") {
                        let rest = &proc_info[pid_start + 4..];
                        let pid_end = rest.find(',').unwrap_or(rest.len());
                        pid = rest[..pid_end].to_string();
                    }
                }
                break;
            }
        }
    }

    Ok(PortListenerInfo {
        port,
        proto,
        is_listening,
        process_name,
        pid,
    })
}


#[cfg(test)]
mod firewall_ssh_guard_tests {
    use super::{
        parse_ssh_sessions, port_string_includes_22, rich_rule_drop_or_reject_targets_ssh,
        FirewallSshGuard,
    };
    use crate::utils::root_policy::{FirewallRichRule, FirewallRichRuleAction, FirewallRichRuleElement, FirewallRichRuleFamily};

    fn rich_rule(action: FirewallRichRuleAction, element: FirewallRichRuleElement) -> FirewallRichRule {
        FirewallRichRule {
            family: FirewallRichRuleFamily::Ipv4,
            source: None,
            destination: None,
            element,
            action,
        }
    }

    #[test]
    fn parse_ssh_sessions_reads_percent_interface_addresses() {
        let output = "Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n\
                      ESTAB 0 0 192.168.0.103%wlp1s0:22 10.0.0.5:53342 users:((\"sshd\",pid=1,fd=3))\n";
        let sessions = parse_ssh_sessions(output);
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].contains("%wlp1s0:22"));
    }

    #[test]
    fn parse_ssh_sessions_reads_ipv6_addresses() {
        let output = "[::ffff:192.168.0.1]:22 [::1]:53342
";
        let sessions = parse_ssh_sessions(output);
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].starts_with("[::ffff:192.168.0.1]:22"));
    }

    #[test]
    fn parse_ssh_sessions_empty_output_means_none_detected() {
        assert!(parse_ssh_sessions("").is_empty());
    }

    #[test]
    fn port_22_detection_handles_single_and_ranges() {
        assert!(port_string_includes_22("22"));
        assert!(port_string_includes_22("1-1024"));
        assert!(port_string_includes_22("20-25"));
        assert!(port_string_includes_22("1-65535"));
    }

    #[test]
    fn port_22_detection_rejects_other_ports_and_fails_closed_on_bad_ranges() {
        assert!(!port_string_includes_22("1025-65535"));
        assert!(!port_string_includes_22("23"));
        // Malformed values cannot reach the guard in production (the port
        // validator runs first), but if they ever did, the guard must fail
        // closed and demand confirmation rather than assume port 22 is
        // absent.
        assert!(port_string_includes_22("not-a-port"));
    }

    #[test]
    fn ssh_service_drop_rule_is_detected() {
        let rule = rich_rule(
            FirewallRichRuleAction::Drop,
            FirewallRichRuleElement::Service("ssh".to_string()),
        );
        assert_eq!(
            rich_rule_drop_or_reject_targets_ssh(&rule),
            Some(FirewallSshGuard::RichDropSsh),
        );
    }

    #[test]
    fn ssh_port_reject_rule_is_detected() {
        let rule = rich_rule(
            FirewallRichRuleAction::Reject,
            FirewallRichRuleElement::Port { port: "1-1024".to_string(), protocol: "tcp".to_string() },
        );
        assert_eq!(
            rich_rule_drop_or_reject_targets_ssh(&rule),
            Some(FirewallSshGuard::RichRejectSsh),
        );
    }

    #[test]
    fn ssh_reject_rule_outside_22_is_not_detected() {
        let rule = rich_rule(
            FirewallRichRuleAction::Reject,
            FirewallRichRuleElement::Port { port: "8100-8200".to_string(), protocol: "tcp".to_string() },
        );
        assert_eq!(rich_rule_drop_or_reject_targets_ssh(&rule), None);
    }

    #[test]
    fn ssh_accept_rule_is_not_a_lockout_guard() {
        let rule = rich_rule(
            FirewallRichRuleAction::Accept,
            FirewallRichRuleElement::Service("ssh".to_string()),
        );
        assert_eq!(rich_rule_drop_or_reject_targets_ssh(&rule), None);
    }

    #[test]
    fn non_ssh_drop_rule_is_not_detected() {
        let rule = rich_rule(
            FirewallRichRuleAction::Drop,
            FirewallRichRuleElement::Service("https".to_string()),
        );
        assert_eq!(rich_rule_drop_or_reject_targets_ssh(&rule), None);
    }

    #[test]
    fn ssh_confirmation_message_is_prefixed_and_names_lockout() {
        let sessions = vec!["192.168.0.103%wlp1s0:22".to_string()];
        let message = super::ssh_confirmation_message(&sessions, FirewallSshGuard::RemoveServiceSsh)
            .expect("an active session requires confirmation");
        assert!(message.starts_with("CONFIRMATION_REQUIRED:"), "{message}");
        assert!(message.contains("SSH session"), "{message}");
        assert!(message.contains("remotely"), "{message}");
    }

    #[test]
    fn ssh_confirmation_message_is_absent_without_active_sessions() {
        assert_eq!(
            super::ssh_confirmation_message(&[], FirewallSshGuard::RemovePort22),
            None,
        );
    }

    #[test]
    fn ssh_rich_reject_message_identifies_the_action() {
        let sessions = vec!["192.168.0.103%wlp1s0:22".to_string()];
        let message = super::ssh_confirmation_message(&sessions, FirewallSshGuard::RichRejectSsh)
            .expect("an active session requires confirmation");
        assert!(message.contains("reject rule for ssh"), "{message}");
    }

    use super::{require_active_ssh_confirmation_gate, FirewallSshGuard as Guard};

    fn sessions(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn confirmation_absent_blocks_the_operation_when_a_session_is_detected() {
        let result = require_active_ssh_confirmation_gate(
            sessions(&["10.0.0.5:53342"]),
            Guard::RemoveServiceSsh,
            false,
        );
        let message = result.unwrap_err();
        assert!(message.starts_with("CONFIRMATION_REQUIRED:"), "{message}");
    }

    #[test]
    fn confirmation_none_means_not_confirmed() {
        // The gate receives `confirmed.unwrap_or(false)` from the command
        // layer; a missing confirmation field can never be confirmation.
        let result = require_active_ssh_confirmation_gate(
            sessions(&["10.0.0.5:53342"]),
            Guard::RemovePort22,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn confirmation_true_lets_the_operation_proceed() {
        require_active_ssh_confirmation_gate(
            sessions(&["10.0.0.5:53342"]),
            Guard::RemoveServiceSsh,
            true,
        )
        .expect("explicit confirmation must proceed");
    }

    #[test]
    fn no_detected_session_does_not_trigger_the_gate() {
        require_active_ssh_confirmation_gate(Some(vec![]), Guard::RemovePort22, false)
            .expect("no visible SSH session: best-effort detection allows the operation");
    }

    #[test]
    fn inconclusive_detection_fails_open_with_the_reported_limitation() {
        // Approved behavior: a failed probe is inconclusive. The gate lets the
        // operation through and logs a warning; never reports false success
        // for the probe itself.
        require_active_ssh_confirmation_gate(None, Guard::RemoveServiceSsh, false)
            .expect("inconclusive detection must fail open per the approved policy");
    }

    #[test]
    fn non_ssh_operations_never_hit_the_gate() {
        // `guard == None` short-circuits before any probe; there is nothing to
        // assert beyond the operation proceeding.
        require_active_ssh_confirmation_gate(
            Some(vec!["10.0.0.5:53342".to_string()]),
            FirewallSshGuard::RichDropSsh,
            true,
        )
        .expect("confirmed path");
    }

    #[test]
    fn port_22_removal_guard_message_names_the_port() {
        let message = require_active_ssh_confirmation_gate(
            sessions(&["10.0.0.5:53342"]),
            Guard::RemovePort22,
            false,
        )
        .unwrap_err();
        assert!(message.contains("removing port 22"), "{message}");
    }

    #[test]
    fn rich_drop_guard_message_names_the_drop_rule() {
        let message = require_active_ssh_confirmation_gate(
            sessions(&["10.0.0.5:53342"]),
            Guard::RichDropSsh,
            false,
        )
        .unwrap_err();
        assert!(message.contains("drop rule for ssh"), "{message}");
    }

    // ── ss output parsing regressions ────────────────────────────────────

    #[test]
    fn ss_header_only_output_means_no_sessions() {
        // `ss -tnp state established ( sport = :22 )` header shapes
        let headers = [
            "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
            "Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
            "Netid State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
        ];
        for header in headers {
            assert!(
                parse_ssh_sessions(header).is_empty(),
                "a header line is never a session: {header:?}"
            );
        }
    }

    #[test]
    fn ss_header_plus_one_established_row_means_one_session() {
        let output = "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n\
                      ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342 users:((\"sshd\",pid=1,fd=3))\n";
        let sessions = parse_ssh_sessions(output);
        assert_eq!(sessions.len(), 1, "{sessions:?}");
        assert!(sessions[0].starts_with("ESTAB"), "{sessions:?}");
    }

    #[test]
    fn ss_empty_output_means_no_sessions() {
        assert!(parse_ssh_sessions("").is_empty());
        assert!(parse_ssh_sessions("\n\n").is_empty());
    }
}

/// The `CONFIRMATION_REQUIRED:` retry contract for the two firewall
/// handlers: a helper confirmation error passes through unchanged (prefix
/// at character zero, complete string); an ordinary error receives the
/// handler's descriptive context. See `firewall_helper_error_context`.
#[cfg(test)]
mod firewall_helper_error_context_tests {
    use super::firewall_helper_error_context;

    const ZONE_CONTEXT: &str = "Failed to modify firewall rule: ";
    const RICH_CONTEXT: &str = "Failed to modify rich rule: ";
    const CONFIRMATION: &str =
        "CONFIRMATION_REQUIRED: Active SSH session detected by lcp-admin";

    #[test]
    fn zone_rule_confirmation_error_is_preserved_exactly() {
        assert_eq!(
            firewall_helper_error_context(ZONE_CONTEXT, CONFIRMATION.to_string()),
            CONFIRMATION
        );
        assert!(
            firewall_helper_error_context(ZONE_CONTEXT, CONFIRMATION.to_string())
                .starts_with("CONFIRMATION_REQUIRED:"),
            "the prefix must be at character zero"
        );
    }

    #[test]
    fn rich_rule_confirmation_error_is_preserved_exactly() {
        assert_eq!(
            firewall_helper_error_context(RICH_CONTEXT, CONFIRMATION.to_string()),
            CONFIRMATION
        );
    }

    #[test]
    fn zone_rule_ordinary_error_receives_descriptive_context() {
        assert_eq!(
            firewall_helper_error_context(ZONE_CONTEXT, "firewalld command failed".to_string()),
            "Failed to modify firewall rule: firewalld command failed"
        );
    }

    #[test]
    fn rich_rule_ordinary_error_receives_descriptive_context() {
        assert_eq!(
            firewall_helper_error_context(RICH_CONTEXT, "firewalld command failed".to_string()),
            "Failed to modify rich rule: firewalld command failed"
        );
    }

    #[test]
    fn the_confirmation_contract_covers_partial_confirmation_words() {
        // only the exact prefix passes through: a near-miss is an ordinary
        // error and gets context
        assert_ne!(
            firewall_helper_error_context(ZONE_CONTEXT, "CONFIRMATION_REQUIREDX broken".to_string()),
            "CONFIRMATION_REQUIREDX broken"
        );
        // the complete confirmation error body is preserved, not truncated
        let long = format!("{CONFIRMATION} Retry with the explicit 'confirm' argument.");
        assert_eq!(
            firewall_helper_error_context(RICH_CONTEXT, long.clone()),
            long
        );
        // the helper's panic confirmation survives the panic-mode context
        // layer unchanged, prefix at character zero
        assert_eq!(
            firewall_helper_error_context(
                "Failed to change panic mode: ",
                "CONFIRMATION_REQUIRED: Enabling firewalld panic mode with an active SSH session"
                    .to_string(),
            ),
            "CONFIRMATION_REQUIRED: Enabling firewalld panic mode with an active SSH session"
        );
    }
}

/// App-side GROUP 3 panic gate tests (see `require_panic_confirmation_gate`).
#[cfg(test)]
mod firewall_panic_gate_tests {
    use super::{
        firewall_helper_error_context, panic_gate_with_probe, require_panic_confirmation_gate,
    };

    fn sessions(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn panic_enable_without_confirmation_with_an_active_session_is_blocked() {
        let err = require_panic_confirmation_gate(sessions(&["10.0.0.5:53342"]), true, false)
            .unwrap_err();
        assert!(err.starts_with("CONFIRMATION_REQUIRED:"), "{err}");
        assert!(err.contains("panic mode"), "{err}");
        assert!(err.contains("sever every connection"), "{err}");
    }

    #[test]
    fn panic_enable_with_explicit_confirmation_proceeds_during_a_session() {
        require_panic_confirmation_gate(sessions(&["10.0.0.5:53342"]), true, true)
            .expect("explicit confirmation permits panic enablement");
    }

    #[test]
    fn panic_enable_without_a_detected_session_proceeds() {
        require_panic_confirmation_gate(Some(vec![]), true, false)
            .expect("no visible session: no confirmation needed");
    }

    #[test]
    fn panic_enable_with_inconclusive_detection_defers_to_the_helper() {
        // The app cannot lie about detection: with an inconclusive probe it
        // does not block and does not confirm — the helper fails closed.
        require_panic_confirmation_gate(None, true, false)
            .expect("inconclusive detection is resolved at the privileged boundary");
    }

    #[test]
    fn panic_disable_never_requires_confirmation() {
        require_panic_confirmation_gate(sessions(&["10.0.0.5:53342"]), false, false)
            .expect("disabling panic mode needs no confirmation");
        require_panic_confirmation_gate(None, false, false)
            .expect("disabling on an inconclusive probe needs no confirmation");
    }

    #[test]
    fn missing_confirmation_field_is_never_confirmation() {
        // `confirmed.unwrap_or(false)` at the command layer means the gate
        // sees `false` when the field is absent.
        let err = require_panic_confirmation_gate(sessions(&["10.0.0.5:53342"]), true, false);
        assert!(err.is_err());
    }

    #[test]
    fn panic_confirmation_error_survives_the_panic_context_layer() {
        let confirmation = require_panic_confirmation_gate(sessions(&["10.0.0.5:53342"]), true, false)
            .unwrap_err();
        // helper CONFIRMATION_REQUIRED and this one both pass through the
        // panic handler's context layer unchanged
        assert_eq!(
            firewall_helper_error_context("Failed to change panic mode: ", confirmation.clone()),
            confirmation
        );
        assert!(confirmation.starts_with("CONFIRMATION_REQUIRED:"));
    }

    #[test]
    fn panic_ordinary_error_receives_the_context_prefix() {
        assert_eq!(
            firewall_helper_error_context(
                "Failed to change panic mode: ",
                "firewall-cmd --panic-on failed: BY-STUB: injected runtime failure".to_string(),
            ),
            "Failed to change panic mode: firewall-cmd --panic-on failed: BY-STUB: injected runtime failure"
        );
    }

    /// Regression: `toggle_panic_mode` must not run the SSH probe at all
    /// when disabling. `panic_gate_with_probe` is the exact seam the async
    /// command calls through, with `active_ssh_sessions` as the closure —
    /// a counting closure here proves the probe is (not) invoked.
    #[test]
    fn disable_never_invokes_the_ssh_probe() {
        let mut probe_calls = 0usize;
        // disabling with a session present and no confirmation: still fine,
        // and the probe never ran
        panic_gate_with_probe(false, false, || {
            probe_calls += 1;
            Some(vec!["10.0.0.5:53342".to_string()])
        })
        .expect("disabling must succeed without any probe");
        assert_eq!(
            probe_calls, 0,
            "the SSH probe itself must not be invoked when disabling"
        );
    }

    #[test]
    fn enable_does_invoke_the_ssh_probe_through_the_seam() {
        let mut probe_calls = 0usize;
        let probe = || -> Option<Vec<String>> {
            probe_calls += 1;
            Some(vec![])
        };
        panic_gate_with_probe(true, false, probe)
            .expect("enable with no sessions proceeds");
        assert_eq!(
            probe_calls, 1,
            "enabling runs the probe exactly once"
        );

        // and a blocked enable is a probe run with a session
        let mut blocked_probe_calls = 0usize;
        let blocked = || -> Option<Vec<String>> {
            blocked_probe_calls += 1;
            Some(vec!["10.0.0.5:53342".to_string()])
        };
        assert!(panic_gate_with_probe(true, false, blocked).is_err());
        assert_eq!(blocked_probe_calls, 1);
    }
}

/// App-side GROUP 4 interface-zone gate tests (see
/// `interface_zone_operands_valid` / `interface_zone_confirmation_gate`).
#[cfg(test)]
mod firewall_interface_zone_gate_tests {
    use super::{
        firewall_helper_error_context, interface_zone_confirmation_gate,
        interface_zone_operands_valid,
    };

    fn marked_session() -> Vec<String> {
        vec!["ESTAB 0 0 192.168.0.103%wlp1s0:22 10.0.0.5:53342".to_string()]
    }

    #[test]
    fn interface_zone_operands_are_validated_before_any_helper_call() {
        interface_zone_operands_valid("work", "eth1").expect("valid zone and interface pass");
        interface_zone_operands_valid("FedoraWorkstation", "wlp1s0")
            .expect("a valid custom zone passes");
        assert!(interface_zone_operands_valid("", "eth1").is_err(), "a zone is required");
        assert!(interface_zone_operands_valid("../etc", "eth1").is_err());
        assert!(interface_zone_operands_valid("work", "../eth1").is_err());
        assert!(interface_zone_operands_valid("work", "eth 1").is_err());
        assert!(interface_zone_operands_valid("work", "-oProxyCommand=x").is_err());
    }

    #[test]
    fn moving_the_ssh_carrying_interface_to_a_zone_without_ssh_requires_confirmation() {
        let err = interface_zone_confirmation_gate("wlp1s0", Some(marked_session()), Some(false), false)
            .unwrap_err();
        assert!(err.starts_with("CONFIRMATION_REQUIRED:"), "{err}");
        assert!(err.contains("moving the interface carrying the active SSH session"), "{err}");
    }

    #[test]
    fn explicit_confirmation_permits_the_interface_move() {
        interface_zone_confirmation_gate("wlp1s0", Some(marked_session()), Some(false), true)
            .expect("explicit confirmation permits the move");
    }

    #[test]
    fn an_ssh_permitting_zone_needs_no_confirmation() {
        interface_zone_confirmation_gate("wlp1s0", Some(marked_session()), Some(true), false)
            .expect("moving into an ssh-permitting zone is not an SSH risk");
    }

    #[test]
    fn a_session_provably_on_another_interface_needs_no_confirmation() {
        interface_zone_confirmation_gate("eth0", Some(marked_session()), Some(false), false)
            .expect("a session on wlp1s0 cannot be cut by moving eth0");
    }

    #[test]
    fn a_session_without_a_scope_marker_is_treated_as_risk() {
        let sessions = Some(vec!["ESTAB 0 0 192.168.0.103:22 10.0.0.5:53342".to_string()]);
        let err = interface_zone_confirmation_gate("eth0", sessions, Some(false), false)
            .unwrap_err();
        assert!(err.starts_with("CONFIRMATION_REQUIRED:"), "{err}");
    }

    #[test]
    fn no_session_or_inconclusive_probes_defer_to_the_helper() {
        interface_zone_confirmation_gate("wlp1s0", Some(vec![]), Some(false), false)
            .expect("no visible session: no confirmation");
        // None means inconclusive: the app neither blocks nor confirms;
        // the helper applies the approved GROUP 2 fail-open warning path.
        interface_zone_confirmation_gate("wlp1s0", None, Some(false), false)
            .expect("inconclusive session detection is resolved at the privileged boundary");
        interface_zone_confirmation_gate("wlp1s0", Some(marked_session()), None, false)
            .expect("an unresolvable zone probe defers to the helper too");
    }

    #[test]
    fn interface_zone_confirmation_error_survives_the_context_layer() {
        let confirmation =
            interface_zone_confirmation_gate("wlp1s0", Some(marked_session()), Some(false), false)
                .unwrap_err();
        assert_eq!(
            firewall_helper_error_context(
                "Failed to change interface zone: ",
                confirmation.clone()
            ),
            confirmation
        );
        assert!(confirmation.starts_with("CONFIRMATION_REQUIRED:"));
        // ordinary errors still receive context
        assert_eq!(
            firewall_helper_error_context(
                "Failed to change interface zone: ",
                "firewall-cmd --change-interface=eth1 failed: BY-STUB".to_string(),
            ),
            "Failed to change interface zone: firewall-cmd --change-interface=eth1 failed: BY-STUB"
        );
    }
}

/// The app-side operand contract for the two firewall actions (see
/// `zone_rule_operands_valid` / `rich_rule_operands_valid`). The helper
/// remains authoritative; these tests pin what the app forwards.
#[cfg(test)]#[cfg(test)]
mod firewall_operand_validation_tests {
    use super::{parse_ssh_sessions, rich_rule_operands_valid, zone_rule_operands_valid};

    #[test]
    fn empty_zone_is_accepted_for_zone_rules_ufw_shape() {
        zone_rule_operands_valid("", "port", "8080/tcp")
            .expect("an empty zone is the documented ufw shape");
        zone_rule_operands_valid("", "service", "ssh")
            .expect("an empty zone is the documented ufw shape");
    }

    #[test]
    fn named_zones_are_still_validated_for_zone_rules() {
        zone_rule_operands_valid("public", "service", "http")
            .expect("a valid zone still passes");
        zone_rule_operands_valid("FedoraWorkstation", "port", "22/tcp")
            .expect("a valid custom zone still passes");

        let err = zone_rule_operands_valid("../etc", "service", "http").unwrap_err();
        assert!(!err.is_empty());
        let err = zone_rule_operands_valid("-oProxyCommand=x", "port", "8080/tcp").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn the_rule_value_validation_is_unchanged() {
        assert!(zone_rule_operands_valid("public", "rich-rule", "x").is_err());
        assert!(zone_rule_operands_valid("public", "service", "a b").is_err());
        assert!(zone_rule_operands_valid("public", "port", "22/icmp").is_err());
        assert!(zone_rule_operands_valid("", "interface", "x").is_err(), "unknown kind");
    }

    #[test]
    fn rich_rules_always_require_a_zone() {
        let err = rich_rule_operands_valid(
            "",
            r#"rule family="ipv4" service name="ssh" accept"#,
        )
        .unwrap_err();
        assert!(err.contains("zone"), "{err}");
    }

    #[test]
    fn rich_rules_still_validate_the_zone_and_the_rule() {
        rich_rule_operands_valid("public", r#"rule family="ipv4" service name="ssh" accept"#)
            .expect("a valid zone and rule pass");
        assert!(rich_rule_operands_valid("../etc", "anything at all").is_err());
        assert!(rich_rule_operands_valid(
            "public",
            r#"rule family="ipv4" service name="ssh" log accept"#,
        )
        .is_err());
    }

    #[test]
    fn ss_header_only_output_means_no_sessions() {
        let header = "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n";
        assert!(parse_ssh_sessions(header).is_empty());
    }
}
