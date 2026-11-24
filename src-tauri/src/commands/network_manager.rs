use crate::utils::privilege::ensure_safe_operand;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::net::IpAddr;
use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};

/// Run one named NetworkManager helper action. The helper re-validates the
/// verb, connection and property shape; writes never use the generic nmcli
/// channel.
async fn run_network_named_action(action: &str, params: &[String]) -> Result<(), String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 1);
    args.push(action.to_string());
    args.extend_from_slice(params);
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, "").await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

/// Same named-action channel, but with a secret supplied on stdin. The secret
/// never appears in the helper's argv (or in any child process argv).
async fn run_network_named_action_with_input(
    action: &str,
    params: &[String],
    input: &str,
) -> Result<(), String> {
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; the action was not executed.".to_string()
    })?;
    let mut args = Vec::with_capacity(params.len() + 1);
    args.push(action.to_string());
    args.extend_from_slice(params);
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, input).await?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if message.contains(&format!("the action '{action}'")) && message.contains("does not") {
            return Err(format!(
                "The installed administrator helper does not support '{action}'. Reinstall the current helper."
            ));
        }
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub async fn network_get_interfaces() -> Result<String, String> {
    let output = crate::utils::privilege::tokio::Command::new("ip")
        .args(["-j", "a"])
        .output()
        .await
        .map_err(|e| format!("Could not run ip: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn network_get_dns() -> Result<String, String> {
    let output = crate::utils::privilege::tokio::Command::new("resolvectl")
        .arg("status")
        .output()
        .await;

    if let Ok(output) = output {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    // Fallback to reading /etc/resolv.conf.
    tokio::fs::read_to_string("/etc/resolv.conf")
        .await
        .map_err(|e| format!("Could not read DNS configuration: {e}"))
}

#[tauri::command]
pub async fn network_list_connections() -> Result<String, String> {
    // Keep nmcli's default escaping ('-c no' would turn a profile named
    // "Corp: US-East" into two fields and shift every following column).
    // The frontend splits on unescaped ':' and unescapes the values.
    let output = crate::utils::privilege::tokio::Command::new("nmcli")
        .args(["-t", "-f", "UUID,NAME,TYPE,DEVICE,STATE", "connection", "show"])
        .output()
        .await
        .map_err(|e| format!("Could not run nmcli: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn network_get_connection(uuid: String) -> Result<String, String> {
    ensure_safe_operand(&uuid, "connection id")?;
    let output = crate::utils::privilege::tokio::Command::new("nmcli")
        .args([
            "-t", "-c", "no", "-f", "ipv4,ipv6,connection", "connection", "show", &uuid,
        ])
        .output()
        .await
        .map_err(|e| format!("Could not run nmcli: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn network_save_connection(
    uuid: String,
    settings: std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let is_new = uuid.is_empty();
    let connection = if is_new {
        settings
            .get("connection.id")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "A connection name (connection.id) is required for a new connection.".to_string()
            })?
    } else {
        ensure_safe_operand(&uuid, "connection id")?;
        uuid.clone()
    };

    let mut params = Vec::with_capacity(settings.len() * 2 + 2);
    params.push(if is_new { "add" } else { "modify" }.to_string());
    params.push(connection);
    for (key, value) in settings {
        // An empty value means clear for an existing nmcli profile. The
        // privileged argv forbids empty strings, so the helper translates a
        // sentinel into nmcli's empty argument. A new profile just skips it.
        if value.is_empty() {
            if is_new {
                continue;
            }
            params.push(key);
            params.push(crate::utils::root_policy::HELPER_CLEAR_VALUE.to_string());
        } else {
            params.push(key);
            params.push(value);
        }
    }
    run_network_named_action("nmcli_connection_save", &params).await?;
    Ok(String::new())
}

/// A connection profile can be named anything, so a UUID-shaped input is not
/// proof that it is not the loopback profile. Resolve the name/device before
/// any destructive action.
async fn connection_is_loopback(uuid: &str) -> bool {
    let output = crate::utils::privilege::tokio::Command::new("nmcli")
        .args(["-t", "-f", "NAME,DEVICE", "connection", "show", uuid])
        .output()
        .await;
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().next() else {
        return false;
    };
    let fields = split_nmcli_fields(line);
    let name = fields.first().map(|value| value.trim().to_lowercase()).unwrap_or_default();
    let device = fields.get(1).map(|value| value.trim().to_lowercase()).unwrap_or_default();
    name == "lo" || name.starts_with("loopback") || device == "lo"
}

/// Store a Wi-Fi PSK (or other supported NM keyfile secret) without putting
/// it in any process argument list.
#[tauri::command]
pub async fn network_store_connection_secret(
    connection: String,
    kind: String,
    secret: String,
) -> Result<(), String> {
    if kind != "wifi-psk" && kind != "vpn-password" {
        return Err(format!("Unsupported secret kind '{kind}'."));
    }
    if connection.trim().is_empty() {
        return Err("A connection name is required.".to_string());
    }
    run_network_named_action_with_input(
        "nm_store_keyfile_secret",
        &[connection, kind],
        &secret,
    )
    .await
}

/// Create a bridge and its first Ethernet slave as one operation. A partial
/// bridge (or a slave without a bridge) is worse than no bridge, so a failure
/// after the bridge was added rolls it back.
#[tauri::command]
pub async fn network_create_bridge(
    name: String,
    bridge_interface: String,
    port_interface: String,
    settings: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    ensure_safe_operand(&name, "bridge profile name")?;
    crate::utils::root_policy::firewall_interface_name_valid(&bridge_interface)?;
    crate::utils::root_policy::firewall_interface_name_valid(&port_interface)?;
    if bridge_interface.eq_ignore_ascii_case("lo")
        || port_interface.eq_ignore_ascii_case("lo")
        || bridge_interface == port_interface
    {
        return Err("A bridge needs a bridge interface and a different physical port.".to_string());
    }

    let mut bridge_params = vec![
        String::from("add"),
        name.clone(),
        String::from("connection.type"),
        String::from("bridge"),
        String::from("connection.id"),
        name.clone(),
        String::from("connection.interface-name"),
        bridge_interface.clone(),
    ];
    for (key, value) in &settings {
        if key == "connection.id" || key == "connection.type" || key == "connection.interface-name" {
            continue;
        }
        if !value.is_empty() {
            bridge_params.push(key.clone());
            bridge_params.push(value.clone());
        }
    }
    run_network_named_action("nmcli_connection_save", &bridge_params).await?;

    let slave_name = format!("{name} port");
    let slave_params = vec![
        String::from("add"),
        slave_name.clone(),
        String::from("connection.id"),
        slave_name,
        String::from("connection.type"),
        String::from("802-3-ethernet"),
        String::from("connection.interface-name"),
        port_interface,
        String::from("connection.master"),
        bridge_interface,
        String::from("connection.slave-type"),
        String::from("bridge"),
        String::from("connection.autoconnect"),
        String::from("yes"),
        String::from("ipv4.method"),
        String::from("disabled"),
        String::from("ipv6.method"),
        String::from("disabled"),
    ];
    if let Err(error) = run_network_named_action("nmcli_connection_save", &slave_params).await {
        // The delete action is UUID-only, so resolve the UUID the bridge just
        // received before rolling it back.
        let rollback = match connection_uuid_by_name(&name).await {
            Some(uuid) => run_network_named_action("nmcli_connection_delete", &[uuid]).await,
            None => Err("its UUID could not be resolved".to_string()),
        };
        return Err(if rollback.is_ok() {
            format!("Failed to add the bridge port; the bridge profile was rolled back: {error}")
        } else {
            format!(
                "Failed to add the bridge port; the bridge profile could not be rolled back: {error}"
            )
        });
    }
    Ok(())
}

/// Resolve the UUID of a connection by its profile name. Used where an action
/// only accepts UUIDs but the caller has just created the profile by name.
async fn connection_uuid_by_name(name: &str) -> Option<String> {
    let output = crate::utils::privilege::tokio::Command::new("nmcli")
        .args(["-t", "-f", "UUID", "connection", "show", name])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|uuid| !uuid.is_empty())
}

#[tauri::command]
pub async fn network_delete_connection(uuid: String) -> Result<String, String> {
    let lower = uuid.trim().to_lowercase();
    if lower == "lo" || lower == "loopback" || connection_is_loopback(&uuid).await {
        return Err("Action blocked: The loopback connection (lo) is essential for system IPC and cannot be deleted.".to_string());
    }
    run_network_named_action("nmcli_connection_delete", &[uuid]).await?;
    Ok(String::new())
}

#[tauri::command]
pub async fn network_up_connection(uuid: String) -> Result<String, String> {
    run_network_named_action("nmcli_connection_up", &[uuid]).await?;
    Ok(String::new())
}

#[tauri::command]
pub async fn network_down_connection(uuid: String) -> Result<String, String> {
    let lower = uuid.trim().to_lowercase();
    if lower == "lo" || lower == "loopback" || connection_is_loopback(&uuid).await {
        return Err("Action blocked: Cannot deactivate loopback interface (lo).".to_string());
    }
    run_network_named_action("nmcli_connection_down", &[uuid]).await?;
    Ok(String::new())
}

#[tauri::command]
pub async fn network_set_interface_state(iface: String, up: bool) -> Result<String, String> {
    let iface_clean = iface.trim().to_lowercase();
    if !up && (iface_clean == "lo" || iface_clean.starts_with("loopback")) {
        return Err("Action blocked: Cannot bring down loopback interface (lo). It is vital for inter-process communication.".to_string());
    }
    run_network_named_action(
        "nmcli_interface_state",
        &[iface, if up { "up" } else { "down" }.to_string()],
    )
    .await?;
    Ok(String::new())
}

#[derive(Debug, serde::Serialize)]
pub struct VpnProfile {
    pub uuid: String,
    pub name: String,
    pub vpn_type: String,
    pub device: String,
    pub active: bool,
}

/// nmcli's terse output escapes the `:` separator and `\` inside field values.
/// Parse it by hand so profile names containing `:` survive the round trip
/// (splitting on a raw ':' silently truncates the name and shifts every
/// following field, which makes VPN profiles disappear from the list).
fn split_nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for c in line.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
}

#[tauri::command]
pub async fn network_get_vpn_profiles() -> Result<Vec<VpnProfile>, String> {
    let output = crate::utils::privilege::tokio::Command::new("nmcli")
        .args(["-t", "-f", "UUID,NAME,TYPE,DEVICE,STATE", "connection", "show"])
        .output()
        .await
        .map_err(|e| format!("Could not run nmcli: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut profiles = Vec::new();
    
    for line in stdout.lines() {
        let parts = split_nmcli_fields(line);
        if parts.len() >= 5 {
            let uuid = parts[0].clone();
            let name = parts[1].clone();
            let conn_type = parts[2].clone();
            let device = parts[3].clone();
            let state = parts[4].clone();
            
            if conn_type == "vpn" || conn_type == "wireguard" {
                let active = !state.trim().is_empty() && state != "--";
                profiles.push(VpnProfile {
                    uuid,
                    name,
                    vpn_type: conn_type,
                    device,
                    active,
                });
            }
        }
    }
    
    Ok(profiles)
}

/// Format one `wireguard.peers` element in nmcli's documented syntax:
/// `<base64-public-key> [attribute=value ...]`. `allowed-ips` is a
/// semicolon-separated list; the commas/spaces used by wg-quick are
/// normalized here. This is not `public-key ...` — the key itself comes first.
fn wireguard_peer_setting(
    public_key: &str,
    endpoint: &str,
    allowed_ips: &str,
    keepalive: &str,
) -> String {
    let mut peer = public_key.trim().to_string();
    if !endpoint.trim().is_empty() {
        peer.push_str(" endpoint=");
        peer.push_str(endpoint.trim());
    }
    let allowed: Vec<&str> = allowed_ips
        .split([',', ' ', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    peer.push_str(" allowed-ips=");
    if allowed.is_empty() {
        peer.push_str("0.0.0.0/0;::/0");
    } else {
        peer.push_str(&allowed.join(";"));
    }
    if !keepalive.trim().is_empty() {
        peer.push_str(" persistent-keepalive=");
        peer.push_str(keepalive.trim());
    }
    peer
}

fn parse_wireguard_conf(content: &str) -> Result<(String, String, Option<String>, Vec<String>), String> {
    let mut private_key = String::new();
    let mut address = String::new();
    let mut dns = None;
    let mut peers = Vec::new();
    
    let mut current_section = "";
    
    let mut peer_pubkey = String::new();
    let mut peer_endpoint = String::new();
    let mut peer_allowed_ips = String::new();
    let mut peer_keepalive = String::new();
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1].to_lowercase();
            if section == "peer" {
                if !peer_pubkey.is_empty() {
                    peers.push(wireguard_peer_setting(
                        &peer_pubkey,
                        &peer_endpoint,
                        &peer_allowed_ips,
                        &peer_keepalive,
                    ));
                    peer_pubkey = String::new();
                    peer_endpoint = String::new();
                    peer_allowed_ips = String::new();
                    peer_keepalive = String::new();
                }
            }
            current_section = if section == "interface" { "interface" } else { "peer" };
            continue;
        }
        
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            match current_section {
                "interface" => {
                    if key == "privatekey" {
                        private_key = val.to_string();
                    } else if key == "address" {
                        address = val.to_string();
                    } else if key == "dns" {
                        dns = Some(val.to_string());
                    }
                }
                "peer" => {
                    if key == "publickey" {
                        peer_pubkey = val.to_string();
                    } else if key == "endpoint" {
                        peer_endpoint = val.to_string();
                    } else if key == "allowedips" {
                        peer_allowed_ips = val.to_string();
                    } else if key == "persistentkeepalive" {
                        peer_keepalive = val.to_string();
                    }
                }
                _ => {}
            }
        }
    }
    
    if !peer_pubkey.is_empty() {
        peers.push(wireguard_peer_setting(
            &peer_pubkey,
            &peer_endpoint,
            &peer_allowed_ips,
            &peer_keepalive,
        ));
    }
    
    if private_key.is_empty() {
        return Err("Missing PrivateKey in [Interface]".to_string());
    }
    
    Ok((private_key, address, dns, peers))
}

/// 32 random bytes, base64 encoded. WireGuard private keys are raw 32-byte
/// values; using /dev/urandom means the helper no longer depends on `wg` being
/// installed and never falls back to a known placeholder key.
fn generate_wireguard_private_key() -> Result<String, String> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|e| format!("Could not generate a WireGuard key: {e}"))?;
    Ok(BASE64.encode(bytes))
}

fn valid_wireguard_key(key: &str) -> bool {
    BASE64
        .decode(key.trim())
        .map(|bytes| bytes.len() == 32)
        .unwrap_or(false)
}

/// Split comma/space separated addresses into IPv4 and IPv6 groups.
fn split_address_families(addresses: &str) -> (Vec<String>, Vec<String>) {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for token in addresses.split([',', ' ']).filter(|value| !value.trim().is_empty()) {
        let token = token.trim().to_string();
        let ip_part = token.split('/').next().unwrap_or(&token);
        match ip_part.parse::<IpAddr>() {
            Ok(IpAddr::V4(_)) => v4.push(token),
            Ok(IpAddr::V6(_)) => v6.push(token),
            Err(_) => {}
        }
    }
    (v4, v6)
}

fn wireguard_address_params(connection: &str, address: &str) -> Result<Vec<String>, String> {
    let (v4, v6) = split_address_families(address);
    if v4.is_empty() && v6.is_empty() {
        return Err(format!("'{address}' is not a valid WireGuard interface address."));
    }

    let mut params = vec![String::from("modify"), connection.to_string()];
    if !v4.is_empty() {
        params.push(String::from("ipv4.addresses"));
        params.push(v4.join(","));
        params.push(String::from("ipv4.method"));
        params.push(String::from("manual"));
    } else {
        params.push(String::from("ipv4.method"));
        params.push(String::from("disabled"));
    }
    if !v6.is_empty() {
        params.push(String::from("ipv6.addresses"));
        params.push(v6.join(","));
        params.push(String::from("ipv6.method"));
        params.push(String::from("manual"));
    } else {
        params.push(String::from("ipv6.method"));
        params.push(String::from("disabled"));
    }
    Ok(params)
}

#[tauri::command]
pub async fn network_import_vpn_profile(name: String, file_path: String) -> Result<String, String> {
    let is_openvpn = file_path.to_lowercase().ends_with(".ovpn");


    if is_openvpn {
        // Use the nmcli_connection_import helper action instead of pkexec.
        // The helper reads the file itself and confines paths to /home, /tmp, /var/tmp, /run_media.
        run_network_named_action(
            "nmcli_connection_import",
            &[String::from("openvpn"), file_path.clone()],
        )
        .await
        .map_err(|e| format!("Failed to import VPN profile: {e}"))?;
        Ok(String::new())
    } else {
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file: {e}"))?;

        let (private_key, address, dns, peers) = parse_wireguard_conf(&content)?;

        let safe_name = if name.trim().is_empty() {
            std::path::Path::new(&file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("wg-imported")
                .to_string()
        } else {
            name
        };

        let ifname = format!("wg-{}", safe_name.to_lowercase().replace(' ', "-"));

        // Step 1: add the wireguard connection via nmcli_connection_save
        run_network_named_action(
            "nmcli_connection_save",
            &[
                String::from("add"),
                safe_name.clone(),
                String::from("connection.type"),
                String::from("wireguard"),
                String::from("connection.id"),
                safe_name.clone(),
                String::from("connection.interface-name"),
                ifname.clone(),
            ],
        )
        .await
        .map_err(|e| format!("Failed to add wireguard connection: {e}"))?;

        // Step 2: configure wireguard settings, splitting IPv4 and IPv6
        // addresses so an IPv6 tunnel is not written into ipv4.addresses.
        let mut settings = wireguard_address_params(&safe_name, &address)?;
        settings.push(String::from("wireguard.private-key"));
        settings.push(private_key.clone());
        run_network_named_action("nmcli_connection_save", &settings)
            .await
            .map_err(|e| format!("Failed to configure wireguard: {e}"))?;

        // Step 3: optional DNS, routed to the matching address family.
        if let Some(dns_servers) = dns {
            let (dns4, dns6) = split_address_families(&dns_servers);
            let mut dns_params = vec![String::from("modify"), safe_name.clone()];
            if !dns4.is_empty() {
                dns_params.push(String::from("ipv4.dns"));
                dns_params.push(dns4.join(","));
            }
            if !dns6.is_empty() {
                dns_params.push(String::from("ipv6.dns"));
                dns_params.push(dns6.join(","));
            }
            if dns_params.len() > 2 {
                run_network_named_action("nmcli_connection_save", &dns_params)
                    .await
                    .map_err(|e| format!("Failed to set wireguard DNS: {e}"))?;
            }
        }

        // Step 4: peers
        for peer in peers {
            run_network_named_action(
                "nmcli_connection_save",
                &[
                    String::from("modify"),
                    safe_name.clone(),
                    String::from("+wireguard.peers"),
                    peer.clone(),
                ],
            )
            .await
            .map_err(|e| format!("Failed to add wireguard peer: {e}"))?;
        }

        Ok(format!("WireGuard connection '{}' successfully imported.", safe_name))
    }
}

#[tauri::command]
pub async fn network_create_vpn_profile(
    name: String,
    vpn_type: String,
    gateway: String,
    username: Option<String>,
    password: Option<String>,
    peer_public_key: Option<String>,
    address: Option<String>,
    ca_cert: Option<String>,
    client_cert: Option<String>,
    client_key: Option<String>,
) -> Result<String, String> {
    if vpn_type == "openvpn" {
        let con_name = if name.trim().is_empty() { "vpn-manual" } else { &name };

        run_network_named_action(
            "nmcli_connection_save",
            &[
                String::from("add"),
                con_name.to_string(),
                String::from("connection.type"),
                String::from("vpn"),
                String::from("vpn.service-type"),
                String::from("org.freedesktop.NetworkManager.openvpn"),
                String::from("connection.id"),
                con_name.to_string(),
                String::from("connection.interface-name"),
                String::from("*"),
            ],
        )
        .await
        .map_err(|e| format!("Failed to create openvpn connection: {e}"))?;

        let user_str = username.unwrap_or_default();
        let data_setting = format!("gateway={}, username={}", gateway, user_str);
        run_network_named_action(
            "nmcli_connection_save",
            &[
                String::from("modify"),
                con_name.to_string(),
                String::from("vpn.data"),
                data_setting,
            ],
        )
        .await
        .map_err(|e| format!("Failed to configure openvpn parameters: {e}"))?;

        // Store the password in NetworkManager's root-only keyfile through
        // stdin. Putting it in vpn.secrets argv would expose it to `ps`.
        if let Some(pass) = password.filter(|value| !value.is_empty()) {
            run_network_named_action_with_input(
                "nm_store_keyfile_secret",
                &[con_name.to_string(), String::from("vpn-password")],
                &pass,
            )
            .await
            .map_err(|e| format!("Failed to store openvpn credentials: {e}"))?;
        }

        let mut cert_params = vec![String::from("modify"), con_name.to_string()];
        for (key, value) in [("vpn.ca", &ca_cert), ("vpn.cert", &client_cert), ("vpn.key", &client_key)] {
            if let Some(path) = value.as_ref().filter(|path| !path.trim().is_empty()) {
                cert_params.push(key.to_string());
                cert_params.push(path.trim().to_string());
            }
        }
        if cert_params.len() > 2 {
            run_network_named_action("nmcli_connection_save", &cert_params)
                .await
                .map_err(|e| format!("Failed to store openvpn certificates: {e}"))?;
        }

        Ok(format!("OpenVPN connection '{}' created.", con_name))
    } else {
        let con_name = if name.trim().is_empty() { "wg-manual" } else { &name };
        let ifname = format!("wg-{}", con_name.to_lowercase().replace(' ', "-"));

        // A WireGuard peer is unusable without the server's public key
        let peer_key = peer_public_key
            .as_ref()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .ok_or_else(|| "A peer public key is required for WireGuard profiles.".to_string())?;
        if !valid_wireguard_key(&peer_key) {
            return Err(
                "The WireGuard peer public key must be a base64-encoded 32-byte key.".to_string(),
            );
        }
        let address = address
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "A WireGuard interface address is required (for example 10.8.0.2/32)."
                    .to_string()
            })?;

        run_network_named_action(
            "nmcli_connection_save",
            &[
                String::from("add"),
                con_name.to_string(),
                String::from("connection.type"),
                String::from("wireguard"),
                String::from("connection.id"),
                con_name.to_string(),
                String::from("connection.interface-name"),
                ifname.clone(),
            ],
        )
        .await
        .map_err(|e| format!("Failed to create wireguard connection: {e}"))?;

        let private_key = generate_wireguard_private_key()?;
        let mut params = wireguard_address_params(con_name, &address)?;
        params.push(String::from("wireguard.private-key"));
        params.push(private_key);
        run_network_named_action("nmcli_connection_save", &params)
            .await
            .map_err(|e| format!("Failed to configure wireguard credentials: {e}"))?;

        if !gateway.is_empty() {
            let (v4, v6) = split_address_families(&address);
            let allowed_ips = match (v4.is_empty(), v6.is_empty()) {
                (false, true) => "0.0.0.0/0",
                (true, false) => "::/0",
                _ => "0.0.0.0/0;::/0",
            };
            let peer_setting = format!("{peer_key} endpoint={gateway} allowed-ips={allowed_ips}");
            run_network_named_action(
                "nmcli_connection_save",
                &[
                    String::from("modify"),
                    con_name.to_string(),
                    String::from("+wireguard.peers"),
                    peer_setting,
                ],
            )
            .await
            .map_err(|e| format!("Failed to add wireguard peer: {e}"))?;
        }

        Ok(format!("WireGuard connection '{}' created.", con_name))
    }
}

#[tauri::command]
pub async fn network_test_ping() -> Result<f64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
        
    let mut total_ms = 0.0;
    let iterations = 3;
    
    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let res = client.head("https://speed.cloudflare.com/")
            .send()
            .await;
            
        match res {
            Ok(_) => {
                total_ms += start.elapsed().as_secs_f64() * 1000.0;
            }
            Err(e) => {
                return Err(format!("Ping request failed: {e}"));
            }
        }
    }
    
    Ok(total_ms / (iterations as f64))
}

#[tauri::command]
pub async fn network_test_download() -> Result<f64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
        
    let start = std::time::Instant::now();
    let res = client.get("https://speed.cloudflare.com/__down?bytes=5000000")
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;
        
    if !res.status().is_success() {
        return Err(format!("Download request returned status: {}", res.status()));
    }
    
    let bytes = res.bytes()
        .await
        .map_err(|e| format!("Failed to read download bytes: {e}"))?;
        
    let duration = start.elapsed().as_secs_f64();
    if duration == 0.0 {
        return Ok(0.0);
    }
    
    let size_bytes = bytes.len() as f64;
    let speed_mbps = (size_bytes * 8.0) / (1_000_000.0 * duration);
    Ok(speed_mbps)
}

#[tauri::command]
pub async fn network_test_upload() -> Result<f64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
        
    let size_bytes = 2_000_000;
    let data = vec![0u8; size_bytes];
    
    let start = std::time::Instant::now();
    let res = client.post("https://speed.cloudflare.com/__up")
        .body(data)
        .send()
        .await
        .map_err(|e| format!("Upload request failed: {e}"))?;
        
    if !res.status().is_success() {
        return Err(format!("Upload request returned status: {}", res.status()));
    }
    
    let duration = start.elapsed().as_secs_f64();
    if duration == 0.0 {
        return Ok(0.0);
    }
    
    let speed_mbps = ((size_bytes as f64) * 8.0) / (1_000_000.0 * duration);
    Ok(speed_mbps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nmcli_terse_fields_keep_escaped_colons_and_backslashes() {
        let fields = split_nmcli_fields(r"uuid:Corp\: US-East:802-3-ethernet:eth0:activated");
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[1], "Corp: US-East");
        assert_eq!(fields[2], "802-3-ethernet");

        let fields = split_nmcli_fields(r"a:path\\name:b");
        assert_eq!(fields[1], r"path\name");
    }

    #[test]
    fn wireguard_config_parses_ipv4_ipv6_addresses_and_peers() {
        let config = r#"
[Interface]
PrivateKey = private-key
Address = 10.8.0.2/32, fd00::2/128
DNS = 1.1.1.1, 2606:4700:4700::1111

[Peer]
PublicKey = peer-key
Endpoint = vpn.example.com:51820
AllowedIPs = 0.0.0.0/0, ::/0
"#;
        let (private_key, address, dns, peers) = parse_wireguard_conf(config).unwrap();
        assert_eq!(private_key, "private-key");
        assert_eq!(address, "10.8.0.2/32, fd00::2/128");
        assert_eq!(dns.as_deref(), Some("1.1.1.1, 2606:4700:4700::1111"));
        assert_eq!(peers.len(), 1);
        assert!(peers[0].starts_with("peer-key "), "{}", peers[0]);
        assert!(peers[0].contains("endpoint=vpn.example.com:51820"), "{}", peers[0]);
        assert!(peers[0].contains("allowed-ips=0.0.0.0/0;::/0"), "{}", peers[0]);
    }

    #[test]
    fn address_families_are_split_for_nmcli() {
        let (v4, v6) = split_address_families("10.8.0.2/32, fd00::2/128");
        assert_eq!(v4, vec!["10.8.0.2/32"]);
        assert_eq!(v6, vec!["fd00::2/128"]);
    }

    #[test]
    fn generated_wireguard_keys_are_valid_base64_32_byte_values() {
        let key = generate_wireguard_private_key().expect("generate key");
        assert_eq!(key.len(), 44);
        assert!(valid_wireguard_key(&key));
        assert!(!valid_wireguard_key("too-short"));
    }

    #[test]
    fn nmcli_wireguard_peers_use_the_documented_attribute_syntax() {
        let peer = wireguard_peer_setting(
            "peer-key",
            "vpn.example.com:51820",
            "10.0.0.0/8, fd00::/64",
            "25",
        );
        assert_eq!(
            peer,
            "peer-key endpoint=vpn.example.com:51820 \
             allowed-ips=10.0.0.0/8;fd00::/64 persistent-keepalive=25"
        );

        // No endpoint is valid; a missing AllowedIPs defaults to a full tunnel.
        assert_eq!(
            wireguard_peer_setting("peer-key", "", "", ""),
            "peer-key allowed-ips=0.0.0.0/0;::/0"
        );
    }
}
