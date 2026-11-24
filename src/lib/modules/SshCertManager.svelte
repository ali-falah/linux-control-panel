<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Key, ShieldCheck, Lock, FileKey, AlertTriangle, CheckCircle2, XCircle,
    Plus, Trash2, Copy, RefreshCw, Server, ShieldAlert, Terminal, Globe,
    Play, RotateCw, Ban, X, Check, Info, Shield, Clock,
    MoreVertical, Pencil, Fingerprint, HardDrive, Unlock
  } from '@lucide/svelte';
  import { invokeSafe } from '../utils/ipc';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import Input from '../components/ui/Input.svelte';
  import Select from '../components/ui/Select.svelte';
  import ContextMenu from '../components/ui/ContextMenu.svelte';
  import { portal } from '../actions/portal.ts';

  // ─── Types ───────────────────────────────────────────────────────────────────

  interface SshKeyItem {
    name: string;
    key_type: string;
    path: string;
    pub_key_path?: string;
    fingerprint: string;
    public_key: string;
    comment: string;
    has_private: boolean;
  }

  interface SshClientHost {
    host: string;
    hostname: string;
    user: string;
    port: string;
    identity_file: string;
    proxy_jump: string;
    extra_config: string;
  }

  interface KnownHostItem {
    line_number: number;
    host: string;
    key_type: string;
    fingerprint: string;
    marker: string;
    raw: string;
  }

  interface AuthorizedKeyItem {
    line_number: number;
    key_type: string;
    key_data: string;
    comment: string;
    options: string;
    raw: string;
  }

  interface SshdHardeningStatus {
    permit_root_login: string;
    password_authentication: string;
    pubkey_authentication: string;
    x11_forwarding: string;
    port: string;
    config_path: string;
    is_evaluated: boolean;
    source: 'daemon' | 'config' | 'none' | string;
    error?: string;
  }

  interface SslCertItem {
    name: string;
    subject: string;
    issuer: string;
    not_before: string;
    not_after: string;
    days_valid: number;
    path: string;
    san_domains: string[];
    is_expired: boolean;
    is_expiring_soon: boolean;
    is_self_signed: boolean;
    date_parse_error: boolean;
  }

  interface Fail2banJailInfo {
    jail_name: string;
    currently_banned: number;
    total_banned: number;
    banned_ips: string[];
  }

  interface Fail2banStatus {
    is_installed: boolean;
    is_active: boolean;
    needs_elevation: boolean;
    total_banned_ips: number;
    jails: Fail2banJailInfo[];
    error?: string;
  }

  interface MenuItem {
    label: string;
    icon?: any;
    action?: () => void | Promise<void>;
    danger?: boolean;
    disabled?: boolean;
    divider?: boolean;
  }

  type TabId = 'keys' | 'client_config' | 'known_hosts' | 'authorized' | 'certs' | 'threats';
  type KeyFilter = 'all' | 'pair' | 'public';
  type CertFilter = 'all' | 'valid' | 'expiring';

  // ─── State ───────────────────────────────────────────────────────────────────

  const tabIds: TabId[] = ['keys', 'client_config', 'known_hosts', 'authorized', 'certs', 'threats'];
  let activeTab = $state<TabId>(
    uiStore.targetSubTab && tabIds.includes(uiStore.targetSubTab as TabId)
      ? (uiStore.targetSubTab as TabId)
      : 'keys'
  );
  if (uiStore.targetSubTab && tabIds.includes(uiStore.targetSubTab as TabId)) {
    uiStore.targetSubTab = null;
  }

  let loading = $state(false);
  let evaluatingSshd = $state(false);
  let evaluatingFail2ban = $state(false);
  let generatingKey = $state(false);
  let savingHost = $state(false);
  let savingKey = $state(false);
  let banning = $state(false);
  let loadErrors = $state<string[]>([]);

  let sshKeys = $state<SshKeyItem[]>([]);
  let clientHosts = $state<SshClientHost[]>([]);
  let knownHosts = $state<KnownHostItem[]>([]);
  let authorizedKeys = $state<AuthorizedKeyItem[]>([]);
  let sshdStatus = $state<SshdHardeningStatus | null>(null);
  let sslCerts = $state<SslCertItem[]>([]);
  let fail2banStatus = $state<Fail2banStatus | null>(null);

  let keySearch = $state('');
  let keyFilter = $state<KeyFilter>('all');
  let clientSearch = $state('');
  let knownSearch = $state('');
  let authSearch = $state('');
  let certSearch = $state('');
  let certFilter = $state<CertFilter>('all');

  // Generate-key form
  let showGenModal = $state(false);
  let genKeyType = $state<'ed25519' | 'rsa' | 'ecdsa'>('ed25519');
  let genBits = $state(4096);
  let genFilename = $state('id_ed25519_custom');
  let genComment = $state('user@linux-control-panel');
  let genPassphrase = $state('');
  let generatedKeyResult = $state<{ filename: string; publicKey: string } | null>(null);

  // Client-host form
  let showHostModal = $state(false);
  let editingHost = $state(false);
  let hostAlias = $state('');
  let hostName = $state('');
  let hostUser = $state('');
  let hostPort = $state('22');
  let hostKey = $state('');
  let hostProxy = $state('');
  let hostExtra = $state('');

  // Authorized-key form
  let showAuthModal = $state(false);
  let newAuthPubKey = $state('');

  // Live TLS form
  let showTlsModal = $state(false);
  let tlsHost = $state('example.com');
  let tlsPort = $state(443);
  let tlsLoading = $state(false);
  let tlsResult = $state<SslCertItem | null>(null);
  let tlsError = $state<string | null>(null);

  // Fail2ban ban form
  let showBanModal = $state(false);
  let banJail = $state('sshd');
  let banIp = $state('');

  // Context menu
  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuTitle = $state('');
  let menuSubtitle = $state('');
  let menuItems = $state<MenuItem[]>([]);

  // ─── Derived ─────────────────────────────────────────────────────────────────

  let filteredSshKeys = $derived(
    sshKeys.filter((key) => {
      if (keyFilter === 'pair' && !key.has_private) return false;
      if (keyFilter === 'public' && key.has_private) return false;
      const query = keySearch.trim().toLowerCase();
      return (
        !query ||
        key.name.toLowerCase().includes(query) ||
        key.key_type.toLowerCase().includes(query) ||
        key.fingerprint.toLowerCase().includes(query) ||
        key.comment.toLowerCase().includes(query)
      );
    })
  );

  let filteredClientHosts = $derived(
    clientHosts.filter((host) => {
      const query = clientSearch.trim().toLowerCase();
      return (
        !query ||
        host.host.toLowerCase().includes(query) ||
        host.hostname.toLowerCase().includes(query) ||
        host.user.toLowerCase().includes(query) ||
        host.proxy_jump.toLowerCase().includes(query)
      );
    })
  );

  let filteredKnownHosts = $derived(
    knownHosts.filter((host) => {
      const query = knownSearch.trim().toLowerCase();
      return (
        !query ||
        host.host.toLowerCase().includes(query) ||
        host.key_type.toLowerCase().includes(query) ||
        host.fingerprint.toLowerCase().includes(query)
      );
    })
  );

  let filteredAuthorizedKeys = $derived(
    authorizedKeys.filter((key) => {
      const query = authSearch.trim().toLowerCase();
      return (
        !query ||
        key.comment.toLowerCase().includes(query) ||
        key.key_type.toLowerCase().includes(query) ||
        key.options.toLowerCase().includes(query)
      );
    })
  );

  let expiringCertsCount = $derived(
    sslCerts.filter((cert) => cert.is_expiring_soon || cert.is_expired).length
  );
  let expiredCertsCount = $derived(sslCerts.filter((cert) => cert.is_expired).length);

  let filteredSslCerts = $derived(
    sslCerts.filter((cert) => {
      const query = certSearch.trim().toLowerCase();
      const matches =
        !query ||
        cert.name.toLowerCase().includes(query) ||
        cert.subject.toLowerCase().includes(query) ||
        cert.issuer.toLowerCase().includes(query) ||
        cert.san_domains.some((san) => san.toLowerCase().includes(query));
      if (!matches) return false;
      if (certFilter === 'expiring') return cert.is_expiring_soon || cert.is_expired;
      if (certFilter === 'valid') return !cert.is_expired;
      return true;
    })
  );

  let keyPairCount = $derived(sshKeys.filter((key) => key.has_private).length);
  let certFilterCounts = $derived({
    all: sslCerts.length,
    valid: sslCerts.filter((cert) => !cert.is_expired).length,
    expiring: expiringCertsCount,
  });
  let keyFilterCounts = $derived({
    all: sshKeys.length,
    pair: keyPairCount,
    public: sshKeys.length - keyPairCount,
  });

  let keyFilterOptions = $derived([
    { id: 'all', label: 'All', count: keyFilterCounts.all },
    { id: 'pair', label: 'Key pairs', count: keyFilterCounts.pair },
    { id: 'public', label: 'Public only', count: keyFilterCounts.public },
  ]);

  let certFilterOptions = $derived([
    { id: 'all', label: 'All', count: certFilterCounts.all },
    { id: 'valid', label: 'Valid', count: certFilterCounts.valid },
    { id: 'expiring', label: 'Expiring', count: certFilterCounts.expiring },
  ]);

  let parsedAuthPreview = $derived.by(() => {
    const raw = newAuthPubKey.trim();
    if (!raw || /[\r\n\0]/.test(raw)) return null;
    const parts = raw.split(/\s+/);
    const index = parts.findIndex(
      (part, position) =>
        (part.startsWith('ssh-') ||
          part.startsWith('ecdsa-') ||
          part.startsWith('sk-ssh-') ||
          part.startsWith('sk-ecdsa-')) &&
        position + 1 < parts.length &&
        /^[A-Za-z0-9+/=]+$/.test(parts[position + 1])
    );
    if (index < 0 || !parts[index + 1]) return null;
    return {
      type: parts[index],
      comment: parts.slice(index + 2).join(' ') || 'No comment',
    };
  });

  // ─── Loading ─────────────────────────────────────────────────────────────────

  onMount(() => {
    loadAllData();
    return () => {
      // A closed page must not keep a generated private-key passphrase around.
      genPassphrase = '';
    };
  });

  // Closing the generation modal discards the passphrase immediately.
  $effect(() => {
    if (!showGenModal) {
      genPassphrase = '';
    }
  });

  function noteError(message: string) {
    if (!loadErrors.includes(message)) loadErrors = [...loadErrors, message];
  }

  async function loadAllData() {
    loading = true;
    loadErrors = [];
    await Promise.all([
      loadSshKeys(),
      loadClientConfig(),
      loadKnownHosts(),
      loadAuthorizedKeys(),
      loadSshdHardening(false),
      loadSslCerts(),
      loadFail2banStatus(false),
    ]);
    loading = false;
  }

  async function loadSshKeys() {
    const result = await invokeSafe<SshKeyItem[]>('vault_list_ssh_keys', {}, { quiet: true });
    if (result) sshKeys = [...result];
    else noteError('Could not read ~/.ssh keys.');
  }

  async function loadClientConfig() {
    const result = await invokeSafe<SshClientHost[]>('vault_list_ssh_client_config', {}, { quiet: true });
    if (result) clientHosts = [...result];
    else noteError('Could not read ~/.ssh/config.');
  }

  async function loadKnownHosts() {
    const result = await invokeSafe<KnownHostItem[]>('vault_list_known_hosts', {}, { quiet: true });
    if (result) knownHosts = [...result];
    else noteError('Could not read ~/.ssh/known_hosts.');
  }

  async function loadAuthorizedKeys() {
    const result = await invokeSafe<AuthorizedKeyItem[]>('vault_list_authorized_keys', {}, { quiet: true });
    if (result) authorizedKeys = [...result];
    else noteError('Could not read ~/.ssh/authorized_keys.');
  }

  async function loadSshdHardening(manual = false) {
    if (manual) evaluatingSshd = true;
    const result = await invokeSafe<SshdHardeningStatus>(
      'vault_get_sshd_hardening',
      { elevated: manual },
      { quiet: true }
    );
    if (result) {
      sshdStatus = result;
      if (manual) {
        if (result.is_evaluated) {
          uiStore.addToast(
            result.source === 'daemon'
              ? 'Effective sshd configuration loaded.'
              : 'sshd settings parsed from the configuration file.',
            'success'
          );
        } else {
          uiStore.addToast(
            'Could not evaluate sshd. Check administrator authorization and file permissions.',
            'warning'
          );
        }
      }
    } else if (manual) {
      uiStore.addToast('Could not evaluate the SSH daemon configuration.', 'error');
    } else {
      noteError('Could not load the SSH daemon posture.');
    }
    if (manual) evaluatingSshd = false;
  }

  async function loadSslCerts() {
    const result = await invokeSafe<SslCertItem[]>('vault_list_ssl_certs', {}, { quiet: true });
    if (result) sslCerts = [...result];
    else noteError('Could not inspect local TLS certificates. Is OpenSSL installed?');
  }

  async function loadFail2banStatus(manual = false) {
    if (manual) evaluatingFail2ban = true;
    const result = await invokeSafe<Fail2banStatus>(
      'vault_get_fail2ban_status',
      { elevated: manual },
      { quiet: true }
    );
    if (result) {
      fail2banStatus = result;
      if (manual && result.error && !result.needs_elevation) {
        uiStore.addToast(`Fail2ban: ${result.error}`, 'warning');
      }
    } else if (manual) {
      uiStore.addToast('Could not read fail2ban status.', 'error');
    } else {
      noteError('Could not read fail2ban status.');
    }
    if (manual) evaluatingFail2ban = false;
  }

  // ─── Helpers ─────────────────────────────────────────────────────────────────

  async function copyText(text: string, label: string) {
    if (!text) {
      uiStore.addToast(`No ${label.toLowerCase()} to copy.`, 'warning');
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast(`${label} copied.`, 'success');
    } catch {
      uiStore.addToast('Clipboard access was denied.', 'error');
    }
  }

  function openMenu(
    position: MouseEvent | HTMLElement,
    title: string,
    subtitle: string,
    items: MenuItem[]
  ) {
    if (position instanceof MouseEvent) {
      menuX = position.clientX;
      menuY = position.clientY;
    } else {
      const rect = position.getBoundingClientRect();
      menuX = rect.right - 4;
      menuY = rect.bottom + 4;
    }
    menuTitle = title;
    menuSubtitle = subtitle;
    menuItems = items;
    menuOpen = true;
  }

  function sshHostCommand(host: SshClientHost): string {
    // A Host line may contain several patterns; the first one is the usable
    // shortcut for a terminal command.
    return `ssh ${host.host.split(/\s+/)[0]}`;
  }

  function sshdTone(value: string, good: string[]): 'success' | 'warning' | 'error' | 'muted' {
    if (value === 'unset') return 'muted';
    return good.includes(value) ? 'success' : 'error';
  }

  function certTone(cert: SslCertItem): 'success' | 'warning' | 'error' {
    if (cert.is_expired) return 'error';
    if (cert.is_expiring_soon || cert.date_parse_error) return 'warning';
    return 'success';
  }

  function certLabel(cert: SslCertItem): string {
    if (cert.date_parse_error) return 'Expiry unknown';
    if (cert.is_expired) return 'Expired';
    if (cert.is_expiring_soon) return `${cert.days_valid} days left`;
    return `${cert.days_valid} days left`;
  }

  function certProgress(cert: SslCertItem): number {
    if (cert.date_parse_error || cert.is_expired) return 0;
    return Math.max(2, Math.min(100, Math.round((cert.days_valid / 365) * 100)));
  }

  function resetGeneratedResult() {
    generatedKeyResult = null;
  }

  function closeModals() {
    showGenModal = false;
    showHostModal = false;
    showAuthModal = false;
    showTlsModal = false;
    showBanModal = false;
    generatedKeyResult = null;
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    if (menuOpen) {
      menuOpen = false;
      return;
    }
    closeModals();
  }

  // ─── SSH key actions ─────────────────────────────────────────────────────────

  async function handleGenerateKey() {
    if (!genFilename.trim()) {
      uiStore.addToast('Enter a filename for the new key.', 'warning');
      return;
    }
    if (genKeyType === 'rsa' && ![2048, 3072, 4096].includes(Number(genBits))) {
      uiStore.addToast('Choose a valid RSA size.', 'warning');
      return;
    }
    generatingKey = true;
    const result = await invokeSafe<string>(
      'vault_generate_ssh_key',
      {
        keyType: genKeyType,
        bits: genKeyType === 'ed25519' ? undefined : Number(genBits),
        filename: genFilename.trim(),
        comment: genComment.trim(),
        passphrase: genPassphrase,
      },
      { showToastOnError: true }
    );
    generatingKey = false;
    if (result) {
      const filename = genFilename.trim().replace(/\.pub$/i, '');
      generatedKeyResult = { filename, publicKey: result };
      showGenModal = false;
      genPassphrase = '';
      uiStore.addToast('SSH key pair generated.', 'success');
      await loadSshKeys();
    }
  }

  function askDeleteKey(key: SshKeyItem) {
    uiStore.confirm(
      'Delete SSH key pair',
      `Delete "${key.name}" and its public key from ~/.ssh? This cannot be undone.`,
      async () => {
        const result = await invokeSafe<string>(
          'vault_delete_ssh_key',
          { name: key.name },
          { showToastOnError: true }
        );
        if (result) {
          uiStore.addToast(result, 'success');
          await loadSshKeys();
        }
      },
      true
    );
  }

  function keyMenuItems(key: SshKeyItem): MenuItem[] {
    return [
      { label: 'Copy public key', icon: Copy, disabled: !key.public_key, action: () => copyText(key.public_key, 'Public key') },
      { label: 'Copy fingerprint', icon: Fingerprint, disabled: !key.fingerprint, action: () => copyText(key.fingerprint, 'Fingerprint') },
      { label: 'Copy private key path', icon: HardDrive, action: () => copyText(key.path, 'Key path') },
      { divider: true, label: '' },
      { label: 'Delete key pair', icon: Trash2, danger: true, action: () => askDeleteKey(key) },
    ];
  }

  // ─── Client host actions ─────────────────────────────────────────────────────

  function resetHostForm() {
    editingHost = false;
    hostAlias = '';
    hostName = '';
    hostUser = '';
    hostPort = '22';
    hostKey = '';
    hostProxy = '';
    hostExtra = '';
  }

  function openAddHost() {
    resetHostForm();
    showHostModal = true;
  }

  function openEditHost(host: SshClientHost) {
    editingHost = true;
    hostAlias = host.host;
    hostName = host.hostname;
    hostUser = host.user;
    hostPort = host.port || '22';
    hostKey = host.identity_file;
    hostProxy = host.proxy_jump;
    hostExtra = host.extra_config;
    showHostModal = true;
  }

  async function handleSaveHost() {
    if (!hostAlias.trim() || !hostName.trim()) {
      uiStore.addToast('Host alias and target hostname are required.', 'warning');
      return;
    }
    const port = Number(hostPort);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      uiStore.addToast('The SSH port must be between 1 and 65535.', 'warning');
      return;
    }
    savingHost = true;
    const host: SshClientHost = {
      host: hostAlias.trim(),
      hostname: hostName.trim(),
      user: hostUser.trim(),
      port: String(port),
      identity_file: hostKey.trim(),
      proxy_jump: hostProxy.trim(),
      extra_config: hostExtra.trim(),
    };
    const result = await invokeSafe(
      'vault_upsert_ssh_client_host',
      { host },
      { showToastOnError: true }
    );
    savingHost = false;
    if (result !== null) {
      uiStore.addToast(editingHost ? 'Host profile updated.' : 'Host profile added.', 'success');
      showHostModal = false;
      resetHostForm();
      await loadClientConfig();
    }
  }

  function askDeleteHost(host: SshClientHost) {
    uiStore.confirm(
      'Remove host profile',
      `Remove "${host.host}" from ~/.ssh/config? Other host blocks and comments will be preserved.`,
      async () => {
        const result = await invokeSafe(
          'vault_delete_ssh_client_host',
          { hostName: host.host },
          { showToastOnError: true }
        );
        if (result !== null) {
          uiStore.addToast(`Removed "${host.host}".`, 'success');
          await loadClientConfig();
        }
      },
      true
    );
  }

  function hostMenuItems(host: SshClientHost): MenuItem[] {
    return [
      { label: 'Copy ssh command', icon: Terminal, action: () => copyText(sshHostCommand(host), 'SSH command') },
      { label: 'Edit host profile', icon: Pencil, action: () => openEditHost(host) },
      { label: 'Copy hostname', icon: Globe, disabled: !host.hostname, action: () => copyText(host.hostname, 'Hostname') },
      { label: 'Copy identity file path', icon: Key, disabled: !host.identity_file, action: () => copyText(host.identity_file, 'Identity path') },
      { divider: true, label: '' },
      { label: 'Remove host profile', icon: Trash2, danger: true, action: () => askDeleteHost(host) },
    ];
  }

  // ─── Known hosts actions ─────────────────────────────────────────────────────

  function askRemoveKnownHost(host: KnownHostItem) {
    uiStore.confirm(
      'Remove known host',
      `Remove the host key for "${host.host}"? The next connection will ask you to verify the server again.`,
      async () => {
        const result = await invokeSafe(
          'vault_remove_known_host',
          { lineNumber: host.line_number, raw: host.raw },
          { showToastOnError: true }
        );
        if (result !== null) {
          uiStore.addToast(`Removed ${host.host}.`, 'success');
          await loadKnownHosts();
        }
      },
      true
    );
  }

  function askClearKnownHosts() {
    uiStore.confirm(
      'Clear all known hosts',
      'Remove every entry from ~/.ssh/known_hosts? Every server will need to be verified again.',
      async () => {
        const result = await invokeSafe<string>(
          'vault_clear_known_hosts',
          {},
          { showToastOnError: true }
        );
        if (result) {
          uiStore.addToast(result, 'success');
          await loadKnownHosts();
        }
      },
      true
    );
  }

  function knownHostMenuItems(host: KnownHostItem): MenuItem[] {
    return [
      { label: host.marker ? 'Copy marker' : 'Copy host', icon: Copy, action: () => copyText(host.marker || host.host, 'Host') },
      { label: 'Copy fingerprint', icon: Fingerprint, disabled: !host.fingerprint, action: () => copyText(host.fingerprint, 'Fingerprint') },
      { divider: true, label: '' },
      { label: 'Remove host key', icon: Trash2, danger: true, action: () => askRemoveKnownHost(host) },
    ];
  }

  // ─── Authorized keys actions ─────────────────────────────────────────────────

  async function handleAddAuthorizedKey() {
    if (!newAuthPubKey.trim()) {
      uiStore.addToast('Paste a public key first.', 'warning');
      return;
    }
    savingKey = true;
    const result = await invokeSafe<string>(
      'vault_add_authorized_key',
      { pubKey: newAuthPubKey.trim() },
      { showToastOnError: true }
    );
    savingKey = false;
    if (result) {
      uiStore.addToast(result, 'success');
      showAuthModal = false;
      newAuthPubKey = '';
      await loadAuthorizedKeys();
    }
  }

  function askRemoveAuthorizedKey(key: AuthorizedKeyItem) {
    uiStore.confirm(
      'Revoke authorized key',
      `Revoke ${key.comment || key.key_type}? Incoming logins using this key will be blocked.`,
      async () => {
        const result = await invokeSafe<string>(
          'vault_remove_authorized_key',
          { lineNumber: key.line_number, raw: key.raw },
          { showToastOnError: true }
        );
        if (result) {
          uiStore.addToast(result, 'success');
          await loadAuthorizedKeys();
        }
      },
      true
    );
  }

  function authorizedMenuItems(key: AuthorizedKeyItem): MenuItem[] {
    return [
      { label: 'Copy public key', icon: Copy, action: () => copyText(key.raw, 'Authorized key') },
      { label: 'Copy key comment', icon: Terminal, disabled: !key.comment, action: () => copyText(key.comment, 'Comment') },
      { divider: true, label: '' },
      { label: 'Revoke this key', icon: Trash2, danger: true, action: () => askRemoveAuthorizedKey(key) },
    ];
  }

  // ─── TLS inspector ───────────────────────────────────────────────────────────

  async function handleTestTls() {
    if (!tlsHost.trim()) {
      uiStore.addToast('Enter a hostname or IP address.', 'warning');
      return;
    }
    const port = Number(tlsPort);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      uiStore.addToast('The port must be between 1 and 65535.', 'warning');
      return;
    }
    tlsLoading = true;
    tlsResult = null;
    tlsError = null;
    try {
      const result = await invokeSafe<SslCertItem>(
        'vault_test_remote_ssl',
        { host: tlsHost.trim(), port },
        { showToastOnError: true }
      );
      if (result) tlsResult = result;
    } catch (error) {
      tlsError = String(error);
    } finally {
      tlsLoading = false;
    }
  }

  // ─── Fail2ban actions ────────────────────────────────────────────────────────

  async function authorizeFail2ban() {
    await loadFail2banStatus(true);
  }

  async function handleServiceAction(action: 'start' | 'restart') {
    const run = async () => {
      const result = await invokeSafe(
        'vault_manage_fail2ban_service',
        { action },
        { showToastOnError: true }
      );
      if (result !== null) {
        uiStore.addToast(`Fail2ban ${action === 'start' ? 'started' : 'restarted'}.`, 'success');
        await loadFail2banStatus(true);
      }
    };
    if (action === 'restart') {
      uiStore.confirm(
        'Restart fail2ban',
        'Restart the fail2ban service? Active bans can be reloaded from the jail database.',
        run,
        false
      );
    } else {
      await run();
    }
  }

  async function handleUnban(jail: string, ip: string) {
    uiStore.confirm(
      'Unban IP address',
      `Remove ${ip} from jail ${jail}? The address may be able to attempt logins again.`,
      async () => {
        const result = await invokeSafe<string>(
          'vault_unban_ip',
          { jail, ip },
          { showToastOnError: true }
        );
        if (result) {
          uiStore.addToast(result, 'success');
          await loadFail2banStatus(true);
        }
      },
      false
    );
  }

  async function handleManualBan() {
    if (!banJail || !banIp.trim()) {
      uiStore.addToast('Choose a jail and enter an IP address.', 'warning');
      return;
    }
    if (!fail2banStatus?.is_active) {
      uiStore.addToast('Start the fail2ban service before adding a ban.', 'warning');
      return;
    }
    banning = true;
    const result = await invokeSafe<string>(
      'vault_ban_ip',
      { jail: banJail, ip: banIp.trim() },
      { showToastOnError: true }
    );
    banning = false;
    if (result) {
      uiStore.addToast(result, 'success');
      showBanModal = false;
      banIp = '';
      await loadFail2banStatus(true);
    }
  }

  function openBanModal() {
    banJail = fail2banStatus?.jails[0]?.jail_name || 'sshd';
    banIp = '';
    showBanModal = true;
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="module-page ssh-vault-page">
  <PageHeader title="SSH &amp; SSL Vault" icon={Lock}>
    <div class="header-actions">
      {#if expiringCertsCount > 0}
        <span class="vault-status-pill warning" title="Certificates expiring within 30 days or already expired">
          <AlertTriangle size={12} />
          {expiringCertsCount} cert{expiringCertsCount === 1 ? '' : 's'} at risk
        </span>
      {/if}
      {#if fail2banStatus?.is_active}
        <span class="vault-status-pill active" title="Fail2ban is active">
          <ShieldCheck size={12} />
          {fail2banStatus.total_banned_ips} banned
        </span>
      {:else if fail2banStatus?.is_installed}
        <span class="vault-status-pill muted">
          <ShieldAlert size={12} />
          Fail2ban inactive
        </span>
      {/if}
      <button class="header-refresh-btn" onclick={loadAllData} disabled={loading} title="Reload all vault data">
        <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} />
        <span>Refresh</span>
      </button>
    </div>
  </PageHeader>

  <div class="vault-overview" aria-label="Vault overview">
    <KpiCard icon={Key} value={sshKeys.length} label="SSH keys" subtext={`${keyPairCount} key pair${keyPairCount === 1 ? '' : 's'}`} onclick={() => (activeTab = 'keys')} />
    <KpiCard icon={Server} value={clientHosts.length} label="Host profiles" subtext="~/.ssh/config" onclick={() => (activeTab = 'client_config')} />
    <KpiCard
      icon={FileKey}
      value={sslCerts.length}
      label="Certificates"
      statusText={expiredCertsCount > 0 ? `${expiredCertsCount} expired` : expiringCertsCount > 0 ? `${expiringCertsCount} soon` : 'Healthy'}
      statusType={expiredCertsCount > 0 ? 'error' : expiringCertsCount > 0 ? 'warning' : 'success'}
      onclick={() => (activeTab = 'certs')}
    />
    <KpiCard
      icon={ShieldAlert}
      value={fail2banStatus?.is_active ? fail2banStatus.total_banned_ips : '—'}
      label="Banned IPs"
      subtext={fail2banStatus?.is_active ? `${fail2banStatus.jails.length} jails` : 'Fail2ban'}
      statusText={fail2banStatus?.is_active ? 'Active' : 'Off'}
      statusType={fail2banStatus?.is_active ? 'success' : 'muted'}
      onclick={() => (activeTab = 'threats')}
    />
  </div>

  <div class="vault-subnav-bar">
    <div class="tab-scroll">
      <TabGroup
        size="sm"
        tabs={[
          { id: 'keys', label: 'SSH Keys', count: sshKeys.length, icon: Key },
          { id: 'client_config', label: 'Client Config', count: clientHosts.length, icon: Server },
          { id: 'known_hosts', label: 'Known Hosts', count: knownHosts.length, icon: Globe },
          { id: 'authorized', label: 'Authorized & SSHD', count: authorizedKeys.length, icon: ShieldCheck },
          { id: 'certs', label: 'SSL Certificates', count: sslCerts.length, icon: FileKey },
          { id: 'threats', label: 'Threat Defense', count: fail2banStatus?.total_banned_ips ?? 0, icon: ShieldAlert },
        ]}
        bind:activeTab
      />
    </div>

    <div class="subnav-actions">
      {#if activeTab === 'keys'}
        <Button variant="primary" size="sm" onclick={() => { generatedKeyResult = null; showGenModal = true; }}>
          <Plus size={13} /> Generate SSH Key
        </Button>
      {:else if activeTab === 'client_config'}
        <Button variant="primary" size="sm" onclick={openAddHost}>
          <Plus size={13} /> Add Host Profile
        </Button>
      {:else if activeTab === 'known_hosts'}
        {#if knownHosts.length > 0}
          <Button variant="danger" size="sm" onclick={askClearKnownHosts} title="Remove every entry from known_hosts">
            <Trash2 size={13} /> Flush Known Hosts
          </Button>
        {/if}
      {:else if activeTab === 'authorized'}
        <Button variant="primary" size="sm" onclick={() => (showAuthModal = true)}>
          <Plus size={13} /> Add Authorized Key
        </Button>
      {:else if activeTab === 'certs'}
        <Button variant="primary" size="sm" onclick={() => (showTlsModal = true)}>
          <Globe size={13} /> Test Live TLS
        </Button>
      {:else if activeTab === 'threats'}
        {#if fail2banStatus?.is_installed}
          <Button variant="primary" size="sm" onclick={openBanModal}>
            <Ban size={13} /> Ban IP
          </Button>
        {/if}
      {/if}
    </div>
  </div>

  {#if loadErrors.length > 0}
    <div class="vault-alert" role="alert">
      <AlertTriangle size={15} />
      <span>{loadErrors[0]}</span>
      {#if loadErrors.length > 1}<span class="muted">+{loadErrors.length - 1} more</span>{/if}
      <button class="link-btn" onclick={loadAllData}>Retry</button>
    </div>
  {/if}

  <div class="tab-main-body">
    <!-- ═══ SSH KEYS ═══ -->
    {#if activeTab === 'keys'}
      <div class="panel-toolbar">
        <SearchBar bind:value={keySearch} placeholder="Search key name, algorithm, fingerprint, or comment…" count={filteredSshKeys.length} total={sshKeys.length} />
        <FilterChips options={keyFilterOptions} bind:value={keyFilter} label="Filter SSH keys by completeness" />
      </div>

      {#if filteredSshKeys.length === 0}
        <div class="vault-empty-card">
          <EmptyState
            icon={Key}
            title={keySearch ? 'No keys match this search' : 'No SSH keys yet'}
            description={keySearch ? 'Try a different name, algorithm, or fingerprint.' : 'Generate an Ed25519 key pair for passwordless SSH, Git, and server access.'}
            actionLabel={keySearch ? '' : 'Generate Key'}
            actionIcon={Plus}
            onAction={() => (showGenModal = true)}
          />
        </div>
      {:else}
        <div class="vault-grid">
          {#each filteredSshKeys as key (key.path)}
            <article
              class="vault-card"
              oncontextmenu={(event) => {
                event.preventDefault();
                openMenu(event, key.name, key.path, keyMenuItems(key));
              }}
            >
              <div class="vault-card-head">
                <div class="vault-icon"><Key size={15} /></div>
                <div class="vault-title">
                  <h3>{key.name}</h3>
                  <span class="mono truncate" title={key.path}>{key.path}</span>
                </div>
                <Badge variant={key.key_type.includes('ed25519') ? 'info' : 'muted'}>{key.key_type}</Badge>
                <button
                  class="icon-btn"
                  aria-label="SSH key actions"
                  title="Actions"
                  onclick={(event) => {
                    event.stopPropagation();
                    openMenu(event.currentTarget as HTMLElement, key.name, key.path, keyMenuItems(key));
                  }}
                >
                  <MoreVertical size={14} />
                </button>
              </div>

              <div class="vault-meta">
                <div class="meta-row">
                  <span class="meta-label">Fingerprint</span>
                  <span class="meta-value mono truncate" title={key.fingerprint}>{key.fingerprint}</span>
                  <button class="copy-mini" onclick={() => copyText(key.fingerprint, 'Fingerprint')} title="Copy fingerprint" aria-label="Copy fingerprint">
                    <Copy size={11} />
                  </button>
                </div>
                <div class="meta-row">
                  <span class="meta-label">Comment</span>
                  <span class="meta-value truncate" title={key.comment}>{key.comment || 'No comment'}</span>
                </div>
              </div>

              <div class="vault-card-footer">
                <span class="status-chip {key.has_private ? 'success' : 'muted'}">
                  {#if key.has_private}<CheckCircle2 size={12} /> Key pair{:else}<Info size={12} /> Public only{/if}
                </span>
                <div class="card-actions">
                  <Button variant="ghost" size="sm" onclick={() => copyText(key.public_key, 'Public key')} disabled={!key.public_key}>
                    <Copy size={12} /> Public key
                  </Button>
                  <Button variant="ghost" size="sm" onclick={() => copyText(key.fingerprint, 'Fingerprint')}>
                    <Fingerprint size={12} /> Fingerprint
                  </Button>
                </div>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- ═══ CLIENT CONFIG ═══ -->
    {#if activeTab === 'client_config'}
      <div class="panel-toolbar">
        <SearchBar bind:value={clientSearch} placeholder="Search alias, hostname, user, or bastion…" count={filteredClientHosts.length} total={clientHosts.length} />
      </div>

      {#if filteredClientHosts.length === 0}
        <div class="vault-empty-card">
          <EmptyState
            icon={Server}
            title={clientSearch ? 'No host profiles match' : 'No SSH host profiles'}
            description={clientSearch ? 'Try a different alias or hostname.' : 'Add servers to ~/.ssh/config and connect with a short alias.'}
            actionLabel={clientSearch ? '' : 'Add Host Profile'}
            actionIcon={Plus}
            onAction={openAddHost}
          />
        </div>
      {:else}
        <div class="vault-grid">
          {#each filteredClientHosts as host (host.host)}
            <article
              class="vault-card"
              oncontextmenu={(event) => {
                event.preventDefault();
                openMenu(event, host.host, sshHostCommand(host), hostMenuItems(host));
              }}
            >
              <div class="vault-card-head">
                <div class="vault-icon"><Server size={15} /></div>
                <div class="vault-title">
                  <h3>{host.host}</h3>
                  <span class="mono truncate" title={host.hostname}>
                    {host.user ? `${host.user}@` : ''}{host.hostname || 'No HostName'} : {host.port || '22'}
                  </span>
                </div>
                {#if host.proxy_jump}<Badge variant="info">ProxyJump</Badge>{/if}
                <button
                  class="icon-btn"
                  aria-label="Host profile actions"
                  title="Actions"
                  onclick={(event) => {
                    event.stopPropagation();
                    openMenu(event.currentTarget as HTMLElement, host.host, sshHostCommand(host), hostMenuItems(host));
                  }}
                >
                  <MoreVertical size={14} />
                </button>
              </div>

              <div class="vault-meta">
                <div class="meta-row">
                  <span class="meta-label">Identity</span>
                  <span class="meta-value mono truncate" title={host.identity_file || 'Default keys'}>
                    {host.identity_file || 'Default (~/.ssh/id_*)'}
                  </span>
                </div>
                {#if host.proxy_jump}
                  <div class="meta-row">
                    <span class="meta-label">Bastion</span>
                    <span class="meta-value mono truncate" title={host.proxy_jump}>{host.proxy_jump}</span>
                  </div>
                {/if}
                {#if host.extra_config}
                  <div class="meta-row">
                    <span class="meta-label">Extra</span>
                    <span class="meta-value truncate" title={host.extra_config}>{host.extra_config.split('\n').length} option line(s)</span>
                  </div>
                {/if}
              </div>

              <div class="vault-card-footer">
                <span class="status-chip success"><CheckCircle2 size={12} /> Configured</span>
                <div class="card-actions">
                  <Button variant="ghost" size="sm" onclick={() => copyText(sshHostCommand(host), 'SSH command')}>
                    <Terminal size={12} /> Copy ssh
                  </Button>
                  <Button variant="ghost" size="sm" onclick={() => openEditHost(host)}>
                    <Pencil size={12} /> Edit
                  </Button>
                </div>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- ═══ KNOWN HOSTS ═══ -->
    {#if activeTab === 'known_hosts'}
      <div class="panel-toolbar">
        <SearchBar bind:value={knownSearch} placeholder="Search host, key type, or fingerprint…" count={filteredKnownHosts.length} total={knownHosts.length} />
      </div>

      {#if filteredKnownHosts.length === 0}
        <div class="vault-empty-card">
          <EmptyState icon={Globe} title="No known hosts" description={knownSearch ? 'No entries match this search.' : 'OpenSSH will add host keys here the first time you connect to a server.'} />
        </div>
      {:else}
        <div class="vault-list">
          {#each filteredKnownHosts as host (host.line_number)}
            <article
              class="vault-row-card"
              oncontextmenu={(event) => {
                event.preventDefault();
                openMenu(event, host.host, `Line ${host.line_number}`, knownHostMenuItems(host));
              }}
            >
              <div class="vault-row-main">
                <div class="vault-icon"><Globe size={14} /></div>
                <div class="vault-title">
                  <h3>
                    {host.host}
                    {#if host.marker}<Badge variant="warning">{host.marker.replace('@', '')}</Badge>{/if}
                  </h3>
                  <span class="mono truncate" title={host.fingerprint}>{host.fingerprint}</span>
                </div>
                <Badge variant="muted">{host.key_type}</Badge>
              </div>
              <div class="vault-row-actions">
                <span class="line-number mono">#{host.line_number}</span>
                <button class="icon-btn" onclick={() => copyText(host.fingerprint, 'Fingerprint')} title="Copy fingerprint" aria-label="Copy fingerprint">
                  <Copy size={13} />
                </button>
                <Button variant="danger" size="sm" onclick={() => askRemoveKnownHost(host)} title="Remove this host key">
                  <Trash2 size={12} /> Remove
                </Button>
              </div>
            </article>
          {/each}
        </div>
        <div class="vault-note">
          <ShieldAlert size={14} />
          Only remove a host key when you trust the new fingerprint. Removing entries exposes you to a first-connection trust prompt.
        </div>
      {/if}
    {/if}

    <!-- ═══ AUTHORIZED & SSHD ═══ -->
    {#if activeTab === 'authorized'}
      <div class="panel-toolbar">
        <SearchBar bind:value={authSearch} placeholder="Search comment, key type, or restrictions…" count={filteredAuthorizedKeys.length} total={authorizedKeys.length} />
      </div>

      {#if filteredAuthorizedKeys.length === 0}
        <div class="vault-empty-card">
          <EmptyState
            icon={ShieldCheck}
            title={authSearch ? 'No keys match' : 'No authorized keys'}
            description={authSearch ? 'Try another comment or key type.' : 'Add public keys here to allow inbound SSH logins with cryptographic authentication.'}
            actionLabel={authSearch ? '' : 'Add Authorized Key'}
            actionIcon={Plus}
            onAction={() => (showAuthModal = true)}
          />
        </div>
      {:else}
        <div class="vault-grid">
          {#each filteredAuthorizedKeys as key (key.line_number)}
            <article
              class="vault-card"
              oncontextmenu={(event) => {
                event.preventDefault();
                openMenu(event, key.comment || key.key_type, `Line ${key.line_number}`, authorizedMenuItems(key));
              }}
            >
              <div class="vault-card-head">
                <div class="vault-icon"><ShieldCheck size={15} /></div>
                <div class="vault-title">
                  <h3>{key.comment || 'No comment'}</h3>
                  <span class="mono truncate" title={key.key_data}>{key.key_data.slice(0, 32)}…</span>
                </div>
                <Badge variant="info">{key.key_type}</Badge>
                <button
                  class="icon-btn"
                  aria-label="Authorized key actions"
                  title="Actions"
                  onclick={(event) => {
                    event.stopPropagation();
                    openMenu(event.currentTarget as HTMLElement, key.comment || key.key_type, `Line ${key.line_number}`, authorizedMenuItems(key));
                  }}
                >
                  <MoreVertical size={14} />
                </button>
              </div>
              <div class="vault-meta">
                <div class="meta-row">
                  <span class="meta-label">Access</span>
                  <span class="meta-value">
                    {#if key.options}
                      <Badge variant="warning">{key.options}</Badge>
                    {:else}
                      Full shell access
                    {/if}
                  </span>
                </div>
                <div class="meta-row">
                  <span class="meta-label">Line</span>
                  <span class="meta-value mono">#{key.line_number}</span>
                </div>
              </div>
              <div class="vault-card-footer">
                <span class="status-chip success"><CheckCircle2 size={12} /> Authorized</span>
                <div class="card-actions">
                  <Button variant="ghost" size="sm" onclick={() => copyText(key.raw, 'Authorized key')}>
                    <Copy size={12} /> Copy
                  </Button>
                  <Button variant="danger" size="sm" onclick={() => askRemoveAuthorizedKey(key)}>
                    <Trash2 size={12} /> Revoke
                  </Button>
                </div>
              </div>
            </article>
          {/each}
        </div>
      {/if}

      <section class="vault-card sshd-card">
        <div class="vault-card-head">
          <div class="vault-icon"><Lock size={15} /></div>
          <div class="vault-title">
            <h3>SSH daemon posture</h3>
            <span>
              {sshdStatus?.source === 'daemon'
                ? 'Effective live configuration (sshd -T)'
                : sshdStatus?.source === 'config'
                  ? 'Parsed from /etc/ssh/sshd_config'
                  : 'Not evaluated yet'}
            </span>
          </div>
          <Button variant="outline" size="sm" onclick={() => loadSshdHardening(true)} disabled={evaluatingSshd}>
            <RefreshCw size={12} class={evaluatingSshd ? 'animate-spin-slow' : ''} />
            {sshdStatus?.source === 'daemon' ? 'Refresh' : 'Authorize & evaluate'}
          </Button>
        </div>

        {#if sshdStatus && !sshdStatus.is_evaluated}
          <div class="vault-callout warning">
            <ShieldAlert size={16} />
            <div>
              <strong>Administrator authorization required</strong>
              <p>
                {sshdStatus.error || 'The active sshd configuration could not be read. Authorize once to run sshd -T and read the effective settings.'}
              </p>
            </div>
            <Button variant="primary" size="sm" onclick={() => loadSshdHardening(true)} disabled={evaluatingSshd}>
              <Unlock size={13} /> Authorize
            </Button>
          </div>
        {:else if sshdStatus}
          {#if sshdStatus.source === 'config'}
            <div class="vault-callout info">
              <Info size={15} />
              <span>These values were parsed from the main file and included fragments. Authorize for the authoritative <code>sshd -T</code> view.</span>
            </div>
          {/if}
          <div class="sshd-grid">
            <div class="sshd-item">
              <div class="sshd-item-top">
                <span class="sshd-label">PermitRootLogin</span>
                <Badge variant={sshdTone(sshdStatus.permit_root_login, ['no', 'prohibit-password', 'without-password', 'forced-commands-only'])}>{sshdStatus.permit_root_login}</Badge>
              </div>
              <span class="sshd-desc">
                {sshdStatus.permit_root_login === 'no'
                  ? 'Direct root remote access is disabled.'
                  : ['prohibit-password', 'without-password'].includes(sshdStatus.permit_root_login)
                    ? 'Root may log in with keys only.'
                    : sshdStatus.permit_root_login === 'forced-commands-only'
                      ? 'Root may run forced commands with keys only.'
                      : 'Root password logins may be accepted.'}
              </span>
            </div>
            <div class="sshd-item">
              <div class="sshd-item-top">
                <span class="sshd-label">PasswordAuthentication</span>
                <Badge variant={sshdTone(sshdStatus.password_authentication, ['no'])}>{sshdStatus.password_authentication}</Badge>
              </div>
              <span class="sshd-desc">
                {sshdStatus.password_authentication === 'no' ? 'Key-only authentication is enforced.' : 'Interactive password login is enabled.'}
              </span>
            </div>
            <div class="sshd-item">
              <div class="sshd-item-top">
                <span class="sshd-label">PubkeyAuthentication</span>
                <Badge variant={sshdTone(sshdStatus.pubkey_authentication, ['yes'])}>{sshdStatus.pubkey_authentication}</Badge>
              </div>
              <span class="sshd-desc">
                {sshdStatus.pubkey_authentication === 'yes' ? 'Cryptographic key login is active.' : 'Public-key authentication is disabled.'}
              </span>
            </div>
            <div class="sshd-item">
              <div class="sshd-item-top">
                <span class="sshd-label">X11Forwarding</span>
                <Badge variant={sshdTone(sshdStatus.x11_forwarding, ['no'])}>{sshdStatus.x11_forwarding}</Badge>
              </div>
              <span class="sshd-desc">
                {sshdStatus.x11_forwarding === 'no' ? 'X11 tunneling is disabled.' : 'GUI tunneling over SSH is allowed.'}
              </span>
            </div>
            <div class="sshd-item">
              <div class="sshd-item-top">
                <span class="sshd-label">SSH Port</span>
                <Badge variant="muted">{sshdStatus.port}</Badge>
              </div>
              <span class="sshd-desc">
                {sshdStatus.port === '22' ? 'Standard listening port (22/TCP).' : 'Custom listening port.'}
              </span>
            </div>
          </div>
        {/if}
      </section>
    {/if}

    <!-- ═══ CERTIFICATES ═══ -->
    {#if activeTab === 'certs'}
      <div class="panel-toolbar">
        <SearchBar bind:value={certSearch} placeholder="Search subject, issuer, or SAN…" count={filteredSslCerts.length} total={sslCerts.length} />
        <FilterChips options={certFilterOptions} bind:value={certFilter} label="Filter certificates by validity" />
      </div>

      {#if filteredSslCerts.length === 0}
        <div class="vault-empty-card">
          <EmptyState
            icon={FileKey}
            title={certSearch ? 'No certificates match' : 'No local server certificates found'}
            description={certSearch ? 'Try another domain, issuer, or SAN.' : 'Scanned /etc/letsencrypt, nginx, Apache, httpd, and mkcert stores. Use Test Live TLS to inspect a remote endpoint.'}
            actionLabel={certSearch ? '' : 'Test Live TLS'}
            actionIcon={Globe}
            onAction={() => (showTlsModal = true)}
          />
        </div>
      {:else}
        <div class="vault-grid">
          {#each filteredSslCerts as cert (cert.path + cert.subject)}
            <article class="vault-card">
              <div class="vault-card-head">
                <div class="vault-icon" class:warn={cert.is_expiring_soon || cert.is_expired}>
                  <FileKey size={15} />
                </div>
                <div class="vault-title">
                  <h3>{cert.name}</h3>
                  <span class="truncate" title={cert.subject}>{cert.subject || 'Unknown subject'}</span>
                </div>
                <Badge variant={certTone(cert)}>{certLabel(cert)}</Badge>
              </div>

              <div class="cert-progress" title={cert.date_parse_error ? 'Expiry could not be parsed' : `${cert.days_valid} days remaining`}>
                <span class="cert-progress-fill {certTone(cert)}" style="width: {certProgress(cert)}%"></span>
              </div>

              <div class="vault-meta">
                <div class="meta-row">
                  <span class="meta-label">Issuer</span>
                  <span class="meta-value truncate" title={cert.issuer}>{cert.issuer || 'Unknown'}</span>
                </div>
                <div class="meta-row">
                  <span class="meta-label">Valid until</span>
                  <span class="meta-value">{cert.not_after || 'Unknown'}</span>
                </div>
                <div class="meta-row">
                  <span class="meta-label">Path</span>
                  <span class="meta-value mono truncate" title={cert.path}>{cert.path}</span>
                  <button class="copy-mini" onclick={() => copyText(cert.path, 'Certificate path')} title="Copy path" aria-label="Copy certificate path">
                    <Copy size={11} />
                  </button>
                </div>
              </div>

              {#if cert.san_domains.length > 0}
                <div class="san-pill-wrap">
                  {#each cert.san_domains.slice(0, 4) as san}
                    <span class="san-pill">{san}</span>
                  {/each}
                  {#if cert.san_domains.length > 4}
                    <span class="san-pill muted">+{cert.san_domains.length - 4} more</span>
                  {/if}
                </div>
              {/if}

              <div class="vault-card-footer">
                <span class="status-chip {certTone(cert)}">
                  {#if cert.is_expired}<XCircle size={12} /> Expired{:else if cert.date_parse_error}<Info size={12} /> Date unknown{:else if cert.is_expiring_soon}<Clock size={12} /> Renew soon{:else}<CheckCircle2 size={12} /> Valid{/if}
                </span>
                <div class="card-actions">
                  {#if cert.is_self_signed}<Badge variant="warning">Self-signed</Badge>{/if}
                  <Button variant="ghost" size="sm" onclick={() => copyText(cert.path, 'Certificate path')}>
                    <Copy size={12} /> Path
                  </Button>
                </div>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- ═══ THREAT DEFENSE ═══ -->
    {#if activeTab === 'threats'}
      {#if !fail2banStatus}
        <div class="vault-empty-card">
          <EmptyState icon={ShieldAlert} title="Fail2ban status not loaded" description="Refresh to check whether fail2ban is installed and running." actionLabel="Refresh" actionIcon={RefreshCw} onAction={() => loadFail2banStatus(false)} />
        </div>
      {:else if !fail2banStatus.is_installed}
        <div class="vault-card install-card">
          <div class="vault-icon"><ShieldAlert size={18} /></div>
          <h3>Fail2ban is not installed</h3>
          <p>Fail2ban blocks abusive SSH and web clients after repeated failures. Install it to enable jail monitoring and IP bans here.</p>
          <div class="install-command">
            <code>sudo dnf install -y fail2ban &amp;&amp; sudo systemctl enable --now fail2ban</code>
            <Button variant="outline" size="sm" onclick={() => copyText('sudo dnf install -y fail2ban && sudo systemctl enable --now fail2ban', 'Install command')}>
              <Copy size={12} /> Copy
            </Button>
          </div>
        </div>
      {:else if fail2banStatus.needs_elevation}
        <div class="vault-callout warning">
          <ShieldAlert size={18} />
          <div>
            <strong>Administrator authorization required</strong>
            <p>Fail2ban’s control socket is root-only. Authorize once to read jail status or manage bans; the approval is cached by PolicyKit.</p>
          </div>
          <Button variant="primary" size="sm" onclick={authorizeFail2ban} disabled={evaluatingFail2ban}>
            <Unlock size={13} /> Authorize status
          </Button>
        </div>
      {:else}
        <div class="vault-card status-overview-card">
          <div class="status-overview-left">
            <span class="status-dot {fail2banStatus.is_active ? 'online' : 'offline'}"></span>
            <div>
              <strong>{fail2banStatus.is_active ? 'Fail2ban is active' : 'Fail2ban is inactive'}</strong>
              <span>
                {fail2banStatus.is_active
                  ? `Monitoring ${fail2banStatus.jails.length} jail${fail2banStatus.jails.length === 1 ? '' : 's'} · ${fail2banStatus.total_banned_ips} active ban${fail2banStatus.total_banned_ips === 1 ? '' : 's'}`
                  : 'No jail data is available while the service is stopped.'}
              </span>
            </div>
          </div>
          <div class="card-actions">
            <Button variant="outline" size="sm" onclick={() => loadFail2banStatus(true)} disabled={evaluatingFail2ban}>
              <RefreshCw size={12} class={evaluatingFail2ban ? 'animate-spin-slow' : ''} /> Refresh
            </Button>
            {#if fail2banStatus.is_active}
              <Button variant="outline" size="sm" onclick={() => handleServiceAction('restart')}>
                <RotateCw size={12} /> Restart
              </Button>
            {:else}
              <Button variant="primary" size="sm" onclick={() => handleServiceAction('start')}>
                <Play size={12} /> Start Service
              </Button>
            {/if}
          </div>
        </div>

        {#if fail2banStatus.error}
          <div class="vault-alert" role="alert">
            <AlertTriangle size={15} />
            <span>{fail2banStatus.error}</span>
          </div>
        {/if}

        {#if fail2banStatus.jails.length === 0}
          <div class="vault-empty-card">
            <EmptyState icon={Shield} title="No jails reported" description="Fail2ban is running but did not report any jails. Check /etc/fail2ban/jail.local." />
          </div>
        {:else}
          <div class="jails-grid">
            {#each fail2banStatus.jails as jail (jail.jail_name)}
              <article class="vault-card jail-card">
                <div class="vault-card-head">
                  <div class="vault-icon"><Shield size={15} /></div>
                  <div class="vault-title">
                    <h3>{jail.jail_name}</h3>
                    <span>{jail.currently_banned} currently banned · {jail.total_banned} total</span>
                  </div>
                  <Badge variant={jail.currently_banned > 0 ? 'error' : 'muted'}>{jail.currently_banned} banned</Badge>
                </div>
                {#if jail.banned_ips.length === 0}
                  <p class="jail-empty">No active IP blocks.</p>
                {:else}
                  <div class="ip-pills-wrap">
                    {#each jail.banned_ips as ip (ip)}
                      <span class="banned-ip-pill">
                        <span class="mono">{ip}</span>
                        <button class="copy-mini" onclick={() => copyText(ip, 'IP address')} title="Copy IP" aria-label="Copy IP">
                          <Copy size={10} />
                        </button>
                        <button class="unban-btn" onclick={() => handleUnban(jail.jail_name, ip)} title="Unban {ip}" aria-label="Unban {ip}">
                          <X size={11} />
                        </button>
                      </span>
                    {/each}
                  </div>
                {/if}
              </article>
            {/each}
          </div>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<!-- ═══ Context menu ═══ -->
<ContextMenu
  bind:isOpen={menuOpen}
  x={menuX}
  y={menuY}
  title={menuTitle}
  subtitle={menuSubtitle}
  items={menuItems}
/>

<!-- ═══ Generate key modal ═══ -->
{#if showGenModal}
  <div use:portal class="modal-backdrop" onclick={() => (showGenModal = false)} onkeydown={(event) => { if (event.key === 'Escape') showGenModal = false; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="gen-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><Key size={18} class="accent" /><h3 id="gen-title">Generate SSH key pair</h3></div>
        <button class="icon-btn" onclick={() => (showGenModal = false)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <span class="form-label">Algorithm</span>
          <div class="radio-pill-group">
            <button type="button" class="radio-pill-btn" class:active={genKeyType === 'ed25519'} onclick={() => { genKeyType = 'ed25519'; genBits = 4096; genFilename = 'id_ed25519_custom'; }}>
              <strong>Ed25519</strong><span>Recommended · fast and compact</span>
            </button>
            <button type="button" class="radio-pill-btn" class:active={genKeyType === 'rsa'} onclick={() => { genKeyType = 'rsa'; genBits = 4096; genFilename = 'id_rsa_custom'; }}>
              <strong>RSA</strong><span>Legacy compatibility · larger keys</span>
            </button>
            <button type="button" class="radio-pill-btn" class:active={genKeyType === 'ecdsa'} onclick={() => { genKeyType = 'ecdsa'; genBits = 256; genFilename = 'id_ecdsa_custom'; }}>
              <strong>ECDSA</strong><span>NIST curves · optional compatibility</span>
            </button>
          </div>
        </div>

        {#if genKeyType !== 'ed25519'}
          <div class="form-group">
            <Select id="gen-bits" label="Key size (bits)" bind:value={genBits}>
              {#if genKeyType === 'rsa'}
                <option value={2048}>2048</option>
                <option value={3072}>3072</option>
                <option value={4096}>4096 (recommended)</option>
              {:else}
                <option value={256}>256</option>
                <option value={384}>384</option>
                <option value={521}>521</option>
              {/if}
            </Select>
          </div>
        {/if}

        <Input id="gen-key-name" label="File name in ~/.ssh/" bind:value={genFilename} placeholder="id_ed25519_project" class="mono" />
        <span class="field-hint">Creates <code>~/.ssh/{genFilename || '…'}</code> and <code>~/.ssh/{genFilename || '…'}.pub</code>. Existing files are never overwritten.</span>

        <Input id="gen-key-comment" label="Comment / identity tag" bind:value={genComment} placeholder="user@linux-control-panel" />
        <Input id="gen-key-pass" label="Passphrase (optional)" type="password" bind:value={genPassphrase} placeholder="Leave empty for a passwordless key" />
        <span class="field-hint">The passphrase is sent to ssh-keygen through a pseudo-terminal, never through a process argument list.</span>
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (showGenModal = false)}>Cancel</Button>
        <Button variant="primary" onclick={handleGenerateKey} disabled={generatingKey}>
          <Key size={14} /> {generatingKey ? 'Generating…' : 'Generate key pair'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═══ Generated key result ═══ -->
{#if generatedKeyResult}
  <div use:portal class="modal-backdrop" onclick={() => (generatedKeyResult = null)} onkeydown={(event) => { if (event.key === 'Escape') generatedKeyResult = null; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="generated-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><CheckCircle2 size={18} class="success" /><h3 id="generated-title">SSH key pair created</h3></div>
        <button class="icon-btn" onclick={() => (generatedKeyResult = null)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        <p class="text-sm muted">The private key is at <code>~/.ssh/{generatedKeyResult.filename}</code>. Copy the public key to a server’s <code>authorized_keys</code> or to your Git host.</p>
        <pre class="pubkey-preview">{generatedKeyResult.publicKey}</pre>
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (generatedKeyResult = null)}>Close</Button>
        <Button variant="primary" onclick={() => { copyText(generatedKeyResult!.publicKey, 'Public key'); generatedKeyResult = null; }}>
          <Copy size={14} /> Copy public key
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═══ Host profile modal ═══ -->
{#if showHostModal}
  <div use:portal class="modal-backdrop" onclick={() => (showHostModal = false)} onkeydown={(event) => { if (event.key === 'Escape') showHostModal = false; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="host-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><Server size={18} class="accent" /><h3 id="host-title">{editingHost ? 'Edit host profile' : 'Add host profile'}</h3></div>
        <button class="icon-btn" onclick={() => (showHostModal = false)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        <div class="form-row-2">
          <Input id="host-alias" label="Shortcut / alias *" bind:value={hostAlias} placeholder="dev-server" disabled={editingHost} class="mono" />
          <Input id="host-name" label="Target hostname or IP *" bind:value={hostName} placeholder="192.168.1.100" class="mono" />
        </div>
        {#if editingHost}<span class="field-hint">The alias identifies the block. To rename it, remove this profile and add a new one.</span>{/if}

        <div class="form-row-2">
          <Input id="host-user" label="Remote user" bind:value={hostUser} placeholder="ubuntu" />
          <Input id="host-port" label="SSH port" type="number" min="1" max="65535" bind:value={hostPort} placeholder="22" class="mono" />
        </div>

        <Input id="host-key" label="Identity file (optional)" bind:value={hostKey} placeholder="~/.ssh/id_ed25519" class="mono" list="ssh-key-list" />
        <datalist id="ssh-key-list">
          {#each sshKeys as key}
            <option value={key.path}></option>
          {/each}
        </datalist>

        <Input id="host-proxy" label="ProxyJump / bastion (optional)" bind:value={hostProxy} placeholder="bastion.example.com" class="mono" />

        <details class="advanced-details">
          <summary>Advanced SSH options</summary>
          <textarea class="form-textarea mono" rows="4" bind:value={hostExtra} placeholder="ServerAliveInterval 60&#10;ForwardAgent yes"></textarea>
          <span class="field-hint">One directive per line. Host and Match blocks are not allowed here.</span>
        </details>
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (showHostModal = false)}>Cancel</Button>
        <Button variant="primary" onclick={handleSaveHost} disabled={savingHost}>
          <Check size={14} /> {savingHost ? 'Saving…' : 'Save host profile'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═══ Authorized key modal ═══ -->
{#if showAuthModal}
  <div use:portal class="modal-backdrop" onclick={() => (showAuthModal = false)} onkeydown={(event) => { if (event.key === 'Escape') showAuthModal = false; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="auth-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><ShieldCheck size={18} class="accent" /><h3 id="auth-title">Add authorized public key</h3></div>
        <button class="icon-btn" onclick={() => (showAuthModal = false)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label class="form-label" for="auth-key-input">OpenSSH public key</label>
          <textarea id="auth-key-input" class="form-textarea mono" rows="4" bind:value={newAuthPubKey} placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA… admin@laptop"></textarea>
          <span class="field-hint">Exactly one key. It will be appended to <code>~/.ssh/authorized_keys</code> and grants inbound login access.</span>
        </div>

        {#if parsedAuthPreview}
          <div class="parsed-key-preview">
            <Badge variant="info">{parsedAuthPreview.type}</Badge>
            <span class="muted">{parsedAuthPreview.comment}</span>
          </div>
        {:else if newAuthPubKey.trim()}
          <div class="vault-callout warning"><AlertTriangle size={14} /><span>This does not look like an OpenSSH public key.</span></div>
        {/if}
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (showAuthModal = false)}>Cancel</Button>
        <Button variant="primary" onclick={handleAddAuthorizedKey} disabled={savingKey}>
          <Plus size={14} /> {savingKey ? 'Adding…' : 'Add authorized key'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═══ Live TLS modal ═══ -->
{#if showTlsModal}
  <div use:portal class="modal-backdrop" onclick={() => (showTlsModal = false)} onkeydown={(event) => { if (event.key === 'Escape') showTlsModal = false; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="tls-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><Globe size={18} class="accent" /><h3 id="tls-title">Inspect a live TLS certificate</h3></div>
        <button class="icon-btn" onclick={() => (showTlsModal = false)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        <div class="form-row-2">
          <Input id="tls-host" label="Hostname or IP" bind:value={tlsHost} placeholder="example.com or 2001:db8::1" class="mono" />
          <Input id="tls-port" label="Port" type="number" min="1" max="65535" bind:value={tlsPort} placeholder="443" class="mono" />
        </div>
        <Button variant="primary" class="full-width" onclick={handleTestTls} disabled={tlsLoading}>
          <RefreshCw size={13} class={tlsLoading ? 'animate-spin-slow' : ''} /> {tlsLoading ? 'Connecting…' : 'Inspect certificate'}
        </Button>

        {#if tlsError}
          <div class="vault-callout danger"><AlertTriangle size={15} /><span>{tlsError}</span></div>
        {/if}

        {#if tlsResult}
          <div class="tls-result">
            <div class="tls-result-head">
              <strong>{tlsResult.name}</strong>
              <Badge variant={certTone(tlsResult)}>{certLabel(tlsResult)}</Badge>
            </div>
            <div class="meta-row"><span class="meta-label">Issuer</span><span class="meta-value">{tlsResult.issuer || 'Unknown'}</span></div>
            <div class="meta-row"><span class="meta-label">Valid</span><span class="meta-value">{tlsResult.not_before} → {tlsResult.not_after}</span></div>
            {#if tlsResult.san_domains.length > 0}
              <div class="san-pill-wrap">
                {#each tlsResult.san_domains as san}<span class="san-pill">{san}</span>{/each}
              </div>
            {/if}
            {#if tlsResult.is_self_signed}<Badge variant="warning">Self-signed certificate</Badge>{/if}
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (showTlsModal = false)}>Close</Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═══ Ban IP modal ═══ -->
{#if showBanModal}
  <div use:portal class="modal-backdrop" onclick={() => (showBanModal = false)} onkeydown={(event) => { if (event.key === 'Escape') showBanModal = false; }} role="presentation">
    <div class="modal-card" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="ban-title" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group"><Ban size={18} class="danger" /><h3 id="ban-title">Ban an IP address</h3></div>
        <button class="icon-btn" onclick={() => (showBanModal = false)} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="modal-body">
        {#if !fail2banStatus?.is_active}
          <div class="vault-callout warning"><AlertTriangle size={14} /><span>Fail2ban is inactive. Start the service before adding bans.</span></div>
        {/if}
        <div class="form-group">
          <label class="form-label" for="ban-jail">Jail</label>
          <select id="ban-jail" class="form-select mono" bind:value={banJail}>
            {#if fail2banStatus?.jails.length}
              {#each fail2banStatus.jails as jail}
                <option value={jail.jail_name}>{jail.jail_name} · {jail.currently_banned} banned</option>
              {/each}
            {:else}
              <option value="sshd">sshd</option>
              <option value="recidive">recidive</option>
              <option value="nginx-http-auth">nginx-http-auth</option>
            {/if}
          </select>
        </div>
        <Input id="ban-ip" label="IP address" bind:value={banIp} placeholder="192.0.2.1 or 2001:db8::1" class="mono" />
        <span class="field-hint">The address is added to the selected jail and dropped by the host firewall.</span>
      </div>
      <div class="modal-footer">
        <Button variant="secondary" onclick={() => (showBanModal = false)}>Cancel</Button>
        <Button variant="danger" onclick={handleManualBan} disabled={!banIp.trim() || banning || !fail2banStatus?.is_active}>
          <Ban size={14} /> {banning ? 'Banning…' : 'Ban IP'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .module-page.ssh-vault-page {
    display: flex;
    flex-direction: column;
    gap: var(--page-content-gap);
    padding: 24px;
    min-height: 100%;
    box-sizing: border-box;
  }

  .header-actions,
  .subnav-actions,
  .card-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .vault-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    border: 1px solid transparent;
  }
  .vault-status-pill.warning {
    background: color-mix(in srgb, var(--color-warning) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-warning) 32%, transparent);
    color: var(--color-warning);
  }
  .vault-status-pill.active {
    background: color-mix(in srgb, var(--color-success) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-success) 32%, transparent);
    color: var(--color-success);
  }
  .vault-status-pill.muted {
    background: var(--color-bg-surface);
    border-color: var(--color-border);
    color: var(--color-text-muted);
  }

  .header-refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: 7px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .header-refresh-btn:hover:not(:disabled) {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }
  .header-refresh-btn:disabled { opacity: 0.55; cursor: default; }

  .vault-overview {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 8px;
  }

  .vault-subnav-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .tab-scroll {
    min-width: 0;
    max-width: 100%;
    overflow-x: auto;
    scrollbar-width: thin;
    padding-bottom: 2px;
  }
  .tab-scroll :global(.tab-bar) { flex-wrap: nowrap; }

  .panel-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .panel-toolbar :global(.search-bar),
  .panel-toolbar :global(.search-container) {
    flex: 1 1 280px;
    max-width: 520px;
  }

  .tab-main-body { display: flex; flex-direction: column; gap: 10px; }

  .vault-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 10px;
  }

  .vault-card,
  .vault-empty-card,
  .vault-row-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    box-sizing: border-box;
  }
  .vault-card { padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .vault-empty-card { padding: 30px 16px; }
  .vault-row-card { padding: 10px 14px; display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .vault-row-card:hover, .vault-card:hover { border-color: color-mix(in srgb, var(--color-accent) 45%, var(--color-border)); }

  .vault-card-head { display: flex; align-items: flex-start; gap: 10px; min-width: 0; }
  .vault-icon {
    width: 30px; height: 30px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border-radius: 8px;
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
    color: var(--color-accent);
  }
  .vault-icon.warn { background: color-mix(in srgb, var(--color-warning) 14%, transparent); color: var(--color-warning); }
  .vault-title { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .vault-title h3 { margin: 0; font-size: 13px; font-weight: 650; color: var(--color-text-primary); display: flex; align-items: center; gap: 6px; }
  .vault-title span { font-size: 11px; color: var(--color-text-muted); }

  .vault-meta { display: flex; flex-direction: column; gap: 5px; }
  .meta-row { display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 11.5px; }
  .meta-label { width: 74px; flex-shrink: 0; color: var(--color-text-muted); font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.03em; }
  .meta-value { min-width: 0; color: var(--color-text-secondary); }
  .truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .vault-card-footer {
    display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap;
    padding-top: 8px; border-top: 1px solid color-mix(in srgb, var(--color-border) 70%, transparent);
  }

  .copy-mini, .icon-btn, .unban-btn, .link-btn {
    border: 0; background: transparent; color: var(--color-text-muted); cursor: pointer;
    display: inline-flex; align-items: center; justify-content: center; padding: 3px; border-radius: 5px;
  }
  .copy-mini:hover, .icon-btn:hover, .unban-btn:hover, .link-btn:hover { color: var(--color-text-primary); background: var(--color-bg-hover); }
  .unban-btn:hover { color: var(--color-error); }
  .link-btn { color: var(--color-accent); font-size: 11.5px; font-weight: 600; }

  .status-chip {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 3px 8px; border-radius: 6px; font-size: 10.5px; font-weight: 600;
  }
  .status-chip.success { background: color-mix(in srgb, var(--color-success) 13%, transparent); color: var(--color-success); }
  .status-chip.warning { background: color-mix(in srgb, var(--color-warning) 13%, transparent); color: var(--color-warning); }
  .status-chip.error, .status-chip.danger { background: color-mix(in srgb, var(--color-error) 13%, transparent); color: var(--color-error); }
  .status-chip.muted { background: var(--color-bg-surface); color: var(--color-text-muted); }

  .vault-list { display: flex; flex-direction: column; gap: 6px; }
  .vault-row-main { display: flex; align-items: center; gap: 10px; min-width: 0; flex: 1; }
  .vault-row-actions { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .line-number { color: var(--color-text-muted); font-size: 10.5px; }

  .vault-note {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 12px; font-size: 11.5px; color: var(--color-text-muted);
    border: 1px dashed var(--color-border); border-radius: 8px;
  }

  .vault-alert {
    display: flex; align-items: center; gap: 8px;
    padding: 9px 12px; border-radius: 8px; font-size: 12px;
    border: 1px solid color-mix(in srgb, var(--color-error) 28%, transparent);
    background: color-mix(in srgb, var(--color-error) 8%, transparent);
    color: var(--color-text-secondary);
  }

  .vault-callout {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 11px 13px; border-radius: 9px; font-size: 12px;
    border: 1px solid var(--color-border); background: var(--color-bg-surface);
    color: var(--color-text-secondary);
  }
  .vault-callout > div { flex: 1; }
  .vault-callout strong { display: block; color: var(--color-text-primary); font-size: 12.5px; margin-bottom: 2px; }
  .vault-callout p { margin: 0; line-height: 1.45; }
  .vault-callout.warning { border-color: color-mix(in srgb, var(--color-warning) 32%, transparent); background: color-mix(in srgb, var(--color-warning) 8%, transparent); }
  .vault-callout.info { border-color: color-mix(in srgb, var(--color-info) 28%, transparent); background: color-mix(in srgb, var(--color-info) 7%, transparent); }
  .vault-callout.danger { border-color: color-mix(in srgb, var(--color-error) 32%, transparent); background: color-mix(in srgb, var(--color-error) 8%, transparent); }

  .sshd-card { gap: 0; }
  .sshd-card > .vault-card-head { padding-bottom: 10px; }
  .sshd-grid {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 8px;
  }
  .sshd-item {
    border: 1px solid var(--color-border); border-radius: 9px; padding: 10px 12px;
    display: flex; flex-direction: column; gap: 6px; background: var(--color-bg-surface);
  }
  .sshd-item-top { display: flex; align-items: center; justify-content: space-between; gap: 6px; flex-wrap: wrap; }
  .sshd-item-top :global(.badge) { max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .sshd-label { overflow-wrap: anywhere; }
  .sshd-label { font-size: 12px; font-weight: 600; color: var(--color-text-primary); }
  .sshd-desc { font-size: 10.5px; line-height: 1.4; color: var(--color-text-muted); }

  .cert-progress { height: 5px; border-radius: 999px; background: var(--color-bg-surface); overflow: hidden; }
  .cert-progress-fill { display: block; height: 100%; border-radius: inherit; background: var(--color-success); }
  .cert-progress-fill.warning { background: var(--color-warning); }
  .cert-progress-fill.error { background: var(--color-error); }

  .san-pill-wrap { display: flex; flex-wrap: wrap; gap: 4px; }
  .san-pill {
    font-family: var(--font-mono); font-size: 10px; padding: 1px 5px;
    border: 1px solid var(--color-border); border-radius: 4px; color: var(--color-text-secondary);
  }
  .san-pill.muted { color: var(--color-text-muted); }

  .status-overview-card { flex-direction: row; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .status-overview-left { display: flex; align-items: center; gap: 10px; }
  .status-overview-left strong { display: block; font-size: 13px; color: var(--color-text-primary); }
  .status-overview-left span:not(.status-dot) { display: block; font-size: 11.5px; color: var(--color-text-muted); margin-top: 1px; }
  .status-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
  .status-dot.online { background: var(--color-success); box-shadow: 0 0 8px color-mix(in srgb, var(--color-success) 70%, transparent); }
  .status-dot.offline { background: var(--color-error); }

  .jails-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 10px; }
  .jail-card { gap: 8px; }
  .jail-empty { margin: 0; font-size: 11.5px; color: var(--color-text-muted); font-style: italic; }
  .ip-pills-wrap { display: flex; flex-wrap: wrap; gap: 5px; }
  .banned-ip-pill {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 10.5px; padding: 2px 6px; border-radius: 5px;
    color: var(--color-error); border: 1px solid color-mix(in srgb, var(--color-error) 28%, transparent);
    background: color-mix(in srgb, var(--color-error) 8%, transparent);
  }

  .install-card { align-items: flex-start; }
  .install-card h3 { margin: 0; font-size: 15px; color: var(--color-text-primary); }
  .install-card p { margin: 0; font-size: 12px; color: var(--color-text-muted); line-height: 1.5; }
  .install-command {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    width: 100%; padding: 8px 10px; border-radius: 7px;
    background: var(--color-bg-surface); border: 1px solid var(--color-border);
  }
  .install-command code { flex: 1; font-size: 11px; word-break: break-all; }

  .modal-backdrop {
    position: fixed; inset: 0; z-index: 99990; padding: 20px;
    background: rgba(0, 0, 0, 0.68);
    display: flex; align-items: center; justify-content: center;
  }
  .modal-card {
    width: 560px; max-width: calc(100vw - 32px); max-height: calc(100vh - 40px);
    display: flex; flex-direction: column; overflow: hidden;
    background: var(--color-bg-card); border: 1px solid var(--color-border);
    border-radius: 14px; box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45);
  }
  .modal-header, .modal-footer {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    padding: 13px 18px; border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-surface);
  }
  .modal-footer { justify-content: flex-end; border-bottom: 0; border-top: 1px solid var(--color-border); }
  .modal-title-group { display: flex; align-items: center; gap: 9px; }
  .modal-title-group h3 { margin: 0; font-size: 14px; font-weight: 650; color: var(--color-text-primary); }
  .modal-body { padding: 18px; display: flex; flex-direction: column; gap: 13px; overflow-y: auto; }

  .form-group { display: flex; flex-direction: column; gap: 5px; }
  .form-label { font-size: 11.5px; font-weight: 600; color: var(--color-text-secondary); }
  .form-row-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .form-textarea, .form-select {
    width: 100%; padding: 8px 11px; box-sizing: border-box;
    background: var(--color-bg-base); border: 1px solid var(--color-border);
    border-radius: 7px; color: var(--color-text-primary); font-size: 12px; outline: none;
  }
  .form-textarea:focus, .form-select:focus { border-color: var(--color-accent); }
  .field-hint { font-size: 10.5px; color: var(--color-text-muted); line-height: 1.4; }

  .radio-pill-group { display: flex; flex-direction: column; gap: 6px; }
  .radio-pill-btn {
    display: flex; flex-direction: column; align-items: flex-start; gap: 2px;
    padding: 9px 12px; border: 1px solid var(--color-border); border-radius: 8px;
    background: var(--color-bg-surface); color: var(--color-text-muted);
    font-size: 12px; text-align: left; cursor: pointer;
  }
  .radio-pill-btn strong { color: var(--color-text-primary); font-size: 12.5px; }
  .radio-pill-btn.active { border-color: var(--color-accent); background: color-mix(in srgb, var(--color-accent) 7%, transparent); }

  .advanced-details { border-top: 1px dashed var(--color-border); padding-top: 10px; }
  .advanced-details summary { cursor: pointer; font-size: 11.5px; font-weight: 600; color: var(--color-text-secondary); margin-bottom: 7px; }

  .pubkey-preview {
    margin: 0; padding: 11px; border-radius: 8px; border: 1px solid var(--color-border);
    background: var(--color-bg-surface); color: var(--color-text-primary);
    font-family: var(--font-mono); font-size: 11px; word-break: break-all; white-space: pre-wrap;
  }

  .parsed-key-preview { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: 7px; background: var(--color-bg-surface); }

  .tls-result { display: flex; flex-direction: column; gap: 7px; padding: 11px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--color-bg-surface); }
  .tls-result-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 12.5px; }

  .full-width { width: 100%; justify-content: center; }
  .mono { font-family: var(--font-mono); }
  .muted { color: var(--color-text-muted); }
  .text-sm { font-size: 12px; }
  .accent { color: var(--color-accent); }
  .success { color: var(--color-success); }
  .danger { color: var(--color-error); }

  @media (max-width: 720px) {
    .module-page.ssh-vault-page { padding: 16px; }
    .vault-grid, .jails-grid { grid-template-columns: 1fr; }
    .form-row-2 { grid-template-columns: 1fr; }
    .vault-row-card { align-items: flex-start; flex-direction: column; }
    .vault-row-actions { width: 100%; justify-content: flex-end; }
  }
</style>
