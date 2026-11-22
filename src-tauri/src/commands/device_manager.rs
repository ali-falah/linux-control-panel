use crate::utils::privilege::ensure_safe_operand;
use crate::utils::root_policy::{is_module_name, pci_slot_valid};
use crate::utils::privilege::{run_privileged_output, RootAuth};
use serde::Serialize;

/// Run a read-only inspection tool and turn "binary missing" into an
/// actionable message instead of a bare OS error.
async fn run_read_tool(tool: &str, package: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = crate::utils::privilege::tokio::Command::new(tool);
    for arg in args {
        cmd.arg(*arg);
    }
    let output = ::tokio::time::timeout(
        std::time::Duration::from_secs(20),
        cmd.output(),
    )
    .await
    .map_err(|_| format!("{tool} did not finish within 20 seconds."))?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{tool} is not installed. Install the '{package}' package and try again.")
        } else {
            format!("Could not run {tool}: {e}")
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(if detail.is_empty() {
            format!("{tool} failed with exit status {}", output.status)
        } else {
            format!("{tool} failed: {detail}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run one named helper action and return its stdout. The helper is
/// authoritative and validates every argument; SMART/inventory/PCI requests
/// never reach a generic privileged program channel.
async fn run_named_helper_action(action: &str, params: &[String]) -> Result<String, String> {
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
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let outdated = message.contains(&format!("program '{action}' was not found"))
            || (action == "smart_data"
                && params.len() > 1
                && message.contains("takes exactly one device path"));
        if outdated {
            return Err(format!(
                "The installed administrator helper does not support '{action}' yet. \
                 Reinstall the current lcp-admin helper (or the Control Panel package) and try again."
            ));
        }
        return Err(message);
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
pub async fn device_get_all() -> Result<String, String> {
    run_read_tool("lshw", "lshw", &["-json"]).await
}

/// Detailed inventory through the root helper: serial numbers, capabilities
/// and other fields unprivileged lshw withholds.
#[tauri::command]
pub async fn device_get_all_detailed() -> Result<String, String> {
    run_named_helper_action("inventory_scan", &[]).await
}

#[tauri::command]
pub async fn device_get_smart_drives() -> Result<String, String> {
    run_read_tool("smartctl", "smartmontools", &["--scan", "-j"]).await
}

#[tauri::command]
pub async fn device_get_smart_data(
    device: String,
    device_type: Option<String>,
) -> Result<String, String> {
    let mut params = vec![device];
    if let Some(device_type) = device_type {
        params.push(device_type);
    }
    run_named_helper_action("smart_data", &params).await
}

#[tauri::command]
pub async fn device_trigger_self_test(device: String, test_type: String) -> Result<String, String> {
    run_named_helper_action("smart_self_test", &[device, test_type]).await
}

#[tauri::command]
pub async fn device_get_topology() -> Result<HardwareTopology, String> {
    let (lspci, lsusb) = ::tokio::join!(
        run_read_tool("lspci", "pciutils", &["-D"]),
        run_read_tool("lsusb", "usbutils", &[])
    );
    let lspci_stdout = lspci?;
    let lsusb_stdout = lsusb?;

    Ok(HardwareTopology {
        pci_devices: parse_lspci(&lspci_stdout),
        usb_devices: parse_lsusb(&lsusb_stdout),
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
    pci_slot_valid(&slot)?;
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

/// Reset one PCI function's driver by unbinding and rebinding it. This is the
/// device-level reset that works while the module is still in use.
#[tauri::command]
pub async fn device_reset_pci_driver(slot: String) -> Result<String, String> {
    pci_slot_valid(&slot)?;
    run_named_helper_action("pci_reset", &[slot]).await
}

/// Unload (rmmod) or reload (rmmod + modprobe) a kernel module.
/// Requires root and honours the app's admin-mode gate; only "unload" and
/// "reload" are accepted actions (allowlist for safety).
#[tauri::command]
pub async fn device_set_module_state(module: String, action: String) -> Result<String, String> {
    if action != "unload" && action != "reload" {
        return Err(format!("Invalid action '{}'. Only 'unload' or 'reload' are permitted.", action));
    }

    // Refuse to touch modules that carry the running system's storage, display,
    // or input. Keep this in sync with what the UI warns about.
    let blocked: &[&str] = &[
        "nvme", "ahci", "sd_mod", "ext4", "xfs", "btrfs", "dm_mod", "md_mod",
        "dm_crypt", "dm_thin_pool", "dm_snapshot", "virtio_blk", "virtio_scsi", "pcieport",
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

    // Validate before it is used as a sysfs path or an rmmod operand.
    if !is_module_name(&module) {
        return Err(format!(
            "Invalid kernel module name '{module}'. Use letters, digits, '_' or '-'."
        ));
    }
    ensure_safe_operand(&module, "kernel module name")?;

    // rmmod refuses a module that is still referenced. Checking first avoids a
    // pointless administrator prompt and explains why instead of echoing
    // "Module is in use". A driver bound to a device always has a reference.
    let sysfs_name = module.replace('-', "_");
    if let Ok(refcount) = std::fs::read_to_string(format!("/sys/module/{sysfs_name}/refcnt")) {
        if refcount.trim() != "0" {
            return Err(format!(
                "Module '{module}' is in use (refcount {}). Detach the device or stop the users \
                 of this driver before unloading it.",
                refcount.trim()
            ));
        }
    }

    let args = [module.clone()];

    // A reload is an unload followed by a load: plain `modprobe <module>` is a
    // no-op while the module is already loaded, so it never reloaded anything.
    let unload = run_privileged_output("rmmod", &args, &RootAuth::Polkit).await?;
    if !unload.status.success() {
        let stderr = String::from_utf8_lossy(&unload.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("Could not unload module '{module}'.")
        } else {
            stderr
        });
    }

    if action == "unload" {
        return Ok(format!("Module '{module}' unloaded successfully."));
    }

    let load = run_privileged_output("modprobe", &args, &RootAuth::Polkit).await?;
    if !load.status.success() {
        let stderr = String::from_utf8_lossy(&load.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!(
                "Module '{module}' was unloaded but could not be loaded again. \
                 Run 'modprobe {module}' in a terminal to restore the device."
            )
        } else {
            format!(
                "Module '{module}' was unloaded but could not be loaded again: {stderr}. \
                 Run 'modprobe {module}' in a terminal to restore the device."
            )
        });
    }

    Ok(format!("Module '{module}' reloaded successfully."))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pci_addresses_classes_and_revisions() {
        let output = "\
0000:00:00.0 Host bridge: Intel Corporation 8th Gen Core Processor Host Bridge/DRAM Registers (rev 07)
0000:00:1f.6 Ethernet controller: Intel Corporation Ethernet Connection (7) I219-V (rev 10)
0000:00:02.0 VGA compatible controller: Intel Corporation UHD Graphics 630 (Desktop) (rev 02)
";
        let devices = parse_lspci(output);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[1].slot, "0000:00:1f.6");
        assert_eq!(devices[1].class, "Ethernet controller");
        assert_eq!(devices[1].rev, "rev 10");
        assert_eq!(
            devices[1].vendor_device,
            "Intel Corporation Ethernet Connection (7) I219-V"
        );
        // A parenthesised model name before the revision must survive.
        assert_eq!(
            devices[2].vendor_device,
            "Intel Corporation UHD Graphics 630 (Desktop)"
        );
        assert_eq!(devices[2].rev, "rev 02");
    }

    #[test]
    fn ignores_malformed_pci_lines() {
        let devices = parse_lspci("not a pci line\n\n0000:00:1f.6 Ethernet controller: Vendor Device\n");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].rev, "");
    }

    #[test]
    fn parses_usb_bus_device_id_and_name() {
        let output = "\
Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub
Bus 001 Device 002: ID 8087:0024 Intel Corp. Integrated Rate Matching Hub
Bus 002 Device 003: ID 046d:c52b Logitech, Inc. Unifying Receiver
";
        let devices = parse_lsusb(output);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].bus, "001");
        assert_eq!(devices[0].device, "001");
        assert_eq!(devices[0].id, "1d6b:0002");
        assert_eq!(devices[0].name, "Linux Foundation 2.0 root hub");
        assert_eq!(devices[2].bus, "002");
        assert_eq!(devices[2].id, "046d:c52b");
    }

    #[test]
    fn accepts_only_pci_addresses_for_sysfs_lookups() {
        assert!(pci_slot_valid("0000:00:1f.6").is_ok());
        assert!(pci_slot_valid("0000:3b:00.0").is_ok());
        assert!(pci_slot_valid("").is_err());
        assert!(pci_slot_valid("../../etc/shadow").is_err());
        assert!(pci_slot_valid("0000:00:1f.6/../../x").is_err());
        assert!(pci_slot_valid("00000000000000000").is_err());
    }

    #[test]
    fn ignores_malformed_usb_lines() {
        let devices = parse_lsusb("Bus 001 Device 002: missing id\n\nrandom text\n");
        assert!(devices.is_empty());
    }
}
