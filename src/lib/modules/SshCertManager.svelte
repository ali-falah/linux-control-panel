<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { 
    Key, ShieldCheck, Lock, FileKey, AlertTriangle, CheckCircle2, XCircle, 
    Plus, Trash2, Copy, RefreshCw, HelpCircle, Server, Eye, ExternalLink, ShieldAlert
  } from '@lucide/svelte';
  import { invokeSafe } from '../utils/ipc';
  import { uiStore } from '../stores/ui.svelte.ts';
  import TabGroup from '../components/ui/TabGroup.svelte';

  // Types
  interface SshKeyItem {
    name: string;
    key_type: string;
    path: string;
    pub_key_path?: string;
    fingerprint: string;
    public_key: string;
    has_private: boolean;
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
  }

  interface SslCertItem {
    name: string;
    subject: string;
    issuer: string;
    not_before: string;
    not_after: string;
    days_valid: number;
    path: string;
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

  // Active State
  let activeTab = $state<'keys' | 'authorized' | 'certs' | 'threats'>('keys');
  let loading = $state(false);

  // Data Stores
  let sshKeys = $state<SshKeyItem[]>([]);
  let authorizedKeys = $state<AuthorizedKeyItem[]>([]);
  let sshdStatus = $state<SshdHardeningStatus | null>(null);
  let sslCerts = $state<SslCertItem[]>([]);
  let fail2banStatus = $state<Fail2banStatus | null>(null);

  // Key Generation Modal State
  let showGenModal = $state(false);
  let genKeyType = $state<'ed25519' | 'rsa'>('ed25519');
  let genBits = $state(4096);
  let genFilename = $state('id_ed25519_custom');
  let genComment = $state('user@linux-control-panel');
  let genPassphrase = $state('');
  let generatedKeyResult = $state<{ filename: string; publicKey: string } | null>(null);

  // Add Authorized Key Modal State
  let showAddAuthModal = $state(false);
  let newAuthPubKey = $state('');

  // Initial Load
  onMount(() => {
    loadAllData();
  });

  async function loadAllData() {
    loading = true;
    await Promise.all([
      loadSshKeys(),
      loadAuthorizedKeys(),
      loadSshdHardening(),
      loadSslCerts(),
      loadFail2banStatus(),
    ]);
    loading = false;
  }

  async function loadSshKeys() {
    const res = await invokeSafe<SshKeyItem[]>('vault_list_ssh_keys', {}, { quiet: true });
    if (res) sshKeys = [...res];
  }

  async function loadAuthorizedKeys() {
    const res = await invokeSafe<AuthorizedKeyItem[]>('vault_list_authorized_keys', {}, { quiet: true });
    if (res) authorizedKeys = [...res];
  }

  async function loadSshdHardening() {
    const res = await invokeSafe<SshdHardeningStatus>('vault_get_sshd_hardening', {}, { quiet: true });
    if (res) sshdStatus = res;
  }

  async function loadSslCerts() {
    const res = await invokeSafe<SslCertItem[]>('vault_list_ssl_certs', {}, { quiet: true });
    if (res) sslCerts = [...res];
  }

  async function loadFail2banStatus() {
    const res = await invokeSafe<Fail2banStatus>('vault_get_fail2ban_status', {}, { quiet: true });
    if (res) fail2banStatus = res;
  }

  /**
   * Force a focused reload of SSH keys after a mutation.
   * Uses tick() + microtask delay to guarantee Svelte reactivity triggers.
   */
  async function reloadAfterMutation() {
    // Small delay to let filesystem settle after write/delete
    await new Promise(r => setTimeout(r, 150));
    
    const freshKeys = await invokeSafe<SshKeyItem[]>('vault_list_ssh_keys', {}, { quiet: true });
    if (freshKeys) {
      sshKeys = freshKeys;
    }
    
    const freshAuth = await invokeSafe<AuthorizedKeyItem[]>('vault_list_authorized_keys', {}, { quiet: true });
    if (freshAuth) {
      authorizedKeys = freshAuth;
    }
    
    await tick();
  }

  async function handleGenerateKey() {
    const filename = genFilename.trim();
    if (!filename) {
      uiStore.showToast('Please enter a valid filename for the key pair', 'error');
      return;
    }

    showGenModal = false;

    const res = await invokeSafe<string>('vault_generate_ssh_key', {
      keyType: genKeyType,
      bits: genKeyType === 'rsa' ? genBits : null,
      filename,
      comment: genComment.trim() || null,
      passphrase: genPassphrase || null,
    }, { showToastOnError: true });

    if (res) {
      uiStore.showToast(`SSH Key pair '${filename}' generated successfully`, 'success');
      generatedKeyResult = { filename, publicKey: res };
      genPassphrase = '';
      await reloadAfterMutation();
    } else {
      showGenModal = true;
    }
  }

  function handleDeleteKey(keyName: string) {
    uiStore.confirm(
      'Delete SSH Key Pair',
      `Are you sure you want to permanently delete the SSH key pair '${keyName}' from ~/.ssh/? This action cannot be undone.`,
      async () => {
        const res = await invokeSafe<string>('vault_delete_ssh_key', { name: keyName }, { showToastOnError: true });
        if (res) {
          uiStore.showToast(`SSH key pair '${keyName}' deleted successfully`, 'success');
          await reloadAfterMutation();
        }
      },
      true
    );
  }

  async function handleAddAuthorizedKey() {
    const keyStr = newAuthPubKey.trim();
    if (!keyStr) {
      uiStore.showToast('Public key content cannot be empty', 'error');
      return;
    }
    showAddAuthModal = false;

    const res = await invokeSafe<string>('vault_add_authorized_key', {
      pubKey: keyStr,
    }, { showToastOnError: true });

    if (res) {
      uiStore.showToast('Authorized key added successfully', 'success');
      newAuthPubKey = '';
      await reloadAfterMutation();
    } else {
      showAddAuthModal = true;
    }
  }

  function handleRemoveAuthorizedKey(lineNum: number) {
    uiStore.confirm(
      'Revoke Authorized Key',
      `Are you sure you want to revoke and remove authorized key on line #${lineNum}? Remote logins using this key will be blocked.`,
      async () => {
        const res = await invokeSafe<string>('vault_remove_authorized_key', {
          lineNumber: lineNum,
        }, { showToastOnError: true });

        if (res) {
          uiStore.showToast('Authorized key removed', 'success');
          await reloadAfterMutation();
        }
      },
      true
    );
  }

  async function handleUnbanIp(jail: string, ip: string) {
    const res = await invokeSafe<string>('vault_unban_ip', { jail, ip }, { showToastOnError: true });
    if (res) {
      uiStore.showToast(`IP ${ip} unbanned from jail ${jail}`, 'success');
      loadFail2banStatus();
    }
  }

  function copyToClipboard(text: string, label: string) {
    navigator.clipboard.writeText(text);
    uiStore.showToast(`${label} copied to clipboard`, 'info');
  }
</script>

<div class="vault-container">
  <!-- Top Header Banner with Guide -->
  <div class="header-card">
    <div class="header-left">
      <div class="header-icon-box">
        <ShieldCheck size={24} color="var(--color-accent)" />
      </div>
      <div>
        <h2 class="header-title">Zero-Trust Security & Certificate Vault</h2>
        <p class="header-sub">
          Centralized control hub for SSH key pairs, remote access authorized keys, SSH daemon hardening, SSL/TLS certificate lifecycles, and Fail2ban threat defense.
        </p>
      </div>
    </div>

    <div class="header-metrics">
      <div class="metric-pill" title="Total SSH key pairs stored in ~/.ssh/">
        <Key size={13} />
        <span>Keys: <strong>{sshKeys.length}</strong></span>
      </div>
      <div class="metric-pill" title="Total public keys authorized to access this system">
        <Lock size={13} />
        <span>Authorized: <strong>{authorizedKeys.length}</strong></span>
      </div>
      <div class="metric-pill" title="Active SSL/TLS Certificates detected on host">
        <FileKey size={13} />
        <span>Certs: <strong>{sslCerts.length}</strong></span>
      </div>
      <div class="metric-pill warning" title="Currently banned IP addresses monitored by Fail2ban">
        <ShieldAlert size={13} />
        <span>Banned IPs: <strong>{fail2banStatus?.total_banned_ips || 0}</strong></span>
      </div>
      <button class="btn btn-secondary btn-icon" onclick={loadAllData} title="Refresh all security vault metrics" disabled={loading}>
        <span class={loading ? 'spin-icon' : ''}>
          <RefreshCw size={14} />
        </span>
      </button>
    </div>
  </div>

  <!-- Navigation Tabs -->
  <TabGroup
    tabs={[
      { id: 'keys', label: '🔑 SSH Key Vault', badge: sshKeys.length.toString() },
      { id: 'authorized', label: '🔒 Authorized Keys & SSHD', badge: authorizedKeys.length.toString() },
      { id: 'certs', label: '📜 SSL/TLS Certificates', badge: sslCerts.length.toString() },
      { id: 'threats', label: '🛑 Threat Defense & Fail2ban', badge: (fail2banStatus?.total_banned_ips || 0).toString() }
    ]}
    bind:activeTab={activeTab}
  />

  <!-- TAB 1: SSH KEY VAULT -->
  {#if activeTab === 'keys'}
    <div class="tab-content">
      <!-- Guide Banner -->
      <div class="guide-banner">
        <HelpCircle size={18} class="guide-icon" />
        <div class="guide-text">
          <strong>What is the SSH Key Vault?</strong>
          <p>
            SSH keys provide cryptographic authentication without transmitting passwords over the network. 
            <strong>Ed25519</strong> is the recommended modern algorithm offering maximum security with high performance. 
            Always protect private keys with a strong passphrase.
          </p>
        </div>
        <button class="btn btn-primary" onclick={() => showGenModal = true} title="Generate a new cryptographic SSH key pair">
          <Plus size={14} /> Generate New SSH Key
        </button>
      </div>

      <!-- Keys Table -->
      <div class="card table-card">
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>KEY NAME</th>
                <th>TYPE</th>
                <th>FINGERPRINT (SHA256)</th>
                <th>PRIVATE KEY</th>
                <th>PUBLIC KEY</th>
                <th>ACTIONS</th>
              </tr>
            </thead>
            <tbody>
              {#if sshKeys.length === 0}
                <tr>
                  <td colspan="6" class="empty-cell">No SSH key pairs found in ~/.ssh/</td>
                </tr>
              {:else}
                {#each sshKeys as key}
                  <tr>
                    <td class="font-mono strong">{key.name}</td>
                    <td>
                      <span class="badge {key.key_type.includes('ed25519') ? 'badge-accent' : 'badge-neutral'}" title="Algorithm type for this key pair">
                        {key.key_type}
                      </span>
                    </td>
                    <td class="font-mono text-muted text-sm">{key.fingerprint}</td>
                    <td>
                      {#if key.has_private}
                        <span class="status-chip success" title="Private key stored in ~/.ssh/{key.name}">
                          <CheckCircle2 size={12} /> Present
                        </span>
                      {:else}
                        <span class="status-chip muted" title="Public key only (no private key)">
                          <XCircle size={12} /> Pub Only
                        </span>
                      {/if}
                    </td>
                    <td>
                      <button 
                        class="btn-text" 
                        onclick={() => copyToClipboard(key.public_key, `${key.name}.pub`)}
                        title="Copy public key content to clipboard"
                      >
                        <Copy size={13} /> Copy Public Key
                      </button>
                    </td>
                    <td>
                      <div style="display:flex; gap:6px; align-items:center;">
                        <button 
                          class="btn btn-secondary btn-sm" 
                          onclick={() => copyToClipboard(key.path, 'Key path')}
                          title="Copy file location path"
                        >
                          Path
                        </button>
                        <button 
                          class="btn btn-danger btn-sm" 
                          onclick={() => handleDeleteKey(key.name)}
                          title="Permanently delete this SSH key pair (~/.ssh/{key.name})"
                        >
                          <Trash2 size={13} /> Delete Key
                        </button>
                      </div>
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

  <!-- TAB 2: AUTHORIZED KEYS & SSHD -->
  {#if activeTab === 'authorized'}
    <div class="tab-content">
      <!-- Guide Banner -->
      <div class="guide-banner">
        <HelpCircle size={18} class="guide-icon" />
        <div class="guide-text">
          <strong>Authorized Remote Keys (`~/.ssh/authorized_keys`) & SSHD Hardening</strong>
          <p>
            The <code>authorized_keys</code> file lists public keys allowed to log into this system remotely without a password. 
            Keep SSH safe by disabling root logins (<code>PermitRootLogin no</code>) and forcing public key authentication.
          </p>
        </div>
        <button class="btn btn-primary" onclick={() => showAddAuthModal = true} title="Paste and authorize a new public key">
          <Plus size={14} /> Add Authorized Key
        </button>
      </div>

      <!-- Authorized Keys Section -->
      <div class="card table-card">
        <div class="card-header">
          <h3 class="card-title">Authorized Remote Access Keys ({authorizedKeys.length})</h3>
        </div>
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>LINE</th>
                <th>KEY TYPE</th>
                <th>COMMENT / IDENTITY</th>
                <th>RESTRICTION OPTIONS</th>
                <th>ACTIONS</th>
              </tr>
            </thead>
            <tbody>
              {#if authorizedKeys.length === 0}
                <tr>
                  <td colspan="5" class="empty-cell">No authorized keys configured in ~/.ssh/authorized_keys</td>
                </tr>
              {:else}
                {#each authorizedKeys as ak}
                  <tr>
                    <td class="font-mono">#{ak.line_number}</td>
                    <td><span class="badge badge-accent">{ak.key_type}</span></td>
                    <td class="font-mono strong">{ak.comment || 'No comment provided'}</td>
                    <td>
                      {#if ak.options}
                        <span class="badge badge-warning" title="Restriction options applied to key">{ak.options}</span>
                      {:else}
                        <span class="text-muted text-sm">Full Shell Access</span>
                      {/if}
                    </td>
                    <td>
                      <button 
                        class="btn btn-danger btn-sm" 
                        onclick={() => handleRemoveAuthorizedKey(ak.line_number)}
                        title="Revoke and remove this key from authorized_keys"
                      >
                        <Trash2 size={13} /> Revoke Key
                      </button>
                    </td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        </div>
      </div>

      <!-- SSHD Hardening Status -->
      {#if sshdStatus}
        <div class="card grid-card mt-4">
          <div class="card-header">
            <h3 class="card-title">SSH Daemon Security Hardening (`/etc/ssh/sshd_config`)</h3>
          </div>
          <div class="sshd-grid">
            <div class="sshd-item">
              <span class="sshd-label" title="Controls whether root superuser can log in directly via SSH">PermitRootLogin:</span>
              <span class="badge {sshdStatus.permit_root_login === 'no' ? 'badge-success' : 'badge-danger'}">
                {sshdStatus.permit_root_login}
              </span>
            </div>
            <div class="sshd-item">
              <span class="sshd-label" title="Controls whether password-based SSH logins are permitted">PasswordAuthentication:</span>
              <span class="badge {sshdStatus.password_authentication === 'no' ? 'badge-success' : 'badge-warning'}">
                {sshdStatus.password_authentication}
              </span>
            </div>
            <div class="sshd-item">
              <span class="sshd-label" title="Controls public key authentication requirement">PubkeyAuthentication:</span>
              <span class="badge {sshdStatus.pubkey_authentication === 'yes' ? 'badge-success' : 'badge-danger'}">
                {sshdStatus.pubkey_authentication}
              </span>
            </div>
            <div class="sshd-item">
              <span class="sshd-label" title="Controls GUI X11 forwarding over SSH tunnel">X11Forwarding:</span>
              <span class="badge {sshdStatus.x11_forwarding === 'no' ? 'badge-success' : 'badge-warning'}">
                {sshdStatus.x11_forwarding}
              </span>
            </div>
            <div class="sshd-item">
              <span class="sshd-label" title="Configured SSH daemon listening port">SSH Port:</span>
              <span class="badge badge-neutral font-mono">{sshdStatus.port}</span>
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- TAB 3: SSL/TLS CERTIFICATES -->
  {#if activeTab === 'certs'}
    <div class="tab-content">
      <!-- Guide Banner -->
      <div class="guide-banner">
        <HelpCircle size={18} class="guide-icon" />
        <div class="guide-text">
          <strong>System SSL / TLS Certificate Manager</strong>
          <p>
            Scans system certificate stores (<code>/etc/pki/tls/certs/</code>, <code>/etc/nginx/ssl/</code>, Let's Encrypt). 
            Monitor expiration dates to prevent downtime and service security warnings.
          </p>
        </div>
      </div>

      <!-- Certificates Table -->
      <div class="card table-card">
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>CERTIFICATE NAME</th>
                <th>SUBJECT / DOMAIN</th>
                <th>ISSUER</th>
                <th>EXPIRATION DATE</th>
                <th>HEALTH STATUS</th>
                <th>ACTIONS</th>
              </tr>
            </thead>
            <tbody>
              {#if sslCerts.length === 0}
                <tr>
                  <td colspan="6" class="empty-cell">No SSL/TLS certificates detected in system stores</td>
                </tr>
              {:else}
                {#each sslCerts as cert}
                  <tr>
                    <td class="font-mono strong">{cert.name}</td>
                    <td class="text-sm">{cert.subject || 'Self-Signed / Local'}</td>
                    <td class="text-sm text-muted">{cert.issuer || 'Internal CA'}</td>
                    <td class="font-mono text-sm">{cert.not_after || 'N/A'}</td>
                    <td>
                      {#if cert.is_expired}
                        <span class="badge badge-danger" title="Certificate has expired!">EXPIRED</span>
                      {:else if cert.is_expiring_soon}
                        <span class="badge badge-warning" title="Certificate expires in less than 30 days">EXPIRING SOON</span>
                      {:else}
                        <span class="badge badge-success" title="Certificate is valid and active">VALID</span>
                      {/if}
                    </td>
                    <td>
                      <button 
                        class="btn btn-secondary btn-sm" 
                        onclick={() => copyToClipboard(cert.path, 'Certificate path')}
                        title="Copy certificate file path"
                      >
                        Copy Path
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

  <!-- TAB 4: THREAT DEFENSE & FAIL2BAN -->
  {#if activeTab === 'threats'}
    <div class="tab-content">
      <!-- Guide Banner -->
      <div class="guide-banner">
        <HelpCircle size={18} class="guide-icon" />
        <div class="guide-text">
          <strong>Fail2ban Threat Defense & Automated IP Blocking</strong>
          <p>
            Fail2ban scans log files (e.g. <code>/var/log/secure</code>) and automatically alters firewall rules to ban IP addresses 
            that demonstrate malicious login attempt spikes.
          </p>
        </div>
      </div>

      {#if !fail2banStatus?.is_installed}
        <div class="card alert-card">
          <AlertTriangle size={20} color="var(--color-warning)" />
          <div>
            <strong>Fail2ban is not installed on this system</strong>
            <p class="text-sm text-muted">Install fail2ban using <code>sudo dnf install fail2ban</code> to enable dynamic brute-force IP ban management.</p>
          </div>
        </div>
      {:else}
        <div class="card table-card">
          <div class="card-header">
            <h3 class="card-title">Active Fail2ban Jails ({fail2banStatus.jails.length})</h3>
          </div>
          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>JAIL NAME</th>
                  <th>CURRENTLY BANNED IPS</th>
                  <th>BANNED IP ADDRESS LIST</th>
                  <th>ACTIONS</th>
                </tr>
              </thead>
              <tbody>
                {#if fail2banStatus.jails.length === 0}
                  <tr>
                    <td colspan="4" class="empty-cell">No active Fail2ban jails running</td>
                  </tr>
                {:else}
                  {#each fail2banStatus.jails as jail}
                    <tr>
                      <td class="font-mono strong">{jail.jail_name}</td>
                      <td><span class="badge badge-danger">{jail.currently_banned} IPs</span></td>
                      <td class="font-mono text-sm">
                        {#if jail.banned_ips.length === 0}
                          <span class="text-muted">None</span>
                        {:else}
                          <div class="ip-pills">
                            {#each jail.banned_ips as ip}
                              <span class="ip-pill">
                                {ip}
                                <button 
                                  class="unban-btn" 
                                  onclick={() => handleUnbanIp(jail.jail_name, ip)}
                                  title="Unban IP address from jail {jail.jail_name}"
                                >
                                  ×
                                </button>
                              </span>
                            {/each}
                          </div>
                        {/if}
                      </td>
                      <td>
                        <button 
                          class="btn btn-secondary btn-sm" 
                          onclick={() => loadFail2banStatus()}
                          title="Refresh jail metrics"
                        >
                          Refresh
                        </button>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- Modal: Generate SSH Key -->
{#if showGenModal}
  <div class="modal-overlay" onclick={() => showGenModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <h3>Generate New SSH Key Pair</h3>
        <button class="close-btn" onclick={() => showGenModal = false}>×</button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="gen-key-type" title="Ed25519 is recommended for high security and performance">Key Algorithm Type:</label>
          <select id="gen-key-type" bind:value={genKeyType}>
            <option value="ed25519">Ed25519 (Recommended)</option>
            <option value="rsa">RSA (Legacy / Compatibility)</option>
          </select>
        </div>

        {#if genKeyType === 'rsa'}
          <div class="form-group">
            <label for="gen-bits">RSA Bit Length:</label>
            <select id="gen-bits" bind:value={genBits}>
              <option value={4096}>4096-bit (Strong)</option>
              <option value={3072}>3072-bit (Standard)</option>
              <option value={2048}>2048-bit (Minimum)</option>
            </select>
          </div>
        {/if}

        <div class="form-group">
          <label for="gen-filename" title="File name under ~/.ssh/">Key Filename:</label>
          <input id="gen-filename" type="text" bind:value={genFilename} placeholder="id_ed25519_custom" />
        </div>

        <div class="form-group">
          <label for="gen-comment" title="Comment stored in public key file">Comment / Email Label:</label>
          <input id="gen-comment" type="text" bind:value={genComment} placeholder="user@my-workstation" />
        </div>

        <div class="form-group">
          <label for="gen-pass" title="Passphrase to encrypt private key file">Passphrase (Optional):</label>
          <input id="gen-pass" type="password" bind:value={genPassphrase} placeholder="Leave empty for passwordless key" />
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showGenModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={handleGenerateKey}>Generate Key Pair</button>
      </div>
    </div>
  </div>
{/if}

<!-- Modal: Add Authorized Key -->
{#if showAddAuthModal}
  <div class="modal-overlay" onclick={() => showAddAuthModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <h3>Add Remote Authorized Public Key</h3>
        <button class="close-btn" onclick={() => showAddAuthModal = false}>×</button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="auth-key-content">Paste Public Key Content (`ssh-ed25519 ...` or `ssh-rsa ...`):</label>
          <textarea 
            id="auth-key-content"
            rows="5" 
            bind:value={newAuthPubKey} 
            placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... user@remote-machine"
          ></textarea>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => showAddAuthModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={handleAddAuthorizedKey}>Append Key</button>
      </div>
    </div>
  </div>
{/if}

<!-- Modal: Generated Key Success Details -->
{#if generatedKeyResult}
  <div class="modal-overlay" onclick={() => generatedKeyResult = null} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <h3 style="display:flex; align-items:center; gap:8px; color:var(--color-success);">
          <CheckCircle2 size={18} /> SSH Key Pair Created Successfully
        </h3>
        <button class="close-btn" onclick={() => generatedKeyResult = null}>×</button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="created-key-name">Generated Key File (`~/.ssh/{generatedKeyResult.filename}`):</label>
          <input id="created-key-name" type="text" readonly value={generatedKeyResult.filename} class="font-mono" />
        </div>

        <div class="form-group">
          <label for="created-pub-key">Public Key Content (Share with remote servers or GitHub):</label>
          <textarea id="created-pub-key" rows="4" readonly value={generatedKeyResult.publicKey} class="font-mono text-sm" style="resize:vertical;"></textarea>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" onclick={() => generatedKeyResult = null}>Close</button>
        <button class="btn btn-primary" onclick={() => copyToClipboard(generatedKeyResult!.publicKey, 'Public key')}>
          <Copy size={14} /> Copy Public Key
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .vault-container {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    height: 100%;
    overflow-y: auto;
  }

  .header-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 16px 20px;
    gap: 20px;
    flex-wrap: wrap;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .header-icon-box {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: var(--color-accent-muted, rgba(37, 99, 235, 0.1));
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .header-title {
    font-size: 17px;
    font-weight: 700;
    margin: 0 0 2px 0;
    color: var(--color-text-primary);
  }

  .header-sub {
    font-size: 12px;
    color: var(--color-text-secondary);
    margin: 0;
    max-width: 620px;
    line-height: 1.4;
  }

  .header-metrics {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .metric-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .metric-pill.warning {
    border-color: rgba(245, 158, 11, 0.3);
    background: rgba(245, 158, 11, 0.08);
    color: var(--color-warning, #f59e0b);
  }

  .guide-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-left: 4px solid var(--color-accent);
    border-radius: 8px;
    padding: 12px 16px;
  }

  .guide-icon {
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .guide-text {
    flex: 1;
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  .guide-text strong {
    color: var(--color-text-primary);
    display: block;
    margin-bottom: 2px;
  }

  .tab-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    overflow: hidden;
  }

  .card-header {
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
  }

  .card-title {
    font-size: 14px;
    font-weight: 600;
    margin: 0;
    color: var(--color-text-primary);
  }

  .table-wrap {
    overflow-x: auto;
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .data-table th {
    text-align: left;
    padding: 10px 14px;
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    font-weight: 600;
    border-bottom: 1px solid var(--color-border);
  }

  .data-table td {
    padding: 11px 14px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-primary);
  }

  .empty-cell {
    text-align: center;
    padding: 30px !important;
    color: var(--color-text-muted);
  }

  .badge {
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    display: inline-block;
  }

  .badge-accent { background: var(--color-accent-muted, rgba(37, 99, 235, 0.15)); color: var(--color-accent); }
  .badge-neutral { background: var(--color-bg-raised); color: var(--color-text-secondary); }
  .badge-success { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
  .badge-warning { background: rgba(245, 158, 11, 0.15); color: #f59e0b; }
  .badge-danger { background: rgba(239, 68, 68, 0.15); color: #ef4444; }

  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 500;
  }
  .status-chip.success { color: #22c55e; }
  .status-chip.muted { color: var(--color-text-muted); }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }

  .btn-primary { background: var(--color-accent); color: #ffffff; }
  .btn-secondary { background: var(--color-bg-raised); color: var(--color-text-primary); border: 1px solid var(--color-border); }
  .btn-danger { background: rgba(239, 68, 68, 0.15); color: #ef4444; border: 1px solid rgba(239, 68, 68, 0.3); }
  .btn-sm { padding: 4px 8px; font-size: 11px; }

  .btn-text {
    background: transparent;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 500;
    padding: 0;
  }

  .sshd-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
    padding: 16px;
  }

  .sshd-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    background: var(--color-bg-raised);
    border-radius: 6px;
    border: 1px solid var(--color-border);
  }

  .sshd-label {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .ip-pills {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .ip-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 4px;
    color: #ef4444;
  }

  .unban-btn {
    background: transparent;
    border: none;
    color: inherit;
    font-weight: bold;
    cursor: pointer;
    font-size: 13px;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 999;
  }

  .modal-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    width: 480px;
    max-width: 90vw;
    overflow: hidden;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.2);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--color-border);
  }

  .modal-header h3 {
    margin: 0;
    font-size: 15px;
    color: var(--color-text-primary);
  }

  .close-btn {
    background: transparent;
    border: none;
    font-size: 18px;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .modal-body {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .form-group label {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  .form-group input, .form-group select, .form-group textarea {
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    font-size: 12px;
  }

  .modal-footer {
    padding: 12px 18px;
    background: var(--color-bg-raised);
    border-top: 1px solid var(--color-border);
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }

  .spin-icon {
    display: inline-flex;
    animation: vault-spin 0.8s linear infinite;
    transform-origin: center;
  }

  @keyframes vault-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
