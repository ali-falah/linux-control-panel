use serde::{Deserialize, Serialize};
use crate::utils::privilege::tokio::Command;
use std::sync::Mutex;
use std::time::SystemTime;

pub static LAST_SYSCTL_FIX_TIME: Mutex<Option<SystemTime>> = Mutex::new(None);

async fn check_sysctl_tampered() -> bool {
    let fix_time = match LAST_SYSCTL_FIX_TIME.lock() {
        Ok(guard) => match *guard {
            Some(t) => t,
            None => return false,
        },
        Err(_) => return false,
    };

    if let Ok(entries) = std::fs::read_dir("/etc/sysctl.d") {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified > fix_time + std::time::Duration::from_secs(2) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ─── Data Structures ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecurityFinding {
    pub id: String,
    pub title: String,
    pub description: String,
    /// "Critical" | "Warning" | "Good" | "Info"
    pub severity: String,
    pub countermeasure: String,
    pub category: String,
    pub has_auto_fix: bool,
    pub is_resolved: bool,
    /// Optional CVE or reference link
    pub reference: Option<String>,
    pub tamper_flag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecurityReport {
    pub score: u32,
    pub findings: Vec<SecurityFinding>,
    pub category_scores: Vec<CategoryScore>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryScore {
    pub category: String,
    pub score: u32,
    pub max_score: u32,
    pub issues: u32,
}

pub static LAST_REPORT: Mutex<Option<(SecurityReport, SystemTime)>> = Mutex::new(None);

pub fn invalidate_audit_cache() {
    if let Ok(mut guard) = LAST_REPORT.lock() {
        *guard = None;
    }
}

// ─── Helper macros ────────────────────────────────────────────────────────────

/// Run a command with a fast timeout (800 ms). Returns stdout string or empty on error.
async fn read_cmd(args: &[&str]) -> String {
    if args.is_empty() { return String::new(); }
    let mut cmd = tokio::process::Command::new(args[0]);
    for a in &args[1..] { cmd.arg(a); }
    cmd.stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());
    let out = tokio::time::timeout(
        tokio::time::Duration::from_millis(800),
        cmd.output(),
    ).await;
    match out {
        Ok(Ok(o)) => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => String::new(),
    }
}

/// Read a file safely without blocking prompts.
async fn read_privileged_file(path: &str) -> String {
    if let Ok(content) = tokio::fs::read_to_string(path).await {
        if !content.trim().is_empty() {
            return content;
        }
    }
    let out = read_cmd(&["sudo", "-n", "cat", path]).await;
    if !out.trim().is_empty() {
        return out;
    }
    String::new()
}

/// Read a world-readable file directly.
async fn read_file(path: &str) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

/// Read a sysctl value from /proc or sysctl command.
async fn read_sysctl(key: &str) -> String {
    let path = format!("/proc/sys/{}", key.replace('.', "/"));
    let v = read_file(&path).await;
    if !v.is_empty() { return v.trim().to_string(); }
    // Fallback: sysctl binary
    let out = read_cmd(&["sysctl", "-n", key]).await;
    out.trim().to_string()
}

fn good(id: &str, title: &str, desc: &str, countermeasure: &str, category: &str, reference: Option<&str>) -> SecurityFinding {
    SecurityFinding {
        id: id.to_string(), title: title.to_string(), description: desc.to_string(),
        severity: "Good".to_string(), countermeasure: countermeasure.to_string(),
        category: category.to_string(), has_auto_fix: true, is_resolved: true,
        reference: reference.map(|s| s.to_string()),
        tamper_flag: None,
    }
}

fn warn(id: &str, title: &str, desc: &str, countermeasure: &str, category: &str, has_fix: bool, reference: Option<&str>) -> SecurityFinding {
    SecurityFinding {
        id: id.to_string(), title: title.to_string(), description: desc.to_string(),
        severity: "Warning".to_string(), countermeasure: countermeasure.to_string(),
        category: category.to_string(), has_auto_fix: has_fix, is_resolved: false,
        reference: reference.map(|s| s.to_string()),
        tamper_flag: None,
    }
}

fn crit(id: &str, title: &str, desc: &str, countermeasure: &str, category: &str, has_fix: bool, reference: Option<&str>) -> SecurityFinding {
    SecurityFinding {
        id: id.to_string(), title: title.to_string(), description: desc.to_string(),
        severity: "Critical".to_string(), countermeasure: countermeasure.to_string(),
        category: category.to_string(), has_auto_fix: has_fix, is_resolved: false,
        reference: reference.map(|s| s.to_string()),
        tamper_flag: None,
    }
}

fn info_finding(id: &str, title: &str, desc: &str, countermeasure: &str, category: &str, has_fix: bool) -> SecurityFinding {
    SecurityFinding {
        id: id.to_string(), title: title.to_string(), description: desc.to_string(),
        severity: "Info".to_string(), countermeasure: countermeasure.to_string(),
        category: category.to_string(), has_auto_fix: has_fix, is_resolved: false,
        reference: None,
        tamper_flag: None,
    }
}

// ─── Main Audit Command ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn security_run_audit(force_refresh: Option<bool>) -> Result<SecurityReport, String> {
    if !force_refresh.unwrap_or(false) {
        if let Ok(guard) = LAST_REPORT.lock() {
            if let Some((ref report, ref time)) = *guard {
                if let Ok(elapsed) = time.elapsed() {
                    if elapsed < std::time::Duration::from_secs(30) {
                        return Ok(report.clone());
                    }
                }
            }
        }
    }

    use std::panic::AssertUnwindSafe;
    use futures::FutureExt;

    let result = AssertUnwindSafe(async {
        crate::log_to_file("INFO", "security_run_audit: started");
        let mut findings: Vec<SecurityFinding> = Vec::new();

        // ── Category point accumulators ───────────────────────────────────────
        // Weighted: Critical=5pts, Warning=3pts, Good=5pts (each check has 5pts max)
        let mut ssh_cur = 0u32;       let mut ssh_max = 0u32;
        let mut kernel_cur = 0u32;    let mut kernel_max = 0u32;
        let mut user_cur = 0u32;      let mut user_max = 0u32;
        let mut fs_cur = 0u32;        let mut fs_max = 0u32;
        let mut network_cur = 0u32;   let mut network_max = 0u32;
        let mut system_cur = 0u32;    let mut system_max = 0u32;

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 1: SSH HARDENING
        // ═══════════════════════════════════════════════════════════════════════
        let cat_ssh = "SSH Hardening";
        // Use `sshd -T` or direct config parsing without blocking prompts
        let sshd_t = read_cmd(&["bash", "-c",
            "sudo -n sshd -T 2>/dev/null || \
             sshd -T -C user=root,host=localhost,addr=127.0.0.1 2>/dev/null || \
             sshd -T 2>/dev/null || \
             /usr/sbin/sshd -T 2>/dev/null || true"]).await;
        let ssh_full = if !sshd_t.trim().is_empty() {
            sshd_t
        } else {
            let ssh_cfg = read_privileged_file("/etc/ssh/sshd_config").await;
            let ssh_cfg_d = read_cmd(&["bash", "-c", "sudo -n cat /etc/ssh/sshd_config.d/*.conf 2>/dev/null || cat /etc/ssh/sshd_config.d/*.conf 2>/dev/null || true"]).await;
            format!("{}\n{}", ssh_cfg, ssh_cfg_d)
        };

        fn ssh_val(cfg: &str, key: &str) -> Option<String> {
            // Find the last active (non-commented) occurrence of key
            let mut last = None;
            for line in cfg.lines() {
                let l = line.trim();
                if l.starts_with('#') { continue; }
                let mut parts = l.splitn(2, char::is_whitespace);
                if let Some(k) = parts.next() {
                    if k.eq_ignore_ascii_case(key) {
                        if let Some(v) = parts.next() {
                            last = Some(v.trim().to_string());
                        }
                    }
                }
            }
            last
        }

        // 1a. SSH Root Login
        ssh_max += 5;
        let permit_val = ssh_val(&ssh_full, "PermitRootLogin").unwrap_or_else(|| "unknown/default".to_string());
        let root_login_secured = matches!(permit_val.as_str(), "no" | "prohibit-password");
        if root_login_secured {
            ssh_cur += 5;
            findings.push(good("ssh_root", "SSH Root Login", &format!("Root login over SSH is secured (PermitRootLogin={}).", permit_val),
                "None required. Maintain current configuration.", cat_ssh, Some("CIS 5.2.8")));
        } else {
            findings.push(crit("ssh_root", "SSH Root Login Enabled",
                &format!("Root login over SSH is permitted (PermitRootLogin={}). Exposes root to brute-force attacks.", permit_val),
                "Set PermitRootLogin to 'prohibit-password' or 'no' in /etc/ssh/sshd_config.", cat_ssh, true, Some("CIS 5.2.8")));
        }

        // 1b. SSH Password Authentication
        ssh_max += 5;
        let pass_auth = ssh_val(&ssh_full, "PasswordAuthentication").unwrap_or_else(|| "yes".to_string());
        let pw_auth_disabled = pass_auth.eq_ignore_ascii_case("no");
        if pw_auth_disabled {
            ssh_cur += 5;
            findings.push(good("ssh_pass_auth", "SSH Password Authentication Disabled",
                "Only key-based SSH authentication is allowed. Password brute-force is not possible.",
                "Maintain key-only SSH to prevent credential stuffing attacks.", cat_ssh, Some("CIS 5.2.11")));
        } else {
            findings.push(warn("ssh_pass_auth", "SSH Password Authentication Enabled",
                "Password-based SSH logins are allowed. Attackers can attempt to brute-force user credentials.",
                "Set 'PasswordAuthentication no' in /etc/ssh/sshd_config and use SSH keys.", cat_ssh, true, Some("CIS 5.2.11")));
        }

        // 1c. SSH MaxAuthTries
        ssh_max += 5;
        let max_tries: u32 = ssh_val(&ssh_full, "MaxAuthTries")
            .and_then(|v| v.parse().ok()).unwrap_or(6);
        if max_tries <= 4 {
            ssh_cur += 5;
            findings.push(good("ssh_max_auth", "SSH MaxAuthTries Restricted",
                &format!("MaxAuthTries is set to {} (≤4). Brute-force window is limited.", max_tries),
                "Continue maintaining a low MaxAuthTries value.", cat_ssh, None));
        } else {
            findings.push(warn("ssh_max_auth", "SSH MaxAuthTries Too High",
                &format!("MaxAuthTries is {} (should be ≤4). Attackers get more attempts per connection.", max_tries),
                "Set 'MaxAuthTries 4' in /etc/ssh/sshd_config.", cat_ssh, true, Some("CIS 5.2.7")));
        }

        // 1d. SSH Port (warning only if still 22)
        ssh_max += 5;
        let ssh_port: u16 = ssh_val(&ssh_full, "Port")
            .and_then(|v| v.parse().ok()).unwrap_or(22);
        if ssh_port != 22 {
            ssh_cur += 5;
            findings.push(good("ssh_port", "SSH Running on Non-Default Port",
                &format!("SSH is listening on port {} (not 22). Reduces automated scan exposure.", ssh_port),
                "Ensure firewall rules are updated to allow the custom port.", cat_ssh, None));
        } else {
            // Warning only — not critical. User may use fail2ban etc.
            findings.push(warn("ssh_port", "SSH Using Default Port 22",
                "SSH is listening on the default port 22. Automated scanners actively target this port.",
                "Consider moving SSH to a non-standard port (e.g., 2222) alongside fail2ban to reduce noise. This is low-severity if you have fail2ban active.", cat_ssh, false, None));
            // Still award partial points since this is advisory
            ssh_cur += 3;
        }

        // 1e. SSH LoginGraceTime
        ssh_max += 5;
        let grace: u32 = {
            let raw = ssh_val(&ssh_full, "LoginGraceTime").unwrap_or_else(|| "120".to_string());
            // Parse "60", "1m", "2m" etc.
            if raw.ends_with('m') {
                raw.trim_end_matches('m').parse::<u32>().unwrap_or(2) * 60
            } else {
                raw.parse().unwrap_or(120)
            }
        };
        if grace <= 60 {
            ssh_cur += 5;
            findings.push(good("ssh_grace", "SSH LoginGraceTime Restricted",
                &format!("LoginGraceTime is {} seconds. Unauthenticated connections are dropped quickly.", grace),
                "Maintain short grace times to prevent connection exhaustion.", cat_ssh, None));
        } else {
            findings.push(warn("ssh_grace", "SSH LoginGraceTime Too Long",
                &format!("LoginGraceTime is {} seconds (should be ≤60). Unauthenticated connections can linger.", grace),
                "Set 'LoginGraceTime 60' in /etc/ssh/sshd_config.", cat_ssh, true, Some("CIS 5.2.14")));
        }

        // 1f. SSH X11 Forwarding
        ssh_max += 5;
        let x11_fwd = ssh_val(&ssh_full, "X11Forwarding").unwrap_or_else(|| "no".to_string()).to_lowercase();
        if x11_fwd == "no" {
            ssh_cur += 5;
            findings.push(good("ssh_x11", "SSH X11 Forwarding Disabled",
                "X11Forwarding is set to 'no'. GUI traffic forwarding over SSH is disabled, reducing attack surface.",
                "Keep X11Forwarding disabled unless explicitly required.", cat_ssh, Some("CIS 5.2.6")));
        } else {
            findings.push(warn("ssh_x11", "SSH X11 Forwarding Enabled",
                "X11Forwarding is enabled. Attackers with access to the remote X display could sniff mouse/keyboard input.",
                "Set 'X11Forwarding no' in /etc/ssh/sshd_config.", cat_ssh, true, Some("CIS 5.2.6")));
        }

        // 1g. SSH Client Idle Timeout
        ssh_max += 5;
        let alive_int: u32 = ssh_val(&ssh_full, "ClientAliveInterval").unwrap_or_else(|| "0".to_string()).parse().unwrap_or(0);
        let alive_cnt: u32 = ssh_val(&ssh_full, "ClientAliveCountMax").unwrap_or_else(|| "3".to_string()).parse().unwrap_or(3);
        if alive_int > 0 && alive_int <= 300 && alive_cnt <= 3 {
            ssh_cur += 5;
            findings.push(good("ssh_idle_timeout", "SSH Client Idle Timeout Configured",
                &format!("ClientAliveInterval is {}s and ClientAliveCountMax is {}. Inactive sessions are automatically terminated.", alive_int, alive_cnt),
                "Maintain idle timeouts to protect unattended terminal sessions.", cat_ssh, Some("CIS 5.2.13")));
        } else {
            findings.push(warn("ssh_idle_timeout", "SSH Client Idle Timeout Unconfigured",
                &format!("ClientAliveInterval is {} (should be 1-300s) and ClientAliveCountMax is {}. Inactive sessions remain open indefinitely.", alive_int, alive_cnt),
                "Set 'ClientAliveInterval 300' and 'ClientAliveCountMax 3' in /etc/ssh/sshd_config.", cat_ssh, true, Some("CIS 5.2.13")));
        }

        crate::log_to_file("INFO", "security_run_audit: SSH checks done");

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 2: KERNEL HARDENING
        // ═══════════════════════════════════════════════════════════════════════
        let cat_kernel = "Kernel Hardening";

        // 2a. ASLR
        kernel_max += 5;
        let aslr = read_sysctl("kernel.randomize_va_space").await;
        if aslr == "2" {
            kernel_cur += 5;
            findings.push(good("kernel_aslr", "ASLR Fully Enabled",
                "Address Space Layout Randomization (ASLR) is at maximum strength (value=2).",
                "Maintain this setting to prevent memory-based exploits.", cat_kernel, Some("CIS 1.6.2")));
        } else {
            findings.push(crit("kernel_aslr", "ASLR Disabled or Partial",
                &format!("kernel.randomize_va_space={} (should be 2). Memory layout is predictable — exploit reliability increases.", aslr),
                "Run: sysctl -w kernel.randomize_va_space=2 and persist in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, Some("CIS 1.6.2")));
        }

        // 2b. SYN Cookies
        kernel_max += 5;
        let syncookies = read_sysctl("net.ipv4.tcp_syncookies").await;
        if syncookies == "1" {
            kernel_cur += 5;
            findings.push(good("kernel_syncookies", "TCP SYN Cookies Enabled",
                "SYN cookie protection is active. SYN flood (DoS) attacks are mitigated.",
                "Maintain this setting to prevent SYN flood attacks.", cat_kernel, Some("CIS 3.3.8")));
        } else {
            findings.push(crit("kernel_syncookies", "TCP SYN Cookies Disabled",
                "net.ipv4.tcp_syncookies=0. The system is vulnerable to SYN flood denial-of-service attacks.",
                "Set net.ipv4.tcp_syncookies=1 in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, Some("CIS 3.3.8")));
        }

        // 2c. IP Forwarding
        kernel_max += 5;
        let ipfwd = read_sysctl("net.ipv4.ip_forward").await;
        if ipfwd == "0" {
            kernel_cur += 5;
            findings.push(good("kernel_ipforward", "IP Forwarding Disabled",
                "The system does not forward IP packets. It cannot be used as an unintended router.",
                "Only enable IP forwarding if this machine is a dedicated router or VPN server.", cat_kernel, Some("CIS 3.1.1")));
        } else {
            findings.push(warn("kernel_ipforward", "IP Forwarding Enabled",
                "net.ipv4.ip_forward=1. This machine can forward packets between interfaces (router mode). Disable unless intentional.",
                "Set net.ipv4.ip_forward=0 in /etc/sysctl.d/99-hardening.conf unless this is a router/VPN.", cat_kernel, true, Some("CIS 3.1.1")));
        }

        // 2d. Kernel Pointer Restriction
        kernel_max += 5;
        let kptr = read_sysctl("kernel.kptr_restrict").await;
        if kptr == "1" || kptr == "2" {
            kernel_cur += 5;
            findings.push(good("kernel_kptr", "Kernel Pointer Leaks Restricted",
                &format!("kernel.kptr_restrict={} — kernel symbol addresses are hidden from unprivileged users.", kptr),
                "A value of 2 is even stronger and hides pointers from root as well.", cat_kernel, None));
        } else {
            findings.push(warn("kernel_kptr", "Kernel Pointer Leaks Allowed",
                "kernel.kptr_restrict=0 — kernel symbol addresses are visible. Attackers can read /proc/kallsyms to aid exploits.",
                "Set kernel.kptr_restrict=1 in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, None));
        }

        // 2e. dmesg Restrict
        kernel_max += 5;
        let dmesg_r = read_sysctl("kernel.dmesg_restrict").await;
        if dmesg_r == "1" {
            kernel_cur += 5;
            findings.push(good("kernel_dmesg", "dmesg Access Restricted",
                "Unprivileged users cannot read kernel ring buffer messages via dmesg.",
                "Maintain this setting to prevent information disclosure.", cat_kernel, None));
        } else {
            findings.push(warn("kernel_dmesg", "dmesg Readable by Unprivileged Users",
                "kernel.dmesg_restrict=0 — any local user can read kernel log messages, which may leak memory addresses and hardware info.",
                "Set kernel.dmesg_restrict=1 in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, None));
        }

        // 2f. ICMP Redirects Accept
        kernel_max += 5;
        let icmp_redir = read_sysctl("net.ipv4.conf.all.accept_redirects").await;
        let icmp_redir6 = read_sysctl("net.ipv6.conf.all.accept_redirects").await;
        if icmp_redir == "0" && (icmp_redir6 == "0" || icmp_redir6.is_empty()) {
            kernel_cur += 5;
            findings.push(good("kernel_icmp_redirect", "ICMP Redirect Acceptance Disabled",
                "The system ignores ICMP redirect packets — routing table cannot be poisoned by attackers.",
                "Maintain these settings to prevent MITM routing attacks.", cat_kernel, Some("CIS 3.2.2")));
        } else {
            findings.push(crit("kernel_icmp_redirect", "ICMP Redirects Accepted",
                "ICMP redirect packets are accepted. A local attacker can manipulate the routing table and redirect traffic through a rogue host (MITM).",
                "Set net.ipv4.conf.all.accept_redirects=0 and net.ipv6.conf.all.accept_redirects=0", cat_kernel, true, Some("CIS 3.2.2")));
        }

        // 2g. Disable Unused Filesystem Modules
        kernel_max += 5;
        let modprobe_conf = read_cmd(&["bash", "-c", "cat /etc/modprobe.d/*.conf 2>/dev/null || true"]).await;
        let fs_disabled = modprobe_conf.contains("cramfs") && modprobe_conf.contains("hfs");
        if fs_disabled {
            kernel_cur += 5;
            findings.push(good("kernel_fs_modules", "Unused Filesystem Modules Blacklisted",
                "Unused legacy filesystem modules (cramfs, hfs, udf, etc.) are blacklisted via modprobe.",
                "Maintain modprobe blacklist to prevent mounting vulnerable legacy filesystems.", cat_kernel, Some("CIS 1.1.1")));
        } else {
            findings.push(warn("kernel_fs_modules", "Unused Legacy Filesystem Modules Unrestricted",
                "Legacy filesystem drivers (cramfs, freevxfs, hfs, hfsplus, jffs2, udf) can be dynamically loaded.",
                "Blacklist unused filesystem modules in /etc/modprobe.d/disable-unused-fs.conf", cat_kernel, true, Some("CIS 1.1.1")));
        }

        // 2h. Reverse Path Filtering (rp_filter)
        kernel_max += 5;
        let rp_filt = read_sysctl("net.ipv4.conf.all.rp_filter").await;
        if rp_filt == "1" {
            kernel_cur += 5;
            findings.push(good("kernel_rp_filter", "Reverse Path Filtering Enabled",
                "net.ipv4.conf.all.rp_filter=1. Source route validation protects against IP address spoofing.",
                "Maintain rp_filter=1 to prevent IP spoofing.", cat_kernel, Some("CIS 3.2.1")));
        } else {
            findings.push(warn("kernel_rp_filter", "Reverse Path Filtering Disabled or Loose",
                "net.ipv4.conf.all.rp_filter=0 (or 2). System accepts network packets with spoofed source addresses.",
                "Set net.ipv4.conf.all.rp_filter=1 in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, Some("CIS 3.2.1")));
        }

        // 2i. SysRq Restriction
        kernel_max += 5;
        let sysrq_val = read_sysctl("kernel.sysrq").await;
        if sysrq_val == "0" || sysrq_val == "4" {
            kernel_cur += 5;
            findings.push(good("kernel_sysrq", "Magic SysRq Key Restricted",
                &format!("kernel.sysrq={} — keyboard SysRq shortcuts are restricted.", sysrq_val),
                "Maintain restricted SysRq settings to prevent physical console reboot/dump exploits.", cat_kernel, Some("CIS 1.5.2")));
        } else {
            findings.push(warn("kernel_sysrq", "Magic SysRq Key Unrestricted",
                &format!("kernel.sysrq={} (should be 0 or 4). Anyone with physical or console access can trigger immediate reboots or dumps.", sysrq_val),
                "Set kernel.sysrq=4 in /etc/sysctl.d/99-hardening.conf", cat_kernel, true, Some("CIS 1.5.2")));
        }

        // 2j. Mask Ctrl-Alt-Del
        kernel_max += 5;
        let cad_status = read_cmd(&["systemctl", "is-enabled", "ctrl-alt-del.target"]).await;
        if cad_status.trim() == "masked" {
            kernel_cur += 5;
            findings.push(good("kernel_ctrl_alt_del", "Ctrl-Alt-Del Reboot Masked",
                "ctrl-alt-del.target is masked. Pressing Ctrl-Alt-Del at the console will not reboot the server.",
                "Maintain masked status to prevent accidental or malicious physical reboots.", cat_kernel, Some("CIS 1.5.1")));
        } else {
            findings.push(warn("kernel_ctrl_alt_del", "Ctrl-Alt-Del Reboot Active",
                "ctrl-alt-del.target is not masked. Anyone with console access can immediately reboot the server.",
                "Run 'systemctl mask ctrl-alt-del.target' to disable reboot shortcut.", cat_kernel, true, Some("CIS 1.5.1")));
        }

        crate::log_to_file("INFO", "security_run_audit: Kernel checks done");

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 3: USER & AUTHENTICATION
        // ═══════════════════════════════════════════════════════════════════════
        let cat_user = "User & Auth";

        // 3a. Password Aging Policy (PASS_MAX_DAYS)
        user_max += 5;
        let login_defs = read_file("/etc/login.defs").await;
        let mut cur_max = "unknown".to_string();
        let mut good_pass_policy = false;
        let mut pass_min_len = "unknown".to_string();
        for line in login_defs.lines() {
            let l = line.trim();
            if l.starts_with('#') { continue; }
            let p: Vec<&str> = l.split_whitespace().collect();
            if p.len() >= 2 {
                if p[0] == "PASS_MAX_DAYS" {
                    cur_max = p[1].to_string();
                    if let Ok(d) = p[1].parse::<u32>() { if d <= 90 { good_pass_policy = true; } }
                }
                if p[0] == "PASS_MIN_LEN" { pass_min_len = p[1].to_string(); }
            }
        }
        if good_pass_policy {
            user_cur += 5;
            findings.push(good("pass_policy", "Password Aging Policy",
                &format!("PASS_MAX_DAYS={} (≤90 days). Regular password rotation is enforced.", cur_max),
                "Also ensure PASS_MIN_LEN ≥ 12 for strong passwords.", cat_user, Some("CIS 5.4.1")));
        } else {
            findings.push(warn("pass_policy", "Weak Password Aging Policy",
                &format!("Passwords never expire or expire too late (PASS_MAX_DAYS={}). Compromised credentials remain valid indefinitely.", cur_max),
                "Set PASS_MAX_DAYS to 90 and PASS_MIN_LEN to at least 12 in /etc/login.defs.", cat_user, true, Some("CIS 5.4.1")));
        }
        let _ = pass_min_len; // used in countermeasure text

        // 3b. Users with Empty Passwords
        user_max += 5;
        let shadow = read_privileged_file("/etc/shadow").await;
        let empty_pw_users: Vec<String> = shadow.lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    let hash = parts[1];
                    // Empty or "!" or "*" means no/locked password; "" means empty (insecure)
                    if hash.is_empty() { Some(parts[0].to_string()) } else { None }
                } else { None }
            })
            .collect();
        if empty_pw_users.is_empty() {
            user_cur += 5;
            findings.push(good("empty_passwords", "No Accounts with Empty Passwords",
                "All user accounts have a password set or are properly locked.",
                "Regularly audit user accounts and ensure all have strong passwords.", cat_user, Some("CIS 5.4.2")));
        } else {
            findings.push(crit("empty_passwords", "Accounts with Empty Passwords Found",
                &format!("The following accounts have no password: {}. Anyone can log in as these users without credentials.", empty_pw_users.join(", ")),
                "Run 'passwd <username>' to set passwords, or click Apply Fix to lock accounts via 'passwd -l'.", cat_user, true, Some("CIS 5.4.2")));
        }

        // 3c. NOPASSWD in sudoers
        user_max += 5;
        let sudoers = read_privileged_file("/etc/sudoers").await;
        let sudoers_d = read_cmd(&["bash", "-c", "cat /etc/sudoers.d/* 2>/dev/null || true"]).await;
        let sudoers_full = format!("{}\n{}", sudoers, sudoers_d);
        let nopasswd_lines: Vec<String> = sudoers_full.lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with('#') && t.contains("NOPASSWD") && t.contains("ALL")
            })
            .map(|l| l.trim().to_string())
            .collect();
        if nopasswd_lines.is_empty() {
            user_cur += 5;
            findings.push(good("sudo_nopasswd", "No Dangerous NOPASSWD Sudoers Rules",
                "No sudoers entries grant passwordless ALL privileges. Privilege escalation requires authentication.",
                "Review sudoers periodically. Use specific commands in NOPASSWD rules, not ALL.", cat_user, Some("CIS 5.3.7")));
        } else {
            findings.push(crit("sudo_nopasswd", "Dangerous NOPASSWD:ALL Sudoers Rules Found",
                &format!("The following entries grant passwordless root access: {}. A compromised user account instantly becomes root.", nopasswd_lines.join("; ")),
                "Edit /etc/sudoers and remove or restrict NOPASSWD:ALL entries.", cat_user, false, Some("CIS 5.3.7")));
        }

        // 3d. Multiple UID 0 accounts
        user_max += 5;
        let passwd_file = read_file("/etc/passwd").await;
        let uid0_users: Vec<String> = passwd_file.lines()
            .filter_map(|line| {
                let p: Vec<&str> = line.splitn(7, ':').collect();
                if p.len() >= 3 && p[2] == "0" { Some(p[0].to_string()) } else { None }
            })
            .filter(|u| u != "root")
            .collect();
        if uid0_users.is_empty() {
            user_cur += 5;
            findings.push(good("uid0", "No Duplicate UID 0 Accounts",
                "Only root has UID 0. No other account has unconditional superuser identity.",
                "Maintain this. Never create additional UID 0 accounts.", cat_user, Some("CIS 5.4.3")));
        } else {
            findings.push(crit("uid0", "Duplicate UID 0 Accounts Detected",
                &format!("Non-root accounts with UID 0: {}. These have full root privileges by default.", uid0_users.join(", ")),
                "Remove or change the UID of these accounts immediately. Only 'root' should have UID 0.", cat_user, false, Some("CIS 5.4.3")));
        }

        // 3e. Duplicate UIDs / GIDs
        user_max += 5;
        let mut seen_uids = std::collections::HashSet::new();
        let mut dup_uids = Vec::new();
        for line in passwd_file.lines() {
            let p: Vec<&str> = line.splitn(7, ':').collect();
            if p.len() >= 3 {
                if let Ok(uid) = p[2].parse::<u32>() {
                    if !seen_uids.insert(uid) { dup_uids.push(format!("UID {}", uid)); }
                }
            }
        }
        let group_file = read_file("/etc/group").await;
        let mut seen_gids = std::collections::HashSet::new();
        for line in group_file.lines() {
            let p: Vec<&str> = line.splitn(4, ':').collect();
            if p.len() >= 3 {
                if let Ok(gid) = p[2].parse::<u32>() {
                    if !seen_gids.insert(gid) { dup_uids.push(format!("GID {}", gid)); }
                }
            }
        }
        if dup_uids.is_empty() {
            user_cur += 5;
            findings.push(good("duplicate_uids_gids", "Unique User & Group IDs",
                "All user accounts and groups have unique numeric identifiers.",
                "Maintain unique UIDs and GIDs for distinct user identities.", cat_user, Some("CIS 5.4.4")));
        } else {
            findings.push(warn("duplicate_uids_gids", "Duplicate UIDs or GIDs Detected",
                &format!("Duplicate user/group IDs found: {}. Duplicate IDs allow accounts to access each other's files.", dup_uids.join(", ")),
                "Run 'pwck' and 'grpck' to audit and fix duplicate account numbers.", cat_user, false, Some("CIS 5.4.4")));
        }

        // 3f. PAM Faillock Lockout Policy
        user_max += 5;
        let faillock_conf = read_privileged_file("/etc/security/faillock.conf").await;
        let faillock_d = read_cmd(&["bash", "-c", "cat /etc/security/faillock.conf.d/*.conf 2>/dev/null || true"]).await;
        let faillock_full = format!("{}\n{}", faillock_conf, faillock_d);
        let has_faillock = faillock_full.lines().any(|l| l.trim().starts_with("deny") && !l.trim().starts_with('#'));
        if has_faillock {
            user_cur += 5;
            findings.push(good("pam_faillock", "PAM Account Lockout Active",
                "PAM faillock is configured to lock accounts after repeated failed login attempts.",
                "Maintain lockout policies to prevent SSH brute-force attacks.", cat_user, Some("CIS 5.3.2")));
        } else {
            findings.push(warn("pam_faillock", "PAM Account Lockout Unconfigured",
                "PAM faillock lockout policy is missing or unconfigured. Brute-force login attempts are not throttled at the PAM layer.",
                "Configure PAM faillock in /etc/security/faillock.conf (deny = 5, unlock_time = 900).", cat_user, true, Some("CIS 5.3.2")));
        }

        // 3g. Default System umask Policy
        user_max += 5;
        let has_stricter_umask = login_defs.lines().any(|l| {
            let t = l.trim();
            if t.starts_with('#') { return false; }
            if t.starts_with("UMASK") {
                let parts: Vec<&str> = t.split_whitespace().collect();
                if parts.len() >= 2 { return parts[1] == "027" || parts[1] == "077"; }
            }
            false
        });
        if has_stricter_umask {
            user_cur += 5;
            findings.push(good("umask_policy", "Default System umask Secure",
                "Default UMASK in /etc/login.defs is set to 027 (or 077), protecting newly created files from unauthorized group/other access.",
                "Maintain restrictive umask defaults.", cat_user, Some("CIS 5.4.5")));
        } else {
            findings.push(warn("umask_policy", "Overly Permissive System umask",
                "Default UMASK is set to 022 or unconfigured. Newly created files are world-readable by default.",
                "Set 'UMASK 027' in /etc/login.defs.", cat_user, true, Some("CIS 5.4.5")));
        }

        // 3e. Locked/disabled system accounts with login shell
        user_max += 5;
        let risky_accounts: Vec<String> = passwd_file.lines()
            .filter_map(|line| {
                let p: Vec<&str> = line.splitn(7, ':').collect();
                if p.len() == 7 {
                    let user = p[0];
                    let shell = p[6].trim();
                    let uid: u32 = p[2].parse().unwrap_or(999);
                    // System accounts (uid < 1000, not root) with a real shell are suspicious
                    if uid > 0 && uid < 1000 && !matches!(shell, "/sbin/nologin" | "/bin/false" | "/usr/sbin/nologin" | "") {
                        Some(format!("{}(uid={})", user, uid))
                    } else { None }
                } else { None }
            })
            .collect();
        if risky_accounts.is_empty() {
            user_cur += 5;
            findings.push(good("sys_shell", "System Accounts Have No Login Shell",
                "All system/service accounts are properly locked with nologin or /bin/false.",
                "Periodically audit /etc/passwd for unexpected shell assignments.", cat_user, Some("CIS 5.4.2")));
        } else {
            findings.push(warn("sys_shell", "System Accounts with Login Shells Found",
                &format!("Service accounts with interactive shells: {}. These could be used as backdoor entry points.", risky_accounts.join(", ")),
                "Set shell to /sbin/nologin for service accounts: usermod -s /sbin/nologin <user>", cat_user, false, Some("CIS 5.4.2")));
        }

        crate::log_to_file("INFO", "security_run_audit: User checks done");

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 4: FILESYSTEM & PERMISSIONS
        // ═══════════════════════════════════════════════════════════════════════
        let cat_fs = "Filesystem";

        // 4a. /etc/passwd permissions
        fs_max += 5;
        let passwd_perms = read_cmd(&["stat", "-c", "%a", "/etc/passwd"]).await;
        let passwd_perms = passwd_perms.trim();
        // Should be 644
        if passwd_perms == "644" {
            fs_cur += 5;
            findings.push(good("fs_passwd_perms", "/etc/passwd Permissions Correct",
                "Permissions on /etc/passwd are 644 (world-readable, root-writable). This is the secure default.",
                "Do not make /etc/passwd writable by non-root users.", cat_fs, Some("CIS 6.1.2")));
        } else {
            let msg = if passwd_perms.contains('7') || passwd_perms.ends_with('2') || passwd_perms.ends_with('6') {
                "Critical: /etc/passwd is writable by non-root! Attackers can add a root account."
            } else {
                "Warning: /etc/passwd has unexpected permissions."
            };
            findings.push(crit("fs_passwd_perms", "/etc/passwd Has Incorrect Permissions",
                &format!("{} Current permissions: {}", msg, passwd_perms),
                "Run: chmod 644 /etc/passwd", cat_fs, true, Some("CIS 6.1.2")));
        }

        // 4b. /etc/shadow permissions
        fs_max += 5;
        let shadow_perms = read_cmd(&["stat", "-c", "%a", "/etc/shadow"]).await;
        let shadow_perms = shadow_perms.trim();
        // Should be 000 or 640
        let shadow_ok = matches!(shadow_perms, "0" | "000" | "640" | "400");
        if shadow_ok {
            fs_cur += 5;
            findings.push(good("fs_shadow_perms", "/etc/shadow Permissions Correct",
                &format!("Permissions on /etc/shadow are {} (password hashes are protected).", shadow_perms),
                "Ensure shadow remains inaccessible to non-privileged users.", cat_fs, Some("CIS 6.1.3")));
        } else {
            findings.push(crit("fs_shadow_perms", "/etc/shadow Has Insecure Permissions",
                &format!("Current permissions: {}. Password hashes may be readable by non-root users — enabling offline password cracking.", shadow_perms),
                "Run: chmod 000 /etc/shadow && chown root:root /etc/shadow", cat_fs, true, Some("CIS 6.1.3")));
        }

        // 4c. /tmp sticky bit
        fs_max += 5;
        let tmp_perms = read_cmd(&["stat", "-c", "%a", "/tmp"]).await;
        let tmp_perms = tmp_perms.trim();
        // Sticky bit = leading 1 (e.g. 1777)
        let tmp_sticky = tmp_perms.starts_with('1');
        if tmp_sticky {
            fs_cur += 5;
            findings.push(good("fs_tmp_sticky", "/tmp Sticky Bit Set",
                "The sticky bit is set on /tmp. Users can only delete their own files — prevents file hijacking attacks.",
                "Maintain this configuration. Never remove the sticky bit from /tmp.", cat_fs, Some("CIS 1.1.3")));
        } else {
            findings.push(warn("fs_tmp_sticky", "/tmp Missing Sticky Bit",
                &format!("/tmp permissions are {} — the sticky bit is not set. Users can delete other users' temporary files.", tmp_perms),
                "Run: chmod 1777 /tmp", cat_fs, true, Some("CIS 1.1.3")));
        }

        // 4d. World-writable files in /etc
        fs_max += 5;
        let ww_etc = read_cmd(&["find", "/etc", "-maxdepth", "2", "-type", "f",
            "-perm", "-o+w", "-not", "-path", "/etc/passwd", "2>/dev/null"]).await;
        let ww_files: Vec<&str> = ww_etc.lines().filter(|l| !l.trim().is_empty()).collect();
        if ww_files.is_empty() {
            fs_cur += 5;
            findings.push(good("fs_ww_etc", "No World-Writable Files in /etc",
                "No world-writable configuration files found in /etc. Configuration integrity is maintained.",
                "Periodically scan for world-writable files: find /etc -perm -o+w", cat_fs, None));
        } else {
            findings.push(crit("fs_ww_etc", "World-Writable Configuration Files Found",
                &format!("Files in /etc writable by any user: {}. Attackers can modify system configuration files.", ww_files.join(", ")),
                "Remove world-write permissions: chmod o-w <file> for each listed file.", cat_fs, false, None));
        }

        // 4e. Core dumps via suid_dumpable
        fs_max += 5;
        let coredump = read_sysctl("fs.suid_dumpable").await;
        if coredump == "0" {
            fs_cur += 5;
            findings.push(good("fs_coredump", "SUID Core Dumps Disabled",
                "fs.suid_dumpable=0 — SUID programs do not produce core dumps. Memory contents of privileged processes are protected.",
                "Maintain this setting to prevent credential extraction from core dumps.", cat_fs, Some("CIS 1.6.1")));
        } else {
            findings.push(warn("fs_coredump", "SUID Core Dumps Enabled",
                &format!("fs.suid_dumpable={} — privileged processes can write core dumps that may contain passwords or session tokens.", coredump),
                "Set fs.suid_dumpable=0 in /etc/sysctl.d/99-hardening.conf", cat_fs, true, Some("CIS 1.6.1")));
        }
        // 4e. Partition Mount Options (/tmp, /var/tmp, /dev/shm)
        fs_max += 5;
        let mounts = read_file("/proc/mounts").await;
        let mut missing_opts = Vec::new();
        for target in ["/tmp", "/var/tmp", "/dev/shm"] {
            if let Some(line) = mounts.lines().find(|l| l.contains(&format!(" {} ", target))) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let opts = parts[3];
                    let mut unsecure = Vec::new();
                    if !opts.contains("nodev") { unsecure.push("nodev"); }
                    if !opts.contains("nosuid") { unsecure.push("nosuid"); }
                    if !opts.contains("noexec") && target != "/tmp" { unsecure.push("noexec"); }
                    if !unsecure.is_empty() { missing_opts.push(format!("{} missing {}", target, unsecure.join(","))); }
                }
            }
        }
        if missing_opts.is_empty() {
            fs_cur += 5;
            findings.push(good("fs_mount_options", "Temporary Mount Options Hardened",
                "/tmp, /var/tmp, and /dev/shm have hardening mount options (nodev, nosuid) applied.",
                "Maintain strict mount flags on temporary partitions.", cat_fs, Some("CIS 1.1.2 - 1.1.8")));
        } else {
            findings.push(warn("fs_mount_options", "Temporary Mount Options Missing Hardening",
                &format!("Temporary partition mount options missing hardening flags: {}.", missing_opts.join("; ")),
                "Edit /etc/fstab and add 'nodev,nosuid,noexec' to /tmp, /var/tmp, and /dev/shm mount entries.", cat_fs, false, Some("CIS 1.1.2 - 1.1.8")));
        }

        // 4f. World-Writable Files in Temp Partitions
        fs_max += 5;
        let ww_output = read_cmd(&["bash", "-c", "find /tmp /var/tmp /dev/shm -maxdepth 2 -type f -perm -0002 2>/dev/null | head -5"]).await;
        let ww_files: Vec<String> = ww_output.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        if ww_files.is_empty() {
            fs_cur += 5;
            findings.push(good("fs_world_writable", "No World-Writable Files Found in Key Paths",
                "No unexpected world-writable regular files found in /tmp, /var/tmp, or /dev/shm.",
                "Maintain strict file permissions across all shared directories.", cat_fs, Some("CIS 1.1.21")));
        } else {
            findings.push(warn("fs_world_writable", "World-Writable Files Detected",
                &format!("World-writable regular files found: {}. Any user can overwrite or tamper with these files.", ww_files.join(", ")),
                "Run 'chmod o-w <file>' to remove world-write permissions on sensitive files.", cat_fs, false, Some("CIS 1.1.21")));
        }

        // 4g. Unowned Files / Groups in Temp Partitions
        fs_max += 5;
        let unowned_out = read_cmd(&["bash", "-c", "find /tmp /var/tmp -maxdepth 2 \\( -nouser -o -nogroup \\) 2>/dev/null | head -5"]).await;
        let unowned_files: Vec<String> = unowned_out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        if unowned_files.is_empty() {
            fs_cur += 5;
            findings.push(good("fs_unowned_files", "No Unowned Files Detected",
                "All scanned files belong to valid system users and groups.",
                "Periodically audit for orphaned files left behind by deleted users.", cat_fs, Some("CIS 6.1.11")));
        } else {
            findings.push(warn("fs_unowned_files", "Unowned or Orphaned Files Found",
                &format!("Files without a valid owner/group found: {}. Orphaned files may pose security risks if user IDs are re-assigned.", unowned_files.join(", ")),
                "Run 'chown root:root <file>' or remove orphaned files.", cat_fs, false, Some("CIS 6.1.11")));
        }

        crate::log_to_file("INFO", "security_run_audit: Filesystem checks done");

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 5: NETWORK & SERVICES
        // ═══════════════════════════════════════════════════════════════════════
        let cat_net = "Network & Services";

        // 5a. Firewall active
        network_max += 5;
        let mut fw_active = false;
        if crate::binary_exists("firewall-cmd").await {
            let fw_out = tokio::time::timeout(tokio::time::Duration::from_secs(5),
                Command::new("firewall-cmd").args(["--state"]).output()).await;
            if let Ok(Ok(o)) = fw_out { if o.status.success() { fw_active = true; } }
        }
        if !fw_active && crate::binary_exists("ufw").await {
            let ufw_out = tokio::time::timeout(tokio::time::Duration::from_secs(5),
                Command::new("ufw").args(["status"]).output()).await;
            if let Ok(Ok(o)) = ufw_out {
                if String::from_utf8_lossy(&o.stdout).contains("Status: active") { fw_active = true; }
            }
        }
        if fw_active {
            network_cur += 5;
            findings.push(good("firewall", "System Firewall Active",
                "A system firewall (firewalld or ufw) is currently running and filtering traffic.",
                "Ensure default incoming policy is set to drop/deny and only open necessary ports.", cat_net, Some("CIS 3.5")));
        } else {
            findings.push(crit("firewall", "Firewall Inactive",
                "No active firewall detected. All open network ports are unconditionally exposed to the network.",
                "Enable firewalld ('systemctl enable --now firewalld') or ufw ('ufw enable').", cat_net, true, Some("CIS 3.5")));
        }

        // 5b. Services listening on all interfaces (0.0.0.0) - read-only check
        network_max += 5;
        let ss_out = read_cmd(&["ss", "-tlnp"]).await;
        let exposed_services: Vec<String> = ss_out.lines()
            .skip(1) // skip header
            .filter(|l| l.contains("0.0.0.0:") || l.contains("*:"))
            .filter_map(|l| {
                // Extract port/address column
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 4 {
                    let local = parts[3];
                    // Extract port number
                    let port_str = local.split(':').last().unwrap_or("0");
                    let port: u16 = port_str.parse().unwrap_or(0);
                    // Whitelist common expected ports; flag unexpected ones
                    match port {
                        22 | 80 | 443 | 25 | 587 | 993 | 995 | 53 | 3306 | 5432 | 6379 | 27017 | 8080 | 8443 => None,
                        0 => None,
                        _ if port > 1024 => Some(format!("Port {} ({})", port, l.split_whitespace().last().unwrap_or("unknown"))),
                        _ => None,
                    }
                } else { None }
            })
            .take(5) // limit output length
            .collect();
        if exposed_services.is_empty() {
            network_cur += 5;
            findings.push(good("net_exposed_ports", "No Unexpected Services on All Interfaces",
                "No unexpected services found listening on 0.0.0.0 (all interfaces).",
                "Regularly run 'ss -tlnp' to audit listening services.", cat_net, None));
        } else {
            findings.push(warn("net_exposed_ports", "Unexpected Services Listening on All Interfaces",
                &format!("Services bound to all network interfaces: {}. Verify these are intentional.", exposed_services.join(", ")),
                "Bind services to specific interfaces (127.0.0.1 for local-only). Use firewall rules to restrict access.", cat_net, false, None));
            network_cur += 2; // partial credit
        }

        // 5c. Source routing
        network_max += 5;
        let src_route = read_sysctl("net.ipv4.conf.all.accept_source_route").await;
        let src_route6 = read_sysctl("net.ipv6.conf.all.accept_source_route").await;
        if src_route == "0" && (src_route6 == "0" || src_route6.is_empty()) {
            network_cur += 5;
            findings.push(good("net_src_route", "Source Routing Disabled",
                "IP source routing is disabled. Attackers cannot specify the route packets take through the network.",
                "Maintain these settings to prevent traffic interception.", cat_net, Some("CIS 3.2.1")));
        } else {
            findings.push(crit("net_src_route", "Source Routing Enabled",
                "IP source routing is accepted. Attackers can craft packets that traverse through specific (attacker-controlled) routers.",
                "Set net.ipv4.conf.all.accept_source_route=0 and net.ipv6.conf.all.accept_source_route=0", cat_net, true, Some("CIS 3.2.1")));
        }

        // 5d. Bogus ICMP responses
        network_max += 5;
        let bogus_icmp = read_sysctl("net.ipv4.icmp_ignore_bogus_error_responses").await;
        if bogus_icmp == "1" {
            network_cur += 5;
            findings.push(good("net_bogus_icmp", "Bogus ICMP Error Responses Ignored",
                "The kernel ignores malformed ICMP error packets used in network fingerprinting.",
                "Maintain this setting.", cat_net, Some("CIS 3.2.6")));
        } else {
            findings.push(warn("net_bogus_icmp", "Bogus ICMP Errors Not Ignored",
                "net.ipv4.icmp_ignore_bogus_error_responses=0. Malformed ICMP packets can be used for OS fingerprinting and DoS.",
                "Set net.ipv4.icmp_ignore_bogus_error_responses=1 in /etc/sysctl.d/99-hardening.conf", cat_net, true, Some("CIS 3.2.6")));
        }

        // 5e. Martian packet logging
        network_max += 5;
        let martians = read_sysctl("net.ipv4.conf.all.log_martians").await;
        if martians == "1" {
            network_cur += 5;
            findings.push(good("net_martians", "Martian Packet Logging Enabled",
                "Packets with impossible source addresses are logged. Spoofed traffic is detectable.",
                "Monitor /var/log/messages or journalctl for martian packet warnings.", cat_net, Some("CIS 3.2.4")));
        } else {
            findings.push(info_finding("net_martians", "Martian Packet Logging Disabled",
                "net.ipv4.conf.all.log_martians=0. Spoofed/impossible-source-address packets are silently dropped without audit trail.",
                "Set net.ipv4.conf.all.log_martians=1 in /etc/sysctl.d/99-hardening.conf for better network visibility.", cat_net, true));
            network_cur += 3; // info, not critical
        }

        crate::log_to_file("INFO", "security_run_audit: Network checks done");

        // ═══════════════════════════════════════════════════════════════════════
        // CATEGORY 6: SYSTEM HYGIENE
        // ═══════════════════════════════════════════════════════════════════════
        let cat_sys = "System Hygiene";

        // 6a. SELinux Status
        system_max += 5;
        let mut selinux_enforcing = false;
        if crate::binary_exists("sestatus").await {
            let se_out = tokio::time::timeout(tokio::time::Duration::from_secs(5),
                Command::new("sestatus").output()).await;
            if let Ok(Ok(o)) = se_out {
                let text = String::from_utf8_lossy(&o.stdout);
                if text.contains("SELinux status:                 enabled") && text.contains("Current mode:                   enforcing") {
                    selinux_enforcing = true;
                }
            }
        }
        if selinux_enforcing {
            system_cur += 5;
            findings.push(good("selinux", "SELinux Enforcing",
                "SELinux is enabled and in enforcing mode — mandatory access controls are active.",
                "Maintain enforcing mode. Avoid setting to permissive unless actively debugging.", cat_sys, Some("CIS 1.7")));
        } else {
            findings.push(crit("selinux", "SELinux Disabled or Permissive",
                "SELinux is not enforcing policies. This removes a critical layer of defense against privilege escalation and zero-day vulnerabilities.",
                "Enable SELinux and set to enforcing. A reboot is required after enabling.", cat_sys, true, Some("CIS 1.7")));
        }

        // 6b. Audit Daemon (auditd)
        system_max += 5;
        let auditd_status = read_cmd(&["systemctl", "is-active", "auditd"]).await;
        if auditd_status.trim() == "active" {
            system_cur += 5;
            findings.push(good("auditd", "Audit Daemon (auditd) Running",
                "auditd is active and logging security-relevant system events.",
                "Review audit rules in /etc/audit/rules.d/ to ensure they cover critical paths.", cat_sys, Some("CIS 4.1")));
        } else {
            findings.push(warn("auditd", "Audit Daemon (auditd) Not Running",
                "auditd is not active. Security-relevant events (file access, privilege escalation, authentication) are not being logged.",
                "Enable with: systemctl enable --now auditd", cat_sys, true, Some("CIS 4.1")));
        }

        // 6c. Time Synchronization
        system_max += 5;
        let chronyd = read_cmd(&["systemctl", "is-active", "chronyd"]).await;
        let systemd_timesyncd = read_cmd(&["systemctl", "is-active", "systemd-timesyncd"]).await;
        let ntpd = read_cmd(&["systemctl", "is-active", "ntpd"]).await;
        let time_ok = chronyd.trim() == "active" || systemd_timesyncd.trim() == "active" || ntpd.trim() == "active";
        if time_ok {
            system_cur += 5;
            findings.push(good("time_sync", "Time Synchronization Active",
                "A time synchronization daemon is active (chronyd/systemd-timesyncd/ntpd). System timestamps are accurate.",
                "Accurate time is required for log correlation and Kerberos/TLS certificate validation.", cat_sys, Some("CIS 2.2.1")));
        } else {
            findings.push(warn("time_sync", "No Time Synchronization Service Active",
                "No time synchronization service is running. System clock may drift — log timestamps become unreliable and TLS certificates may fail validation.",
                "Enable with: systemctl enable --now chronyd", cat_sys, true, Some("CIS 2.2.1")));
        }

        // 6d. USBGuard
        system_max += 5;
        let usbguard_status = read_cmd(&["systemctl", "is-active", "usbguard"]).await;
        let has_usbguard = crate::binary_exists("usbguard").await;
        if usbguard_status.trim() == "active" {
            system_cur += 5;
            findings.push(good("usbguard", "USBGuard Active",
                "USBGuard is running and controlling USB device authorization. Physical USB attacks are mitigated.",
                "Review USBGuard policy to ensure only authorized devices are whitelisted.", cat_sys, None));
        } else if has_usbguard {
            findings.push(warn("usbguard", "USBGuard Installed but Not Running",
                "USBGuard is installed but inactive. Malicious USB devices (BadUSB, rubber ducky) can plug in unchallenged.",
                "Enable with: systemctl enable --now usbguard", cat_sys, true, None));
        } else {
            findings.push(info_finding("usbguard", "USBGuard Not Installed",
                "USBGuard is not installed. For systems with physical access risk, any USB device can be plugged in and potentially execute malicious code.",
                "Install usbguard: dnf install usbguard && systemctl enable --now usbguard (review policy after install)", cat_sys, false));
            system_cur += 2; // Info — not critical
        }

        // 6e. Pending security updates
        system_max += 5;
        if crate::binary_exists("dnf").await {
            let sec_updates = tokio::time::timeout(
                tokio::time::Duration::from_secs(3),
                Command::new("dnf").args(["check-update", "--security", "-q", "-C"]).output()
            ).await;
            match sec_updates {
                Ok(Ok(o)) => {
                    // Exit code 100 = updates available, 0 = up to date
                    let pending: Vec<String> = String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|l| !l.trim().is_empty() && !l.starts_with("Last metadata"))
                        .take(5)
                        .map(|l| l.to_string())
                        .collect();
                    if o.status.code() == Some(100) && !pending.is_empty() {
                        findings.push(crit("sec_updates", "Pending Security Updates",
                            &format!("There are security patches available that have not been applied: {}{}",
                                pending.join(", "),
                                if pending.len() == 5 { " (and more...)" } else { "" }),
                            "Apply updates immediately: sudo dnf update --security", cat_sys, false, Some("CVE/RHSA advisories")));
                    } else {
                        system_cur += 5;
                        findings.push(good("sec_updates", "No Pending Security Updates",
                            "The system is fully patched. No outstanding security advisories found via dnf.",
                            "Enable automatic security updates: dnf install dnf-automatic and configure it.", cat_sys, None));
                    }
                }
                _ => {
                    system_cur += 3;
                    findings.push(info_finding("sec_updates", "Security Update Check Unavailable",
                        "Could not check for security updates (cache check timed out). Manually verify patch status.",
                        "Run: dnf check-update --security", cat_sys, false));
                }
            }
        } else if crate::binary_exists("apt").await {
            let apt_out = tokio::time::timeout(tokio::time::Duration::from_secs(3),
                Command::new("apt").args(["list", "--upgradable", "2>/dev/null"]).output()).await;
            match apt_out {
                Ok(Ok(o)) => {
                    let lines: Vec<String> = String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|l| l.contains("security"))
                        .take(5).map(|l| l.to_string()).collect();
                    if lines.is_empty() {
                        system_cur += 5;
                        findings.push(good("sec_updates", "No Pending Security Updates",
                            "No security updates pending (apt).", "Keep system updated regularly.", cat_sys, None));
                    } else {
                        findings.push(crit("sec_updates", "Pending Security Updates (apt)",
                            &format!("Security packages awaiting update: {}", lines.join(", ")),
                            "Run: sudo apt upgrade", cat_sys, false, None));
                    }
                }
                _ => {
                    system_cur += 3;
                    findings.push(info_finding("sec_updates", "Security Update Check Unavailable",
                        "Could not check for security updates.", "Run: apt list --upgradable", cat_sys, false));
                }
            }
        } else {
            system_cur += 3;
            findings.push(info_finding("sec_updates", "Package Manager Not Detected",
                "Could not determine package manager to check for security updates.",
                "Manually verify your system is up to date.", cat_sys, false));
        }

        // 6f. GRUB Bootloader Password
        system_max += 5;
        let grub_cfg = read_privileged_file("/boot/grub2/grub.cfg").await;
        let has_grub_pw = grub_cfg.contains("password_pbkdf2") || grub_cfg.contains("password ");
        if has_grub_pw {
            system_cur += 5;
            findings.push(good("sys_grub_password", "GRUB Bootloader Password Set",
                "GRUB password protection is configured. Unauthorized boot option modifications are prevented.",
                "Maintain GRUB password protection.", cat_sys, Some("CIS 1.4.1")));
        } else {
            findings.push(warn("sys_grub_password", "GRUB Bootloader Unprotected",
                "No GRUB bootloader password detected. Anyone with physical access can edit kernel boot arguments to bypass authentication (e.g. init=/bin/sh).",
                "Run 'grub2-setpassword' to set a bootloader password.", cat_sys, false, Some("CIS 1.4.1")));
        }

        // 6g. Legal Access Warning Banner
        system_max += 5;
        let issue_text = read_file("/etc/issue").await;
        let issue_net = read_file("/etc/issue.net").await;
        let ssh_banner = ssh_val(&ssh_full, "Banner").unwrap_or_default();
        let has_banner = (!issue_text.trim().is_empty() && !issue_text.contains("Kernel \\r on an \\m"))
            || (!issue_net.trim().is_empty() && !issue_net.contains("Kernel \\r on an \\m"))
            || (!ssh_banner.is_empty() && ssh_banner != "none");
        if has_banner {
            system_cur += 5;
            findings.push(good("sys_legal_banner", "Legal Access Banner Configured",
                "A legal access warning banner is present in /etc/issue or /etc/ssh/sshd_config.",
                "Maintain warning banners to establish legal notice for unauthorized access.", cat_sys, Some("CIS 1.7.1")));
        } else {
            findings.push(warn("sys_legal_banner", "Legal Access Banner Missing",
                "No custom legal warning banner configured in /etc/issue or /etc/ssh/sshd_config.",
                "Set an authorized access warning banner in /etc/issue and /etc/ssh/sshd_config.", cat_sys, true, Some("CIS 1.7.1")));
        }

        // 6h. Loaded Audit Rules
        system_max += 5;
        let auditctl_out = read_cmd(&["auditctl", "-l"]).await;
        let has_audit_rules = !auditctl_out.contains("No rules") && !auditctl_out.trim().is_empty();
        if has_audit_rules {
            system_cur += 5;
            findings.push(good("sys_audit_rules", "Auditd Rules Loaded",
                "Security audit rules are active and loaded into the audit kernel subsystem.",
                "Maintain comprehensive rules in /etc/audit/rules.d/audit.rules.", cat_sys, Some("CIS 4.1.3")));
        } else {
            findings.push(warn("sys_audit_rules", "No Security Audit Rules Loaded",
                "auditctl reports no active audit rules loaded. auditd is running but not tracking system file/privilege calls.",
                "Load standard CIS audit rules into /etc/audit/rules.d/ and run 'augenrules --load'.", cat_sys, false, Some("CIS 4.1.3")));
        }

        // 6i. Listening Network Ports Audit
        system_max += 5;
        let listening_out = read_cmd(&["ss", "-tuln"]).await;
        let open_ports: Vec<String> = listening_out.lines()
            .skip(1)
            .filter(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 5 {
                    let addr = parts[4];
                    !addr.starts_with("127.") && !addr.starts_with("[::1]:") && !addr.starts_with("::1:")
                } else { false }
            })
            .filter_map(|l| {
                let p: Vec<&str> = l.split_whitespace().collect();
                if p.len() >= 5 { Some(p[4].to_string()) } else { None }
            })
            .take(6)
            .collect();
        system_cur += 5;
        if open_ports.is_empty() {
            findings.push(good("sys_listening_ports", "Listening Services Audit (Loopback Only)",
                "All listening network services are bound to localhost (127.0.0.0/8 or [::1]). No external listening ports detected.",
                "Regularly audit listening ports using 'ss -tuln'.", cat_sys, Some("CIS 3.5")));
        } else {
            findings.push(info_finding("sys_listening_ports", "Active External Listening Network Sockets",
                &format!("Listening sockets on non-loopback interfaces: {}. Review these services to ensure they are intended.", open_ports.join(", ")),
                "Review listening network services. Disable or firewall any ports not required for external access.", cat_sys, false));
        }

        crate::log_to_file("INFO", "security_run_audit: System hygiene checks done");

        // ── Sysctl Tamper Check ────────────────────────────────────────────────
        let sysctl_tampered = check_sysctl_tampered().await;
        if sysctl_tampered {
            for f in &mut findings {
                if f.is_resolved && (f.id == "aslr" || f.id == "syncookies" || f.id == "ipfwd" || f.id == "kptr" || f.id == "dmesg_r" || f.id == "coredump" || f.id == "bogus_icmp" || f.id == "martians" || f.id == "src_route") {
                    f.tamper_flag = Some("modified after fix — rescan required.".to_string());
                }
            }
        }

        // ── Load and inject Runtime Threats (with strict 1s timeout) ───────────
        let mut threat_issues = 0;
        if let Ok(Ok(threats)) = tokio::time::timeout(
            tokio::time::Duration::from_millis(1000),
            crate::commands::audit_log::get_runtime_threats(Some(7), None, None)
        ).await {
            for t in threats {
                threat_issues += 1;
                findings.push(SecurityFinding {
                    id: t.id,
                    title: t.title,
                    description: t.description,
                    severity: t.severity,
                    countermeasure: "Investigate log events in Command Audit or Auth Events.".to_string(),
                    category: "Runtime Threats".to_string(),
                    has_auto_fix: false,
                    is_resolved: false,
                    reference: None,
                    tamper_flag: None,
                });
            }
        }

        // ── Build category scores ─────────────────────────────────────────────
        let cat_score = |cur: u32, max: u32| if max > 0 { cur * 100 / max } else { 100 };

        let category_scores = vec![
            CategoryScore {
                category: cat_ssh.to_string(),
                score: cat_score(ssh_cur, ssh_max),
                max_score: ssh_max,
                issues: findings.iter().filter(|f| f.category == cat_ssh && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: cat_kernel.to_string(),
                score: cat_score(kernel_cur, kernel_max),
                max_score: kernel_max,
                issues: findings.iter().filter(|f| f.category == cat_kernel && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: cat_user.to_string(),
                score: cat_score(user_cur, user_max),
                max_score: user_max,
                issues: findings.iter().filter(|f| f.category == cat_user && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: cat_fs.to_string(),
                score: cat_score(fs_cur, fs_max),
                max_score: fs_max,
                issues: findings.iter().filter(|f| f.category == cat_fs && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: cat_net.to_string(),
                score: cat_score(network_cur, network_max),
                max_score: network_max,
                issues: findings.iter().filter(|f| f.category == cat_net && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: cat_sys.to_string(),
                score: cat_score(system_cur, system_max),
                max_score: system_max,
                issues: findings.iter().filter(|f| f.category == cat_sys && !f.is_resolved).count() as u32,
            },
            CategoryScore {
                category: "Runtime Threats".to_string(),
                score: if threat_issues == 0 { 100 } else { 0 },
                max_score: 0,
                issues: threat_issues,
            },
        ];

        // ── Overall score ─────────────────────────────────────────────────────
        let total_max = ssh_max + kernel_max + user_max + fs_max + network_max + system_max;
        let total_cur = ssh_cur + kernel_cur + user_cur + fs_cur + network_cur + system_cur;

        // Penalty: any Critical finding caps score at max 60%
        let has_critical = findings.iter().any(|f| f.severity == "Critical" && !f.is_resolved);
        let raw_score = if total_max > 0 { total_cur * 100 / total_max } else { 100 };
        let score = if has_critical { raw_score.min(60) } else { raw_score };

        let report = SecurityReport { score, findings, category_scores };
        if let Ok(mut guard) = LAST_REPORT.lock() {
            *guard = Some((report.clone(), SystemTime::now()));
        }
        crate::log_to_file("INFO", &format!("security_run_audit: completed score={}", score));
        Ok(report)

    }).catch_unwind().await;

    match result {
        Ok(res) => res,
        Err(err) => {
            let msg = if let Some(s) = err.downcast_ref::<&str>() { s.to_string() }
                else if let Some(s) = err.downcast_ref::<String>() { s.clone() }
                else { "Unknown panic".to_string() };
            Err(format!("Rust panic: {}", msg))
        }
    }
}

// ─── Fix Commands ─────────────────────────────────────────────────────────────

/// Fix/revert SSH Root Login.
#[tauri::command]
pub async fn security_fix_root_ssh(enable: bool) -> Result<String, String> {
    security_fix_ssh_param(
        "PermitRootLogin".to_string(),
        "prohibit-password".to_string(),
        "yes".to_string(),
        enable,
    ).await
}

/// Harden multiple SSH parameters at once. Set `enable=true` to apply hardening, `false` to revert
/// to safer-but-permissive defaults (not insecure defaults).
#[tauri::command]
pub async fn security_fix_ssh_param(param: String, value: String, revert_value: String, enable: bool) -> Result<String, String> {
    let target_value = if enable { &value } else { &revert_value };

    // Sanitise: only allow safe characters in param and value
    if !param.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err("Invalid SSH parameter name.".to_string());
    }
    if !target_value.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ') {
        return Err("Invalid SSH parameter value.".to_string());
    }

    let config_file = "/etc/ssh/sshd_config";
    // If the key exists (commented or not), replace it. Otherwise append.
    let script = format!(
        r#"
if grep -qiE '^#?{param}\s' {cfg}; then
    sed -i "s|^#\\?{param}.*|{param} {val}|gI" {cfg}
else
    echo "{param} {val}" >> {cfg}
fi
(systemctl is-active --quiet sshd && systemctl reload sshd) || (systemctl is-active --quiet ssh && systemctl reload ssh) || true
"#,
        param = param,
        val = target_value,
        cfg = config_file
    );

    let out = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(format!("SSH {} set to '{}'.", param, target_value))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Apply/revert a kernel sysctl parameter persistently.
/// Strategy: write the config file FIRST (verified), then apply live.
/// Uses mktemp+install pattern to avoid shell-redirection failures on
/// systems where `echo >>` to /etc/sysctl.d/ may be denied by SELinux
/// or when the file does not yet exist.
#[tauri::command]
pub async fn security_fix_kernel_param(key: String, value: String, revert_value: String, enable: bool) -> Result<String, String> {
    let target = if enable { &value } else { &revert_value };

    // Input validation — only allow safe sysctl key/value characters
    if !key.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
        return Err("Invalid sysctl key.".to_string());
    }
    if !target.trim().chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ') {
        return Err("Invalid sysctl value (only digits and dash allowed).".to_string());
    }

    // Single consistent filename — matches the countermeasure descriptions shown in the UI.
    let conf_file = "/etc/sysctl.d/99-hardening.conf";

    // Robust script that:
    //   1. Uses `set -e` so ANY step failure aborts the whole script (no silent partial success).
    //   2. Builds the new file content in a temp file in /tmp (always writable, no SELinux issue).
    //   3. Uses `install` to atomically copy to destination with correct ownership/mode.
    //   4. Writes the file BEFORE running sysctl -w, so on any failure the live value
    //      is not changed either.
    //   5. Verifies the write with `sysctl -p` (reloads from the file).
    let script = format!(
        "set -e\n\
         mkdir -p /etc/sysctl.d\n\
         TMP=$(mktemp /tmp/sysctl_harden_XXXXXX)\n\
         trap 'rm -f \"$TMP\"' EXIT\n\
         if [ -f '{cfg}' ]; then grep -v '^{key}' '{cfg}' > \"$TMP\" 2>/dev/null || true; fi\n\
         printf '%s = %s\\n' '{key}' '{val}' >> \"$TMP\"\n\
         install -m 0644 -o root -g root \"$TMP\" '{cfg}'\n\
         sysctl -w '{key}={val}'\n\
         sysctl -p '{cfg}'",
        key = key, val = target, cfg = conf_file
    );

    let out = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .stderr(std::process::Stdio::piped())
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        if let Ok(mut guard) = LAST_SYSCTL_FIX_TIME.lock() {
            *guard = Some(SystemTime::now());
        }
        Ok(format!(
            "{} = {} — written to {} and applied immediately",
            key, target, conf_file
        ))
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        Err(if stderr.is_empty() {
            format!("Failed to apply {} — script exited with error (check system logs)", key)
        } else {
            stderr
        })
    }
}

/// Enable/disable firewall.
#[tauri::command]
pub async fn security_fix_firewall() -> Result<String, String> {
    let has_firewalld = crate::binary_exists("firewall-cmd").await;
    if has_firewalld {
        let _ = Command::new("pkexec").args(["systemctl", "enable", "--now", "firewalld"]).output().await;
        let out = Command::new("pkexec").args(["firewall-cmd", "--set-default-zone=drop"])
            .output().await.map_err(|e| e.to_string())?;
        if out.status.success() { return Ok("Firewalld enabled with default drop zone.".to_string()); }
    }
    let has_ufw = crate::binary_exists("ufw").await;
    if has_ufw {
        let _ = Command::new("pkexec").args(["ufw", "--force", "enable"]).output().await;
        let out = Command::new("pkexec").args(["ufw", "default", "deny", "incoming"])
            .output().await.map_err(|e| e.to_string())?;
        if out.status.success() { return Ok("UFW enabled with default deny incoming.".to_string()); }
    }
    Err("Neither firewalld nor ufw could be configured.".to_string())
}

/// Fix password aging policy.
#[tauri::command]
pub async fn security_fix_password_policy() -> Result<String, String> {
    let script = "sed -i 's/^PASS_MAX_DAYS.*/PASS_MAX_DAYS   90/g' /etc/login.defs && \
                  sed -i 's/^PASS_MIN_LEN.*/PASS_MIN_LEN    12/g' /etc/login.defs";
    let out = Command::new("pkexec").args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok("Password policy: PASS_MAX_DAYS=90, PASS_MIN_LEN=12 applied.".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Enable/disable SELinux.
#[tauri::command]
pub async fn security_fix_selinux(enable: bool) -> Result<String, String> {
    let script = if enable {
        "sed -i 's/^SELINUX=.*/SELINUX=enforcing/g' /etc/selinux/config && touch /.autorelabel"
    } else {
        "sed -i 's/^SELINUX=.*/SELINUX=permissive/g' /etc/selinux/config"
    };

    let out = Command::new("pkexec").args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        if enable {
            Ok("SELinux set to enforcing. A system reboot is required to relabel the filesystem.".to_string())
        } else {
            Ok("SELinux set to permissive (not disabled — still logs violations). A reboot is required.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Enable/disable auditd.
#[tauri::command]
pub async fn security_fix_auditd(enable: bool) -> Result<String, String> {
    let action = if enable { "enable" } else { "disable" };
    let out = Command::new("pkexec")
        .args(["systemctl", action, "--now", "auditd"])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(format!("auditd {}d successfully.", action))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Enable time synchronization (chronyd preferred, fallback to systemd-timesyncd).
#[tauri::command]
pub async fn security_fix_time_sync(enable: bool) -> Result<String, String> {
    let action = if enable { "enable" } else { "disable" };

    if crate::binary_exists("chronyd").await || crate::binary_exists("chronyc").await {
        let out = Command::new("pkexec")
            .args(["systemctl", action, "--now", "chronyd"])
            .output().await.map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok(format!("chronyd {}d successfully.", action));
        }
    }

    // fallback: systemd-timesyncd
    let out = Command::new("pkexec")
        .args(["systemctl", action, "--now", "systemd-timesyncd"])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(format!("systemd-timesyncd {}d successfully.", action))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Fix /tmp sticky bit — with tmpfiles.d persistence.
/// On Fedora/systemd, /tmp is often a tmpfs mounted fresh on boot.
/// `chmod` alone only lasts until next reboot. We also write a
/// /etc/tmpfiles.d/tmp-sticky.conf entry so systemd-tmpfiles-setup
/// resets the correct permissions at every boot.
#[tauri::command]
pub async fn security_fix_tmp_sticky(enable: bool) -> Result<String, String> {
    // The `D` type in tmpfiles.d: create or adjust directory permissions at boot.
    let script = if enable {
        // Set sticky bit now AND persist via tmpfiles.d
        "set -e\n\
         chmod 1777 /tmp\n\
         mkdir -p /etc/tmpfiles.d\n\
         printf 'D /tmp 1777 root root -\\n' > /etc/tmpfiles.d/tmp-sticky.conf"
    } else {
        // Remove sticky bit and clean up the tmpfiles.d entry
        "set -e\n\
         chmod 777 /tmp\n\
         rm -f /etc/tmpfiles.d/tmp-sticky.conf"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .stderr(std::process::Stdio::piped())
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        if enable {
            Ok("/tmp permissions set to 1777 (sticky). Persisted via /etc/tmpfiles.d/tmp-sticky.conf — survives reboots.".to_string())
        } else {
            Ok("/tmp sticky bit removed. /etc/tmpfiles.d/tmp-sticky.conf deleted.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Append a fix event to the local audit log.
///
/// Log path: ~/.config/linux-control-panel/security_advisor.log
/// Format:   one NDJSON line per event, e.g.:
///   {"ts":"2026-08-09T01:40:00Z","finding_id":"kernel_aslr","title":"ASLR...","action":"apply","outcome":"kernel.randomize_va_space = 2 ..."}
#[tauri::command]
pub async fn security_log_fix(
    finding_id: String,
    finding_title: String,
    action: String,   // "apply" | "revert"
    outcome: String,  // success message from the backend
) -> Result<(), String> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Build ISO-8601 timestamp (seconds precision, no external crate needed)
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ts = format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + secs / 31_557_600,
        ((secs % 31_557_600) / 2_628_000) + 1,
        ((secs % 2_628_000) / 86_400) + 1,
        (secs % 86_400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    );

    // Resolve log path: prefer XDG_CONFIG_HOME, fall back to ~/.config
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            std::path::PathBuf::from(home).join(".config")
        });
    let log_dir = config_dir.join("linux-control-panel");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Could not create log directory: {e}"))?;
    let log_path = log_dir.join("security_advisor.log");

    // Escape the strings minimally for safe JSON embedding
    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
    }

    let line = format!(
        "{{\"ts\":\"{ts}\",\"finding_id\":\"{id}\",\"title\":\"{title}\",\"action\":\"{action}\",\"outcome\":\"{outcome}\"}}\n",
        ts = ts,
        id = esc(&finding_id),
        title = esc(&finding_title),
        action = esc(&action),
        outcome = esc(&outcome),
    );

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Could not open log file: {e}"))?;

    file.write_all(line.as_bytes())
        .map_err(|e| format!("Could not write to log file: {e}"))?;

    Ok(())
}

/// Fix /etc/passwd permissions.
#[tauri::command]
pub async fn security_fix_passwd_perms() -> Result<String, String> {
    let out = Command::new("pkexec")
        .args(["bash", "-c", "chmod 644 /etc/passwd && chown root:root /etc/passwd"])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok("/etc/passwd permissions set to 644, owner root:root.".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Fix /etc/shadow permissions.
#[tauri::command]
pub async fn security_fix_shadow_perms() -> Result<String, String> {
    let out = Command::new("pkexec")
        .args(["bash", "-c", "chmod 000 /etc/shadow && chown root:root /etc/shadow"])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok("/etc/shadow permissions set to 000, owner root:root.".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Enable/disable USBGuard.
#[tauri::command]
pub async fn security_fix_usbguard(enable: bool) -> Result<String, String> {
    let action = if enable { "enable" } else { "disable" };
    let out = Command::new("pkexec")
        .args(["systemctl", action, "--now", "usbguard"])
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(format!("USBGuard {}d.", action))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Lock user account with empty password (passwd -l <user>).
#[tauri::command]
pub async fn security_fix_lock_account(user: Option<String>) -> Result<String, String> {
    let username = match user {
        Some(u) if !u.trim().is_empty() => u,
        _ => {
            let shadow = read_privileged_file("/etc/shadow").await;
            shadow.lines()
                .find_map(|line| {
                    let parts: Vec<&str> = line.splitn(3, ':').collect();
                    if parts.len() >= 2 && parts[1].is_empty() {
                        Some(parts[0].to_string())
                    } else { None }
                })
                .ok_or_else(|| "No accounts with empty passwords found.".to_string())?
        }
    };

    let out = Command::new("pkexec")
        .args(["passwd", "-l", &username])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        Ok(format!("Account '{username}' has been locked (passwd -l {username})."))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Configure PAM faillock lockout policy (deny = 5, unlock_time = 900).
#[tauri::command]
pub async fn security_fix_pam_faillock(enable: bool) -> Result<String, String> {
    let script = if enable {
        "set -e\n\
         mkdir -p /etc/security/faillock.conf.d\n\
         printf 'deny = 5\\nunlock_time = 900\\nfail_interval = 900\\n' > /etc/security/faillock.conf.d/50-hardening.conf\n\
         if command -v authselect >/dev/null 2>&1; then authselect enable-feature with-faillock 2>/dev/null || true; fi"
    } else {
        "rm -f /etc/security/faillock.conf.d/50-hardening.conf"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        if enable {
            Ok("PAM faillock configured: deny = 5, unlock_time = 900s via /etc/security/faillock.conf.d/50-hardening.conf.".to_string())
        } else {
            Ok("PAM faillock hardening configuration removed.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Set default system umask 027 in /etc/login.defs.
#[tauri::command]
pub async fn security_fix_umask(enable: bool) -> Result<String, String> {
    let script = if enable {
        "if grep -qE '^\\s*UMASK' /etc/login.defs; then sed -i 's/^\\s*UMASK.*/UMASK 027/g' /etc/login.defs; else echo 'UMASK 027' >> /etc/login.defs; fi"
    } else {
        "sed -i 's/^\\s*UMASK.*/UMASK 022/g' /etc/login.defs"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        if enable {
            Ok("Default system UMASK set to 027 in /etc/login.defs.".to_string())
        } else {
            Ok("Default system UMASK set to 022 in /etc/login.defs.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Set SSH idle timeout parameters (ClientAliveInterval 300, ClientAliveCountMax 3).
#[tauri::command]
pub async fn security_fix_ssh_idle_timeout(enable: bool) -> Result<String, String> {
    let script = if enable {
        "set -e\n\
         mkdir -p /etc/ssh/sshd_config.d\n\
         printf 'ClientAliveInterval 300\\nClientAliveCountMax 3\\n' > /etc/ssh/sshd_config.d/50-idle-timeout.conf\n\
         systemctl reload sshd 2>/dev/null || systemctl reload ssh 2>/dev/null || true"
    } else {
        "set -e\n\
         rm -f /etc/ssh/sshd_config.d/50-idle-timeout.conf\n\
         systemctl reload sshd 2>/dev/null || systemctl reload ssh 2>/dev/null || true"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        if enable {
            Ok("SSH idle timeout set: ClientAliveInterval=300, ClientAliveCountMax=3.".to_string())
        } else {
            Ok("SSH idle timeout configuration removed.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Blacklist legacy unused filesystem kernel modules via modprobe.d.
#[tauri::command]
pub async fn security_fix_blacklist_fs_modules(enable: bool) -> Result<String, String> {
    let script = if enable {
        "set -e\n\
         mkdir -p /etc/modprobe.d\n\
         printf 'install cramfs /bin/true\\ninstall freevxfs /bin/true\\ninstall hfs /bin/true\\ninstall hfsplus /bin/true\\ninstall jffs2 /bin/true\\ninstall udf /bin/true\\n' > /etc/modprobe.d/disable-unused-fs.conf"
    } else {
        "rm -f /etc/modprobe.d/disable-unused-fs.conf"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        if enable {
            Ok("Unused filesystem kernel modules (cramfs, hfs, udf, etc.) disabled via /etc/modprobe.d/disable-unused-fs.conf.".to_string())
        } else {
            Ok("Modprobe blacklist for unused filesystem modules removed.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Mask/unmask systemd ctrl-alt-del.target reboot signal.
#[tauri::command]
pub async fn security_fix_mask_ctrl_alt_del(enable: bool) -> Result<String, String> {
    let action = if enable { "mask" } else { "unmask" };
    let out = Command::new("pkexec")
        .args(["systemctl", action, "ctrl-alt-del.target"])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        Ok(format!("ctrl-alt-del.target {}ed successfully.", action))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Configure legal access warning banner (/etc/issue, /etc/issue.net, sshd Banner).
#[tauri::command]
pub async fn security_fix_legal_banner(enable: bool) -> Result<String, String> {
    let script = if enable {
        "set -e\n\
         BANNER='Authorized uses only. All activity may be monitored and reported.'\n\
         printf \"$BANNER\\n\" > /etc/issue\n\
         printf \"$BANNER\\n\" > /etc/issue.net\n\
         mkdir -p /etc/ssh/sshd_config.d\n\
         printf 'Banner /etc/issue.net\\n' > /etc/ssh/sshd_config.d/50-banner.conf\n\
         systemctl reload sshd 2>/dev/null || systemctl reload ssh 2>/dev/null || true"
    } else {
        "set -e\n\
         rm -f /etc/ssh/sshd_config.d/50-banner.conf\n\
         systemctl reload sshd 2>/dev/null || systemctl reload ssh 2>/dev/null || true"
    };

    let out = Command::new("pkexec")
        .args(["bash", "-c", script])
        .output().await.map_err(|e| e.to_string())?;

    if out.status.success() {
        invalidate_audit_cache();
        if enable {
            Ok("Legal access warning banner configured in /etc/issue and /etc/ssh/sshd_config.".to_string())
        } else {
            Ok("SSH legal warning banner configuration removed.".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

