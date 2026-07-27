use serde::{Deserialize, Serialize};
use tokio::process::Command;
use std::fs;
use std::io::{BufRead, BufReader};


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterface {
    pub name: String,
    pub ip4: Option<String>,
    pub ip6: Option<String>,
    pub is_up: bool,
    pub iface_type: String, // "ethernet", "wifi", "loopback", "other"
    pub mac: Option<String>,
}

/// Returns a list of all network interfaces with their IP addresses.
/// Uses `ip -j addr show` (JSON). Falls back to text parsing if unavailable.
#[tauri::command]
pub async fn get_network_interfaces() -> Result<Vec<NetworkInterface>, String> {
    // Try JSON mode first (ip >= 5.x, available on all modern Fedora/RHEL)
    let output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .await
        .map_err(|e| format!("Failed to run ip: {e}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_ip_json(&stdout)
    } else {
        // Fallback: text parsing
        let text_out = Command::new("ip")
            .args(["addr", "show"])
            .output()
            .await
            .map_err(|e| format!("Failed to run ip addr show: {e}"))?;
        let stdout = String::from_utf8_lossy(&text_out.stdout).to_string();
        parse_ip_text(&stdout)
    }
}

// ─── JSON Parsing ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct IpJsonEntry {
    ifname: String,
    flags: Vec<String>,
    link_type: Option<String>,
    address: Option<String>,
    addr_info: Vec<AddrInfo>,
}

#[derive(Deserialize)]
struct AddrInfo {
    family: String,
    local: String,
    scope: Option<String>,
}

fn parse_ip_json(json: &str) -> Result<Vec<NetworkInterface>, String> {
    let entries: Vec<IpJsonEntry> =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {e}"))?;

    let mut result = Vec::new();
    for e in entries {
        let is_up = e.flags.iter().any(|f| f == "UP");
        let ip4 = e.addr_info.iter()
            .find(|a| a.family == "inet" && a.scope.as_deref() != Some("host"))
            .map(|a| a.local.clone());
        let ip6 = e.addr_info.iter()
            .find(|a| a.family == "inet6" && a.scope.as_deref().unwrap_or("") == "global")
            .map(|a| a.local.clone());

        let iface_type = classify_interface(&e.ifname, e.link_type.as_deref());

        result.push(NetworkInterface {
            name: e.ifname,
            ip4,
            ip6,
            is_up,
            iface_type,
            mac: e.address,
        });
    }

    // Sort: non-loopback first, then by name
    result.sort_by(|a, b| {
        let a_lo = a.iface_type == "loopback";
        let b_lo = b.iface_type == "loopback";
        a_lo.cmp(&b_lo).then(a.name.cmp(&b.name))
    });

    Ok(result)
}

// ─── Text Fallback Parsing ───────────────────────────────────────────────────

fn parse_ip_text(text: &str) -> Result<Vec<NetworkInterface>, String> {
    let mut result = Vec::new();
    let mut current: Option<NetworkInterface> = None;

    for line in text.lines() {
        // New interface line: "2: eth0: <flags> ..."
        if line.starts_with(|c: char| c.is_ascii_digit()) {
            if let Some(iface) = current.take() {
                result.push(iface);
            }
            // Parse "N: <name>: <FLAGS>"
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() >= 2 {
                let name = parts[1].trim().to_string();
                let flags_str = parts.get(2).copied().unwrap_or("");
                let is_up = flags_str.contains("UP");
                let iface_type = classify_interface(&name, None);
                current = Some(NetworkInterface {
                    name,
                    ip4: None,
                    ip6: None,
                    is_up,
                    iface_type,
                    mac: None,
                });
            }
        } else if let Some(ref mut iface) = current {
            let trimmed = line.trim();
            // "inet 192.168.1.42/24 ..."
            if trimmed.starts_with("inet ") && !trimmed.starts_with("inet6") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(addr) = parts.get(1) {
                    // Strip prefix length
                    let ip = addr.split('/').next().unwrap_or(addr).to_string();
                    if ip != "127.0.0.1" {
                        iface.ip4 = Some(ip);
                    }
                }
            } else if trimmed.starts_with("inet6 ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(addr) = parts.get(1) {
                    let ip = addr.split('/').next().unwrap_or(addr).to_string();
                    // Only global scope
                    if trimmed.contains("scope global") {
                        iface.ip6 = Some(ip);
                    }
                }
            } else if trimmed.starts_with("link/ether ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(mac) = parts.get(1) {
                    iface.mac = Some(mac.to_string());
                }
            }
        }
    }

    if let Some(iface) = current {
        result.push(iface);
    }

    // Sort: non-loopback first
    result.sort_by(|a, b| {
        let a_lo = a.iface_type == "loopback";
        let b_lo = b.iface_type == "loopback";
        a_lo.cmp(&b_lo).then(a.name.cmp(&b.name))
    });

    Ok(result)
}

// ─── Interface Classifier ────────────────────────────────────────────────────

fn classify_interface(name: &str, link_type: Option<&str>) -> String {
    if name == "lo" || link_type == Some("loopback") {
        return "loopback".to_string();
    }
    // Wifi patterns: wlan*, wl*, wlp*
    if name.starts_with("wlan") || name.starts_with("wl") {
        return "wifi".to_string();
    }
    // Ethernet patterns: eth*, en*, eno*, enp*, ens*
    if name.starts_with("eth") || name.starts_with("en") {
        return "ethernet".to_string();
    }
    // Virtual / bridge / tunnel
    if name.starts_with("vir") || name.starts_with("br") || name.starts_with("docker")
        || name.starts_with("tun") || name.starts_with("tap") || name.starts_with("veth")
    {
        return "virtual".to_string();
    }
    "other".to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// SYSTEM STATS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemStats {
    pub cpu_percent: f64,
    pub cpu_per_core: Vec<f64>,
    pub cpu_cores: usize,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub ram_percent: f64,
    pub swap_used_mb: u64,
    pub swap_total_mb: u64,
    pub swap_percent: f64,
    pub uptime_seconds: u64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
}

/// Read CPU idle/total jiffies from /proc/stat for a single line.
fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    // Format: "cpu0 user nice system idle iowait irq softirq steal guest guest_nice"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 { return None; }
    let values: Vec<u64> = parts[1..].iter()
        .filter_map(|v| v.parse::<u64>().ok())
        .collect();
    if values.len() < 4 { return None; }
    let idle = values[3];
    let total: u64 = values.iter().sum();
    Some((idle, total))
}

/// Sample CPU usage over a 200ms interval by reading /proc/stat twice.
async fn sample_cpu() -> (f64, Vec<f64>) {
    fn read_stat() -> Vec<(u64, u64)> {
        let content = fs::read_to_string("/proc/stat").unwrap_or_default();
        content.lines()
            .filter(|l| l.starts_with("cpu"))
            .filter_map(|l| parse_cpu_line(l))
            .collect()
    }

    let sample1 = read_stat();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let sample2 = read_stat();

    let calc_percent = |s1: (u64, u64), s2: (u64, u64)| -> f64 {
        let d_total = s2.1.saturating_sub(s1.1);
        let d_idle  = s2.0.saturating_sub(s1.0);
        if d_total == 0 { return 0.0; }
        let used = d_total.saturating_sub(d_idle);
        (used as f64 / d_total as f64) * 100.0
    };

    let overall = if !sample1.is_empty() && !sample2.is_empty() {
        calc_percent(sample1[0], sample2[0])
    } else { 0.0 };

    let per_core: Vec<f64> = sample1.iter().skip(1)
        .zip(sample2.iter().skip(1))
        .map(|(&s1, &s2)| calc_percent(s1, s2))
        .collect();

    (overall, per_core)
}

#[tauri::command]
pub async fn get_system_stats() -> Result<SystemStats, String> {
    let (cpu_percent, cpu_per_core) = sample_cpu().await;
    let cpu_cores = cpu_per_core.len().max(1);

    // RAM & Swap from /proc/meminfo
    let meminfo = fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?;
    let mut mem_total = 0u64;
    let mut mem_available = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in meminfo.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let val = parts[1].parse::<u64>().unwrap_or(0);
        match parts[0] {
            "MemTotal:"     => mem_total = val,
            "MemAvailable:" => mem_available = val,
            "SwapTotal:"    => swap_total = val,
            "SwapFree:"     => swap_free = val,
            _ => {}
        }
    }

    let ram_used_mb  = (mem_total.saturating_sub(mem_available)) / 1024;
    let ram_total_mb = mem_total / 1024;
    let ram_percent  = if ram_total_mb > 0 { (ram_used_mb as f64 / ram_total_mb as f64) * 100.0 } else { 0.0 };
    let swap_used_mb = swap_total.saturating_sub(swap_free) / 1024;
    let swap_total_mb = swap_total / 1024;
    let swap_percent = if swap_total_mb > 0 { (swap_used_mb as f64 / swap_total_mb as f64) * 100.0 } else { 0.0 };

    // Uptime from /proc/uptime
    let uptime_seconds = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0) as u64;

    // Load average from /proc/loadavg
    let loadavg = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let loads: Vec<f64> = loadavg.split_whitespace()
        .take(3)
        .filter_map(|v| v.parse::<f64>().ok())
        .collect();
    let load_1  = loads.first().copied().unwrap_or(0.0);
    let load_5  = loads.get(1).copied().unwrap_or(0.0);
    let load_15 = loads.get(2).copied().unwrap_or(0.0);

    Ok(SystemStats {
        cpu_percent,
        cpu_per_core,
        cpu_cores,
        ram_used_mb,
        ram_total_mb,
        ram_percent,
        swap_used_mb,
        swap_total_mb,
        swap_percent,
        uptime_seconds,
        load_1,
        load_5,
        load_15,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// DISK USAGE
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMount {
    pub mount: String,
    pub device: String,
    pub fs_type: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent: f64,
}

#[tauri::command]
pub async fn get_disk_usage() -> Result<Vec<DiskMount>, String> {
    let output_bytes = Command::new("df")
        .args(["--block-size=1", "--output=source,fstype,size,used,avail,pcent,target"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output_bytes.stdout).to_string();
    let mut mounts = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 { continue; }

        let device  = parts[0].to_string();
        let fs_type = parts[1].to_string();
        let total   = parts[2].parse::<u64>().unwrap_or(0);
        let used    = parts[3].parse::<u64>().unwrap_or(0);
        let free    = parts[4].parse::<u64>().unwrap_or(0);
        let pct_str = parts[5].trim_end_matches('%');
        let percent = pct_str.parse::<f64>().unwrap_or(0.0);
        let mount   = parts[6].to_string();

        // Skip pseudo/virtual filesystems
        if fs_type == "tmpfs" || fs_type == "devtmpfs" || fs_type == "overlay"
            || fs_type == "squashfs" || device.starts_with("none")
            || mount.starts_with("/proc") || mount.starts_with("/sys")
            || mount.starts_with("/dev/pts") || mount.starts_with("/run")
        {
            continue;
        }

        mounts.push(DiskMount {
            mount,
            device,
            fs_type,
            total_gb: total as f64 / 1_073_741_824.0,
            used_gb:  used  as f64 / 1_073_741_824.0,
            free_gb:  free  as f64 / 1_073_741_824.0,
            percent,
        });
    }

    mounts.sort_by(|a, b| a.mount.cmp(&b.mount));
    Ok(mounts)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROCESS LIST
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub mem_rss_mb: f64,
    pub state: String,
    pub user: String,
    pub threads: u32,
}

#[tauri::command]
pub async fn get_process_list() -> Result<Vec<ProcessEntry>, String> {
    // Get total RAM for % calculation
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mem_total_kb: u64 = meminfo.lines()
        .find(|l| l.starts_with("MemTotal:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    // Sample /proc/stat for total jiffies
    let stat1 = fs::read_to_string("/proc/stat").unwrap_or_default();
    let total_jiffies_1: u64 = stat1.lines()
        .find(|l| l.starts_with("cpu "))
        .map(|l| l.split_whitespace().skip(1).filter_map(|v| v.parse::<u64>().ok()).sum())
        .unwrap_or(1);

    // Snapshot all proc utime+stime
    let proc_dir = fs::read_dir("/proc").map_err(|e| e.to_string())?;
    let mut snapshot1: Vec<(u32, u64, u64, String, String, String, u32, u64)> = Vec::new();

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let pid_str = name.to_string_lossy();
        let Ok(pid) = pid_str.parse::<u32>() else { continue };

        let stat_path = format!("/proc/{pid}/stat");
        let Ok(stat) = fs::read_to_string(&stat_path) else { continue };
        let parts: Vec<&str> = stat.split_whitespace().collect();
        if parts.len() < 24 { continue; }

        // Extract fields from /proc/pid/stat
        let proc_name = parts[1].trim_matches(|c| c == '(' || c == ')').to_string();
        let state = parts[2].to_string();
        let utime: u64 = parts[13].parse().unwrap_or(0);
        let stime: u64 = parts[14].parse().unwrap_or(0);
        let threads: u32 = parts[19].parse().unwrap_or(1);

        // RSS in pages → convert to KB
        let rss_pages: u64 = parts[23].parse().unwrap_or(0);
        let page_size = 4096u64; // common on Linux x86_64
        let rss_kb = rss_pages * page_size / 1024;

        // Username from /proc/pid/status
        let uid = fs::read_to_string(format!("/proc/{pid}/status"))
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("Uid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        let user = uid_to_user(uid);

        snapshot1.push((pid, utime + stime, rss_kb, proc_name, state, user, threads, uid as u64));
    }

    // Wait 300ms then re-sample for CPU %
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let stat2 = fs::read_to_string("/proc/stat").unwrap_or_default();
    let total_jiffies_2: u64 = stat2.lines()
        .find(|l| l.starts_with("cpu "))
        .map(|l| l.split_whitespace().skip(1).filter_map(|v| v.parse::<u64>().ok()).sum())
        .unwrap_or(1);
    let d_total = total_jiffies_2.saturating_sub(total_jiffies_1).max(1);

    let mut processes: Vec<ProcessEntry> = snapshot1.iter().filter_map(|(pid, jiffies1, rss_kb, name, state, user, threads, _uid)| {
        let stat_path = format!("/proc/{pid}/stat");
        let stat2 = fs::read_to_string(&stat_path).ok()?;
        let parts2: Vec<&str> = stat2.split_whitespace().collect();
        if parts2.len() < 15 { return None; }
        let utime2: u64 = parts2[13].parse().unwrap_or(0);
        let stime2: u64 = parts2[14].parse().unwrap_or(0);
        let jiffies2 = utime2 + stime2;
        let d_proc = jiffies2.saturating_sub(*jiffies1);
        let cpu_pct = (d_proc as f64 / d_total as f64) * 100.0;

        let mem_pct = (*rss_kb as f64 / mem_total_kb as f64) * 100.0;
        let mem_rss_mb = *rss_kb as f64 / 1024.0;

        let cmdline = fs::read_to_string(format!("/proc/{pid}/cmdline"))
            .unwrap_or_default()
            .replace('\0', " ")
            .trim()
            .chars()
            .take(120)
            .collect::<String>();

        Some(ProcessEntry {
            pid: *pid,
            name: name.clone(),
            cmdline: if cmdline.is_empty() { format!("[{name}]") } else { cmdline },
            cpu_percent: (cpu_pct * 10.0).round() / 10.0,
            mem_percent: (mem_pct * 10.0).round() / 10.0,
            mem_rss_mb: (mem_rss_mb * 10.0).round() / 10.0,
            state: state.clone(),
            user: user.clone(),
            threads: *threads,
        })
    }).collect();

    // Sort by CPU% descending, take top 1000
    processes.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
    processes.truncate(1000);

    Ok(processes)
}

fn uid_to_user(uid: u32) -> String {
    let passwd = fs::read_to_string("/etc/passwd").unwrap_or_default();
    for line in passwd.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 3 && parts[2].parse::<u32>().ok() == Some(uid) {
            return parts[0].to_string();
        }
    }
    uid.to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// KILL PROCESS (safety-gated)
// ═══════════════════════════════════════════════════════════════════════════

/// Send a signal to a process.
/// Hard safety rules enforced in Rust (not just UI):
/// - PID 1 (systemd/init) → ALWAYS blocked
/// - PID ≤ 100 → ALWAYS blocked (system processes)
/// - Only signals 15 (SIGTERM) and 9 (SIGKILL) are allowed
#[tauri::command]
pub fn kill_process(pid: u32, signal: u32) -> Result<String, String> {
    // ── Guard 1: Safe signal whitelist ────────────────────────────────────
    if signal != 15 && signal != 9 {
        return Err(format!("Signal {signal} is not allowed. Only SIGTERM (15) and SIGKILL (9) are permitted."));
    }

    // ── Guard 2: PID 1 is always blocked ──────────────────────────────────
    if pid == 1 {
        return Err("Cannot signal PID 1 (systemd/init). This would crash your system.".to_string());
    }

    // ── Guard 3: PID ≤ 100 are always blocked ─────────────────────────────
    if pid <= 100 {
        return Err(format!("Cannot signal PID {pid}. PIDs ≤ 100 are reserved for critical system processes."));
    }

    // ── Guard 4: Verify process still exists ──────────────────────────────
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Ok(format!("Process {pid} no longer exists."));
    }

    // ── Send signal ────────────────────────────────────────────────────────
    let result = unsafe { libc::kill(pid as i32, signal as i32) };
    if result == 0 {
        let sig_name = if signal == 15 { "SIGTERM" } else { "SIGKILL" };
        crate::log_to_file("INFO", &format!("Sent {sig_name} to PID {pid}"));
        Ok(format!("Signal {sig_name} sent to process {pid}."))
    } else {
        Err(format!("Failed to signal process {pid}: permission denied or process not found."))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTraffic {
    pub interface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartHealth {
    pub disk_path: String,
    pub model: String,
    pub health_status: String,
}

#[tauri::command]
pub async fn get_network_traffic() -> Result<Vec<NetworkTraffic>, String> {
    let content = fs::read_to_string("/proc/net/dev").map_err(|e| e.to_string())?;
    let mut traffics = Vec::new();
    
    for line in content.lines().skip(2) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 10 {
            let interface = parts[0].trim_end_matches(':').to_string();
            let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
            let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
            traffics.push(NetworkTraffic { interface, rx_bytes, tx_bytes });
        }
    }
    
    Ok(traffics)
}

#[tauri::command]
pub async fn get_smart_health() -> Result<Vec<SmartHealth>, String> {
    if !crate::utils::privilege::check_sudo_status() {
        return Ok(Vec::new());
    }

    let lsblk = crate::utils::privilege::tokio::Command::new("lsblk")
        .args(["-d", "-n", "-o", "NAME,TYPE"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let out = String::from_utf8_lossy(&lsblk.stdout);
    let mut results = Vec::new();

    for line in out.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "disk" {
            let dev_name = parts[0];
            if dev_name.starts_with("zram") || dev_name.starts_with("loop") {
                continue;
            }
            let dev = format!("/dev/{}", dev_name);
            
            let mut cmd = crate::utils::privilege::tokio::Command::new("pkexec");
            cmd.args(["smartctl", "-H", "-i", &dev]);
            cmd.stdout(std::process::Stdio::piped());
            let smart = cmd.output().await;
                
            if let Ok(smart_out) = smart {
                let stdout = String::from_utf8_lossy(&smart_out.stdout);
                
                let mut health_status = "UNKNOWN".to_string();
                let mut model = "Unknown Device".to_string();
                
                for s_line in stdout.lines() {
                    if s_line.starts_with("SMART overall-health self-assessment test result:") || s_line.starts_with("SMART Health Status:") {
                        health_status = s_line.split(':').nth(1).unwrap_or("UNKNOWN").trim().to_string();
                    } else if s_line.starts_with("Device Model:") || s_line.starts_with("Model Number:") || s_line.starts_with("Model Family:") {
                        model = s_line.split(':').nth(1).unwrap_or("Unknown").trim().to_string();
                    }
                }
                
                // If it is just an empty return or failed authentication, we might default to UNKNOWN.
                // We only add if we successfully got some real info, or at least we skip if it's completely empty.
                if health_status != "UNKNOWN" || model != "Unknown Device" {
                    results.push(SmartHealth {
                        disk_path: dev,
                        model,
                        health_status,
                    });
                }
            }
        }
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════════
// OS INFO
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub hostname: String,
    pub name: String,
    pub os_version: String,
    pub kernel_version: String,
}

#[tauri::command]
pub async fn get_os_info() -> Result<OsInfo, String> {
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_else(|_| "Unknown".to_string())
        .trim()
        .to_string();

    let kernel_version = fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "Unknown".to_string())
        .trim()
        .to_string();

    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut name = "Linux".to_string();
    let mut os_version = "".to_string();

    for line in os_release.lines() {
        if line.starts_with("NAME=") {
            name = line.trim_start_matches("NAME=").trim_matches('"').to_string();
        } else if line.starts_with("VERSION=") {
            os_version = line.trim_start_matches("VERSION=").trim_matches('"').to_string();
        }
    }

    Ok(OsInfo {
        hostname,
        name,
        os_version,
        kernel_version,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// DISK I/O STATS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskStats {
    pub device: String,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

#[tauri::command]
pub async fn get_disk_io_stats() -> Result<Vec<DiskStats>, String> {
    // Get valid physical disk names from /sys/block
    let mut block_devices = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("loop") && !name.starts_with("ram") && !name.starts_with("zram") {
                block_devices.insert(name);
            }
        }
    }

    let content = fs::read_to_string("/proc/diskstats").map_err(|e| e.to_string())?;
    let mut stats = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 10 {
            let device = parts[2].to_string();
            if block_devices.contains(&device) {
                // sectors read is at index 5, sectors written is at index 9
                let sectors_read = parts[5].parse::<u64>().unwrap_or(0);
                let sectors_written = parts[9].parse::<u64>().unwrap_or(0);
                stats.push(DiskStats {
                    device,
                    read_bytes: sectors_read * 512,
                    write_bytes: sectors_written * 512,
                });
            }
        }
    }
    Ok(stats)
}

// ═══════════════════════════════════════════════════════════════════════════
// ACTIVE CONNECTIONS (ss -tunp)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActiveConnection {
    pub protocol: String,
    pub local_address: String,
    pub remote_address: String,
    pub state: String,
    pub process_name: String,
    pub pid: Option<u32>,
}

#[tauri::command]
pub async fn get_active_connections() -> Result<Vec<ActiveConnection>, String> {
    // If root privileges are active, run with pkexec to get process names for all users
    let mut cmd = if crate::utils::privilege::check_sudo_status() {
        let mut c = crate::utils::privilege::tokio::Command::new("pkexec");
        c.args(["ss", "-tunp"]);
        c
    } else {
        let mut c = crate::utils::privilege::tokio::Command::new("ss");
        c.args(["-tunp"]);
        c
    };
    cmd.stdout(std::process::Stdio::piped());
    
    let output = cmd.output().await.map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let mut connections = Vec::new();
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let protocol = parts[0].to_string();
            let state = parts[1].to_string();
            let local_address = parts[4].to_string();
            let remote_address = parts[5].to_string();
            
            let mut process_name = "N/A".to_string();
            let mut pid = None;
            if parts.len() >= 7 {
                let proc_part = parts[6..].join(" ");
                if let Some(start) = proc_part.find("\"") {
                    if let Some(end) = proc_part[start+1..].find("\"") {
                        process_name = proc_part[start+1..start+1+end].to_string();
                    }
                }
                if let Some(pid_start) = proc_part.find("pid=") {
                    let pid_part = &proc_part[pid_start+4..];
                    if let Some(pid_end) = pid_part.find(',') {
                        if let Ok(p) = pid_part[..pid_end].parse::<u32>() {
                            pid = Some(p);
                        }
                    } else if let Some(pid_end) = pid_part.find(')') {
                        if let Ok(p) = pid_part[..pid_end].parse::<u32>() {
                            pid = Some(p);
                        }
                    }
                }
            }
            
            connections.push(ActiveConnection {
                protocol,
                local_address,
                remote_address,
                state,
                process_name,
                pid,
            });
        }
    }
    
    Ok(connections)
}

// ═══════════════════════════════════════════════════════════════════════════
// CURRENT USER
// ═══════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub fn get_current_user() -> Result<String, String> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    if !user.is_empty() {
        return Ok(user);
    }
    
    // Fallback using whoami
    let output = std::process::Command::new("whoami").output();
    if let Ok(o) = output {
        let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !name.is_empty() {
            return Ok(name);
        }
    }
    Ok("unknown".to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// DASHBOARD RESOURCES, GATEWAY PING, AND SYSTEM EVENTS
// ═══════════════════════════════════════════════════════════════════════════



fn get_default_gateway() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(via_idx) = parts.iter().position(|&x| x == "via") {
            if via_idx + 1 < parts.len() {
                return Some(parts[via_idx + 1].to_string());
            }
        }
    }
    None
}

#[tauri::command]
pub async fn ping_interface_gateway(iface: String) -> Result<String, String> {
    let gateway = get_default_gateway().ok_or_else(|| "No default gateway".to_string())?;
    
    let mut cmd = std::process::Command::new("ping");
    cmd.args(["-I", &iface, "-c", "1", "-W", "1", &gateway]);
    
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("timeout".to_string());
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("time=") {
            if let Some(idx) = line.find("time=") {
                let time_part = &line[idx + 5..];
                if let Some(space_idx) = time_part.find(' ') {
                    let ms_val = &time_part[..space_idx];
                    if let Ok(ms) = ms_val.parse::<f64>() {
                        let rounded = ms.round() as u64;
                        let display = if rounded == 0 { "<1ms".to_string() } else { format!("{}ms", rounded) };
                        return Ok(display);
                    }
                }
            }
        }
    }
    
    Err("timeout".to_string())
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SystemEvents {
    pub last_boot_time: String,
    pub error_count: u64,
    pub warning_count: u64,
}

fn get_last_boot_time() -> Option<String> {
    let output = std::process::Command::new("who")
        .arg("-b")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = stdout.lines().next() {
        if line.contains("system boot") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                return Some(format!("{} {}", parts[2], parts[3]));
            }
        }
    }
    
    // Fallback: current time - uptime
    let uptime = fs::read_to_string("/proc/uptime").ok()?;
    let uptime_secs = uptime.split_whitespace().next()?.parse::<f64>().ok()?;
    let boot_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).ok()?
        .as_secs_f64() - uptime_secs;
        
    let output = std::process::Command::new("date")
        .args(["-d", &format!("@{}", boot_epoch as u64), "+%Y-%m-%d %H:%M"])
        .output()
        .ok()?;
    let date_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !date_str.is_empty() {
        return Some(date_str);
    }
    
    None
}

#[tauri::command]
pub async fn get_system_events() -> Result<SystemEvents, String> {
    let last_boot_time = get_last_boot_time().unwrap_or_else(|| "Unknown".to_string());
    
    let error_output = std::process::Command::new("sh")
        .args(["-c", "journalctl -p err -b --no-pager -q | wc -l"])
        .output()
        .map_err(|e| e.to_string())?;
    let error_count = String::from_utf8_lossy(&error_output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    let warning_output = std::process::Command::new("sh")
        .args(["-c", "journalctl -p warning -b --no-pager -q | wc -l"])
        .output()
        .map_err(|e| e.to_string())?;
    let warning_count = String::from_utf8_lossy(&warning_output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    Ok(SystemEvents {
        last_boot_time,
        error_count,
        warning_count,
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct NetworkDetails {
    pub gateway: Option<String>,
    pub dns: Vec<String>,
}

fn get_dns_servers() -> Vec<String> {
    let mut dns = Vec::new();
    
    // 1. Try systemd-resolved upstream resolv.conf first
    if let Ok(content) = fs::read_to_string("/run/systemd/resolve/resolv.conf") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == "nameserver" {
                let ip = parts[1].to_string();
                if ip != "127.0.0.53" && !dns.contains(&ip) {
                    dns.push(ip);
                }
            }
        }
    }
    
    // 2. Fallback to /etc/resolv.conf
    if dns.is_empty() {
        if let Ok(content) = fs::read_to_string("/etc/resolv.conf") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == "nameserver" {
                    let ip = parts[1].to_string();
                    if !dns.contains(&ip) {
                        dns.push(ip);
                    }
                }
            }
        }
    }
    
    // 3. Fallback to resolvectl
    if dns.is_empty() || (dns.len() == 1 && dns[0] == "127.0.0.53") {
        if let Ok(output) = std::process::Command::new("resolvectl").arg("dns").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                for part in parts {
                    if part.parse::<std::net::IpAddr>().is_ok() {
                        let ip = part.to_string();
                        if !dns.contains(&ip) && ip != "127.0.0.53" {
                            dns.push(ip);
                        }
                    }
                }
            }
        }
    }
    
    dns
}

#[tauri::command]
pub async fn get_network_details() -> Result<NetworkDetails, String> {
    let gateway = get_default_gateway();
    let dns = get_dns_servers();
    Ok(NetworkDetails { gateway, dns })
}

#[tauri::command]
pub async fn ping_gateway(ip: String) -> Result<String, String> {
    // 1. Try ICMP ping first
    let mut cmd = std::process::Command::new("ping");
    cmd.args(["-c", "1", "-W", "1", &ip]);
    
    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("time=") {
                    if let Some(idx) = line.find("time=") {
                        let time_part = &line[idx + 5..];
                        if let Some(space_idx) = time_part.find(' ') {
                            let ms_val = &time_part[..space_idx];
                            if let Ok(ms) = ms_val.parse::<f64>() {
                                let rounded = ms.round() as u64;
                                let display = if rounded == 0 { "<1ms".to_string() } else { format!("{}ms", rounded) };
                                return Ok(display);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Try TCP fallback to common ports (53 DNS, 80 HTTP, 443 HTTPS)
    let ports = [53, 80, 443];
    for &port in &ports {
        let addr_str = format!("{}:{}", ip, port);
        if let Ok(addr) = addr_str.parse::<std::net::SocketAddr>() {
            let start = std::time::Instant::now();
            if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(800)).is_ok() {
                let elapsed = start.elapsed().as_millis();
                let display = if elapsed == 0 { "<1ms".to_string() } else { format!("{}ms", elapsed) };
                return Ok(display);
            }
        }
    }
    
    Err("timeout".to_string())
}

#[tauri::command]
pub async fn get_cpu_temperature() -> Result<Option<f32>, String> {
    let mut max_temp: Option<f32> = None;
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if name_str.starts_with("thermal_zone") {
                let temp_path = entry.path().join("temp");
                if let Ok(temp_str) = fs::read_to_string(temp_path) {
                    if let Ok(temp_val) = temp_str.trim().parse::<f32>() {
                        let temp_c = temp_val / 1000.0;
                        if max_temp.is_none() || temp_c > max_temp.unwrap() {
                            max_temp = Some(temp_c);
                        }
                    }
                }
            }
        }
    }
    Ok(max_temp)
}

#[tauri::command]
pub async fn get_last_system_update() -> Result<String, String> {
    let output = std::process::Command::new("sh")
        .args(["-c", "for pkg in dnf5 dnf python3-dnf; do rpm -q --last $pkg 2>/dev/null && break; done | head -1"])
        .output()
        .map_err(|e| format!("Failed to run rpm command: {e}"))?;

    if !output.status.success() {
        return Ok("Unknown".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    if parts.len() > 1 {
        let date_str = parts[1..].join(" ");
        Ok(date_str)
    } else {
        Ok("Unknown".to_string())
    }
}

#[tauri::command]
pub async fn get_failed_services_count() -> Result<u32, String> {
    let output = std::process::Command::new("sh")
        .args(["-c", "systemctl --failed --no-legend | wc -l"])
        .output()
        .map_err(|e| format!("Failed to run systemctl: {e}"))?;

    if !output.status.success() {
        return Ok(0);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Ok(count) = stdout.parse::<u32>() {
        Ok(count)
    } else {
        Ok(0)
    }
}

#[derive(serde::Serialize)]
pub struct StorageDistribution {
    pub rpm_gb: f64,
    pub flatpak_gb: f64,
    pub system_gb: f64,
}

#[tauri::command]
pub async fn get_storage_distribution() -> Result<StorageDistribution, String> {
    // 1. Get RPM size
    let rpm_gb = match tokio::process::Command::new("rpm")
        .args(["-qa", "--queryformat", "%{SIZE}\n"])
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let total_bytes: u64 = stdout
                .lines()
                .map(|line| line.parse::<u64>().unwrap_or(0))
                .sum();
            total_bytes as f64 / (1024.0 * 1024.0 * 1024.0) // GB
        }
        Err(_) => 42.0 // fallback
    };

    // 2. Get Flatpak size
    let flatpak_gb = match tokio::process::Command::new("flatpak")
        .args(["list", "--columns=size"])
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut total_bytes: u64 = 0;
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let val = parts[0].parse::<f64>().unwrap_or(0.0);
                    let unit = parts[1].to_lowercase();
                    let multiplier = if unit.contains("gb") {
                        1024.0 * 1024.0 * 1024.0
                    } else if unit.contains("mb") {
                        1024.0 * 1024.0
                    } else if unit.contains("kb") {
                        1024.0
                    } else {
                        1.0
                    };
                    total_bytes += (val * multiplier) as u64;
                }
            }
            total_bytes as f64 / (1024.0 * 1024.0 * 1024.0) // GB
        }
        Err(_) => 12.0 // fallback
    };

    // 3. Get root disk used size
    let mut total_used_gb = 62.0; // default/fallback
    if let Ok(mounts) = get_disk_usage().await {
        if let Some(root_mount) = mounts.iter().find(|m| m.mount == "/") {
            total_used_gb = root_mount.used_gb;
        }
    }

    // System = Total Used - RPM - Flatpak
    let system_gb = (total_used_gb - rpm_gb - flatpak_gb).max(8.0);

    Ok(StorageDistribution {
        rpm_gb,
        flatpak_gb,
        system_gb,
    })
}

/// Opens a local directory or file in the default desktop file manager (via xdg-open).
#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open directory '{path}': {e}"))?;
    Ok(())
}

