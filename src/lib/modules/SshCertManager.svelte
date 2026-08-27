<script lang="ts">
  import { onMount } from 'svelte';
  import { 
    Key, ShieldCheck, Lock, FileKey, AlertTriangle, CheckCircle2, XCircle, 
    Plus, Trash2, Copy, RefreshCw, HelpCircle, Server, Eye, ExternalLink, ShieldAlert,
    Terminal, Globe, Activity, Play, RotateCw, Ban, Search, X, Check, Info, Shield,
    Cpu, Laptop, ArrowUpRight, Clock, Sparkles
  } from '@lucide/svelte';
  import { invokeSafe } from '../utils/ipc';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import { portal } from '../actions/portal.ts';

  // ─── Types ───────────────────────────────────────────────────────────────────

  interface SshKeyItem {
    name: string;
    key_type: string;
    path: string;
    pub_key_path?: string;
    fingerprint: string;
    public_key: string;
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
    total_banned_ips: number;
    jails: Fail2banJailInfo[];
  }

  // ─── State ───────────────────────────────────────────────────────────────────

  let activeTab = $state<'keys' | 'client_config' | 'known_hosts' | 'authorized' | 'certs' | 'threats'>(
    uiStore.targetSubTab && ['keys', 'client_config', 'known_hosts', 'authorized', 'certs', 'threats'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'keys'
  );
  if (uiStore.targetSubTab && ['keys', 'client_config', 'known_hosts', 'authorized', 'certs', 'threats'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  let loading = $state(false);
  let evaluatingSshd = $state(false);

  // Data Stores
  let sshKeys = $state<SshKeyItem[]>([]);
  let clientHosts = $state<SshClientHost[]>([]);
  let knownHosts = $state<KnownHostItem[]>([]);
  let authorizedKeys = $state<AuthorizedKeyItem[]>([]);
  let sshdStatus = $state<SshdHardeningStatus | null>(null);
  let sslCerts = $state<SslCertItem[]>([]);
  let fail2banStatus = $state<Fail2banStatus | null>(null);

  // Search & Filter States
  let keySearch = $state('');
  let clientSearch = $state('');
  let knownSearch = $state('');
  let certSearch = $state('');
  let certFilter = $state<'all' | 'expiring' | 'valid'>('all');

  // Modals State
  let showGenModal = $state(false);
  let genKeyType = $state<'ed25519' | 'rsa'>('ed25519');
  let genBits = $state(4096);
  let genFilename = $state('id_ed25519_custom');
  let genComment = $state('user@linux-control-panel');
  let genPassphrase = $state('');
  let generatedKeyResult = $state<{ filename: string; publicKey: string } | null>(null);

  let showAddClientModal = $state(false);
  let newHostAlias = $state('');
  let newHostName = $state('');
  let newHostUser = $state('');
  let newHostPort = $state('22');
  let newHostKey = $state('');
  let newHostProxy = $state('');
  let newHostExtra = $state('');

  let showAddAuthModal = $state(false);
  let newAuthPubKey = $state('');

  let showTestSslModal = $state(false);
  let testSslHost = $state('google.com');
  let testSslPort = $state(443);
  let testSslLoading = $state(false);
  let testSslResult = $state<SslCertItem | null>(null);
  let testSslError = $state<string | null>(null);

  let showBanModal = $state(false);
  let banJailName = $state('sshd');
  let banIpAddress = $state('');

  // ─── Filtered Lists ──────────────────────────────────────────────────────────

  let filteredSshKeys = $derived(
    sshKeys.filter(k => 
      !keySearch || 
      k.name.toLowerCase().includes(keySearch.toLowerCase()) || 
      k.fingerprint.toLowerCase().includes(keySearch.toLowerCase()) ||
      k.key_type.toLowerCase().includes(keySearch.toLowerCase())
    )
  );

  let filteredClientHosts = $derived(
    clientHosts.filter(h => 
      !clientSearch || 
      h.host.toLowerCase().includes(clientSearch.toLowerCase()) || 
      h.hostname.toLowerCase().includes(clientSearch.toLowerCase()) ||
      h.user.toLowerCase().includes(clientSearch.toLowerCase())
    )
  );

  let filteredKnownHosts = $derived(
    knownHosts.filter(k => 
      !knownSearch || 
      k.host.toLowerCase().includes(knownSearch.toLowerCase()) || 
      k.fingerprint.toLowerCase().includes(knownSearch.toLowerCase())
    )
  );

  let filteredSslCerts = $derived(
    sslCerts.filter(c => {
      const matchText = !certSearch || 
        c.name.toLowerCase().includes(certSearch.toLowerCase()) ||
        c.issuer.toLowerCase().includes(certSearch.toLowerCase()) ||
        c.san_domains.some(s => s.toLowerCase().includes(certSearch.toLowerCase()));

      if (!matchText) return false;
      if (certFilter === 'expiring') return c.is_expiring_soon || c.is_expired;
      if (certFilter === 'valid') return !c.is_expired;
      return true;
    })
  );

  // Expiring certs count
  let expiringCertsCount = $derived(
    sslCerts.filter(c => c.is_expiring_soon || c.is_expired).length
  );

  // ─── Data Loading ────────────────────────────────────────────────────────────

  onMount(() => {
    loadAllData();
  });

  async function loadAllData() {
    loading = true;
    await Promise.all([
      loadSshKeys(),
      loadClientConfig(),
      loadKnownHosts(),
      loadAuthorizedKeys(),
      loadSshdHardening(false),
      loadSslCerts(),
      loadFail2banStatus(),
    ]);
    loading = false;
  }

  async function loadSshKeys() {
    const res = await invokeSafe<SshKeyItem[]>('vault_list_ssh_keys', {}, { quiet: true });
    if (res) sshKeys = [...res];
  }

  async function loadClientConfig() {
    const res = await invokeSafe<SshClientHost[]>('vault_list_ssh_client_config', {}, { quiet: true });
    if (res) clientHosts = [...res];
  }

  async function loadKnownHosts() {
    const res = await invokeSafe<KnownHostItem[]>('vault_list_known_hosts', {}, { quiet: true });
    if (res) knownHosts = [...res];
  }

  async function loadAuthorizedKeys() {
    const res = await invokeSafe<AuthorizedKeyItem[]>('vault_list_authorized_keys', {}, { quiet: true });
    if (res) authorizedKeys = [...res];
  }

  async function loadSshdHardening(isManual = false) {
    if (isManual) evaluatingSshd = true;
    const res = await invokeSafe<SshdHardeningStatus>('vault_get_sshd_hardening', {}, { quiet: true });
    if (res) {
      sshdStatus = res;
      if (isManual) {
        if (res.is_evaluated) {
          uiStore.showToast('SSH daemon configuration evaluated successfully', 'success');
        } else {
          uiStore.showToast('Evaluation requires Root mode enabled in status bar', 'warning');
        }
      }
    }
    if (isManual) evaluatingSshd = false;
  }

  async function loadSslCerts() {
    const res = await invokeSafe<SslCertItem[]>('vault_list_ssl_certs', {}, { quiet: true });
    if (res) sslCerts = [...res];
  }

  async function loadFail2banStatus() {
    const res = await invokeSafe<Fail2banStatus>('vault_get_fail2ban_status', {}, { quiet: true });
    if (res) fail2banStatus = res;
  }

  // ─── Actions ─────────────────────────────────────────────────────────────────

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      uiStore.showToast(`${label} copied to clipboard`, 'success');
    } catch {
      uiStore.showToast('Failed to copy to clipboard', 'error');
    }
  }

  async function handleGenerateKey() {
    if (!genFilename.trim()) {
      uiStore.showToast('Please provide a filename for the key', 'warning');
      return;
    }
    const res = await invokeSafe<{ filename: string; public_key: string }>('vault_generate_ssh_key', {
      keyType: genKeyType,
      bits: genBits,
      filename: genFilename.trim(),
      comment: genComment.trim(),
      passphrase: genPassphrase,
    });

    if (res) {
      generatedKeyResult = { filename: res.filename, publicKey: res.public_key };
      showGenModal = false;
      uiStore.showToast('SSH key pair generated successfully', 'success');
      loadSshKeys();
    }
  }

  async function handleDeleteKey(keyName: string) {
    if (!confirm(`Are you sure you want to delete SSH key pair "${keyName}"? This action cannot be undone.`)) return;
    const res = await invokeSafe<string>('vault_delete_ssh_key', { keyName });
    if (res) {
      uiStore.showToast(res, 'success');
      loadSshKeys();
    }
  }

  async function handleAddClientHost() {
    if (!newHostAlias.trim() || !newHostName.trim()) {
      uiStore.showToast('Host alias and HostName/IP are required', 'warning');
      return;
    }

    const res = await invokeSafe<string>('vault_add_ssh_client_host', {
      host: newHostAlias.trim(),
      hostname: newHostName.trim(),
      user: newHostUser.trim() || null,
      port: newHostPort.trim() || null,
      identityFile: newHostKey.trim() || null,
      proxyJump: newHostProxy.trim() || null,
      extraConfig: newHostExtra.trim() || null,
    });

    if (res) {
      uiStore.showToast('Host alias saved to ~/.ssh/config', 'success');
      showAddClientModal = false;
      newHostAlias = '';
      newHostName = '';
      newHostUser = '';
      newHostPort = '22';
      newHostKey = '';
      newHostProxy = '';
      newHostExtra = '';
      loadClientConfig();
    }
  }

  async function handleDeleteClientHost(host: string) {
    if (!confirm(`Remove host profile "${host}" from ~/.ssh/config?`)) return;
    const res = await invokeSafe<string>('vault_delete_ssh_client_host', { host });
    if (res) {
      uiStore.showToast(res, 'success');
      loadClientConfig();
    }
  }

  async function handleRemoveKnownHost(lineNumber: number, host: string) {
    const res = await invokeSafe<string>('vault_remove_known_host', { lineNumber });
    if (res) {
      uiStore.showToast(`Removed entry for ${host}`, 'success');
      loadKnownHosts();
    }
  }

  async function handleClearKnownHosts() {
    if (!confirm('Are you sure you want to clear ALL known host entries from ~/.ssh/known_hosts?')) return;
    const res = await invokeSafe<string>('vault_clear_known_hosts');
    if (res) {
      uiStore.showToast(res, 'success');
      loadKnownHosts();
    }
  }

  async function handleAddAuthorizedKey() {
    if (!newAuthPubKey.trim()) {
      uiStore.showToast('Please paste a valid SSH public key', 'warning');
      return;
    }
    const res = await invokeSafe<string>('vault_add_authorized_key', { publicKey: newAuthPubKey.trim() });
    if (res) {
      uiStore.showToast(res, 'success');
      showAddAuthModal = false;
      newAuthPubKey = '';
      loadAuthorizedKeys();
    }
  }

  async function handleRemoveAuthorizedKey(lineNumber: number) {
    if (!confirm(`Revoke authorized key at line #${lineNumber}? Incoming logins with this key will be blocked.`)) return;
    const res = await invokeSafe<string>('vault_remove_authorized_key', { lineNumber });
    if (res) {
      uiStore.showToast(res, 'success');
      loadAuthorizedKeys();
    }
  }

  async function handleTestRemoteSsl() {
    if (!testSslHost.trim()) {
      uiStore.showToast('Please specify a domain or IP to test', 'warning');
      return;
    }
    testSslLoading = true;
    testSslResult = null;
    testSslError = null;

    try {
      const res = await invokeSafe<SslCertItem>('vault_test_remote_ssl', {
        host: testSslHost.trim(),
        port: Number(testSslPort) || 443,
      });
      if (res) {
        testSslResult = res;
      }
    } catch (e: any) {
      testSslError = e?.toString() || 'Failed to inspect remote TLS certificate';
    } finally {
      testSslLoading = false;
    }
  }

  async function handleUnbanIp(jail: string, ip: string) {
    const res = await invokeSafe<string>('vault_unban_ip', { jail, ip });
    if (res) {
      uiStore.showToast(`Unbanned ${ip} from jail ${jail}`, 'success');
      loadFail2banStatus();
    }
  }

  async function handleManualBan() {
    if (!banJailName || !banIpAddress.trim()) {
      uiStore.showToast('Please select a jail and specify an IP address', 'warning');
      return;
    }
    const res = await invokeSafe<string>('vault_ban_ip', { jail: banJailName, ip: banIpAddress.trim() });
    if (res) {
      uiStore.showToast(`Successfully banned ${banIpAddress} into jail ${banJailName}`, 'success');
      showBanModal = false;
      banIpAddress = '';
      loadFail2banStatus();
    }
  }

  async function handleControlFail2ban(action: 'start' | 'restart') {
    const res = await invokeSafe<string>('vault_control_fail2ban', { action });
    if (res) {
      uiStore.showToast(res, 'success');
      loadFail2banStatus();
    }
  }
</script>

<div class="module-root">
  <!-- ── Page Header ────────────────────────────────────────────────────────── -->
  <PageHeader 
    title="SSH &amp; SSL Vault" 
    subtitle="Manage cryptographic identities, SSH server posture, client profiles, and TLS certificates" 
    icon={Lock}
  >
    <div class="header-actions">
      <button 
        class="btn btn-secondary btn-sm" 
        onclick={loadAllData} 
        disabled={loading}
        title="Reload all SSH keys, configs, SSL certs, and Fail2ban posture"
      >
        <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} />
        <span>Refresh All</span>
      </button>
    </div>
  </PageHeader>

  <!-- ── Top KPI Stat Overview Cards ────────────────────────────────────────── -->
  <div class="kpi-grid">
    <div class="kpi-card" onclick={() => activeTab = 'keys'} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && (activeTab = 'keys')}>
      <div class="kpi-icon-wrap accent">
        <Key size={18} />
      </div>
      <div class="kpi-body">
        <div class="kpi-title">SSH Key Pairs</div>
        <div class="kpi-value">{sshKeys.length}</div>
        <div class="kpi-desc">In ~/.ssh/ directory</div>
      </div>
    </div>

    <div class="kpi-card" onclick={() => activeTab = 'client_config'} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && (activeTab = 'client_config')}>
      <div class="kpi-icon-wrap primary">
        <Laptop size={18} />
      </div>
      <div class="kpi-body">
        <div class="kpi-title">Client Profiles</div>
        <div class="kpi-value">{clientHosts.length}</div>
        <div class="kpi-desc">Host aliases configured</div>
      </div>
    </div>

    <div class="kpi-card" onclick={() => activeTab = 'known_hosts'} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && (activeTab = 'known_hosts')}>
      <div class="kpi-icon-wrap info">
        <Globe size={18} />
      </div>
      <div class="kpi-body">
        <div class="kpi-title">Known Hosts</div>
        <div class="kpi-value">{knownHosts.length}</div>
        <div class="kpi-desc">Remembered host keys</div>
      </div>
    </div>

    <div class="kpi-card" onclick={() => activeTab = 'certs'} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && (activeTab = 'certs')}>
      <div class="kpi-icon-wrap {expiringCertsCount > 0 ? 'warning' : 'success'}">
        <FileKey size={18} />
      </div>
      <div class="kpi-body">
        <div class="kpi-title">SSL Certificates</div>
        <div class="kpi-value">{sslCerts.length}</div>
        <div class="kpi-desc">
          {#if expiringCertsCount > 0}
            <span class="text-warning">⚠️ {expiringCertsCount} expiring / expired</span>
          {:else}
            <span class="text-success">All valid &amp; healthy</span>
          {/if}
        </div>
      </div>
    </div>

    <div class="kpi-card" onclick={() => activeTab = 'threats'} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && (activeTab = 'threats')}>
      <div class="kpi-icon-wrap {fail2banStatus?.is_active ? 'success' : 'muted'}">
        <ShieldAlert size={18} />
      </div>
      <div class="kpi-body">
        <div class="kpi-title">Threat Defense</div>
        <div class="kpi-value">{fail2banStatus?.total_banned_ips ?? 0}</div>
        <div class="kpi-desc">
          {fail2banStatus?.is_active ? `Fail2ban Active (${fail2banStatus?.jails?.length || 0} jails)` : 'Fail2ban Inactive'}
        </div>
      </div>
    </div>
  </div>

  <!-- ── Navigation Tabs ────────────────────────────────────────────────────── -->
  <div class="tab-bar-nav">
    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'keys'} 
      onclick={() => activeTab = 'keys'}
    >
      <Key size={15} />
      <span>SSH Keys</span>
      {#if sshKeys.length > 0}
        <span class="tab-pill">{sshKeys.length}</span>
      {/if}
    </button>

    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'client_config'} 
      onclick={() => activeTab = 'client_config'}
    >
      <Laptop size={15} />
      <span>SSH Client Config</span>
      {#if clientHosts.length > 0}
        <span class="tab-pill">{clientHosts.length}</span>
      {/if}
    </button>

    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'known_hosts'} 
      onclick={() => activeTab = 'known_hosts'}
    >
      <Globe size={15} />
      <span>Known Hosts</span>
      {#if knownHosts.length > 0}
        <span class="tab-pill">{knownHosts.length}</span>
      {/if}
    </button>

    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'authorized'} 
      onclick={() => activeTab = 'authorized'}
    >
      <ShieldCheck size={15} />
      <span>Authorized &amp; SSHD</span>
      {#if authorizedKeys.length > 0}
        <span class="tab-pill">{authorizedKeys.length}</span>
      {/if}
    </button>

    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'certs'} 
      onclick={() => activeTab = 'certs'}
    >
      <FileKey size={15} />
      <span>SSL Certificates</span>
      {#if sslCerts.length > 0}
        <span class="tab-pill" class:pill-warn={expiringCertsCount > 0}>{sslCerts.length}</span>
      {/if}
    </button>

    <button 
      class="tab-nav-btn" 
      class:active={activeTab === 'threats'} 
      onclick={() => activeTab = 'threats'}
    >
      <ShieldAlert size={15} />
      <span>Threat Defense</span>
      {#if (fail2banStatus?.total_banned_ips ?? 0) > 0}
        <span class="tab-pill pill-danger">{fail2banStatus?.total_banned_ips}</span>
      {/if}
    </button>
  </div>

  <!-- ── Tab Body Content ──────────────────────────────────────────────────── -->
  <div class="tab-main-body">
    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 1: SSH KEY VAULT
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'keys'}
      <div class="tab-panel">
        <!-- Feature Context & Action Header -->
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <Key size={18} />
            </div>
            <div class="guide-copy">
              <h4>Local SSH Cryptographic Key Vault</h4>
              <p>
                Private/public key pairs located in <code>~/.ssh/</code>. Ed25519 is recommended for optimal performance and modern cryptography.
              </p>
            </div>
          </div>
          <button class="btn btn-primary" onclick={() => showGenModal = true}>
            <Plus size={14} /> Generate SSH Key
          </button>
        </div>

        <!-- Filter & Table Card -->
        <div class="card table-card">
          <div class="table-toolbar">
            <div class="search-input-wrap">
              <Search size={14} class="search-icon" />
              <input 
                type="text" 
                placeholder="Search key name, algorithm, or fingerprint..." 
                bind:value={keySearch} 
              />
              {#if keySearch}
                <button class="clear-search-btn" onclick={() => keySearch = ''}><X size={13} /></button>
              {/if}
            </div>
            <span class="table-count-label">Showing {filteredSshKeys.length} of {sshKeys.length} keys</span>
          </div>

          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>KEY IDENTIFIER</th>
                  <th>ALGORITHM</th>
                  <th>FINGERPRINT (SHA256)</th>
                  <th>STATUS</th>
                  <th>PUBLIC KEY</th>
                  <th style="text-align: right;">ACTIONS</th>
                </tr>
              </thead>
              <tbody>
                {#if filteredSshKeys.length === 0}
                  <tr>
                    <td colspan="6" class="empty-cell">
                      <div class="empty-state-box">
                        <Key size={32} class="empty-icon" />
                        <div class="empty-title">No SSH Keys Found</div>
                        <div class="empty-desc">
                          {keySearch ? 'No keys matched your search filter.' : 'Generate a new Ed25519 key pair to start passwordless SSH authentication.'}
                        </div>
                        {#if !keySearch}
                          <button class="btn btn-primary btn-sm mt-2" onclick={() => showGenModal = true}>
                            <Plus size={13} /> Generate Key
                          </button>
                        {/if}
                      </div>
                    </td>
                  </tr>
                {:else}
                  {#each filteredSshKeys as key}
                    <tr>
                      <td>
                        <div class="key-name-col">
                          <span class="font-mono strong">{key.name}</span>
                          <span class="text-xs text-muted font-mono">{key.path}</span>
                        </div>
                      </td>
                      <td>
                        <span class="badge {key.key_type.includes('ed25519') ? 'badge-accent' : 'badge-neutral'}">
                          {key.key_type}
                        </span>
                      </td>
                      <td>
                        <div class="fingerprint-cell">
                          <span class="font-mono text-xs">{key.fingerprint}</span>
                          <button 
                            class="copy-mini-btn" 
                            onclick={() => copyToClipboard(key.fingerprint, 'Fingerprint')} 
                            title="Copy Fingerprint"
                          >
                            <Copy size={11} />
                          </button>
                        </div>
                      </td>
                      <td>
                        {#if key.has_private}
                          <span class="status-chip success" title="Both private and public keys are present on disk">
                            <CheckCircle2 size={12} /> Key Pair
                          </span>
                        {:else}
                          <span class="status-chip muted" title="Only public key file exists">
                            <Info size={12} /> Pub Only
                          </span>
                        {/if}
                      </td>
                      <td>
                        <button 
                          class="btn-text" 
                          onclick={() => copyToClipboard(key.public_key, `${key.name}.pub`)}
                          title="Copy public key contents to paste into remote servers or GitHub"
                        >
                          <Copy size={13} /> Copy Public Key
                        </button>
                      </td>
                      <td style="text-align: right;">
                        <button 
                          class="btn btn-danger btn-sm" 
                          onclick={() => handleDeleteKey(key.name)}
                          title="Permanently delete this key pair from ~/.ssh/"
                        >
                          <Trash2 size={13} /> Delete
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    {/if}

    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 2: SSH CLIENT CONFIG (~/.ssh/config)
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'client_config'}
      <div class="tab-panel">
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <Laptop size={18} />
            </div>
            <div class="guide-copy">
              <h4>SSH Client Host Aliases (<code>~/.ssh/config</code>)</h4>
              <p>
                Configure server shortcuts with custom ports, usernames, and specific identity keys. Connect with a single <code>ssh &lt;alias&gt;</code> command.
              </p>
            </div>
          </div>
          <button class="btn btn-primary" onclick={() => showAddClientModal = true}>
            <Plus size={14} /> Add Host Profile
          </button>
        </div>

        <div class="card table-card">
          <div class="table-toolbar">
            <div class="search-input-wrap">
              <Search size={14} class="search-icon" />
              <input 
                type="text" 
                placeholder="Search host alias, target hostname, or user..." 
                bind:value={clientSearch} 
              />
              {#if clientSearch}
                <button class="clear-search-btn" onclick={() => clientSearch = ''}><X size={13} /></button>
              {/if}
            </div>
            <span class="table-count-label">Showing {filteredClientHosts.length} of {clientHosts.length} profiles</span>
          </div>

          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>HOST ALIAS</th>
                  <th>TARGET HOSTNAME / IP</th>
                  <th>USER</th>
                  <th>PORT</th>
                  <th>IDENTITY KEY</th>
                  <th>QUICK CONNECT</th>
                  <th style="text-align: right;">ACTIONS</th>
                </tr>
              </thead>
              <tbody>
                {#if filteredClientHosts.length === 0}
                  <tr>
                    <td colspan="7" class="empty-cell">
                      <div class="empty-state-box">
                        <Laptop size={32} class="empty-icon" />
                        <div class="empty-title">No Host Profiles Configured</div>
                        <div class="empty-desc">
                          {clientSearch ? 'No client hosts matched your search filter.' : 'Add your servers to ~/.ssh/config for 1-click terminal connections.'}
                        </div>
                        {#if !clientSearch}
                          <button class="btn btn-primary btn-sm mt-2" onclick={() => showAddClientModal = true}>
                            <Plus size={13} /> Add Host Profile
                          </button>
                        {/if}
                      </div>
                    </td>
                  </tr>
                {:else}
                  {#each filteredClientHosts as host}
                    <tr>
                      <td class="font-mono strong text-accent">{host.host}</td>
                      <td class="font-mono">{host.hostname}</td>
                      <td class="font-mono text-muted">{host.user || '—'}</td>
                      <td class="font-mono text-muted">{host.port || '22'}</td>
                      <td class="font-mono text-xs text-muted">
                        {#if host.identity_file}
                          <span class="badge badge-neutral">{host.identity_file}</span>
                        {:else}
                          <span>Default (~/.ssh/id_*)</span>
                        {/if}
                      </td>
                      <td>
                        <button 
                          class="btn-text font-mono" 
                          onclick={() => copyToClipboard(`ssh ${host.host}`, 'SSH command')}
                          title="Copy terminal connection command"
                        >
                          <Terminal size={12} /> ssh {host.host}
                        </button>
                      </td>
                      <td style="text-align: right;">
                        <button 
                          class="btn btn-danger btn-sm" 
                          onclick={() => handleDeleteClientHost(host.host)}
                          title="Remove host profile from config"
                        >
                          <Trash2 size={13} /> Delete
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    {/if}

    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 3: KNOWN HOSTS (~/.ssh/known_hosts)
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'known_hosts'}
      <div class="tab-panel">
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <Globe size={18} />
            </div>
            <div class="guide-copy">
              <h4>Remote Server Fingerprints (<code>~/.ssh/known_hosts</code>)</h4>
              <p>
                Stored cryptographic fingerprints of servers you have connected to. Removing an entry resolves <em>"Host key verification failed"</em> warnings after a remote OS reinstallation.
              </p>
            </div>
          </div>
          {#if knownHosts.length > 0}
            <button class="btn btn-danger btn-sm" onclick={handleClearKnownHosts}>
              <Trash2 size={13} /> Flush Known Hosts
            </button>
          {/if}
        </div>

        <div class="card table-card">
          <div class="table-toolbar">
            <div class="search-input-wrap">
              <Search size={14} class="search-icon" />
              <input 
                type="text" 
                placeholder="Search server domain, IP, or fingerprint..." 
                bind:value={knownSearch} 
              />
              {#if knownSearch}
                <button class="clear-search-btn" onclick={() => knownSearch = ''}><X size={13} /></button>
              {/if}
            </div>
            <span class="table-count-label">Showing {filteredKnownHosts.length} of {knownHosts.length} entries</span>
          </div>

          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>LINE</th>
                  <th>REMOTE SERVER / IP</th>
                  <th>ALGORITHM</th>
                  <th>FINGERPRINT / KEY DATA</th>
                  <th style="text-align: right;">ACTION</th>
                </tr>
              </thead>
              <tbody>
                {#if filteredKnownHosts.length === 0}
                  <tr>
                    <td colspan="5" class="empty-cell">
                      <div class="empty-state-box">
                        <Globe size={32} class="empty-icon" />
                        <div class="empty-title">No Known Hosts Recorded</div>
                        <div class="empty-desc">Your ~/.ssh/known_hosts file is currently clean and empty.</div>
                      </div>
                    </td>
                  </tr>
                {:else}
                  {#each filteredKnownHosts as kh}
                    <tr>
                      <td class="font-mono text-muted">#{kh.line_number}</td>
                      <td class="font-mono strong">{kh.host}</td>
                      <td>
                        <span class="badge badge-neutral">{kh.key_type}</span>
                      </td>
                      <td class="font-mono text-xs text-muted truncate-cell" title={kh.raw}>
                        {kh.fingerprint !== 'Available' ? kh.fingerprint : kh.raw.slice(0, 45) + '...'}
                      </td>
                      <td style="text-align: right;">
                        <button 
                          class="btn btn-danger btn-sm" 
                          onclick={() => handleRemoveKnownHost(kh.line_number, kh.host)}
                          title="Remove this host key to fix verification errors"
                        >
                          <Trash2 size={13} /> Remove
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    {/if}

    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 4: AUTHORIZED KEYS & SSHD HARDENING
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'authorized'}
      <div class="tab-panel">
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <ShieldCheck size={18} />
            </div>
            <div class="guide-copy">
              <h4>Authorized Remote Keys &amp; Daemon Hardening Posture</h4>
              <p>
                The <code>~/.ssh/authorized_keys</code> file controls which remote machines can log in. Keep your OpenSSH daemon hardened against brute-force attacks.
              </p>
            </div>
          </div>
          <button class="btn btn-primary" onclick={() => showAddAuthModal = true}>
            <Plus size={14} /> Add Authorized Key
          </button>
        </div>

        <!-- Authorized Keys Table Card -->
        <div class="card table-card">
          <div class="card-header-bar">
            <h3 class="card-section-title">Authorized Public Keys ({authorizedKeys.length})</h3>
          </div>

          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>LINE</th>
                  <th>ALGORITHM</th>
                  <th>IDENTITY / COMMENT</th>
                  <th>ACCESS RESTRICTIONS</th>
                  <th style="text-align: right;">ACTIONS</th>
                </tr>
              </thead>
              <tbody>
                {#if authorizedKeys.length === 0}
                  <tr>
                    <td colspan="5" class="empty-cell">
                      <div class="empty-state-box">
                        <ShieldCheck size={32} class="empty-icon" />
                        <div class="empty-title">No Authorized Keys Configured</div>
                        <div class="empty-desc">Add public keys to ~/.ssh/authorized_keys to allow incoming logins.</div>
                        <button class="btn btn-primary btn-sm mt-2" onclick={() => showAddAuthModal = true}>
                          <Plus size={13} /> Add Key
                        </button>
                      </div>
                    </td>
                  </tr>
                {:else}
                  {#each authorizedKeys as ak}
                    <tr>
                      <td class="font-mono text-muted">#{ak.line_number}</td>
                      <td><span class="badge badge-accent">{ak.key_type}</span></td>
                      <td class="font-mono strong">{ak.comment || 'No comment provided'}</td>
                      <td>
                        {#if ak.options}
                          <span class="badge badge-warning">{ak.options}</span>
                        {:else}
                          <span class="text-muted text-xs">Full Shell Access</span>
                        {/if}
                      </td>
                      <td style="text-align: right;">
                        <button 
                          class="btn btn-danger btn-sm" 
                          onclick={() => handleRemoveAuthorizedKey(ak.line_number)}
                          title="Revoke login access for this key"
                        >
                          <Trash2 size={13} /> Revoke
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>

        <!-- SSHD Hardening Cards Section -->
        {#if sshdStatus}
          <div class="card grid-card mt-4">
            <div class="card-header-bar">
              <div>
                <h3 class="card-section-title">SSH Daemon Security Posture</h3>
                <span class="card-section-sub">Active daemon settings resolved via <code>/etc/ssh/sshd_config</code></span>
              </div>
              <button 
                class="btn btn-secondary btn-sm" 
                onclick={() => loadSshdHardening(true)} 
                disabled={evaluatingSshd}
              >
                <RefreshCw size={12} class={evaluatingSshd ? 'animate-spin-slow' : ''} /> 
                <span>{evaluatingSshd ? 'Evaluating...' : 'Re-evaluate'}</span>
              </button>
            </div>

            {#if sshdStatus.error}
              <div class="alert-banner-warn">
                <div class="alert-banner-left">
                  <AlertTriangle size={16} class="text-warning flex-shrink-0" />
                  <span>{sshdStatus.error}. Enable Root Mode for live daemon configuration analysis.</span>
                </div>
                <button 
                  class="btn btn-primary btn-sm"
                  onclick={() => window.dispatchEvent(new CustomEvent('request-root-auth'))}
                >
                  <ShieldCheck size={13} /> Enable Root Mode
                </button>
              </div>
            {/if}

            <div class="sshd-grid">
              <!-- PermitRootLogin Card -->
              <div class="sshd-item">
                <div class="sshd-item-top">
                  <span class="sshd-label">PermitRootLogin</span>
                  <span class="badge {sshdStatus.permit_root_login === 'no' || sshdStatus.permit_root_login === 'prohibit-password' ? 'badge-success' : 'badge-danger'}">
                    {sshdStatus.permit_root_login}
                  </span>
                </div>
                <span class="sshd-desc">
                  {sshdStatus.permit_root_login === 'prohibit-password' ? 'Root password logins blocked (keys only)' : sshdStatus.permit_root_login === 'no' ? 'Direct root remote access completely disabled' : 'Direct root superuser logins permitted'}
                </span>
              </div>

              <!-- PasswordAuthentication Card -->
              <div class="sshd-item">
                <div class="sshd-item-top">
                  <span class="sshd-label">PasswordAuthentication</span>
                  <span class="badge {sshdStatus.password_authentication === 'no' ? 'badge-success' : 'badge-warning'}">
                    {sshdStatus.password_authentication}
                  </span>
                </div>
                <span class="sshd-desc">
                  {sshdStatus.password_authentication === 'no' ? 'Strict key-only authentication enforced' : 'Interactive password login enabled'}
                </span>
              </div>

              <!-- PubkeyAuthentication Card -->
              <div class="sshd-item">
                <div class="sshd-item-top">
                  <span class="sshd-label">PubkeyAuthentication</span>
                  <span class="badge {sshdStatus.pubkey_authentication === 'yes' ? 'badge-success' : 'badge-danger'}">
                    {sshdStatus.pubkey_authentication}
                  </span>
                </div>
                <span class="sshd-desc">
                  {sshdStatus.pubkey_authentication === 'yes' ? 'Cryptographic public key login active' : 'Public key authentication disabled'}
                </span>
              </div>

              <!-- X11Forwarding Card -->
              <div class="sshd-item">
                <div class="sshd-item-top">
                  <span class="sshd-label">X11Forwarding</span>
                  <span class="badge {sshdStatus.x11_forwarding === 'no' ? 'badge-success' : 'badge-warning'}">
                    {sshdStatus.x11_forwarding}
                  </span>
                </div>
                <span class="sshd-desc">
                  {sshdStatus.x11_forwarding === 'no' ? 'GUI window tunneling over SSH blocked' : 'X11 GUI tunneling over SSH permitted'}
                </span>
              </div>

              <!-- SSH Port Card -->
              <div class="sshd-item">
                <div class="sshd-item-top">
                  <span class="sshd-label">SSH Port</span>
                  <span class="badge badge-neutral font-mono">{sshdStatus.port}</span>
                </div>
                <span class="sshd-desc">
                  {sshdStatus.port === '22' ? 'Standard listening port (22/TCP)' : `Custom listening port (${sshdStatus.port}/TCP)`}
                </span>
              </div>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 5: SSL/TLS CERTIFICATES
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'certs'}
      <div class="tab-panel">
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <FileKey size={18} />
            </div>
            <div class="guide-copy">
              <h4>SSL / TLS Web &amp; Server Certificates</h4>
              <p>
                Discovered certificates across Let's Encrypt (<code>/etc/letsencrypt/live</code>), Nginx, and local trust stores. Includes real-time expiration monitoring.
              </p>
            </div>
          </div>
          <button class="btn btn-primary" onclick={() => showTestSslModal = true}>
            <Globe size={14} /> Test Live TLS Port
          </button>
        </div>

        <div class="card table-card">
          <div class="table-toolbar">
            <div class="search-input-wrap">
              <Search size={14} class="search-icon" />
              <input 
                type="text" 
                placeholder="Search domain, issuer, or SANs..." 
                bind:value={certSearch} 
              />
              {#if certSearch}
                <button class="clear-search-btn" onclick={() => certSearch = ''}><X size={13} /></button>
              {/if}
            </div>

            <div class="filter-chip-group">
              <button 
                class="filter-chip" 
                class:active={certFilter === 'all'} 
                onclick={() => certFilter = 'all'}
              >
                All ({sslCerts.length})
              </button>
              <button 
                class="filter-chip" 
                class:active={certFilter === 'valid'} 
                onclick={() => certFilter = 'valid'}
              >
                Valid ({sslCerts.filter(c => !c.is_expired).length})
              </button>
              <button 
                class="filter-chip" 
                class:active={certFilter === 'expiring'} 
                onclick={() => certFilter = 'expiring'}
              >
                ⚠️ Expiring / Expired ({expiringCertsCount})
              </button>
            </div>
          </div>

          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>DOMAIN / SUBJECT</th>
                  <th>ISSUER</th>
                  <th>EXPIRY COUNTDOWN</th>
                  <th>ALTERNATIVE NAMES (SANS)</th>
                  <th>CERTIFICATE PATH</th>
                  <th style="text-align: right;">ACTIONS</th>
                </tr>
              </thead>
              <tbody>
                {#if filteredSslCerts.length === 0}
                  <tr>
                    <td colspan="6" class="empty-cell">
                      <div class="empty-state-box">
                        <FileKey size={32} class="empty-icon" />
                        <div class="empty-title">No SSL Certificates Found</div>
                        <div class="empty-desc">
                          {certSearch ? 'No certificates matched your search filter.' : 'No active server certificates found in /etc/letsencrypt, /etc/nginx/ssl, or ~/.local/share/mkcert.'}
                        </div>
                        <button class="btn btn-secondary btn-sm mt-2" onclick={() => showTestSslModal = true}>
                          <Globe size={13} /> Test Remote TLS Endpoint
                        </button>
                      </div>
                    </td>
                  </tr>
                {:else}
                  {#each filteredSslCerts as cert}
                    <tr>
                      <td>
                        <div class="domain-cell">
                          <span class="font-mono strong">{cert.name}</span>
                          <span class="text-xs text-muted">{cert.subject}</span>
                        </div>
                      </td>
                      <td class="text-sm text-secondary">{cert.issuer}</td>
                      <td>
                        {#if cert.is_expired}
                          <span class="status-chip danger">
                            <XCircle size={12} /> Expired
                          </span>
                        {:else if cert.is_expiring_soon}
                          <span class="status-chip warning" title="Expires in less than 30 days">
                            <Clock size={12} /> {cert.days_valid} days left
                          </span>
                        {:else}
                          <span class="status-chip success">
                            <CheckCircle2 size={12} /> {cert.days_valid} days left
                          </span>
                        {/if}
                      </td>
                      <td>
                        {#if cert.san_domains.length > 0}
                          <div class="san-pill-wrap">
                            {#each cert.san_domains.slice(0, 3) as san}
                              <span class="san-pill">{san}</span>
                            {/each}
                            {#if cert.san_domains.length > 3}
                              <span class="san-pill muted">+{cert.san_domains.length - 3} more</span>
                            {/if}
                          </div>
                        {:else}
                          <span class="text-xs text-muted">Primary domain only</span>
                        {/if}
                      </td>
                      <td class="font-mono text-xs text-muted truncate-cell" title={cert.path}>
                        {cert.path}
                      </td>
                      <td style="text-align: right;">
                        <button 
                          class="btn btn-secondary btn-sm" 
                          onclick={() => copyToClipboard(cert.path, 'Certificate Path')}
                          title="Copy file path to clipboard"
                        >
                          <Copy size={12} /> Path
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    {/if}

    <!-- ══════════════════════════════════════════════════════════════════════════
         TAB 6: THREAT DEFENSE & FAIL2BAN
         ══════════════════════════════════════════════════════════════════════════ -->
    {#if activeTab === 'threats'}
      <div class="tab-panel">
        <div class="guide-banner">
          <div class="guide-left">
            <div class="guide-icon-badge">
              <ShieldAlert size={18} />
            </div>
            <div class="guide-copy">
              <h4>Fail2ban Intrusion Prevention &amp; IP Jail Defense</h4>
              <p>
                Automatically scans authentication logs for abusive login attempts and updates firewall rules to drop malicious traffic.
              </p>
            </div>
          </div>
          <div class="header-actions">
            {#if fail2banStatus?.is_installed}
              <button 
                class="btn btn-secondary btn-sm" 
                onclick={() => handleControlFail2ban('restart')}
                title="Restart fail2ban service daemon"
              >
                <RotateCw size={13} /> Restart Service
              </button>
              <button class="btn btn-primary btn-sm" onclick={() => {
                if (fail2banStatus?.jails && fail2banStatus.jails.length > 0) {
                  banJailName = fail2banStatus.jails[0].jail_name;
                } else {
                  banJailName = 'sshd';
                }
                showBanModal = true;
              }}>
                <Ban size={13} /> Manually Ban IP
              </button>
            {/if}
          </div>
        </div>

        {#if !fail2banStatus?.is_installed}
          <div class="card p-5 text-center">
            <ShieldAlert size={36} class="text-muted mx-auto mb-2" />
            <h3 class="font-semibold text-lg text-primary mb-1">Fail2ban Is Not Installed</h3>
            <p class="text-sm text-muted max-w-md mx-auto mb-4">
              Install Fail2ban on your host system to automatically block abusive SSH brute-force bots and protect your server.
            </p>
            <div class="font-mono text-xs bg-black/20 p-3 rounded-lg border border-border inline-block">
              sudo dnf install -y fail2ban &amp;&amp; sudo systemctl enable --now fail2ban
            </div>
          </div>
        {:else}
          <!-- Service Status Card -->
          <div class="card status-overview-card mb-4">
            <div class="status-overview-left">
              <div class="status-indicator-dot {fail2banStatus.is_active ? 'online' : 'offline'}"></div>
              <div>
                <div class="font-semibold text-primary text-sm">
                  Fail2ban Daemon: {fail2banStatus.is_active ? 'Running & Active' : 'Stopped / Inactive'}
                </div>
                <div class="text-xs text-muted">
                  Monitoring {fail2banStatus.jails.length} security jail{fail2banStatus.jails.length !== 1 ? 's' : ''} with {fail2banStatus.total_banned_ips} active IP ban{fail2banStatus.total_banned_ips !== 1 ? 's' : ''}
                </div>
              </div>
            </div>
            {#if !fail2banStatus.is_active}
              <button class="btn btn-primary btn-sm" onclick={() => handleControlFail2ban('start')}>
                <Play size={13} /> Start Fail2ban
              </button>
            {/if}
          </div>

          <!-- Jails List Grid -->
          <div class="jails-grid">
            {#each fail2banStatus.jails as jail}
              <div class="card jail-card">
                <div class="jail-header">
                  <div class="jail-title-group">
                    <Shield size={16} class="text-accent" />
                    <span class="jail-name font-mono">{jail.jail_name}</span>
                  </div>
                  <span class="badge {jail.currently_banned > 0 ? 'badge-danger' : 'badge-neutral'}">
                    {jail.currently_banned} banned
                  </span>
                </div>

                <div class="jail-body">
                  <div class="jail-stat-row">
                    <span class="text-xs text-muted">Total Filtered Bans:</span>
                    <span class="text-xs font-semibold text-primary">{jail.total_banned}</span>
                  </div>

                  <div class="banned-ips-section">
                    <span class="text-xs text-secondary font-medium mb-1 block">Active IP Blocks:</span>
                    {#if jail.banned_ips.length === 0}
                      <span class="text-xs text-muted italic">No active bans in this jail</span>
                    {:else}
                      <div class="ip-pills-wrap">
                        {#each jail.banned_ips as ip}
                          <span class="banned-ip-pill">
                            <span>{ip}</span>
                            <button 
                              class="unban-btn" 
                              onclick={() => handleUnbanIp(jail.jail_name, ip)} 
                              title="Unban IP {ip}"
                            >
                              <X size={11} />
                            </button>
                          </span>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<!-- ════════════════════════════════════════════════════════════════════════════
     MODALS
     ════════════════════════════════════════════════════════════════════════════ -->

<!-- 1. Generate SSH Key Modal -->
{#if showGenModal}
  <div use:portal class="modal-backdrop" onclick={() => showGenModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <Key size={18} class="text-accent" />
          <h3>Generate New SSH Key Pair</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showGenModal = false}><X size={16} /></button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="gen-key-type">Cryptographic Algorithm</label>
          <div class="radio-pill-group">
            <button 
              type="button"
              class="radio-pill-btn" 
              class:active={genKeyType === 'ed25519'} 
              onclick={() => { genKeyType = 'ed25519'; genFilename = 'id_ed25519_custom'; }}
            >
              <strong>Ed25519</strong> (Recommended • Fast &amp; Secure)
            </button>
            <button 
              type="button"
              class="radio-pill-btn" 
              class:active={genKeyType === 'rsa'} 
              onclick={() => { genKeyType = 'rsa'; genFilename = 'id_rsa_custom'; }}
            >
              <strong>RSA (4096-bit)</strong> (Legacy Compatibility)
            </button>
          </div>
        </div>

        <div class="form-group">
          <label for="gen-key-name">Key Filename (in ~/.ssh/)</label>
          <input id="gen-key-name" type="text" class="form-input font-mono" bind:value={genFilename} placeholder="id_ed25519_project" />
          <span class="field-hint">Files <code>~/.ssh/{genFilename}</code> and <code>~/.ssh/{genFilename}.pub</code> will be created.</span>
        </div>

        <div class="form-group">
          <label for="gen-key-comment">Comment / Identity Tag</label>
          <input id="gen-key-comment" type="text" class="form-input" bind:value={genComment} placeholder="user@linux-control-panel" />
        </div>

        <div class="form-group">
          <label for="gen-key-pass">Passphrase (Optional)</label>
          <input id="gen-key-pass" type="password" class="form-input" bind:value={genPassphrase} placeholder="Leave empty for passwordless key" />
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showGenModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={handleGenerateKey}>
          <Key size={14} /> Generate Key Pair
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- 2. Generated Key Result Modal -->
{#if generatedKeyResult}
  <div use:portal class="modal-backdrop" onclick={() => generatedKeyResult = null} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <CheckCircle2 size={18} class="text-success" />
          <h3 class="text-success">SSH Key Pair Created!</h3>
        </div>
        <button class="modal-close-btn" onclick={() => generatedKeyResult = null}><X size={16} /></button>
      </div>

      <div class="modal-body">
        <p class="text-sm text-secondary">
          Your key pair has been written to <code>~/.ssh/{generatedKeyResult.filename}</code>. Copy the public key below to add it to your server's <code>authorized_keys</code> or GitHub:
        </p>

        <div class="pubkey-preview-box font-mono text-xs">
          {generatedKeyResult.publicKey}
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => generatedKeyResult = null}>Close</button>
        <button class="btn btn-primary" onclick={() => {
          if (generatedKeyResult) copyToClipboard(generatedKeyResult.publicKey, 'Public Key');
          generatedKeyResult = null;
        }}>
          <Copy size={14} /> Copy Public Key &amp; Close
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- 3. Add Client Host Modal -->
{#if showAddClientModal}
  <div use:portal class="modal-backdrop" onclick={() => showAddClientModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <Laptop size={18} class="text-accent" />
          <h3>Add SSH Client Host Profile</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showAddClientModal = false}><X size={16} /></button>
      </div>

      <div class="modal-body">
        <div class="form-row-2">
          <div class="form-group">
            <label for="cli-host-alias">Host Shortcut / Alias *</label>
            <input id="cli-host-alias" type="text" class="form-input font-mono" bind:value={newHostAlias} placeholder="dev-server" />
            <span class="field-hint">Usage: <code>ssh dev-server</code></span>
          </div>
          <div class="form-group">
            <label for="cli-hostname">Target IP / Domain *</label>
            <input id="cli-hostname" type="text" class="form-input font-mono" bind:value={newHostName} placeholder="192.168.1.100" />
          </div>
        </div>

        <div class="form-row-2">
          <div class="form-group">
            <label for="cli-user">Remote User</label>
            <input id="cli-user" type="text" class="form-input" bind:value={newHostUser} placeholder="root or ubuntu" />
          </div>
          <div class="form-group">
            <label for="cli-port">SSH Port</label>
            <input id="cli-port" type="text" class="form-input font-mono" bind:value={newHostPort} placeholder="22" />
          </div>
        </div>

        <div class="form-group">
          <label for="cli-key">Specific Identity Key Path (Optional)</label>
          <input id="cli-key" type="text" class="form-input font-mono" bind:value={newHostKey} placeholder="~/.ssh/id_ed25519" />
        </div>

        <div class="form-group">
          <label for="cli-proxy">ProxyJump / Bastion (Optional)</label>
          <input id="cli-proxy" type="text" class="form-input font-mono" bind:value={newHostProxy} placeholder="bastion.example.com" />
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showAddClientModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={handleAddClientHost}>
          <Check size={14} /> Save to ~/.ssh/config
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- 4. Add Authorized Key Modal -->
{#if showAddAuthModal}
  <div use:portal class="modal-backdrop" onclick={() => showAddAuthModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <ShieldCheck size={18} class="text-accent" />
          <h3>Add Remote Authorized Public Key</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showAddAuthModal = false}><X size={16} /></button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="auth-key-input">Paste Public Key (OpenSSH Format)</label>
          <textarea 
            id="auth-key-input" 
            class="form-textarea font-mono text-xs" 
            rows="4" 
            bind:value={newAuthPubKey} 
            placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... admin@laptop"
          ></textarea>
          <span class="field-hint">This key will be appended to <code>~/.ssh/authorized_keys</code> to grant inbound SSH login access.</span>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showAddAuthModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={handleAddAuthorizedKey}>
          <Plus size={14} /> Add Authorized Key
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- 5. Test Remote TLS Modal -->
{#if showTestSslModal}
  <div use:portal class="modal-backdrop" onclick={() => showTestSslModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <Globe size={18} class="text-accent" />
          <h3>Live TLS Certificate Inspector</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showTestSslModal = false}><X size={16} /></button>
      </div>

      <div class="modal-body">
        <div class="form-row-2">
          <div class="form-group" style="flex: 3;">
            <label for="tls-host">Domain or Hostname</label>
            <input id="tls-host" type="text" class="form-input font-mono" bind:value={testSslHost} placeholder="example.com or localhost" />
          </div>
          <div class="form-group" style="flex: 1;">
            <label for="tls-port">Port</label>
            <input id="tls-port" type="number" class="form-input font-mono" bind:value={testSslPort} placeholder="443" />
          </div>
        </div>

        <button 
          class="btn btn-primary w-full justify-center" 
          onclick={handleTestRemoteSsl} 
          disabled={testSslLoading}
        >
          <RefreshCw size={13} class={testSslLoading ? 'animate-spin-slow' : ''} />
          <span>{testSslLoading ? 'Querying TLS Handshake...' : 'Inspect Live Certificate'}</span>
        </button>

        {#if testSslError}
          <div class="alert-banner-danger mt-3">
            <AlertTriangle size={15} class="text-error flex-shrink-0" />
            <span class="text-xs">{testSslError}</span>
          </div>
        {/if}

        {#if testSslResult}
          <div class="tls-result-card mt-3">
            <div class="tls-result-header">
              <span class="font-semibold text-primary">{testSslResult.name}</span>
              {#if testSslResult.is_expired}
                <span class="badge badge-danger">Expired</span>
              {:else}
                <span class="badge badge-success">{testSslResult.days_valid} days remaining</span>
              {/if}
            </div>
            <div class="tls-result-detail">
              <div><strong>Issuer:</strong> {testSslResult.issuer}</div>
              <div><strong>Valid Range:</strong> {testSslResult.not_before} &rarr; {testSslResult.not_after}</div>
              {#if testSslResult.san_domains.length > 0}
                <div class="mt-1">
                  <strong>SANs:</strong>
                  <div class="san-pill-wrap mt-1">
                    {#each testSslResult.san_domains as s}
                      <span class="san-pill">{s}</span>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showTestSslModal = false}>Close</button>
      </div>
    </div>
  </div>
{/if}

<!-- 6. Manually Ban IP Modal -->
{#if showBanModal}
  <div use:portal class="modal-backdrop" onclick={() => showBanModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
      <div class="modal-header">
        <div class="modal-title-group">
          <Ban size={18} class="text-danger" />
          <h3>Manually Ban Threat IP Address</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showBanModal = false}><X size={16} /></button>
      </div>

      <div class="modal-body">
        {#if !fail2banStatus?.is_active}
          <div class="alert-banner-warn" style="margin: 0 0 8px 0;">
            <div class="alert-banner-left">
              <AlertTriangle size={15} class="text-warning flex-shrink-0" />
              <span>Fail2ban service is currently inactive. Start the service to enforce active IP bans.</span>
            </div>
            <button class="btn btn-primary btn-sm" onclick={() => handleControlFail2ban('start')}>
              <Play size={12} /> Start Service
            </button>
          </div>
        {/if}

        <div class="form-group">
          <label for="ban-jail-sel">Target Fail2ban Jail</label>
          <select id="ban-jail-sel" class="form-select font-mono" bind:value={banJailName}>
            {#if fail2banStatus?.jails && fail2banStatus.jails.length > 0}
              {#each fail2banStatus.jails as j}
                <option value={j.jail_name}>{j.jail_name} (Active Jail • {j.currently_banned} banned)</option>
              {/each}
            {:else}
              <option value="sshd">sshd (Default SSH Jail)</option>
              <option value="recidive">recidive (Persistent Offender Jail)</option>
              <option value="nginx-http-auth">nginx-http-auth (HTTP Auth Jail)</option>
              <option value="nginx-botsearch">nginx-botsearch (Bot Filter Jail)</option>
            {/if}
          </select>
          <span class="field-hint">Select the firewall filter jail that will drop traffic from this IP address.</span>
        </div>

        <div class="form-group">
          <label for="ban-ip-input">Offending IP Address</label>
          <input id="ban-ip-input" type="text" class="form-input font-mono" bind:value={banIpAddress} placeholder="192.0.2.1" />
          <span class="field-hint">The IP will be added to the chosen jail and dropped by your host firewall (iptables / nftables).</span>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showBanModal = false}>Cancel</button>
        <button class="btn btn-danger" onclick={handleManualBan} disabled={!banIpAddress.trim()}>
          <Ban size={14} /> Ban IP Address
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Layout Root ─────────────────────────────────────────────────────────── */
  .module-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    gap: 18px;
    padding: 24px;
    box-sizing: border-box;
    background: transparent;
  }

  .tab-main-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 24px;
  }

  .tab-panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* ── KPI Stat Cards ──────────────────────────────────────────────────────── */
  .kpi-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 12px;
  }

  @media (max-width: 1080px) {
    .kpi-grid {
      grid-template-columns: repeat(3, 1fr);
    }
  }

  @media (max-width: 720px) {
    .kpi-grid {
      grid-template-columns: 1fr;
    }
  }

  .kpi-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .kpi-card {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .kpi-card:hover {
    border-color: var(--color-accent);
    transform: translateY(-1px);
  }

  .kpi-icon-wrap {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .kpi-icon-wrap.accent { background: rgba(0, 218, 243, 0.12); color: var(--color-accent); }
  .kpi-icon-wrap.primary { background: rgba(59, 130, 246, 0.12); color: #3B82F6; }
  .kpi-icon-wrap.info { background: rgba(14, 165, 233, 0.12); color: #0EA5E9; }
  .kpi-icon-wrap.success { background: rgba(16, 185, 129, 0.12); color: var(--color-success); }
  .kpi-icon-wrap.warning { background: rgba(245, 158, 11, 0.12); color: var(--color-warning); }
  .kpi-icon-wrap.muted { background: rgba(255, 255, 255, 0.06); color: var(--color-text-muted); }

  .kpi-body {
    min-width: 0;
  }

  .kpi-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .kpi-value {
    font-size: 19px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.2;
  }

  .kpi-desc {
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Tab Bar Navigation ─────────────────────────────────────────────────── */
  .tab-bar-nav {
    display: flex;
    gap: 6px;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 2px;
  }

  :global(html.light-mode) .tab-bar-nav {
    border-bottom-color: #E2E8F0;
  }

  .tab-nav-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 9px 14px;
    border-radius: 8px 8px 0 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tab-nav-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.04);
  }

  :global(html.light-mode) .tab-nav-btn:hover {
    background: #F1F5F9;
  }

  .tab-nav-btn.active {
    color: var(--color-accent);
    background: var(--color-bg-card);
    border-color: var(--color-border);
    border-bottom: 1px solid var(--color-bg-card);
    margin-bottom: -1px;
  }

  :global(html.light-mode) .tab-nav-btn.active {
    color: #2563EB;
    background: #FFFFFF;
    border-color: #E2E8F0;
    border-bottom-color: #FFFFFF;
  }

  .tab-pill {
    font-size: 10.5px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
  }

  .tab-nav-btn.active .tab-pill {
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
  }

  :global(html.light-mode) .tab-nav-btn.active .tab-pill {
    background: #EFF6FF;
    color: #2563EB;
  }

  .tab-pill.pill-warn {
    background: rgba(245, 158, 11, 0.2);
    color: var(--color-warning);
  }

  .tab-pill.pill-danger {
    background: rgba(239, 68, 68, 0.2);
    color: var(--color-error);
  }

  /* ── Guide Banners ──────────────────────────────────────────────────────── */
  .guide-banner {
    background: rgba(0, 218, 243, 0.04);
    border: 1px solid rgba(0, 218, 243, 0.15);
    border-radius: 12px;
    padding: 14px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  :global(html.light-mode) .guide-banner {
    background: #EFF6FF;
    border-color: #BFDBFE;
  }

  .guide-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .guide-icon-badge {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: rgba(0, 218, 243, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
  }

  :global(html.light-mode) .guide-icon-badge {
    background: #DBEAFE;
    color: #2563EB;
  }

  .guide-copy h4 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .guide-copy p {
    margin: 2px 0 0 0;
    font-size: 11.5px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  /* ── Table Card & Toolbar ───────────────────────────────────────────────── */
  .card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
  }

  :global(html.light-mode) .card {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .table-toolbar {
    padding: 12px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.08);
  }

  :global(html.light-mode) .table-toolbar {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .search-input-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px 10px;
    width: 320px;
    max-width: 100%;
  }

  :global(html.light-mode) .search-input-wrap {
    background: #FFFFFF;
    border-color: #E2E8F0;
  }

  .search-icon {
    color: var(--color-text-muted);
  }

  .search-input-wrap input {
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 12px;
    width: 100%;
  }

  .clear-search-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 0;
  }

  .table-count-label {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .filter-chip-group {
    display: flex;
    gap: 6px;
  }

  .filter-chip {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--color-text-muted);
    background: transparent;
    border: 1px solid var(--color-border);
    cursor: pointer;
    transition: all 0.15s;
  }

  :global(html.light-mode) .filter-chip {
    border-color: #E2E8F0;
  }

  .filter-chip.active {
    background: rgba(0, 218, 243, 0.12);
    border-color: var(--color-accent);
    color: var(--color-accent);
    font-weight: 600;
  }

  :global(html.light-mode) .filter-chip.active {
    background: #EFF6FF;
    border-color: #2563EB;
    color: #2563EB;
  }

  .table-wrap {
    overflow-x: auto;
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }

  .data-table th {
    text-align: left;
    padding: 10px 16px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    background: rgba(0, 0, 0, 0.12);
    border-bottom: 1px solid var(--color-border);
  }

  :global(html.light-mode) .data-table th {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .data-table td {
    padding: 12px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    vertical-align: middle;
  }

  :global(html.light-mode) .data-table td {
    border-bottom-color: #F1F5F9;
  }

  .data-table tr:hover td {
    background: rgba(255, 255, 255, 0.02);
  }

  :global(html.light-mode) .data-table tr:hover td {
    background: #F8FAFC;
  }

  /* ── Table Cells & Chips ────────────────────────────────────────────────── */
  .key-name-col {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .fingerprint-cell {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .copy-mini-btn {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 2px 5px;
    color: var(--color-text-muted);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }

  .copy-mini-btn:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }

  .status-chip.success { background: rgba(16, 185, 129, 0.12); color: var(--color-success); }
  .status-chip.warning { background: rgba(245, 158, 11, 0.12); color: var(--color-warning); }
  .status-chip.danger { background: rgba(239, 68, 68, 0.12); color: var(--color-error); }
  .status-chip.muted { background: rgba(255, 255, 255, 0.06); color: var(--color-text-muted); }

  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 7px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }

  .badge-accent { background: rgba(0, 218, 243, 0.12); color: var(--color-accent); }
  :global(html.light-mode) .badge-accent { background: #EFF6FF; color: #2563EB; }
  .badge-neutral { background: rgba(255, 255, 255, 0.08); color: var(--color-text-secondary); }
  :global(html.light-mode) .badge-neutral { background: #F1F5F9; color: #475569; }
  .badge-success { background: rgba(16, 185, 129, 0.15); color: var(--color-success); }
  .badge-warning { background: rgba(245, 158, 11, 0.15); color: var(--color-warning); }
  .badge-danger { background: rgba(239, 68, 68, 0.15); color: var(--color-error); }

  .san-pill-wrap {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .san-pill {
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 1px 5px;
    color: var(--color-text-secondary);
  }

  .san-pill.muted {
    color: var(--color-text-muted);
  }

  .domain-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .truncate-cell {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Empty State ────────────────────────────────────────────────────────── */
  .empty-cell {
    text-align: center;
    padding: 40px 16px !important;
  }

  .empty-state-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .empty-icon {
    color: var(--color-text-muted);
    opacity: 0.5;
    margin-bottom: 4px;
  }

  .empty-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .empty-desc {
    font-size: 12px;
    color: var(--color-text-muted);
    max-width: 400px;
  }

  /* ── SSHD Hardening Cards ───────────────────────────────────────────────── */
  .card-header-bar {
    padding: 14px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.06);
  }

  :global(html.light-mode) .card-header-bar {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .card-section-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .card-section-sub {
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin-top: 1px;
    display: block;
  }

  .alert-banner-warn {
    margin: 14px 18px 0 18px;
    padding: 10px 14px;
    background: rgba(245, 158, 11, 0.1);
    border: 1px solid rgba(245, 158, 11, 0.25);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 12px;
  }

  .alert-banner-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .alert-banner-danger {
    padding: 10px 14px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.25);
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sshd-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
    padding: 16px 18px;
  }

  .sshd-item {
    background: rgba(0, 0, 0, 0.15);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .sshd-item {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .sshd-item:hover {
    border-color: var(--color-accent);
  }

  .sshd-item-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }

  .sshd-label {
    font-size: 13px;
    color: var(--color-text-primary);
    font-weight: 600;
  }

  .sshd-desc {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  /* ── Threat Defense / Jails ─────────────────────────────────────────────── */
  .status-overview-card {
    padding: 14px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .status-overview-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .status-indicator-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .status-indicator-dot.online { background: var(--color-success); box-shadow: 0 0 8px var(--color-success); }
  .status-indicator-dot.offline { background: var(--color-error); }

  .jails-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 14px;
  }

  .jail-card {
    padding: 16px;
  }

  .jail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .jail-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .jail-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .jail-stat-row {
    display: flex;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .ip-pills-wrap {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }

  .banned-ip-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.25);
    color: var(--color-error);
    padding: 2px 8px;
    border-radius: 6px;
  }

  .unban-btn {
    background: transparent;
    border: none;
    color: var(--color-error);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 0;
  }

  /* ── Modals & Controls ──────────────────────────────────────────────────── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99990;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .modal-card {
    width: 540px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 40px);
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :global(html.light-mode) .modal-card {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.15);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.12);
  }

  :global(html.light-mode) .modal-header {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .modal-title-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .modal-title-group h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .modal-close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }

  .modal-close-btn:hover {
    color: var(--color-text-primary);
  }

  .modal-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
  }

  .modal-footer {
    padding: 14px 20px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.08);
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }

  :global(html.light-mode) .modal-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .form-group label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .form-input, .form-select, .form-textarea {
    width: 100%;
    padding: 8px 12px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    color: var(--color-text-primary);
    font-size: 12.5px;
    outline: none;
    box-sizing: border-box;
  }

  :global(html.light-mode) .form-input,
  :global(html.light-mode) .form-select,
  :global(html.light-mode) .form-textarea {
    background: #FFFFFF;
    border-color: #CBD5E1;
  }

  .form-input:focus, .form-select:focus, .form-textarea:focus {
    border-color: var(--color-accent);
  }

  .form-row-2 {
    display: flex;
    gap: 12px;
  }

  .radio-pill-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .radio-pill-btn {
    padding: 10px 14px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.1);
    color: var(--color-text-muted);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
    transition: all 0.15s;
  }

  :global(html.light-mode) .radio-pill-btn {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .radio-pill-btn.active {
    border-color: var(--color-accent);
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .radio-pill-btn.active {
    background: #EFF6FF;
    border-color: #2563EB;
    color: #0F172A;
  }

  .field-hint {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .pubkey-preview-box {
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px;
    word-break: break-all;
    user-select: all;
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .pubkey-preview-box {
    background: #F8FAFC;
    border-color: #CBD5E1;
  }

  .tls-result-card {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px;
  }

  :global(html.light-mode) .tls-result-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .tls-result-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    font-size: 13px;
  }

  .tls-result-detail {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* ── General UI Helpers ─────────────────────────────────────────────────── */
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }

  .btn-primary {
    background: var(--color-accent);
    color: #00363d;
  }

  :global(html.light-mode) .btn-primary {
    background: #2563EB;
    color: #FFFFFF;
  }

  .btn-primary:hover {
    filter: brightness(1.1);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .btn-secondary {
    background: #F8FAFC;
    border-color: #E2E8F0;
    color: #0F172A;
  }

  .btn-secondary:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .btn-danger {
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.25);
    color: var(--color-error);
  }

  .btn-danger:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  .btn-sm {
    padding: 5px 10px;
    font-size: 11.5px;
  }

  .btn-text {
    background: transparent;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0;
  }

  :global(html.light-mode) .btn-text {
    color: #2563EB;
  }

  .btn-text:hover {
    text-decoration: underline;
  }

  .font-mono { font-family: var(--font-mono, monospace); }
  .strong { font-weight: 600; color: var(--color-text-primary); }
  .text-accent { color: var(--color-accent); }
  :global(html.light-mode) .text-accent { color: #2563EB; }
  .text-success { color: var(--color-success); }
  .text-warning { color: var(--color-warning); }
  .text-error { color: var(--color-error); }
  .text-primary { color: var(--color-text-primary); }
  .text-secondary { color: var(--color-text-secondary); }
  .text-muted { color: var(--color-text-muted); }
  .text-xs { font-size: 11px; }
  .text-sm { font-size: 12px; }
  .mt-2 { margin-top: 8px; }
  .mt-3 { margin-top: 12px; }
  .mt-4 { margin-top: 16px; }
  .mb-1 { margin-bottom: 4px; }
  .mb-2 { margin-bottom: 8px; }
  .mb-4 { margin-bottom: 16px; }
  .w-full { width: 100%; }
  .justify-center { justify-content: center; }
  .flex-shrink-0 { flex-shrink: 0; }

  .animate-spin-slow {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
