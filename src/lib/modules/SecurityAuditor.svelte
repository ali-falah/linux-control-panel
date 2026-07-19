<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-shell';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import Button from '../components/ui/Button.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import {
    Shield, ShieldAlert, ShieldCheck, Info, CheckCircle2,
    AlertTriangle, XCircle, RefreshCw, Lock, Zap, Download,
    Server, Cpu, User, FolderLock, Network, Settings,
    ExternalLink, RotateCcw
  } from '@lucide/svelte';

  // ── Types ──────────────────────────────────────────────────────────────────
  interface SecurityFinding {
    id: string;
    title: string;
    description: string;
    severity: 'Critical' | 'Warning' | 'Good' | 'Info';
    countermeasure: string;
    category: string;
    has_auto_fix: boolean;
    is_resolved: boolean;
    reference?: string | null;
  }

  interface CategoryScore {
    category: string;
    score: number;
    max_score: number;
    issues: number;
  }

  interface SecurityReport {
    score: number;
    findings: SecurityFinding[];
    category_scores: CategoryScore[];
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let report = $state<SecurityReport | null>(null);
  let loading = $state(false);
  let fixingId = $state<string | null>(null);
  let activeCategory = $state('all');
  let activeSeverity = $state<'Critical' | 'Warning' | 'Good' | 'all'>(
    uiStore.securitySeverityFilter ? uiStore.securitySeverityFilter : 'all'
  );
  if (uiStore.securitySeverityFilter) {
    uiStore.securitySeverityFilter = null;
  }
  let scoreHistory = $state<number[]>([]);
  let expandedId = $state<string | null>(null);

  // ── Category config ────────────────────────────────────────────────────────
  const CATEGORIES = [
    { id: 'all',               label: 'All Checks' },
    { id: 'SSH Hardening',     label: 'SSH' },
    { id: 'Kernel Hardening',  label: 'Kernel' },
    { id: 'User & Auth',       label: 'User & Auth' },
    { id: 'Filesystem',        label: 'Filesystem' },
    { id: 'Network & Services',label: 'Network' },
    { id: 'System Hygiene',    label: 'System' },
  ];

  const CAT_ICONS: Record<string, any> = {
    'SSH Hardening':      Server,
    'Kernel Hardening':   Cpu,
    'User & Auth':        User,
    'Filesystem':         FolderLock,
    'Network & Services': Network,
    'System Hygiene':     Settings,
  };

  // ── Computed ───────────────────────────────────────────────────────────────
  let filteredFindings = $derived.by(() => {
    if (!report) return [];
    const sorted = [...report.findings].sort((a, b) => {
      const sevOrder = { Critical: 0, Warning: 1, Info: 2, Good: 3 };
      if (a.is_resolved !== b.is_resolved) return a.is_resolved ? 1 : -1;
      return (sevOrder[a.severity] ?? 9) - (sevOrder[b.severity] ?? 9);
    });
    let items = sorted;
    if (activeCategory !== 'all') {
      items = items.filter(f => f.category === activeCategory);
    }
    if (activeSeverity !== 'all') {
      if (activeSeverity === 'Good') {
        items = items.filter(f => f.is_resolved);
      } else {
        items = items.filter(f => f.severity === activeSeverity && !f.is_resolved);
      }
    }
    return items;
  });

  let tabsWithCounts = $derived.by(() => {
    if (!report) return CATEGORIES.map(c => ({ id: c.id, label: c.label, count: undefined }));
    return CATEGORIES.map(c => {
      const issues = c.id === 'all'
        ? report!.findings.filter(f => !f.is_resolved).length
        : report!.findings.filter(f => f.category === c.id && !f.is_resolved).length;
      return { id: c.id, label: c.label, count: issues > 0 ? issues : undefined };
    });
  });

  let totalIssues = $derived(report ? report.findings.filter(f => !f.is_resolved).length : 0);
  let criticalCount = $derived(report ? report.findings.filter(f => f.severity === 'Critical' && !f.is_resolved).length : 0);

  // ── Score history (localStorage) ──────────────────────────────────────────
  const HISTORY_KEY = 'security_score_history';
  const MAX_HISTORY = 12;

  function loadHistory() {
    try {
      const raw = localStorage.getItem(HISTORY_KEY);
      if (raw) scoreHistory = JSON.parse(raw);
    } catch { scoreHistory = []; }
  }

  function saveHistory(score: number) {
    scoreHistory = [...scoreHistory.slice(-(MAX_HISTORY - 1)), score];
    try { localStorage.setItem(HISTORY_KEY, JSON.stringify(scoreHistory)); } catch {}
  }

  // ── Audit ──────────────────────────────────────────────────────────────────
  async function runAudit() {
    loading = true;
    try {
      report = await invoke<SecurityReport>('security_run_audit');
      saveHistory(report.score);
      statusStore.setLastCommand('security_audit_executed', 0, true);
    } catch (e) {
      uiStore.addToast(`Audit failed: ${e}`, 'error');
      statusStore.setLastCommand('security_run_audit', 1, false);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadHistory();
    runAudit();
  });

  // ── Fix handlers ───────────────────────────────────────────────────────────

  /** Generic kernel param fix/revert helper */
  async function fixKernelParam(finding: SecurityFinding, key: string, secureValue: string, defaultValue: string) {
    const enable = !finding.is_resolved;
    const action = enable ? 'Apply Hardening' : 'Revert to Default';
    const risk = enable
      ? `This will set ${key}=${secureValue} and persist it in /etc/sysctl.d/99-security-hardening.conf.`
      : `This will revert ${key} to ${defaultValue}. The system will be less hardened.`;

    uiStore.confirm(action, risk, async () => {
      fixingId = finding.id;
      try {
        const msg = await invoke<string>('security_fix_kernel_param', {
          key, value: secureValue, revertValue: defaultValue, enable,
        });
        uiStore.addToast(msg, 'success');
        await runAudit();
      } catch (e) {
        uiStore.addToast(`Failed: ${e}`, 'error');
      } finally { fixingId = null; }
    }, !enable);
  }

  /** Generic SSH param fix/revert helper */
  async function fixSshParam(finding: SecurityFinding, param: string, secureValue: string, defaultValue: string) {
    const enable = !finding.is_resolved;
    const action = enable ? `Harden SSH: ${param}` : `Revert SSH: ${param}`;
    const risk = enable
      ? `This will set ${param}=${secureValue} in sshd_config and reload SSH.`
      : `This will revert ${param} to ${defaultValue}. SSH will be less restricted.`;

    uiStore.confirm(action, risk, async () => {
      fixingId = finding.id;
      try {
        const msg = await invoke<string>('security_fix_ssh_param', {
          param, value: secureValue, revertValue: defaultValue, enable,
        });
        uiStore.addToast(msg, 'success');
        await runAudit();
      } catch (e) {
        uiStore.addToast(`Failed: ${e}`, 'error');
      } finally { fixingId = null; }
    }, !enable);
  }

  async function handleFix(finding: SecurityFinding) {
    switch (finding.id) {

      // ── SSH ──────────────────────────────────────────────────────────────
      case 'ssh_root': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Secure SSH Root Login' : 'Enable Root SSH (Insecure)',
          enable
            ? 'This will set PermitRootLogin to prohibit-password. Root will only be accessible via SSH key.'
            : 'Warning: Enabling root SSH exposes the most privileged account to brute-force attacks.',
          async () => {
            fixingId = finding.id;
            try {
              await invoke('security_fix_root_ssh', { enable });
              uiStore.addToast('Root SSH login configured.', 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'ssh_pass_auth':
        await fixSshParam(finding, 'PasswordAuthentication', 'no', 'yes');
        break;

      case 'ssh_max_auth':
        await fixSshParam(finding, 'MaxAuthTries', '4', '6');
        break;

      case 'ssh_grace':
        await fixSshParam(finding, 'LoginGraceTime', '60', '120');
        break;

      // ── Kernel ───────────────────────────────────────────────────────────
      case 'kernel_aslr':
        await fixKernelParam(finding, 'kernel.randomize_va_space', '2', '1');
        break;

      case 'kernel_syncookies':
        await fixKernelParam(finding, 'net.ipv4.tcp_syncookies', '1', '0');
        break;

      case 'kernel_ipforward':
        await fixKernelParam(finding, 'net.ipv4.ip_forward', '0', '1');
        break;

      case 'kernel_kptr':
        await fixKernelParam(finding, 'kernel.kptr_restrict', '1', '0');
        break;

      case 'kernel_dmesg':
        await fixKernelParam(finding, 'kernel.dmesg_restrict', '1', '0');
        break;

      case 'kernel_icmp_redirect': {
        const enable = !finding.is_resolved;
        const action = enable ? 'Disable ICMP Redirects' : 'Re-enable ICMP Redirects';
        uiStore.confirm(action,
          enable ? 'This will set accept_redirects=0 for both IPv4 and IPv6 and persist the setting.'
                 : 'This will re-enable ICMP redirect acceptance. Only do this if your network requires it.',
          async () => {
            fixingId = finding.id;
            try {
              const v = enable ? '0' : '1';
              await invoke('security_fix_kernel_param', {
                key: 'net.ipv4.conf.all.accept_redirects', value: '0', revertValue: '1', enable,
              });
              await invoke('security_fix_kernel_param', {
                key: 'net.ipv6.conf.all.accept_redirects', value: v, revertValue: '1', enable,
              });
              uiStore.addToast('ICMP redirect settings applied.', 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable);
        break;
      }

      // ── User & Auth ──────────────────────────────────────────────────────
      case 'pass_policy': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Enforce Password Policy' : 'Revert Password Policy',
          enable ? 'Sets PASS_MAX_DAYS=90 and PASS_MIN_LEN=12 in /etc/login.defs. Applies to new password changes only.'
                 : 'Reverts password policy to less restrictive defaults.',
          async () => {
            fixingId = finding.id;
            try {
              await invoke('security_fix_password_policy');
              uiStore.addToast('Password policy updated.', 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      // ── Filesystem ───────────────────────────────────────────────────────
      case 'fs_tmp_sticky': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Set /tmp Sticky Bit' : 'Remove /tmp Sticky Bit',
          enable ? 'This sets /tmp permissions to 1777 (sticky). Users can only delete their own files.'
                 : 'Removes the sticky bit from /tmp. Users can delete each other\'s temporary files.',
          async () => {
            fixingId = finding.id;
            try {
              await invoke('security_fix_tmp_sticky', { enable });
              uiStore.addToast(`/tmp sticky bit ${enable ? 'set' : 'removed'}.`, 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'fs_coredump':
        await fixKernelParam(finding, 'fs.suid_dumpable', '0', '1');
        break;

      case 'fs_passwd_perms': {
        fixingId = finding.id;
        try {
          await invoke('security_fix_passwd_perms');
          uiStore.addToast('/etc/passwd permissions fixed.', 'success');
          await runAudit();
        } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
        finally { fixingId = null; }
        break;
      }

      case 'fs_shadow_perms': {
        uiStore.confirm('Secure /etc/shadow', 'This sets /etc/shadow to permissions 000 (no access for anyone except root via capabilities). This is the standard secure configuration.',
          async () => {
            fixingId = finding.id;
            try {
              await invoke('security_fix_shadow_perms');
              uiStore.addToast('/etc/shadow permissions secured.', 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, false
        );
        break;
      }

      // ── Network ──────────────────────────────────────────────────────────
      case 'firewall': {
        fixingId = finding.id;
        try {
          await invoke('security_fix_firewall');
          uiStore.addToast('Firewall enabled with secure defaults.', 'success');
          await runAudit();
        } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
        finally { fixingId = null; }
        break;
      }

      case 'net_src_route': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Disable Source Routing' : 'Re-enable Source Routing',
          enable ? 'Disables IPv4/IPv6 source routing. This prevents route-based MITM attacks.'
                 : 'Re-enables source routing. Only do this if your network infrastructure explicitly requires it.',
          async () => {
            fixingId = finding.id;
            try {
              await invoke('security_fix_kernel_param', { key: 'net.ipv4.conf.all.accept_source_route', value: '0', revertValue: '1', enable });
              await invoke('security_fix_kernel_param', { key: 'net.ipv6.conf.all.accept_source_route', value: '0', revertValue: '1', enable });
              uiStore.addToast('Source routing settings applied.', 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'net_bogus_icmp':
        await fixKernelParam(finding, 'net.ipv4.icmp_ignore_bogus_error_responses', '1', '0');
        break;

      case 'net_martians':
        await fixKernelParam(finding, 'net.ipv4.conf.all.log_martians', '1', '0');
        break;

      // ── System Hygiene ───────────────────────────────────────────────────
      case 'selinux': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Enable SELinux Enforcing' : 'Set SELinux Permissive',
          enable
            ? 'SELinux will be set to enforcing mode. A filesystem relabel and reboot are required. The system will reboot automatically in 5 seconds after confirming.'
            : 'SELinux will be set to permissive (still logs violations but does not block). A reboot is required.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_selinux', { enable });
              uiStore.addToast(msg, 'success');
              if (enable) {
                uiStore.addToast('System will reboot in 5 seconds to apply SELinux changes...', 'warning', 8000);
                setTimeout(() => {
                  invoke('shell_run_command', { command: 'reboot' }).catch(() => {});
                }, 5000);
              }
            } catch (e) {
              uiStore.addToast(`Failed: ${e}`, 'error');
              fixingId = null;
            }
          }, true
        );
        break;
      }

      case 'auditd': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Enable auditd' : 'Disable auditd',
          enable ? 'Enables the Linux Audit Daemon to log security-relevant events system-wide.'
                 : 'Disables auditd. Security events will no longer be logged. Not recommended.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_auditd', { enable });
              uiStore.addToast(msg, 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'time_sync': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Enable Time Synchronization' : 'Disable Time Synchronization',
          enable ? 'Enables chronyd or systemd-timesyncd for accurate system time. Required for TLS and log correlation.'
                 : 'Disables time synchronization. System clock will drift. Only do this in air-gapped environments.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_time_sync', { enable });
              uiStore.addToast(msg, 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'usbguard': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Enable USBGuard' : 'Disable USBGuard',
          enable ? 'Enables USBGuard to control USB device authorization. Existing authorized devices will continue to work.'
                 : 'Disables USBGuard. All USB devices will be accessible without restriction.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_usbguard', { enable });
              uiStore.addToast(msg, 'success');
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      default:
        uiStore.addToast('No auto-fix available for this check. Please remediate manually.', 'info');
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────
  function getSeverityColor(sev: string) {
    if (sev === 'Critical') return 'var(--color-error)';
    if (sev === 'Warning') return 'var(--color-warning)';
    if (sev === 'Good') return 'var(--color-success)';
    return 'var(--color-info)';
  }

  function getScoreColor(score: number) {
    if (score >= 80) return 'var(--color-success)';
    if (score >= 50) return 'var(--color-warning)';
    return 'var(--color-error)';
  }

  function getScoreLabel(score: number) {
    if (score >= 90) return 'Excellent';
    if (score >= 80) return 'Good';
    if (score >= 60) return 'Fair';
    if (score >= 40) return 'Poor';
    return 'Critical Risk';
  }

  function getCategoryIcon(cat: string) {
    return CAT_ICONS[cat] ?? Shield;
  }

  // Sparkline SVG path from score history
  function sparklinePath(history: number[]): string {
    if (history.length < 2) return '';
    const w = 120, h = 32, pad = 2;
    const minV = Math.min(...history);
    const maxV = Math.max(...history);
    const range = Math.max(maxV - minV, 10);
    const pts = history.map((v, i) => {
      const x = pad + (i / (history.length - 1)) * (w - pad * 2);
      const y = h - pad - ((v - minV) / range) * (h - pad * 2);
      return `${x},${y}`;
    });
    return `M${pts.join('L')}`;
  }

  function isRevertable(finding: SecurityFinding): boolean {
    // These checks have explicit revert logic
    return ['ssh_root','ssh_pass_auth','ssh_max_auth','ssh_grace',
            'kernel_aslr','kernel_syncookies','kernel_ipforward','kernel_kptr',
            'kernel_dmesg','kernel_icmp_redirect',
            'pass_policy','fs_tmp_sticky','fs_coredump',
            'net_src_route','net_bogus_icmp','net_martians',
            'selinux','auditd','time_sync','usbguard'].includes(finding.id);
  }

  // Export report as JSON
  function exportReport() {
    if (!report) return;
    const data = {
      generated: new Date().toISOString(),
      score: report.score,
      category_scores: report.category_scores,
      findings: report.findings,
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `security-audit-${new Date().toISOString().slice(0,10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function openReference(ref: string, title: string) {
    if (!ref) return;
    let url = ref;
    if (!ref.startsWith('http://') && !ref.startsWith('https://')) {
      const query = `${ref} ${title} Linux Hardening`;
      url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
    }
    try {
      await open(url);
    } catch (e) {
      uiStore.addToast(`Failed to open reference: ${e}`, 'error');
    }
  }
</script>

<!-- ── Markup ──────────────────────────────────────────────────────────────── -->
<div class="module-container">
  <div class="header">
    <div class="header-title">
      <Shield size={24} color="var(--color-accent)" />
      <h2>Security Auditor</h2>
      {#if report}
        <span class="header-score-pill" style="background: {getScoreColor(report.score)}20; color: {getScoreColor(report.score)}; border-color: {getScoreColor(report.score)}40">
          {report.score}% — {getScoreLabel(report.score)}
        </span>
      {/if}
    </div>
    <div class="header-actions">
      {#if report}
        <Button variant="ghost" size="sm" onclick={exportReport} title="Export audit report as JSON">
          <Download size={14} />
          Export
        </Button>
      {/if}
      <Button variant="outline" size="sm" onclick={runAudit} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'spin' : ''} />
        {loading ? 'Scanning...' : 'Rescan System'}
      </Button>
    </div>
  </div>

  <div class="content-scroll">
    {#if loading && !report}
      <div class="center-state">
        <div class="scan-animation">
          <Shield size={48} color="var(--color-accent)" />
          <div class="scan-ring"></div>
          <div class="scan-ring ring2"></div>
        </div>
        <div class="scan-label">Analyzing system security configuration...</div>
        <div class="scan-sublabel">Checking SSH, kernel, users, filesystem, network &amp; services</div>
      </div>
    {:else if report}

      <!-- ── Top Row: Score + Category Breakdown ── -->
      <div class="top-row">

        <!-- Main Score Card -->
        <div class="score-card glass-panel">
          <h3>Security Score</h3>
          <div class="score-gauge" style="--score-color: {getScoreColor(report.score)}">
            <svg viewBox="0 0 100 100" class="gauge-svg">
              <circle class="gauge-bg" cx="50" cy="50" r="40"></circle>
              <circle class="gauge-fill" cx="50" cy="50" r="40"
                style="stroke-dasharray: {report.score * 2.513} 251.3;"></circle>
            </svg>
            <div class="score-text">{report.score}<span class="pct">%</span></div>
          </div>
          <div class="score-label">{getScoreLabel(report.score)}</div>

          {#if criticalCount > 0}
            <div class="critical-alert">
              <ShieldAlert size={14} />
              {criticalCount} Critical issue{criticalCount > 1 ? 's' : ''} detected
            </div>
          {/if}

          <!-- Sparkline history -->
          {#if scoreHistory.length >= 2}
            <div class="sparkline-wrap">
              <div class="sparkline-label">Score History</div>
              <svg class="sparkline" viewBox="0 0 120 32" preserveAspectRatio="none">
                <path d={sparklinePath(scoreHistory)} class="sparkline-path" />
                <!-- last point dot -->
                {#if scoreHistory.length > 0}
                  {@const lastX = 2 + ((scoreHistory.length - 1) / (scoreHistory.length - 1)) * 116}
                  {@const minV = Math.min(...scoreHistory)}
                  {@const range = Math.max(Math.max(...scoreHistory) - minV, 10)}
                  {@const lastY = 32 - 2 - ((scoreHistory[scoreHistory.length - 1] - minV) / range) * 28}
                  <circle cx={lastX} cy={lastY} r="2.5" fill={getScoreColor(report.score)} />
                {/if}
              </svg>
            </div>
          {/if}

          <div class="issue-stats">
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Critical' ? 'all' : 'Critical'}
              class="stat-pill critical clickable"
              class:active={activeSeverity === 'Critical'}
            >
              <XCircle size={12} />
              {report.findings.filter(f => f.severity === 'Critical' && !f.is_resolved).length} Critical
            </button>
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Warning' ? 'all' : 'Warning'}
              class="stat-pill warning clickable"
              class:active={activeSeverity === 'Warning'}
            >
              <AlertTriangle size={12} />
              {report.findings.filter(f => f.severity === 'Warning' && !f.is_resolved).length} Warnings
            </button>
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Good' ? 'all' : 'Good'}
              class="stat-pill good clickable"
              class:active={activeSeverity === 'Good'}
            >
              <CheckCircle2 size={12} />
              {report.findings.filter(f => f.is_resolved).length} Passed
            </button>
          </div>
        </div>

        <!-- Category Breakdown -->
        <div class="category-grid">
          {#each report.category_scores as cs}
            {@const CatIcon = getCategoryIcon(cs.category)}
            <button
              class="cat-card glass-panel {activeCategory === cs.category ? 'cat-active' : ''}"
              onclick={() => activeCategory = activeCategory === cs.category ? 'all' : cs.category}
              style="--cat-color: {getScoreColor(cs.score)}"
            >
              <div class="cat-header">
                <CatIcon size={16} />
                <span class="cat-name">{cs.category.replace(' & ', ' &\u200B')}</span>
                {#if cs.issues > 0}
                  <span class="cat-badge">{cs.issues}</span>
                {/if}
              </div>
              <div class="cat-bar-wrap">
                <div class="cat-bar">
                  <div class="cat-bar-fill" style="width: {cs.score}%; background: {getScoreColor(cs.score)}"></div>
                </div>
                <span class="cat-score">{cs.score}%</span>
              </div>
            </button>
          {/each}
        </div>
      </div>

      <!-- ── Category Tabs + Findings ── -->
      <div class="findings-section">
        <div class="findings-header">
          <TabGroup tabs={tabsWithCounts} bind:activeTab={activeCategory} disabled={loading} />
          <span class="issue-badge" class:has-issues={totalIssues > 0}>
            {filteredFindings.filter(f => !f.is_resolved).length} issue{filteredFindings.filter(f => !f.is_resolved).length !== 1 ? 's' : ''}
            {activeCategory !== 'all' ? `in ${activeCategory}` : 'total'}
          </span>
        </div>

        <div class="findings-list">
          {#each filteredFindings as finding (finding.id)}
            <div
              class="finding-card"
              class:resolved={finding.is_resolved}
              class:expanded={expandedId === finding.id}
              style="--sev-color: {getSeverityColor(finding.severity)}"
            >
              <!-- Card Header (always visible) -->
              <button
                class="finding-row"
                onclick={() => expandedId = expandedId === finding.id ? null : finding.id}
              >
                <div class="finding-icon">
                  {#if finding.severity === 'Critical'}<XCircle size={18} />
                  {:else if finding.severity === 'Warning'}<AlertTriangle size={18} />
                  {:else if finding.severity === 'Good'}<CheckCircle2 size={18} />
                  {:else}<Info size={18} />{/if}
                </div>
                <div class="finding-main">
                  <div class="finding-title-row">
                    <span class="finding-title">{finding.title}</span>
                    <div class="finding-badges">
                      <span class="sev-badge" style="background:{getSeverityColor(finding.severity)}20;color:{getSeverityColor(finding.severity)}">
                        {finding.severity}
                      </span>
                      <span class="cat-tag">{finding.category}</span>
                    </div>
                  </div>
                  <div class="finding-desc">{finding.description}</div>
                </div>
                <div class="finding-expand-icon" class:rotated={expandedId === finding.id}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="6 9 12 15 18 9"></polyline>
                  </svg>
                </div>
              </button>

              <!-- Expanded Detail -->
              {#if expandedId === finding.id}
                <div class="finding-detail">
                  <div class="countermeasure-box">
                    <Zap size={14} />
                    <div>
                      <div class="cm-label">Countermeasure</div>
                      <div class="cm-text">{finding.countermeasure}</div>
                    </div>
                  </div>

                  {#if finding.reference}
                    <button
                      type="button"
                      class="reference-row"
                      onclick={() => openReference(finding.reference, finding.title)}
                      title="Open Reference in Browser"
                    >
                      <ExternalLink size={12} />
                      <span>Reference: {finding.reference}</span>
                    </button>
                  {/if}

                  <div class="finding-actions">
                    {#if finding.has_auto_fix}
                      {#if !finding.is_resolved}
                        <Button
                          variant="primary"
                          size="sm"
                          onclick={() => handleFix(finding)}
                          disabled={fixingId === finding.id || loading}
                        >
                          {#if fixingId === finding.id}<div class="spinner-sm"></div>
                          {:else}<Lock size={13} />{/if}
                          Apply Fix
                        </Button>
                      {/if}
                      {#if finding.is_resolved && isRevertable(finding)}
                        <Button
                          variant="outline"
                          size="sm"
                          onclick={() => handleFix(finding)}
                          disabled={fixingId === finding.id || loading}
                        >
                          {#if fixingId === finding.id}<div class="spinner-sm"></div>
                          {:else}<RotateCcw size={13} />{/if}
                          Revert
                        </Button>
                      {/if}
                    {:else if !finding.is_resolved}
                      <span class="manual-label">
                        <Info size={12} />
                        Manual remediation required
                      </span>
                    {/if}
                  </div>
                </div>
              {/if}
            </div>
          {/each}

          {#if filteredFindings.length === 0}
            <div class="empty-state">
              <ShieldCheck size={40} color="var(--color-success)" />
              <div>No issues in this category</div>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<!-- ── Styles ──────────────────────────────────────────────────────────────── -->
<style>
  .module-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* ── Header ─────────────────────────────────────────────────────────────── */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 24px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    background: rgba(2, 6, 23, 0.6);
    backdrop-filter: blur(12px);
    flex-shrink: 0;
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .header-title h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  .header-score-pill {
    font-size: 12px;
    font-weight: 700;
    padding: 3px 10px;
    border-radius: 20px;
    border: 1px solid;
    letter-spacing: 0.3px;
  }

  .header-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* ── Content Scroll ──────────────────────────────────────────────────────── */
  .content-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  /* ── Scan Loading ─────────────────────────────────────────────────────────── */
  .center-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    min-height: 300px;
  }

  .scan-animation {
    position: relative;
    width: 80px;
    height: 80px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .scan-ring {
    position: absolute;
    border-radius: 50%;
    border: 2px solid var(--color-accent);
    opacity: 0;
    animation: scan-pulse 2s ease-out infinite;
  }

  .scan-ring { width: 60px; height: 60px; }
  .ring2 { width: 80px; height: 80px; animation-delay: 0.6s; }

  @keyframes scan-pulse {
    0% { transform: scale(0.8); opacity: 0.8; }
    100% { transform: scale(1.4); opacity: 0; }
  }

  .scan-label {
    font-size: 15px;
    color: var(--color-text-primary);
    font-weight: 500;
  }

  .scan-sublabel {
    font-size: 13px;
    color: var(--color-text-muted);
  }

  /* ── Top Row ──────────────────────────────────────────────────────────────── */
  .top-row {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 16px;
    align-items: start;
  }

  /* ── Glass Panel ─────────────────────────────────────────────────────────── */
  .glass-panel {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 12px;
    padding: 20px;
  }

  /* ── Score Card ──────────────────────────────────────────────────────────── */
  .score-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .score-card h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .score-gauge {
    position: relative;
    width: 112px;
    height: 112px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .gauge-svg {
    transform: rotate(-90deg);
    width: 100%;
    height: 100%;
    filter: drop-shadow(0 0 8px var(--score-color));
  }

  .gauge-bg {
    fill: none;
    stroke: rgba(255, 255, 255, 0.08);
    stroke-width: 9;
  }

  .gauge-fill {
    fill: none;
    stroke: var(--score-color);
    stroke-width: 9;
    stroke-linecap: round;
    transition: stroke-dasharray 1.2s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .score-text {
    position: absolute;
    font-size: 30px;
    font-weight: 700;
    color: var(--score-color);
    letter-spacing: -1px;
  }

  .score-text .pct {
    font-size: 13px;
    opacity: 0.7;
    font-weight: 400;
  }

  .score-label {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-top: -4px;
  }

  .critical-alert {
    display: flex;
    align-items: center;
    gap: 6px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.25);
    color: var(--color-error);
    font-size: 12px;
    font-weight: 600;
    padding: 6px 12px;
    border-radius: 8px;
    width: 100%;
    justify-content: center;
  }

  /* ── Sparkline ───────────────────────────────────────────────────────────── */
  .sparkline-wrap {
    width: 100%;
  }

  .sparkline-label {
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 4px;
  }

  .sparkline {
    width: 100%;
    height: 32px;
    overflow: visible;
  }

  .sparkline-path {
    fill: none;
    stroke: var(--color-accent);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  /* ── Issue Stats ─────────────────────────────────────────────────────────── */
  .issue-stats {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: center;
    width: 100%;
  }

  .stat-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 6px;
  }

  .stat-pill.clickable {
    cursor: pointer;
    border: 1px solid transparent;
    font-family: inherit;
    transition: all 0.2s ease;
  }
  .stat-pill.clickable:hover {
    filter: brightness(1.2);
  }
  .stat-pill.clickable.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #00363d !important;
  }

  .stat-pill.critical { background: rgba(239,68,68,.12); color: var(--color-error); }
  .stat-pill.warning  { background: rgba(251,191,36,.12); color: var(--color-warning); }
  .stat-pill.good     { background: rgba(34,197,94,.12); color: var(--color-success); }

  /* ── Category Grid ───────────────────────────────────────────────────────── */
  .category-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    align-content: start;
  }

  .cat-card {
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 12px 14px;
    cursor: pointer;
    transition: all 0.18s ease;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .cat-card:hover {
    background: rgba(255, 255, 255, 0.05);
    border-color: var(--cat-color);
  }

  .cat-card.cat-active {
    background: color-mix(in srgb, var(--cat-color) 10%, transparent);
    border-color: var(--cat-color);
  }

  .cat-header {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--cat-color);
    font-size: 12px;
    font-weight: 600;
  }

  .cat-name {
    flex: 1;
    color: var(--color-text-primary);
    font-size: 11px;
  }

  .cat-badge {
    background: rgba(239, 68, 68, 0.2);
    color: var(--color-error);
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 10px;
  }

  .cat-bar-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .cat-bar {
    flex: 1;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
  }

  .cat-bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.8s ease;
  }

  .cat-score {
    font-size: 11px;
    font-weight: 700;
    color: var(--cat-color);
    width: 32px;
    text-align: right;
  }

  /* ── Findings Section ────────────────────────────────────────────────────── */
  .findings-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .findings-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .issue-badge {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 12px;
    border-radius: 20px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-secondary);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .issue-badge.has-issues {
    background: rgba(239, 68, 68, 0.1);
    color: var(--color-error);
    border-color: rgba(239, 68, 68, 0.25);
  }

  /* ── Findings List ───────────────────────────────────────────────────────── */
  .findings-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* ── Finding Card ────────────────────────────────────────────────────────── */
  .finding-card {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-left: 3px solid var(--sev-color);
    border-radius: 10px;
    overflow: hidden;
    transition: all 0.18s ease;
  }

  .finding-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
    border-left-color: var(--sev-color);
    background: rgba(255, 255, 255, 0.02);
  }

  .finding-card.resolved {
    opacity: 0.65;
  }

  .finding-card.expanded {
    border-color: rgba(255, 255, 255, 0.1);
    border-left-color: var(--sev-color);
    background: rgba(255, 255, 255, 0.03);
  }

  /* ── Finding Row (click target) ──────────────────────────────────────────── */
  .finding-row {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 14px 16px;
    width: 100%;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .finding-row:hover { background: rgba(255,255,255,0.02); }

  .finding-icon {
    color: var(--sev-color);
    flex-shrink: 0;
    padding-top: 1px;
  }

  .finding-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .finding-title-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .finding-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .finding-badges {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }

  .sev-badge {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 4px;
    text-transform: uppercase;
    font-weight: 700;
    letter-spacing: 0.3px;
  }

  .cat-tag {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.07);
    color: var(--color-text-muted);
    font-weight: 500;
  }

  .finding-desc {
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  .finding-expand-icon {
    color: var(--color-text-muted);
    flex-shrink: 0;
    padding-top: 2px;
    transition: transform 0.2s ease;
  }

  .finding-expand-icon.rotated {
    transform: rotate(180deg);
  }

  /* ── Finding Detail (expanded) ────────────────────────────────────────────── */
  .finding-detail {
    padding: 0 16px 14px 44px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }

  .countermeasure-box {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    background: rgba(0, 218, 243, 0.06);
    border: 1px solid rgba(0, 218, 243, 0.15);
    border-radius: 8px;
    padding: 10px 12px;
    margin-top: 10px;
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .countermeasure-box :global(svg) {
    color: var(--color-accent);
    flex-shrink: 0;
    margin-top: 2px;
  }

  .cm-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-accent);
    font-weight: 700;
    margin-bottom: 3px;
  }

  .cm-text {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  .reference-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--color-text-muted);
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
    transition: color 0.15s ease;
  }
  .reference-row:hover {
    color: var(--color-accent);
    text-decoration: underline;
  }

  .finding-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }

  .manual-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--color-text-muted);
    font-style: italic;
  }

  /* ── Empty State ─────────────────────────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 40px;
    color: var(--color-text-muted);
    font-size: 14px;
  }

  /* ── Spinner ─────────────────────────────────────────────────────────────── */
  .spin {
    animation: spin 1s linear infinite;
  }
  @keyframes spin { 100% { transform: rotate(360deg); } }

  .spinner-sm {
    width: 13px;
    height: 13px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
</style>
