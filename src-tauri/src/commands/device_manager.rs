use crate::utils::privilege::ensure_safe_operand;
use crate::utils::privilege::Command;
use serde::Serialize;

/// Run one named SMART helper action and return its stdout. The helper is
/// authoritative; SMART tooling is no longer reachable through the generic
/// privileged program channel.
async fn run_smart_named_action(action: &str, params: &[String]) -> Result<String, String> {
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

#[derive(Debug, Serialize)]
pub struct PciDevice {
    pub slot: String,
    pub class: String,
    pub vendor_device: String,
    pub rev: String,
}

#[derive(Debug, Serialize)]
pub struct UsbDevice {
    pub bus: String,
    pub device: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct HardwareTopology {
    pub pci_devices: Vec<PciDevice>,
    pub usb_devices: Vec<UsbDevice>,
}

#[tauri::command]
pub fn device_get_all() -> Result<String, String> {
    let output = Command::new("lshw")
        .arg("-json")
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn device_get_smart_drives() -> Result<String, String> {
    let output = Command::new("smartctl")
        .args(["--scan", "-j"])
        .output()
        .map_err(|e| e.to_string())?;

    // smartctl scan returns 0 on success
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn device_get_smart_data(device: String) -> Result<String, String> {
    run_smart_named_action("smart_data", &[device]).await
}

#[tauri::command]
pub async fn device_trigger_self_test(device: String, test_type: String) -> Result<String, String> {
    run_smart_named_action("smart_self_test", &[device, test_type]).await
}

#[tauri::command]
pub fn device_get_topology() -> Result<HardwareTopology, String> {
    // 1. Run lspci -D
    let lspci_output = Command::new("lspci")
        .arg("-D")
        .output()
        .map_err(|e| e.to_string())?;

    let lspci_stdout = String::from_utf8_lossy(&lspci_output.stdout);
    let pci_devices = parse_lspci(&lspci_stdout);

    // 2. Run lsusb
    let lsusb_output = Command::new("lsusb")
        .output()
        .map_err(|e| e.to_string())?;

    let lsusb_stdout = String::from_utf8_lossy(&lsusb_output.stdout);
    let usb_devices = parse_lsusb(&lsusb_stdout);

    Ok(HardwareTopology {
        pci_devices,
        usb_devices,
    })
}

fn parse_lspci(output: &str) -> Vec<PciDevice> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(first_space) = line.find(' ') {
            let slot = line[..first_space].to_string();
            let rest = &line[first_space + 1..];
            if let Some(colon) = rest.find(':') {
                let class = rest[..colon].trim().to_string();
                let mut vendor_device = rest[colon + 1..].trim().to_string();
                let mut rev = String::new();
                if let Some(rev_start) = vendor_device.rfind("(rev ") {
                    if vendor_device.ends_with(')') {
                        rev = vendor_device[rev_start..].trim_matches(|c| c == '(' || c == ')').to_string();
                        vendor_device = vendor_device[..rev_start].trim().to_string();
                    }
                }
                devices.push(PciDevice {
                    slot,
                    class,
                    vendor_device,
                    rev,
                });
            }
        }
    }
    devices
}

/// Get the kernel driver/module bound to a PCI slot (e.g. "0000:00:1f.6").
/// Reads /sys/bus/pci/devices/<slot>/driver/module symlink — completely read-only.
#[tauri::command]
pub fn device_get_pci_driver(slot: String) -> Result<String, String> {
    // Normalise: strip any trailing whitespace
    let slot = slot.trim().to_string();
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", slot);
    let module_path = format!("{}/module", driver_path);

    // Try to resolve the module symlink first
    if let Ok(target) = std::fs::read_link(&module_path) {
        let module_name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !module_name.is_empty() {
            return Ok(module_name);
        }
    }

    // Fallback: read the driver symlink itself
    if let Ok(target) = std::fs::read_link(&driver_path) {
        let driver_name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !driver_name.is_empty() {
            return Ok(driver_name);
        }
    }

    Err(format!("No driver bound to {}", slot))
}

/// Unload (rmmod) or reload (modprobe) a kernel module.
/// Requires root — runs via pkexec so the OS shows a polkit prompt.
/// Only "unload" and "reload" are accepted actions (allowlist for safety).
#[tauri::command]
pub fn device_set_module_state(module: String, action: String) -> Result<String, String> {
    // Safety allowlist
    if action != "unload" && action != "reload" {
        return Err(format!("Invalid action '{}'. Only 'unload' or 'reload' are permitted.", action));
    }

    // Refuse to touch critical system modules
    let blocked: &[&str] = &[
        "nvme", "ahci", "sd_mod", "ext4", "xfs", "btrfs",
        "intel_iommu", "amd_iommu", "i915", "nouveau", "amdgpu",
        "usbhid", "hid_generic", "hid", "ehci_hcd", "xhci_hcd",
        "ohci_hcd", "uhci_hcd",
    ];
    let mod_lower = module.to_lowercase();
    for b in blocked {
        if mod_lower == *b {
            return Err(format!(
                "Module '{}' is on the safety blocklist and cannot be managed here. \
                 Use a terminal with care.",
                module
            ));
        }
    }

    // The module name is an operand of modprobe/rmmod — a name that starts
    // with a dash would be read as an option.
    ensure_safe_operand(&module, "kernel module name")?;
    let (cmd, arg) = match action.as_str() {
        "unload" => ("rmmod", module.as_str()),
        _ => ("modprobe", module.as_str()),
    };

    let output = Command::new("pkexec")
        .args([cmd, arg])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("Module '{}' {}ed successfully.", module, action))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn parse_lsusb(output: &str) -> Vec<UsbDevice> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("Bus ") {
            let parts: Vec<&str> = line.splitn(5, ' ').collect();
            if parts.len() >= 5 {
                let bus = parts[1].to_string();
                let device = parts[3].trim_end_matches(':').to_string();
                let rest = parts[4];
                if rest.starts_with("ID ") {
                    let id_parts: Vec<&str> = rest[3..].splitn(2, ' ').collect();
                    if id_parts.len() >= 2 {
                        let id = id_parts[0].to_string();
                        let name = id_parts[1].trim().to_string();
                        devices.push(UsbDevice {
                            bus,
                            device,
                            id,
                            name,
                        });
                    }
                }
            }
        }
    }
    devices
}
