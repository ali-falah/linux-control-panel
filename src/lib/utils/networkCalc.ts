/**
 * Network Calculation & Subnet Validation Engine
 * Provides IPv4/IPv6 address parsing, CIDR subnet calculations,
 * real-time error detection, gateway reachability checks, and DNS validation.
 */

export interface Ipv4ValidationResult {
  raw: string;
  ip: string;
  cidr: number;
  maskStr: string;
  wildcardStr: string;
  networkIp: string;
  broadcastIp: string;
  firstUsableIp: string;
  lastUsableIp: string;
  totalHosts: number;
  usableHosts: number;
  scope: string; // e.g. "RFC 1918 Private (Class C)", "Loopback", "Public", etc.
  isNetworkAddress: boolean;
  isBroadcastAddress: boolean;
  isLoopback: boolean;
  isLinkLocal: boolean;
  isMulticast: boolean;
  isValid: boolean;
  errors: string[];
  warnings: string[];
}

export interface GatewayValidationResult {
  raw: string;
  ip: string;
  isValid: boolean;
  inSameSubnet: boolean;
  isConflictWithHost: boolean;
  isNetworkOrBroadcast: boolean;
  suggestedGateway: string;
  errors: string[];
  warnings: string[];
}

export interface DnsValidationResult {
  raw: string;
  servers: { ip: string; isValid: boolean; error?: string }[];
  isValid: boolean;
  errors: string[];
}

export interface Ipv6ValidationResult {
  raw: string;
  ip: string;
  prefix: number;
  gateway: string;
  isValid: boolean;
  errors: string[];
}

/**
 * Converts dotted-decimal IPv4 string to 32-bit unsigned integer.
 */
export function ipToNumber(ip: string): number | null {
  const octets = ip.trim().split('.');
  if (octets.length !== 4) return null;
  let num = 0;
  for (let i = 0; i < 4; i++) {
    const o = octets[i].trim();
    if (!/^\d+$/.test(o)) return null;
    const val = parseInt(o, 10);
    if (val < 0 || val > 255) return null;
    if (o.length > 1 && o.startsWith('0')) return null; // Disallow octal format like 010
    num = ((num << 8) + val) >>> 0;
  }
  return num >>> 0;
}

/**
 * Converts 32-bit unsigned integer to dotted-decimal IPv4 string.
 */
export function numberToIp(num: number): string {
  return [
    (num >>> 24) & 255,
    (num >>> 16) & 255,
    (num >>> 8) & 255,
    num & 255
  ].join('.');
}

/**
 * Converts CIDR prefix (0-32) to 32-bit mask integer.
 */
export function cidrToMaskNumber(cidr: number): number {
  if (cidr <= 0) return 0;
  if (cidr >= 32) return 0xFFFFFFFF >>> 0;
  return ((0xFFFFFFFF << (32 - cidr)) >>> 0);
}

/**
 * Converts CIDR prefix (0-32) to dotted-decimal subnet mask string.
 */
export function cidrToSubnetMask(cidr: number): string {
  return numberToIp(cidrToMaskNumber(cidr));
}

/**
 * Converts dotted-decimal subnet mask string to CIDR prefix (0-32).
 * Returns null if mask is non-contiguous or invalid.
 */
export function subnetMaskToCidr(mask: string): number | null {
  const num = ipToNumber(mask);
  if (num === null) return null;
  if (num === 0) return 0;
  const bin = (num >>> 0).toString(2).padStart(32, '0');
  const firstZero = bin.indexOf('0');
  if (firstZero === -1) return 32;
  if (bin.slice(firstZero).includes('1')) return null; // Non-contiguous
  return firstZero;
}

/**
 * Identifies the IP classification / scope (RFC 1918, Loopback, Link-Local, Multicast, Public).
 */
export function getIpv4Scope(num: number): string {
  const o1 = (num >>> 24) & 255;
  const o2 = (num >>> 16) & 255;

  if (o1 === 10) return 'RFC 1918 Private (Class A 10.0.0.0/8)';
  if (o1 === 172 && o2 >= 16 && o2 <= 31) return 'RFC 1918 Private (Class B 172.16.0.0/12)';
  if (o1 === 192 && o2 === 168) return 'RFC 1918 Private (Class C 192.168.0.0/16)';
  if (o1 === 127) return 'Loopback Interface (127.0.0.0/8)';
  if (o1 === 169 && o2 === 254) return 'Link-Local APIPA (169.254.0.0/16)';
  if (o1 === 100 && o2 >= 64 && o2 <= 127) return 'Carrier-Grade NAT (100.64.0.0/10)';
  if (o1 >= 224 && o1 <= 239) return 'Multicast Group (224.0.0.0/4)';
  if (o1 >= 240) return 'Reserved / Experimental (240.0.0.0/4)';
  return 'Global Public IPv4';
}

/**
 * Comprehensive real-time IPv4 validator and subnet calculator.
 */
export function validateIpv4WithCidr(rawInput: string): Ipv4ValidationResult {
  const raw = (rawInput || '').trim();
  const errors: string[] = [];
  const warnings: string[] = [];

  const defaultResult: Ipv4ValidationResult = {
    raw,
    ip: '',
    cidr: 24,
    maskStr: '255.255.255.0',
    wildcardStr: '0.0.0.255',
    networkIp: '',
    broadcastIp: '',
    firstUsableIp: '',
    lastUsableIp: '',
    totalHosts: 0,
    usableHosts: 0,
    scope: '',
    isNetworkAddress: false,
    isBroadcastAddress: false,
    isLoopback: false,
    isLinkLocal: false,
    isMulticast: false,
    isValid: false,
    errors: [],
    warnings: []
  };

  if (!raw) {
    return {
      ...defaultResult,
      errors: ['IP address is required. Format: 192.168.1.10/24']
    };
  }

  // Check CIDR slash
  const parts = raw.split('/');
  if (parts.length === 1) {
    // Check if user entered just an IP without /CIDR
    const baseIpNum = ipToNumber(parts[0]);
    if (baseIpNum !== null) {
      return {
        ...defaultResult,
        ip: parts[0],
        errors: [`Missing subnet prefix. Did you mean ${parts[0]}/24 (255.255.255.0)?`]
      };
    }
    return {
      ...defaultResult,
      errors: ['Invalid IP format. Must specify CIDR prefix (e.g. 192.168.1.10/24).']
    };
  }

  if (parts.length > 2) {
    return {
      ...defaultResult,
      errors: ['Invalid format: multiple slashes found. Example: 192.168.1.10/24']
    };
  }

  const ipStr = parts[0].trim();
  const cidrStr = parts[1].trim();

  // Validate CIDR prefix
  if (!/^\d+$/.test(cidrStr)) {
    errors.push(`Subnet prefix '/${cidrStr}' must be a number between 1 and 32.`);
  }

  const cidr = parseInt(cidrStr, 10);
  if (isNaN(cidr) || cidr < 1 || cidr > 32) {
    errors.push(`Subnet prefix '/${cidrStr}' is out of range. Allowed: /1 to /32.`);
  }

  // Validate IP octets
  const octets = ipStr.split('.');
  if (octets.length !== 4) {
    errors.push(`IPv4 address must contain exactly 4 octets separated by dots (found ${octets.length}).`);
  } else {
    octets.forEach((o, idx) => {
      const trimmed = o.trim();
      if (!/^\d+$/.test(trimmed)) {
        errors.push(`Octet ${idx + 1} ('${o}') is not a valid number.`);
      } else {
        const val = parseInt(trimmed, 10);
        if (val < 0 || val > 255) {
          errors.push(`Octet ${idx + 1} (${val}) exceeds allowable range 0-255.`);
        }
        if (trimmed.length > 1 && trimmed.startsWith('0')) {
          errors.push(`Octet ${idx + 1} ('${trimmed}') has invalid leading zero.`);
        }
      }
    });
  }

  const ipNum = ipToNumber(ipStr);
  if (ipNum === null || errors.length > 0) {
    return {
      ...defaultResult,
      ip: ipStr,
      cidr: isNaN(cidr) ? 24 : cidr,
      errors
    };
  }

  // Subnet calculations
  const maskNum = cidrToMaskNumber(cidr);
  const wildcardNum = (~maskNum) >>> 0;
  const networkNum = (ipNum & maskNum) >>> 0;
  const broadcastNum = (networkNum | wildcardNum) >>> 0;

  const maskStr = numberToIp(maskNum);
  const wildcardStr = numberToIp(wildcardNum);
  const networkIp = numberToIp(networkNum);
  const broadcastIp = numberToIp(broadcastNum);

  const totalHosts = Math.pow(2, 32 - cidr);
  let usableHosts = totalHosts - 2;
  let firstUsableNum = networkNum + 1;
  let lastUsableNum = broadcastNum - 1;

  if (cidr === 31) {
    // RFC 3021 Point-to-Point links: 2 usable host addresses
    usableHosts = 2;
    firstUsableNum = networkNum;
    lastUsableNum = broadcastNum;
  } else if (cidr === 32) {
    // Single host route
    usableHosts = 1;
    firstUsableNum = networkNum;
    lastUsableNum = networkNum;
  } else if (usableHosts < 0) {
    usableHosts = 0;
  }

  const firstUsableIp = numberToIp(firstUsableNum);
  const lastUsableIp = numberToIp(lastUsableNum);
  const scope = getIpv4Scope(ipNum);

  const isLoopback = ((ipNum >>> 24) & 255) === 127;
  const isLinkLocal = ((ipNum >>> 24) & 255) === 169 && ((ipNum >>> 16) & 255) === 254;
  const isMulticast = ((ipNum >>> 24) & 255) >= 224 && ((ipNum >>> 24) & 255) <= 239;
  const isNetworkAddress = cidr < 31 && ipNum === networkNum;
  const isBroadcastAddress = cidr < 31 && ipNum === broadcastNum;

  if (isLoopback) {
    errors.push('Loopback addresses (127.0.0.0/8) cannot be configured on a physical or virtual network interface.');
  }

  if (isMulticast) {
    errors.push('Multicast addresses (224.0.0.0/4) are reserved for multicast groups and cannot be used as host IPs.');
  }

  if (isNetworkAddress) {
    errors.push(`${ipStr} is the Network Identifier (Subnet ID). It cannot be assigned to an interface.`);
  }

  if (isBroadcastAddress) {
    errors.push(`${ipStr} is the Broadcast Address for this /${cidr} subnet. It cannot be assigned to an interface.`);
  }

  if (isLinkLocal) {
    warnings.push('169.254.x.x is a Link-Local (APIPA) address range used when DHCP fails.');
  }

  if (cidr === 32) {
    warnings.push('/32 designates a single host route with no broadcast or other usable addresses on the wire.');
  }

  return {
    raw,
    ip: ipStr,
    cidr,
    maskStr,
    wildcardStr,
    networkIp,
    broadcastIp,
    firstUsableIp,
    lastUsableIp,
    totalHosts,
    usableHosts,
    scope,
    isNetworkAddress,
    isBroadcastAddress,
    isLoopback,
    isLinkLocal,
    isMulticast,
    isValid: errors.length === 0,
    errors,
    warnings
  };
}

/**
 * Validates Gateway against the calculated host IP and Subnet.
 */
export function validateGateway(gwRaw: string, ipCalc: Ipv4ValidationResult): GatewayValidationResult {
  const raw = (gwRaw || '').trim();
  const errors: string[] = [];
  const warnings: string[] = [];

  const defaultGwResult: GatewayValidationResult = {
    raw,
    ip: raw,
    isValid: true,
    inSameSubnet: true,
    isConflictWithHost: false,
    isNetworkOrBroadcast: false,
    suggestedGateway: ipCalc.firstUsableIp || '',
    errors: [],
    warnings: []
  };

  // Gateway is optional for isolated networks
  if (!raw) {
    return defaultGwResult;
  }

  const gwNum = ipToNumber(raw);
  if (gwNum === null) {
    return {
      ...defaultGwResult,
      isValid: false,
      errors: [`Gateway '${raw}' is not a valid IPv4 address.`]
    };
  }

  if (!ipCalc.isValid) {
    // If host IP is not valid yet, we can only validate syntax of gateway
    return {
      ...defaultGwResult,
      isValid: true
    };
  }

  const hostIpNum = ipToNumber(ipCalc.ip);
  const maskNum = cidrToMaskNumber(ipCalc.cidr);
  const networkNum = (hostIpNum! & maskNum) >>> 0;
  const broadcastNum = (networkNum | (~maskNum >>> 0)) >>> 0;

  const gwNetworkNum = (gwNum & maskNum) >>> 0;
  const inSameSubnet = gwNetworkNum === networkNum;
  const isConflictWithHost = gwNum === hostIpNum;
  const isNetworkOrBroadcast = ipCalc.cidr < 31 && (gwNum === networkNum || gwNum === broadcastNum);

  if (!inSameSubnet) {
    errors.push(
      `Subnet Mismatch: Gateway ${raw} is OUT OF SUBNET (${ipCalc.networkIp}/${ipCalc.cidr}). Expected range: ${ipCalc.firstUsableIp} – ${ipCalc.lastUsableIp}.`
    );
  }

  if (isConflictWithHost) {
    errors.push(`Gateway IP (${raw}) cannot be identical to the host interface IP (${ipCalc.ip}).`);
  }

  if (isNetworkOrBroadcast) {
    errors.push(`Gateway (${raw}) cannot be the Network ID or Broadcast address.`);
  }

  // Suggest common standard gateways (.1 or .254)
  let suggested = ipCalc.firstUsableIp;
  if (suggested === ipCalc.ip && ipCalc.lastUsableIp) {
    suggested = ipCalc.lastUsableIp;
  }

  return {
    raw,
    ip: raw,
    isValid: errors.length === 0,
    inSameSubnet,
    isConflictWithHost,
    isNetworkOrBroadcast,
    suggestedGateway: suggested,
    errors,
    warnings
  };
}

/**
 * Validates comma/space-separated DNS servers.
 */
export function validateDnsServers(dnsRaw: string): DnsValidationResult {
  const raw = (dnsRaw || '').trim();
  if (!raw) {
    return { raw, servers: [], isValid: true, errors: [] };
  }

  const items = raw.split(/[, ]+/).filter(Boolean);
  const servers: { ip: string; isValid: boolean; error?: string }[] = [];
  const errors: string[] = [];
  const seen = new Set<string>();

  for (const item of items) {
    const trimmed = item.trim();
    const num = ipToNumber(trimmed);
    if (num === null) {
      const err = `'${trimmed}' is not a valid IPv4 address`;
      servers.push({ ip: trimmed, isValid: false, error: err });
      errors.push(err);
    } else {
      if (seen.has(trimmed)) {
        const err = `Duplicate DNS server '${trimmed}'`;
        servers.push({ ip: trimmed, isValid: false, error: err });
        errors.push(err);
      } else {
        seen.add(trimmed);
        servers.push({ ip: trimmed, isValid: true });
      }
    }
  }

  return {
    raw,
    servers,
    isValid: errors.length === 0,
    errors
  };
}

/**
 * Validates IPv6 Address with prefix length and Gateway.
 */
export function validateIpv6Address(ipWithPrefix: string, gatewayStr: string = ''): Ipv6ValidationResult {
  const raw = (ipWithPrefix || '').trim();
  const errors: string[] = [];

  if (!raw) {
    return { raw, ip: '', prefix: 64, gateway: gatewayStr, isValid: true, errors: [] };
  }

  const parts = raw.split('/');
  if (parts.length !== 2) {
    return {
      raw,
      ip: raw,
      prefix: 64,
      gateway: gatewayStr,
      isValid: false,
      errors: ['IPv6 must include prefix length, e.g. 2001:db8::1/64']
    };
  }

  const ipStr = parts[0].trim();
  const prefix = parseInt(parts[1].trim(), 10);

  if (isNaN(prefix) || prefix < 1 || prefix > 128) {
    errors.push(`IPv6 prefix '/${parts[1]}' must be between 1 and 128.`);
  }

  // Validate IPv6 syntax: 16-bit hex chunks, single double-colon
  const doubleColonCount = (ipStr.match(/::/g) || []).length;
  if (doubleColonCount > 1) {
    errors.push('IPv6 address cannot contain more than one "::" zero-compression.');
  }

  const chunks = ipStr.split(':').filter(c => c.length > 0);
  for (const c of chunks) {
    if (!/^[0-9a-fA-F]{1,4}$/.test(c)) {
      errors.push(`Invalid IPv6 hexadecimal segment '${c}'.`);
    }
  }

  return {
    raw,
    ip: ipStr,
    prefix: isNaN(prefix) ? 64 : prefix,
    gateway: gatewayStr,
    isValid: errors.length === 0,
    errors
  };
}

/**
 * Common Subnet Mask Presets for quick selection.
 */
export const COMMON_SUBNET_MASKS = [
  { cidr: 24, mask: '255.255.255.0', label: '/24 — 255.255.255.0 (254 Hosts • Standard LAN)' },
  { cidr: 16, mask: '255.255.0.0', label: '/16 — 255.255.0.0 (65,534 Hosts • Large Network)' },
  { cidr: 8,  mask: '255.0.0.0', label: '/8 — 255.0.0.0 (16.7M Hosts • Class A)' },
  { cidr: 25, mask: '255.255.255.128', label: '/25 — 255.255.255.128 (126 Hosts)' },
  { cidr: 26, mask: '255.255.255.192', label: '/26 — 255.255.255.192 (62 Hosts)' },
  { cidr: 27, mask: '255.255.255.224', label: '/27 — 255.255.255.224 (30 Hosts)' },
  { cidr: 28, mask: '255.255.255.240', label: '/28 — 255.255.255.240 (14 Hosts)' },
  { cidr: 29, mask: '255.255.255.248', label: '/29 — 255.255.255.248 (6 Hosts)' },
  { cidr: 30, mask: '255.255.255.252', label: '/30 — 255.255.255.252 (2 Hosts • Point-to-Point)' }
];

/**
 * Popular Public DNS Server Presets for 1-click configuration.
 */
export const POPULAR_DNS_PRESETS = [
  { name: 'Cloudflare', primary: '1.1.1.1', secondary: '1.0.0.1', tag: 'Fastest & Private' },
  { name: 'Google DNS', primary: '8.8.8.8', secondary: '8.8.4.4', tag: 'Reliable Global' },
  { name: 'Quad9', primary: '9.9.9.9', secondary: '149.112.112.112', tag: 'Malware Blocking' },
  { name: 'AdGuard', primary: '94.140.14.14', secondary: '94.140.15.15', tag: 'Ad & Tracker Block' },
  { name: 'OpenDNS', primary: '208.67.222.222', secondary: '208.67.220.220', tag: 'Cisco Family' }
];
