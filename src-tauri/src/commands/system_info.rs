use serde::{Deserialize, Serialize};
use tokio::process::Command;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Mutex;

static TELEMETRY_HISTORY: Mutex<Option<VecDeque<SystemStats>>> = Mutex::new(None);
const MAX_TELEMETRY_SAMPLES: usize = 60;

/// Run one named System Info helper action. The helper is authoritative and
/// returns its stdout for the read-only SMART action.
async fn run_system_named_action(action: &str, params: &[String]) -> Result<String, String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 1);
    args.push(action.to_string());
    args.extend_from_slice(params);
    let output = crate::utils::privilege::run_privileged_with_stdin(
        helper,
        &args,
        &crate::utils::privilege::RootAuth::Polkit,
        "",
    )
    .await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}


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
    if parts.is_empty() || !parts[0].starts_with("cpu") {
        return None;
    }
    // Keep positions stable even when a field is malformed.
    let values: Vec<u64> = parts[1..]
        .iter()
        .map(|value| value.parse::<u64>().unwrap_or(0))
        .collect();
    if values.len() < 4 {
        return None;
    }
    // I/O wait is idle time, not CPU work. Guest time is already counted in
    // user/nice, so it must not be added to the total again.
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().take(8).sum();
    Some((idle, total))
}

static PREV_CPU_SAMPLE: Mutex<Option<(std::time::Instant, Vec<(u64, u64)>)>> = Mutex::new(None);

/// Sample CPU usage instantaneously by computing delta against previous /proc/stat snapshot.
async fn sample_cpu() -> (f64, Vec<f64>) {
    fn read_stat() -> Vec<(u64, u64)> {
        let content = fs::read_to_string("/proc/stat").unwrap_or_default();
        content.lines()
            .filter(|l| l.starts_with("cpu"))
            .filter_map(|l| parse_cpu_line(l))
            .collect()
    }

    let calc_percent = |s1: (u64, u64), s2: (u64, u64)| -> f64 {
        let d_total = s2.1.saturating_sub(s1.1);
        let d_idle  = s2.0.saturating_sub(s1.0);
        if d_total == 0 { return 0.0; }
        let used = d_total.saturating_sub(d_idle);
        ((used as f64 / d_total as f64) * 100.0).clamp(0.0, 100.0)
    };

    let current = read_stat();
    let now = std::time::Instant::now();

    let prev_opt = if let Ok(mut guard) = PREV_CPU_SAMPLE.lock() {
        let prev = guard.clone();
        *guard = Some((now, current.clone()));
        prev
    } else {
        None
    };

    if let Some((prev_time, prev_sample)) = prev_opt {
        if !prev_sample.is_empty() && !current.is_empty() && now.duration_since(prev_time).as_millis() >= 40 {
            let overall = calc_percent(prev_sample[0], current[0]);
            let per_core: Vec<f64> = prev_sample.iter().skip(1)
                .zip(current.iter().skip(1))
                .map(|(&s1, &s2)| calc_percent(s1, s2))
                .collect();
            return (overall, per_core);
        }
    }

    // Cold start fallback: Brief 40ms sample
    tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
    let sample2 = read_stat();
    let overall = if !current.is_empty() && !sample2.is_empty() {
        calc_percent(current[0], sample2[0])
    } else { 0.0 };

    let per_core: Vec<f64> = current.iter().skip(1)
        .zip(sample2.iter().skip(1))
        .map(|(&s1, &s2)| calc_percent(s1, s2))
        .collect();

    if let Ok(mut guard) = PREV_CPU_SAMPLE.lock() {
        *guard = Some((std::time::Instant::now(), sample2));
    }

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

    let stats = SystemStats {
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
    };

    if let Ok(mut history_lock) = TELEMETRY_HISTORY.lock() {
        let history = history_lock.get_or_insert_with(|| VecDeque::with_capacity(MAX_TELEMETRY_SAMPLES));
        if history.len() >= MAX_TELEMETRY_SAMPLES {
            history.pop_front();
        }
        history.push_back(stats.clone());
    }

    Ok(stats)
}

#[tauri::command]
pub async fn get_system_stats_history() -> Result<Vec<SystemStats>, String> {
    if let Ok(history_lock) = TELEMETRY_HISTORY.lock() {
        if let Some(ref history) = *history_lock {
            return Ok(history.iter().cloned().collect());
        }
    }
    Ok(Vec::new())
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
    pub ppid: u32,
    pub name: String,
    pub cmdline: String,
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub mem_rss_mb: f64,
    pub state: String,
    pub user: String,
    pub uid: u32,
    /// Whether the panel's user may signal or renice this process. Root-owned
    /// processes are listed but their actions must be disabled.
    pub owned: bool,
    pub threads: u32,
}

struct ProcStat {
    name: String,
    state: String,
    ppid: u32,
    utime: u64,
    stime: u64,
    threads: u32,
    rss_pages: u64,
}

/// Parse `/proc/<pid>/stat`. The command name is wrapped in parentheses and may
/// contain spaces and further parentheses, so the fields start after the last
/// `)`; splitting the whole line would shift every following field.
fn parse_proc_stat(stat: &str) -> Option<ProcStat> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = stat[open + 1..close].to_string();
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }
    // fields[0] is field 3 (state) in proc(5). utime (14), stime (15),
    // num_threads (20) and rss (24) therefore sit at 11, 12, 17 and 21.
    Some(ProcStat {
        name,
        state: fields[0].to_string(),
        ppid: fields[1].parse().unwrap_or(0),
        utime: fields[11].parse().unwrap_or(0),
        stime: fields[12].parse().unwrap_or(0),
        threads: fields[17].parse().unwrap_or(1),
        rss_pages: fields[21].parse().unwrap_or(0),
    })
}

fn page_size() -> u64 {
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value > 0 {
        value as u64
    } else {
        4096
    }
}

fn total_memory_kb() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
}

fn cpu_total_jiffies() -> u64 {
    fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|stat| {
            stat.lines()
                .find(|line| line.starts_with("cpu "))
                .and_then(parse_cpu_line)
        })
        .map(|(_, total)| total)
        .unwrap_or(1)
}

fn read_process_uid(pid: u32) -> Option<u32> {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find(|line| line.starts_with("Uid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// `/etc/passwd` is read once per scan instead of once per process.
fn user_map() -> HashMap<u32, String> {
    let mut users = HashMap::new();
    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 3 {
                if let Ok(uid) = fields[2].parse::<u32>() {
                    users.entry(uid).or_insert_with(|| fields[0].to_string());
                }
            }
        }
    }
    users
}

fn process_cmdline(pid: u32, name: &str) -> String {
    let cmdline = fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .unwrap_or_default()
        .replace('\0', " ")
        .trim()
        .chars()
        .take(240)
        .collect::<String>();
    if cmdline.is_empty() {
        format!("[{name}]")
    } else {
        cmdline
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn current_euid() -> u32 {
    unsafe { libc::geteuid() }
}

/// The panel runs as the desktop user. Only that user's processes (or anything,
/// when the panel itself runs as root) may be signalled or reniced.
fn can_manage_uid(euid: u32, target_uid: u32) -> bool {
    euid == 0 || euid == target_uid
}

#[tauri::command]
pub async fn get_process_list() -> Result<Vec<ProcessEntry>, String> {
    let mem_total_kb = total_memory_kb();
    let page_size = page_size();
    let users = user_map();
    let euid = current_euid();
    let total_jiffies_1 = cpu_total_jiffies();

    struct Snapshot {
        pid: u32,
        stat: ProcStat,
        uid: u32,
        rss_kb: u64,
    }

    let mut snapshot1: Vec<Snapshot> = Vec::new();
    for entry in fs::read_dir("/proc").map_err(|e| e.to_string())?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(parsed) = parse_proc_stat(&stat) else {
            continue;
        };
        let uid = read_process_uid(pid).unwrap_or(0);
        let rss_kb = parsed.rss_pages.saturating_mul(page_size) / 1024;
        snapshot1.push(Snapshot {
            pid,
            stat: parsed,
            uid,
            rss_kb,
        });
    }

    // Wait 300ms then re-sample for CPU %.
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let total_jiffies_2 = cpu_total_jiffies();
    let d_total = total_jiffies_2.saturating_sub(total_jiffies_1).max(1);

    let mut processes = Vec::with_capacity(snapshot1.len());
    for item in snapshot1 {
        let Ok(stat2) = fs::read_to_string(format!("/proc/{}/stat", item.pid)) else {
            continue;
        };
        let Some(second) = parse_proc_stat(&stat2) else {
            continue;
        };
        let jiffies1 = item.stat.utime + item.stat.stime;
        let jiffies2 = second.utime + second.stime;
        let cpu_pct = (jiffies2.saturating_sub(jiffies1) as f64 / d_total as f64) * 100.0;
        let mem_pct = (item.rss_kb as f64 / mem_total_kb as f64) * 100.0;
        let user = users
            .get(&item.uid)
            .cloned()
            .unwrap_or_else(|| item.uid.to_string());

        processes.push(ProcessEntry {
            pid: item.pid,
            ppid: item.stat.ppid,
            name: item.stat.name.clone(),
            cmdline: process_cmdline(item.pid, &item.stat.name),
            cpu_percent: round1(cpu_pct),
            mem_percent: round1(mem_pct),
            mem_rss_mb: round1(item.rss_kb as f64 / 1024.0),
            state: item.stat.state,
            user,
            uid: item.uid,
            owned: can_manage_uid(euid, item.uid),
            threads: item.stat.threads,
        });
    }

    // The UI sorts and builds the tree; the full list keeps parents present.
    processes.sort_by_key(|process| process.pid);
    Ok(processes)
}

/// One process for the connection drawer, without scanning the whole table.
#[tauri::command]
pub async fn get_process_details(pid: u32) -> Result<Option<ProcessEntry>, String> {
    if pid == 0 {
        return Ok(None);
    }
    let Ok(stat1) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return Ok(None);
    };
    let Some(first) = parse_proc_stat(&stat1) else {
        return Ok(None);
    };
    let total1 = cpu_total_jiffies();
    tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
    let Ok(stat2) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return Ok(None);
    };
    let Some(second) = parse_proc_stat(&stat2) else {
        return Ok(None);
    };
    let total2 = cpu_total_jiffies();
    let d_total = total2.saturating_sub(total1).max(1);
    let d_proc = (second.utime + second.stime).saturating_sub(first.utime + first.stime);
    let cpu_pct = (d_proc as f64 / d_total as f64) * 100.0;
    let uid = read_process_uid(pid).unwrap_or(0);
    let users = user_map();
    let rss_kb = first.rss_pages.saturating_mul(page_size()) / 1024;
    let mem_pct = (rss_kb as f64 / total_memory_kb() as f64) * 100.0;

    Ok(Some(ProcessEntry {
        pid,
        ppid: first.ppid,
        name: first.name.clone(),
        cmdline: process_cmdline(pid, &first.name),
        cpu_percent: round1(cpu_pct),
        mem_percent: round1(mem_pct),
        mem_rss_mb: round1(rss_kb as f64 / 1024.0),
        state: first.state,
        user: users.get(&uid).cloned().unwrap_or_else(|| uid.to_string()),
        uid,
        owned: can_manage_uid(current_euid(), uid),
        threads: first.threads,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// KILL PROCESS (safety-gated)
// ═══════════════════════════════════════════════════════════════════════════

/// Send a signal to a process.
/// Hard safety rules enforced in Rust (not just UI):
/// - PID 1 (systemd/init) → ALWAYS blocked
/// - PID ≤ 100 → ALWAYS blocked (system processes)
/// - Signals 1 (SIGHUP), 15 (SIGTERM), 9 (SIGKILL), 19 (SIGSTOP), and 18 (SIGCONT) are allowed
#[tauri::command]
pub fn kill_process(pid: u32, signal: u32) -> Result<String, String> {
    // ── Guard 1: Safe signal whitelist ────────────────────────────────────
    if signal != 15 && signal != 9 && signal != 19 && signal != 18 && signal != 1 {
        return Err(format!("Signal {signal} is not allowed. Allowed: SIGHUP (1), SIGTERM (15), SIGKILL (9), SIGSTOP (19), SIGCONT (18)."));
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
    let Some(target_uid) = read_process_uid(pid) else {
        return Ok(format!("Process {pid} no longer exists."));
    };

    // ── Guard 5: Only the owner may signal it ─────────────────────────────
    // The OS enforces this too, but a clear refusal keeps the UI honest and
    // stops it from offering actions that can only fail.
    if !can_manage_uid(current_euid(), target_uid) {
        return Err(format!(
            "You can only signal your own processes. PID {pid} belongs to another user."
        ));
    }

    // ── Send signal ────────────────────────────────────────────────────────
    let result = unsafe { libc::kill(pid as i32, signal as i32) };
    if result == 0 {
        let sig_name = match signal {
            1 => "SIGHUP",
            15 => "SIGTERM",
            9 => "SIGKILL",
            19 => "SIGSTOP",
            18 => "SIGCONT",
            _ => "SIGNAL",
        };
        crate::log_to_file("INFO", &format!("Sent {sig_name} to PID {pid}"));
        Ok(format!("Signal {sig_name} sent to process {pid}."))
    } else {
        Err(format!("Failed to signal process {pid}: permission denied or process not found."))
    }
}

/// Change process niceness/priority (-20 to 19)
#[tauri::command]
pub async fn renice_process(pid: u32, priority: i32) -> Result<String, String> {
    if priority < -20 || priority > 19 {
        return Err("Priority (niceness) must be between -20 (highest priority) and 19 (lowest priority).".to_string());
    }
    if pid == 1 || pid <= 100 {
        return Err(format!("Cannot change priority for PID {pid} (protected system process)."));
    }
    let Some(target_uid) = read_process_uid(pid) else {
        return Err(format!("Process {pid} no longer exists."));
    };
    if !can_manage_uid(current_euid(), target_uid) {
        return Err("You can only change the priority of your own processes.".to_string());
    }

    if priority < 0 {
        // Lowering the niceness of another user's process needs root rights.
        // The named helper action validates the pid, the negative range and
        // the protected-PID guard before running the fixed renice argv.
        run_system_named_action(
            "process_renice",
            &[pid.to_string(), priority.to_string()],
        )
        .await
        .map_err(|e| format!("Failed to set process priority: {e}"))?;
        return Ok(format!("Successfully adjusted PID {pid} priority to {priority}"));
    }

    let output = tokio::process::Command::new("renice")
        .args(["-n", &priority.to_string(), "-p", &pid.to_string()])
        .output()
        .await
        .map_err(|e| format!("Failed to renice process: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("Failed to set process priority: {err}"));
    }

    Ok(format!("Successfully adjusted PID {pid} priority to {priority}"))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartHealthIssue {
    pub disk_path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartHealthReport {
    pub disks: Vec<SmartHealth>,
    pub errors: Vec<SmartHealthIssue>,
}

/// Keep frontend-visible SMART failures actionable but free of command paths
/// and raw tool output.
fn smart_health_failure(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("administrator helper is not installed") {
        "Administrator helper is not installed".to_string()
    } else if lower.contains("not a block device") || lower.contains("symlink") {
        "Not a valid block device".to_string()
    } else if lower.contains("no longer exists") || lower.contains("could not be inspected") {
        "Device is no longer available".to_string()
    } else if lower.contains("permission") || lower.contains("not authorized") {
        "Permission denied while reading SMART data".to_string()
    } else {
        "SMART data could not be read".to_string()
    }
}

fn parse_smart_health_output(device: &str, stdout: &str) -> Result<SmartHealth, String> {
    let mut health_status = "UNKNOWN".to_string();
    let mut model = "Unknown Device".to_string();
    for line in stdout.lines() {
        if line.starts_with("SMART overall-health self-assessment test result:")
            || line.starts_with("SMART Health Status:")
        {
            health_status = line
                .split(':')
                .nth(1)
                .unwrap_or("UNKNOWN")
                .trim()
                .to_string();
        } else if line.starts_with("Device Model:")
            || line.starts_with("Model Number:")
            || line.starts_with("Model Family:")
        {
            model = line
                .split(':')
                .nth(1)
                .unwrap_or("Unknown")
                .trim()
                .to_string();
        }
    }
    if health_status == "UNKNOWN" && model == "Unknown Device" {
        return Err("No SMART health data returned".to_string());
    }
    Ok(SmartHealth {
        disk_path: device.to_string(),
        model,
        health_status,
    })
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
pub async fn get_smart_health() -> Result<SmartHealthReport, String> {
    let lsblk = crate::utils::privilege::tokio::Command::new("lsblk")
        .args(["-d", "-n", "-o", "NAME,TYPE"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let out = String::from_utf8_lossy(&lsblk.stdout);
    let mut report = SmartHealthReport {
        disks: Vec::new(),
        errors: Vec::new(),
    };

    for line in out.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "disk" {
            let dev_name = parts[0];
            if dev_name.starts_with("zram") || dev_name.starts_with("loop") {
                continue;
            }
            let dev = format!("/dev/{}", dev_name);
            match run_system_named_action("smart_health", &[dev.clone()]).await {
                Ok(stdout) => match parse_smart_health_output(&dev, &stdout) {
                    Ok(disk) => report.disks.push(disk),
                    Err(error) => report.errors.push(SmartHealthIssue {
                        disk_path: dev,
                        error,
                    }),
                },
                Err(error) => report.errors.push(SmartHealthIssue {
                    disk_path: dev,
                    error: smart_health_failure(&error),
                }),
            }
        }
    }

    Ok(report)
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
    pub distro_family: String,
    pub pkg_manager: String,
    pub mac_type: String,
    pub firewall: String,
}

#[tauri::command]
pub async fn get_distro_info() -> Result<crate::utils::distro::DistroInfo, String> {
    Ok(crate::utils::distro::detect_distro())
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

    let distro = crate::utils::distro::detect_distro();
    let name = if !distro.pretty_name.is_empty() {
        distro.pretty_name
    } else if !distro.name.is_empty() {
        distro.name
    } else {
        "Linux".to_string()
    };
    let os_version = distro.version_id;
    let distro_family = format!("{:?}", distro.family).to_lowercase();

    Ok(OsInfo {
        hostname,
        name,
        os_version,
        kernel_version,
        distro_family,
        pkg_manager: distro.pkg_manager,
        mac_type: distro.mac_type,
        firewall: distro.firewall,
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
    pub local_host: String,
    pub local_port: Option<u16>,
    pub remote_host: String,
    pub remote_port: Option<u16>,
    pub remote_is_loopback: bool,
    pub remote_is_external: bool,
}

fn split_endpoint(endpoint: &str) -> (String, Option<u16>) {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint == "*" || endpoint == "*:*" {
        return ("*".to_string(), None);
    }
    // `ss` prints IPv6 endpoints as `[::1]:631` and wildcard ports as `:*`.
    if let Some(rest) = endpoint.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            let host = &rest[..close];
            let port = rest[close + 1..]
                .strip_prefix(':')
                .and_then(|value| value.parse::<u16>().ok());
            return (host.to_string(), port);
        }
    }
    if let Some((host, port)) = endpoint.rsplit_once(':') {
        if port == "*" {
            return (host.to_string(), None);
        }
        if let Ok(port) = port.parse::<u16>() {
            return (host.to_string(), Some(port));
        }
    }
    (endpoint.to_string(), None)
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    host == "localhost"
        || host == "::1"
        || host.starts_with("127.")
        || host.starts_with("::ffff:127.")
        || host.eq_ignore_ascii_case("ip6-localhost")
}

fn is_unspecified_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    matches!(host, "" | "*" | "0.0.0.0" | "::" | "::ffff:0.0.0.0")
}

fn parse_ss_line(line: &str) -> Option<ActiveConnection> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    // Netid State Recv-Q Send-Q Local Peer [Process] — six columns minimum.
    if parts.len() < 6 {
        return None;
    }
    let protocol = parts[0].to_string();
    let state = parts[1].to_string();
    let local_address = parts[4].to_string();
    let remote_address = parts[5].to_string();
    let (local_host, local_port) = split_endpoint(&local_address);
    let (remote_host, remote_port) = split_endpoint(&remote_address);

    let mut process_name = "N/A".to_string();
    let mut pid = None;
    if parts.len() >= 7 {
        let proc_part = parts[6..].join(" ");
        if let Some(start) = proc_part.find('"') {
            if let Some(end) = proc_part[start + 1..].find('"') {
                process_name = proc_part[start + 1..start + 1 + end].to_string();
            }
        }
        if let Some(pid_start) = proc_part.find("pid=") {
            let pid_part = &proc_part[pid_start + 4..];
            let pid_end = pid_part.find(',').or_else(|| pid_part.find(')'));
            if let Some(pid_end) = pid_end {
                if let Ok(parsed) = pid_part[..pid_end].parse::<u32>() {
                    pid = Some(parsed);
                }
            }
        }
    }

    let remote_is_loopback = is_loopback_host(&remote_host);
    let remote_is_external = !remote_is_loopback && !is_unspecified_host(&remote_host);
    Some(ActiveConnection {
        protocol,
        local_address,
        remote_address,
        state,
        process_name,
        pid,
        local_host,
        local_port,
        remote_host,
        remote_port,
        remote_is_loopback,
        remote_is_external,
    })
}

#[tauri::command]
pub async fn get_active_connections() -> Result<Vec<ActiveConnection>, String> {
    let mut cmd = crate::utils::privilege::tokio::Command::new("ss");
    cmd.args(["-tunp"]);
    cmd.stdout(std::process::Stdio::piped());

    let output = cmd.output().await.map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().skip(1).filter_map(parse_ss_line).collect())
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
    
    let error_output = Command::new("sh")
        .args(["-c", "journalctl -p err -b --no-pager -q | head -n 500 | wc -l"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let error_count = String::from_utf8_lossy(&error_output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    let warning_output = Command::new("sh")
        .args(["-c", "journalctl -p warning -b --no-pager -q | head -n 500 | wc -l"])
        .output()
        .await
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

const CPU_THERMAL_TYPES: &[&str] = &[
    "x86_pkg_temp", "coretemp", "k10temp", "zenpower", "cpu", "package", "tctl", "tdie", "soc",
];

/// Pick a CPU sensor rather than the hottest zone overall: on laptops the
/// Wi-Fi card or an SSD zone is often hotter than the package.
fn select_cpu_temperature(zones: &[(String, f32)]) -> Option<f32> {
    let mut cpu_temp: Option<f32> = None;
    let mut any_temp: Option<f32> = None;

    for (kind, temp) in zones {
        if !temp.is_finite() || *temp < -40.0 || *temp > 200.0 {
            continue;
        }
        any_temp = Some(any_temp.map_or(*temp, |current| current.max(*temp)));
        let kind = kind.to_ascii_lowercase();
        if CPU_THERMAL_TYPES
            .iter()
            .any(|candidate| kind.contains(candidate))
        {
            cpu_temp = Some(cpu_temp.map_or(*temp, |current| current.max(*temp)));
        }
    }

    cpu_temp.or(any_temp)
}

#[tauri::command]
pub async fn get_cpu_temperature() -> Result<Option<f32>, String> {
    let mut zones = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            if !file_name.to_string_lossy().starts_with("thermal_zone") {
                continue;
            }
            let kind = fs::read_to_string(entry.path().join("type"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let Ok(temp_str) = fs::read_to_string(entry.path().join("temp")) else {
                continue;
            };
            let Ok(temp_val) = temp_str.trim().parse::<f32>() else {
                continue;
            };
            zones.push((kind, temp_val / 1000.0));
        }
    }
    Ok(select_cpu_temperature(&zones))
}

#[tauri::command]
pub async fn get_last_system_update() -> Result<String, String> {
    let output = Command::new("sh")
        .args(["-c", "for pkg in dnf5 dnf python3-dnf; do rpm -q --last $pkg 2>/dev/null && break; done | head -1"])
        .output()
        .await
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
    let output = Command::new("sh")
        .args(["-c", "systemctl --failed --no-legend | wc -l"])
        .output()
        .await
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

/// Names of the systemd units that are currently in a failed state.
/// Used to jump straight to the journal entries of the unit that broke.
#[tauri::command]
pub async fn get_failed_services() -> Result<Vec<String>, String> {
    let output = Command::new("systemctl")
        .args(["list-units", "--state=failed", "--no-legend", "--no-pager", "--plain"])
        .output()
        .await
        .map_err(|e| format!("Failed to run systemctl: {e}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let units: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let unit = line.split_whitespace().next()?;
            if unit.is_empty() || unit == "UNIT" {
                None
            } else {
                Some(unit.to_string())
            }
        })
        .collect();

    Ok(units)
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
    crate::utils::root_policy::ensure_openable_path(&path)?;
    Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open directory '{path}': {e}"))?;
    Ok(())
}



#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}




#[cfg(test)]
mod smart_health_tests {
    use super::{parse_smart_health_output, smart_health_failure};

    #[test]
    fn successful_output_is_parsed() {
        let disk = parse_smart_health_output(
            "/dev/sda",
            "Device Model: Test Disk\nSMART overall-health self-assessment test result: PASSED\n",
        )
        .expect("valid SMART output");
        assert_eq!(disk.disk_path, "/dev/sda");
        assert_eq!(disk.model, "Test Disk");
        assert_eq!(disk.health_status, "PASSED");
    }

    #[test]
    fn empty_output_is_a_per_device_error() {
        let error = parse_smart_health_output("/dev/sda", "garbage\n").expect_err("no data");
        assert!(error.contains("No SMART health data"), "{error}");
    }

    #[test]
    fn failures_do_not_leak_command_details() {
        let message = smart_health_failure(
            "lcp-admin: smartctl failed: /usr/sbin/smartctl -H -i /dev/sda: exit 4",
        );
        assert_eq!(message, "SMART data could not be read");
        assert!(!message.contains("smartctl"));
        assert!(!message.contains("/usr/sbin"));
        assert!(!message.contains("/dev/sda"));
    }
}

#[cfg(test)]
mod monitoring_tests {
    use super::{
        can_manage_uid, parse_cpu_line, parse_proc_stat, parse_ss_line, select_cpu_temperature,
        split_endpoint,
    };

    #[test]
    fn proc_stat_names_with_spaces_do_not_shift_the_fields() {
        // Real shape from a process whose command name contains spaces.
        let stat = "11222 (npm run tauri:d) S 11047 11222 11047 34816 11222 4194304 7686 0 52 0 40 8 0 0 20 0 11 0 62635 1160687616 17232 18";
        let parsed = parse_proc_stat(stat).expect("stat line is parsed");
        assert_eq!(parsed.name, "npm run tauri:d");
        assert_eq!(parsed.state, "S");
        assert_eq!(parsed.ppid, 11047);
        assert_eq!(parsed.utime, 40);
        assert_eq!(parsed.stime, 8);
        assert_eq!(parsed.threads, 11);
        assert_eq!(parsed.rss_pages, 17232);

        // A closing parenthesis inside the name must not cut it short.
        let tricky =
            "42 (my ) app) R 1 42 42 0 -1 4194304 0 0 0 0 5 6 0 0 20 0 7 0 1234 5678 99 0";
        let parsed = parse_proc_stat(tricky).expect("paren in name is parsed");
        assert_eq!(parsed.name, "my ) app");
        assert_eq!(parsed.ppid, 1);
        assert_eq!(parsed.utime, 5);
        assert_eq!(parsed.stime, 6);
        assert_eq!(parsed.threads, 7);
        assert_eq!(parsed.rss_pages, 99);
    }

    #[test]
    fn cpu_time_counts_iowait_as_idle_and_ignores_guest_double_count() {
        // user nice system idle iowait irq softirq steal guest guest_nice
        let (idle, total) =
            parse_cpu_line("cpu  100 0 50 800 50 10 10 0 40 0").expect("cpu line");
        assert_eq!(idle, 850);
        // 100 + 0 + 50 + 800 + 50 + 10 + 10 + 0; guest/nice are already in user.
        assert_eq!(total, 1020);
    }

    #[test]
    fn ss_lines_parse_endpoints_and_processes() {
        let loopback = parse_ss_line(
            "tcp ESTAB 0 0 127.0.0.1:38120 127.0.0.1:57123 users:((\"openchamber\",pid=16161,fd=33))",
        )
        .expect("tcp line");
        assert_eq!(loopback.local_host, "127.0.0.1");
        assert_eq!(loopback.local_port, Some(38120));
        assert_eq!(loopback.remote_port, Some(57123));
        assert_eq!(loopback.pid, Some(16161));
        assert_eq!(loopback.process_name, "openchamber");
        assert!(loopback.remote_is_loopback);
        assert!(!loopback.remote_is_external);

        let udp = parse_ss_line("udp ESTAB 0 0 192.168.0.103%wlp1s0:68 192.168.0.1:67")
            .expect("udp line without a process");
        assert_eq!(udp.local_port, Some(68));
        assert_eq!(udp.pid, None);
        assert_eq!(udp.process_name, "N/A");
        assert!(udp.remote_is_external);

        let listen = parse_ss_line("tcp LISTEN 0 128 [::]:22 [::]:* users:((\"sshd\",pid=900,fd=3))")
            .expect("ipv6 listen line");
        assert_eq!(listen.local_host, "::");
        assert_eq!(listen.local_port, Some(22));
        assert_eq!(listen.remote_port, None);
        assert!(!listen.remote_is_external);

        // A short line must be ignored, not index past its end.
        assert!(parse_ss_line("tcp ESTAB 0 0 127.0.0.1:1").is_none());
    }

    #[test]
    fn endpoint_splitting_handles_brackets_and_wildcards() {
        assert_eq!(split_endpoint("[::1]:631"), ("::1".to_string(), Some(631)));
        assert_eq!(split_endpoint("0.0.0.0:*"), ("0.0.0.0".to_string(), None));
        assert_eq!(split_endpoint("*:*"), ("*".to_string(), None));
        assert_eq!(
            split_endpoint("[::ffff:127.0.0.1]:8080"),
            ("::ffff:127.0.0.1".to_string(), Some(8080))
        );
    }

    #[test]
    fn cpu_sensor_is_preferred_over_hotter_non_cpu_zones() {
        let zones = vec![
            ("acpitz".to_string(), 70.0),
            ("x86_pkg_temp".to_string(), 55.0),
            ("B0D4".to_string(), 60.0),
        ];
        assert_eq!(select_cpu_temperature(&zones), Some(55.0));

        let no_cpu_sensor = vec![("acpitz".to_string(), 47.0), ("B0D4".to_string(), 52.0)];
        assert_eq!(select_cpu_temperature(&no_cpu_sensor), Some(52.0));

        let invalid = vec![("x86_pkg_temp".to_string(), -273.0)];
        assert_eq!(select_cpu_temperature(&invalid), None);
    }

    #[test]
    fn only_the_owner_or_root_may_manage_a_process() {
        assert!(can_manage_uid(1000, 1000));
        assert!(!can_manage_uid(1000, 0));
        assert!(can_manage_uid(0, 1000));
    }

}
