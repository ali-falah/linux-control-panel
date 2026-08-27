use crate::utils::privilege::Command;
use serde::Serialize;

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
pub fn device_get_smart_data(device: String) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args(["smartctl", "-a", "-j", &device])
        .output()
        .map_err(|e| e.to_string())?;

    // smartctl returns non-zero exit codes even on successful run (e.g. if some SMART attributes are warnings)
    // exit status bits 0 and 1 indicate fatal errors, others are just info/warnings
    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout_str.trim().is_empty() && !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(stdout_str)
}

#[tauri::command]
pub fn device_trigger_self_test(device: String, test_type: String) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args(["smartctl", "-t", &test_type, &device])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
