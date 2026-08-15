<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-shell';
  import { downloadDir } from '@tauri-apps/api/path';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';
  import {
    Shield, ShieldAlert, ShieldCheck, Info, CheckCircle2,
    AlertTriangle, XCircle, RefreshCw, Lock, Zap, Download,
    Server, Cpu, User, FolderLock, Network, Settings,
    ExternalLink, RotateCcw, FolderOpen, ChevronDown, Sparkles, EyeOff, Eye
  } from '@lucide/svelte';
  import AiSecurityAdvisorModal from '../components/AiSecurityAdvisorModal.svelte';
  import { aiStore, getHardcodedFix } from '../stores/aiStore.svelte.ts';

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
    tamper_flag?: string | null;
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

  // ── State & Cache Initialization ───────────────────────────────────────────
  const HISTORY_KEY = 'security_score_history';
  const CACHE_KEY = 'security_report_cache_v2';
  const MAX_HISTORY = 12;

  function getInitialCache(): SecurityReport | null {
    try {
      const raw = sessionStorage.getItem(CACHE_KEY);
      if (raw) return JSON.parse(raw);
    } catch {}
    return null;
  }

  const initialCache = getInitialCache();
  let report = $state<SecurityReport | null>(initialCache);
  let loading = $state(initialCache === null);
  let fixingId = $state<string | null>(null);
  let activeCategory = $state(
    uiStore.securityCategoryFilter ? uiStore.securityCategoryFilter : 'all'
  );
  let activeSeverity = $state<'Critical' | 'Warning' | 'Good' | 'all'>(
    uiStore.securitySeverityFilter ? uiStore.securitySeverityFilter : 'all'
  );
  if (uiStore.securitySeverityFilter) {
    uiStore.securitySeverityFilter = null;
  }
  if (uiStore.securityCategoryFilter) {
    uiStore.securityCategoryFilter = null;
  }
  let scoreHistory = $state<number[]>([]);
  let expandedId = $state<string | null>(null);
  let showExportDropdown = $state(false);
  let exportDropdownRef = $state<HTMLDivElement | null>(null);
  /** Tracks tamper-flagged findings the user has explicitly acknowledged this session */
  let reviewedTamperIds = $state<string[]>([]);

  // Close export dropdown when clicking outside
  $effect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (!showExportDropdown) return;
      if (exportDropdownRef && exportDropdownRef.contains(e.target as Node)) return;
      showExportDropdown = false;
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  });

  // ── Category config ────────────────────────────────────────────────────────
  const CATEGORIES = [
    { id: 'all',               label: 'All Checks' },
    { id: 'SSH Hardening',     label: 'SSH' },
    { id: 'Kernel Hardening',  label: 'Kernel' },
    { id: 'User & Auth',       label: 'User & Auth' },
    { id: 'Filesystem',        label: 'Filesystem' },
    { id: 'Network & Services',label: 'Network' },
    { id: 'System Hygiene',    label: 'System' },
    { id: 'Runtime Threats',   label: 'Runtime Threats' },
  ];

  const CAT_ICONS: Record<string, any> = {
    'SSH Hardening':      Server,
    'Kernel Hardening':   Cpu,
    'User & Auth':        User,
    'Filesystem':         FolderLock,
    'Network & Services': Network,
    'System Hygiene':     Settings,
    'Runtime Threats':    ShieldAlert,
  };

  let activeView = $state<'all' | 'muted'>('all');

  // ── Muted / Ignored Findings State ─────────────────────────────────────────
  const MUTED_KEY = 'security_audit_muted_ids_v1';
  let mutedIds = $state<string[]>([]);

  function loadMuted() {
    try {
      const raw = localStorage.getItem(MUTED_KEY);
      if (raw) mutedIds = JSON.parse(raw);
    } catch { mutedIds = []; }
  }

  function toggleMute(id: string) {
    if (mutedIds.includes(id)) {
      mutedIds = mutedIds.filter(i => i !== id);
      uiStore.addToast(`Finding restored to active checks.`, 'info');
    } else {
      mutedIds = [...mutedIds, id];
      uiStore.addToast(`Finding muted and excluded from penalty score.`, 'success');
    }
    try { localStorage.setItem(MUTED_KEY, JSON.stringify(mutedIds)); } catch {}
  }

  // Effective score taking weighted category scores and muted items into account
  let effectiveScore = $derived.by(() => {
    if (!report) return 0;
    if (mutedIds.length === 0) {
      return report.score;
    }
    const activeFindings = report.findings.filter(f => !mutedIds.includes(f.id));
    if (activeFindings.length === 0) return 100;

    const hasUnmutedCritical = activeFindings.some(f => f.severity === 'Critical' && !f.is_resolved);
    let totalCur = 0;
    let totalMax = 0;
    for (const cs of report.category_scores) {
      const catFindings = activeFindings.filter(f => f.category === cs.category);
      const catPassed = catFindings.filter(f => f.is_resolved).length;
      if (catFindings.length > 0) {
        const catPct = Math.round((catPassed / catFindings.length) * 100);
        totalCur += (catPct * cs.max_score) / 100;
        totalMax += cs.max_score;
      }
    }
    const rawScore = totalMax > 0 ? Math.round((totalCur / totalMax) * 100) : 100;
    return hasUnmutedCritical ? Math.min(rawScore, 60) : rawScore;
  });

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
    if (activeView === 'muted') {
      items = items.filter(f => mutedIds.includes(f.id));
    }
    return items;
  });

  let tabsWithCounts = $derived.by(() => {
    if (!report) return CATEGORIES.map(c => ({ id: c.id, label: c.label, count: undefined }));
    return CATEGORIES.map(c => {
      const issues = c.id === 'all'
        ? report!.findings.filter(f => !f.is_resolved && !mutedIds.includes(f.id)).length
        : report!.findings.filter(f => f.category === c.id && !f.is_resolved && !mutedIds.includes(f.id)).length;
      return { id: c.id, label: c.label, count: issues > 0 ? issues : undefined };
    });
  });

  let totalIssues = $derived(report ? report.findings.filter(f => !f.is_resolved && !mutedIds.includes(f.id)).length : 0);
  let criticalCount = $derived(report ? report.findings.filter(f => f.severity === 'Critical' && !f.is_resolved && !mutedIds.includes(f.id)).length : 0);

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

  function saveCache(rep: SecurityReport) {
    try {
      sessionStorage.setItem(CACHE_KEY, JSON.stringify(rep));
    } catch {}
  }

  // ── Audit ──────────────────────────────────────────────────────────────────
  async function runAudit(forceRefresh: boolean | MouseEvent = true, isBackground = false) {
    if (!isBackground) {
      loading = true;
    }
    const shouldForce = typeof forceRefresh === 'boolean' ? forceRefresh : true;
    try {
      const res = await invoke<SecurityReport>('security_run_audit', { forceRefresh: shouldForce });
      if (res && res.findings) {
        report = res;
        saveCache(res);
        saveHistory(effectiveScore);
        statusStore.setLastCommand('security_audit_executed', 0, true);
      }
    } catch (e) {
      uiStore.addToast(`Audit scan error: ${e}`, 'error');
      statusStore.setLastCommand('security_run_audit', 1, false);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadHistory();
    loadMuted();

    // If cache exists, render immediately and refresh in background; else fetch fresh
    const hasCache = !!report;
    runAudit(false, hasCache);

    const handleReaudit = () => runAudit(true, false);
    window.addEventListener('security-audit-run', handleReaudit);
    return () => window.removeEventListener('security-audit-run', handleReaudit);
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
        // ── Audit log ────────────────────────────────────────────────────────
        invoke('security_log_fix', {
          findingId: finding.id,
          findingTitle: finding.title,
          action: enable ? 'apply' : 'revert',
          outcome: msg,
        }).catch(() => {});
        await runAudit();
      } catch (e) {
        uiStore.addToast(`Failed: ${e}`, 'error');
      } finally { fixingId = null; }
    }, !enable);
  }

  async function handleFix(finding: SecurityFinding) {
    switch (finding.id) {
      // ── SSH Hardening ────────────────────────────────────────────────────
      case 'ssh_root': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Disable SSH Root Login' : 'Enable SSH Root Login',
          enable ? 'PermitRootLogin will be set to no in sshd_config. Root login via SSH will be blocked.'
                 : 'PermitRootLogin will be set to yes. Root can log in directly via SSH. NOT RECOMMENDED.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_root_ssh', { enable });
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'ssh_pass_auth': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Disable Password Authentication' : 'Enable Password Authentication',
          enable ? 'PasswordAuthentication will be set to no. Only SSH keys will be allowed. Make sure your SSH key is added!'
                 : 'PasswordAuthentication will be set to yes.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_ssh_param', {
                param: 'PasswordAuthentication', value: 'no', revertValue: 'yes', enable
              });
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'ssh_max_auth': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Set MaxAuthTries to 4' : 'Revert MaxAuthTries',
          enable ? 'MaxAuthTries will be set to 4 in sshd_config to limit brute force attempts.'
                 : 'MaxAuthTries setting will be removed.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_ssh_param', {
                param: 'MaxAuthTries', value: '4', revertValue: '6', enable
              });
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'ssh_grace': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Set LoginGraceTime to 60s' : 'Revert LoginGraceTime',
          enable ? 'LoginGraceTime will be set to 60 in sshd_config to disconnect unauthenticated clients quickly.'
                 : 'LoginGraceTime setting will be removed.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_ssh_param', {
                param: 'LoginGraceTime', value: '60', revertValue: '120', enable
              });
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      // ── Kernel Hardening ─────────────────────────────────────────────────
      case 'kernel_aslr':
        await fixKernelParam(finding, 'kernel.randomize_va_space', '2', '0');
        break;

      case 'kernel_syncookies':
        await fixKernelParam(finding, 'net.ipv4.tcp_syncookies', '1', '0');
        break;

      case 'kernel_ipforward':
        await fixKernelParam(finding, 'net.ipv4.ip_forward', '0', '1');
        break;

      case 'kernel_kptr':
        await fixKernelParam(finding, 'kernel.kptr_restrict', '2', '0');
        break;

      case 'kernel_dmesg':
        await fixKernelParam(finding, 'kernel.dmesg_restrict', '1', '0');
        break;

      case 'kernel_icmp_redirect':
        await fixKernelParam(finding, 'net.ipv4.conf.all.accept_redirects', '0', '1');
        break;

      // ── User & Auth ──────────────────────────────────────────────────────
      case 'pass_policy': {
        uiStore.confirm(
          'Enforce Password Aging Policy',
          'Sets PASS_MAX_DAYS=90 and PASS_MIN_LEN=12 in /etc/login.defs.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_password_policy');
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: 'apply', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, false
        );
        break;
      }

      // ── Filesystem ───────────────────────────────────────────────────────
      case 'fs_tmp_sticky': {
        const enable = !finding.is_resolved;
        uiStore.confirm(
          enable ? 'Set Sticky Bit on /tmp and /var/tmp' : 'Remove Sticky Bit',
          enable ? 'Executes chmod +t on /tmp and /var/tmp to prevent users from deleting others\' files.'
                 : 'Executes chmod -t on /tmp and /var/tmp.',
          async () => {
            fixingId = finding.id;
            try {
              const msg = await invoke<string>('security_fix_tmp_sticky', { enable });
              uiStore.addToast(msg, 'success');
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
              await runAudit();
            } catch (e) { uiStore.addToast(`Failed: ${e}`, 'error'); }
            finally { fixingId = null; }
          }, !enable
        );
        break;
      }

      case 'fs_coredump': {
        await fixKernelParam(finding, 'fs.suid_dumpable', '0', '1');
        break;
      }

      // ── Network & Services ───────────────────────────────────────────────
      case 'net_src_route':
        await fixKernelParam(finding, 'net.ipv4.conf.all.accept_source_route', '0', '1');
        break;

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
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
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
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
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
              invoke('security_log_fix', { findingId: finding.id, findingTitle: finding.title, action: enable ? 'apply' : 'revert', outcome: msg }).catch(() => {});
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
    const w = 110, h = 26, pad = 2;
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
      score: effectiveScore,
      category_scores: report.category_scores,
      findings: report.findings,
      muted_findings: mutedIds
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `security-audit-${new Date().toISOString().slice(0,10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    uiStore.addToast('JSON audit data exported successfully.', 'success', 6000, 'Open Folder', openDownloadsFolder);
  }

  // Open Downloads Folder in file manager
  async function openDownloadsFolder() {
    try {
      const dir = await downloadDir();
      await invoke('open_folder', { path: dir });
      uiStore.addToast('Opened Downloads folder.', 'info');
    } catch (e) {
      try {
        const user = await invoke<string>('get_current_user');
        await invoke('open_folder', { path: `/home/${user}/Downloads` });
        uiStore.addToast('Opened Downloads folder.', 'info');
      } catch (err) {
        uiStore.addToast(`Could not open folder: ${err}`, 'error');
      }
    }
  }

  // Export report as styled HTML
  function exportHtmlReport() {
    if (!report) return;
    const dateStr = new Date().toLocaleString();
    const html = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8"/>
  <title>Linux Security Audit Report - ${dateStr}</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #0f172a; color: #f8fafc; padding: 40px; margin: 0; }
    .container { max-width: 900px; margin: 0 auto; background: #1e293b; border-radius: 12px; padding: 32px; box-shadow: 0 10px 25px rgba(0,0,0,0.5); }
    .header { display: flex; justify-content: space-between; align-items: center; border-bottom: 2px solid #334155; padding-bottom: 20px; margin-bottom: 24px; }
    .score-badge { font-size: 24px; font-weight: bold; padding: 8px 20px; border-radius: 30px; background: var(--color-accent-muted); color: var(--color-accent); border: 1px solid var(--color-accent); }
    .finding-card { background: #0f172a; border-radius: 8px; padding: 16px; margin-bottom: 12px; border-left: 4px solid #94a3b8; }
    .finding-card.Critical { border-left-color: #ef4444; }
    .finding-card.Warning { border-left-color: #f59e0b; }
    .finding-card.Good { border-left-color: #10b981; }
    .title { font-weight: bold; font-size: 16px; margin-bottom: 6px; display: flex; justify-content: space-between; }
    .badge { font-size: 11px; padding: 2px 8px; border-radius: 4px; text-transform: uppercase; font-weight: bold; }
    .badge.Critical { background: rgba(239, 68, 68, 0.2); color: #ef4444; }
    .badge.Warning { background: rgba(245, 158, 11, 0.2); color: #f59e0b; }
    .badge.Good { background: rgba(16, 185, 129, 0.2); color: #10b981; }
    .desc { color: #cbd5e1; font-size: 13px; margin-bottom: 8px; }
    .remediation { background: var(--color-accent-muted); border-radius: 6px; padding: 10px; font-size: 12px; color: var(--color-accent); font-family: monospace; }
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <div>
        <h1 style="margin:0; font-size: 24px;">🛡️ Linux Security Audit Report</h1>
        <p style="margin: 4px 0 0 0; color: #94a3b8; font-size: 13px;">Generated on ${dateStr}</p>
      </div>
      <div class="score-badge">${effectiveScore}% — ${getScoreLabel(effectiveScore)}</div>
    </div>
    <h2 style="font-size: 18px; border-bottom: 1px solid #334155; padding-bottom: 8px;">Audit Findings (${report.findings.length} total)</h2>
    ${report.findings.map(f => `
      <div class="finding-card ${f.severity}">
        <div class="title">
          <span>${f.title} <small style="color:#94a3b8; font-weight:normal;">(${f.category})</small></span>
          <span class="badge ${f.severity}">${f.is_resolved ? 'PASSED' : f.severity}</span>
        </div>
        <div class="desc">${f.description}</div>
        <div class="remediation">💡 <strong>Remediation:</strong> ${f.countermeasure}</div>
      </div>
    `).join('')}
  </div>
</body>
</html>`;

    const blob = new Blob([html], { type: 'text/html' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `security-audit-report-${new Date().toISOString().slice(0,10)}.html`;
    a.click();
    URL.revokeObjectURL(url);
    uiStore.addToast('Formatted HTML compliance report exported.', 'success', 6000, 'Open Folder', openDownloadsFolder);
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

  function triggerAiAdvisor(finding: SecurityFinding) {
    aiStore.explainFinding({
      id: finding.id,
      title: finding.title,
      severity: finding.severity,
      category: finding.category,
      description: finding.description,
      current_value: finding.countermeasure || '',
      recommendation: finding.countermeasure || '',
    });
  }

  onMount(() => {
    aiStore.checkOllamaStatus();
  });
</script>

<!-- ── Markup ──────────────────────────────────────────────────────────────── -->
<div class="module-page security-auditor">
  <PageHeader title="Security Auditor" subtitle="System Hardening &amp; Security Audit" icon={Shield}>
    {#if report}
      <span class="header-score-pill" style="background: {getScoreColor(effectiveScore)}20; color: {getScoreColor(effectiveScore)}; border-color: {getScoreColor(effectiveScore)}40; margin-right: 8px;">
        {effectiveScore}% — {getScoreLabel(effectiveScore)}
      </span>
      <!-- Single Export Dropdown Button -->
      <div bind:this={exportDropdownRef} class="export-dropdown-wrap">
        <Button variant="outline" size="sm" onclick={() => showExportDropdown = !showExportDropdown} title="Export Audit Report Options">
          <Download size={14} />
          Export Report
          <ChevronDown size={12} />
        </Button>
        {#if showExportDropdown}
          <div class="export-dropdown-menu" role="menu">
            <button class="export-menu-item" onclick={() => { exportHtmlReport(); showExportDropdown = false; }}>
              <Download size={13} style="color:var(--color-accent);" />
              <span>Export HTML Report</span>
            </button>
            <button class="export-menu-item" onclick={() => { exportReport(); showExportDropdown = false; }}>
              <Download size={13} style="color:var(--color-info);" />
              <span>Export JSON Data</span>
            </button>
          </div>
        {/if}
      </div>
    {/if}
    <Button variant="outline" size="sm" onclick={() => runAudit(true)} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'spin' : ''} />
      {loading ? 'Scanning...' : 'Rescan System'}
    </Button>
  </PageHeader>

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

        <!-- Compact Redesigned Score Card -->
        <div class="score-card glass-panel">
          <div class="score-card-header">
            <h3>Security Score</h3>
            <span class="verdict-tag" style="background: {getScoreColor(effectiveScore)}18; color: {getScoreColor(effectiveScore)}; border: 1px solid {getScoreColor(effectiveScore)}40;">
              {getScoreLabel(effectiveScore)}
            </span>
          </div>

          <div class="score-card-body">
            <!-- Compact Ring Gauge -->
            <div class="score-gauge-compact" style="--score-color: {getScoreColor(effectiveScore)}">
              <svg viewBox="0 0 100 100" class="gauge-svg">
                <circle class="gauge-bg" cx="50" cy="50" r="40"></circle>
                <circle class="gauge-fill" cx="50" cy="50" r="40"
                  style="stroke-dasharray: {effectiveScore * 2.513} 251.3;"></circle>
              </svg>
              <div class="score-text-compact">{effectiveScore}<span class="pct">%</span></div>
            </div>

            <div class="score-info-wrap">
              {#if criticalCount > 0}
                <div class="critical-alert-compact">
                  <ShieldAlert size={13} />
                  <span>{criticalCount} Critical issue{criticalCount > 1 ? 's' : ''}</span>
                </div>
              {:else}
                <div class="clean-alert-compact">
                  <ShieldCheck size={13} />
                  <span>No Critical Vulnerabilities</span>
                </div>
              {/if}

              <!-- Sparkline history -->
              {#if scoreHistory.length >= 2}
                <div class="sparkline-wrap-compact">
                  <span class="sparkline-label">History</span>
                  <svg class="sparkline" viewBox="0 0 110 26" preserveAspectRatio="none">
                    <path d={sparklinePath(scoreHistory)} class="sparkline-path" />
                    {#if scoreHistory.length > 0}
                      {@const lastX = 2 + ((scoreHistory.length - 1) / (scoreHistory.length - 1)) * 106}
                      {@const minV = Math.min(...scoreHistory)}
                      {@const range = Math.max(Math.max(...scoreHistory) - minV, 10)}
                      {@const lastY = 26 - 2 - ((scoreHistory[scoreHistory.length - 1] - minV) / range) * 22}
                      <circle cx={lastX} cy={lastY} r="2" fill={getScoreColor(report.score)} />
                    {/if}
                  </svg>
                </div>
              {/if}
            </div>
          </div>

          <div class="issue-stats-compact">
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Critical' ? 'all' : 'Critical'}
              class="stat-pill critical clickable"
              class:active={activeSeverity === 'Critical'}
            >
              <XCircle size={11} />
              {report.findings.filter(f => f.severity === 'Critical' && !f.is_resolved).length} Critical
            </button>
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Warning' ? 'all' : 'Warning'}
              class="stat-pill warning clickable"
              class:active={activeSeverity === 'Warning'}
            >
              <AlertTriangle size={11} />
              {report.findings.filter(f => f.severity === 'Warning' && !f.is_resolved).length} Warnings
            </button>
            <button
              type="button"
              onclick={() => activeSeverity = activeSeverity === 'Good' ? 'all' : 'Good'}
              class="stat-pill good clickable"
              class:active={activeSeverity === 'Good'}
            >
              <CheckCircle2 size={11} />
              {report.findings.filter(f => f.is_resolved).length} Passed
            </button>
            {#if mutedIds.length > 0}
              <button
                type="button"
                onclick={() => { activeView = activeView === 'muted' ? 'all' : 'muted'; activeSeverity = 'all'; activeCategory = 'all'; }}
                class="stat-pill muted clickable"
                class:active={activeView === 'muted'}
                title="Score excludes these {mutedIds.length} finding{mutedIds.length !== 1 ? 's' : ''}"
              >
                <EyeOff size={11} />
                {mutedIds.length} Muted
              </button>
            {/if}
          </div>
        </div>

        <!-- Category Breakdown (Reduced width by 10%) -->
        <div class="category-grid-wrap">
          <div class="category-grid">
            {#each report.category_scores as cs}
              {@const CatIcon = getCategoryIcon(cs.category)}
              {@const isRuntime = cs.category === 'Runtime Threats'}
              <button
                class="cat-card glass-panel {activeCategory === cs.category ? 'cat-active' : ''} {isRuntime && cs.issues > 0 ? 'cat-threat' : ''}"
                onclick={() => activeCategory = activeCategory === cs.category ? 'all' : cs.category}
                style="--cat-color: {isRuntime ? (cs.issues > 0 ? 'var(--color-error)' : 'var(--color-success)') : getScoreColor(cs.score)}"
              >
                <div class="cat-header">
                  <CatIcon size={15} />
                  <span class="cat-name">{cs.category.replace(' & ', ' &\u200B')}</span>
                  {#if cs.issues > 0}
                    <span class="cat-badge">{cs.issues}</span>
                  {/if}
                </div>
                {#if isRuntime}
                  <div class="cat-bar-wrap">
                    {#if cs.issues === 0}
                      <span style="font-size: 11px; color: var(--color-success); font-weight: 600;">✓ Clean</span>
                    {:else}
                      <span style="font-size: 11px; color: var(--color-error); font-weight: 600;">{cs.issues} active threat{cs.issues !== 1 ? 's' : ''}</span>
                    {/if}
                  </div>
                {:else}
                  <div class="cat-bar-wrap">
                    <div class="cat-bar">
                      <div class="cat-bar-fill" style="width: {cs.score}%; background: {getScoreColor(cs.score)}"></div>
                    </div>
                    <span class="cat-score">{cs.score}%</span>
                  </div>
                {/if}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- ── Category Tabs + Findings ── -->
      <div class="findings-header">
        <TabGroup tabs={tabsWithCounts} bind:activeTab={activeCategory} disabled={loading} />
        
        <div class="findings-header-right">
          <span class="issue-badge" class:has-issues={totalIssues > 0}>
            {#if activeSeverity !== 'all' || activeCategory !== 'all'}
              {filteredFindings.length} showing ({totalIssues} unresolved total)
            {:else}
              {totalIssues} issue{totalIssues !== 1 ? 's' : ''} total
            {/if}
          </span>
        </div>
      </div>

        <div class="findings-list">
          {#each filteredFindings as finding (finding.id)}
            <div
              class="finding-card"
              class:resolved={finding.is_resolved}
              class:muted={mutedIds.includes(finding.id)}
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
                    <span class="sev-badge" style="background:{getSeverityColor(finding.severity)}20;color:{getSeverityColor(finding.severity)}">
                      {finding.severity}
                    </span>
                    {#if finding.tamper_flag}
                      <span class="sev-badge" style="background: rgba(245, 158, 11, 0.15); color: var(--color-warning); border: 1px solid rgba(245, 158, 11, 0.3); display: flex; align-items: center; gap: 4px;">
                        <AlertTriangle size={10} />
                        {finding.tamper_flag}
                      </span>
                    {/if}
                    {#if mutedIds.includes(finding.id)}
                      <span class="sev-badge" style="background:rgba(255,255,255,0.1); color:var(--color-text-muted); border: 1px solid rgba(255,255,255,0.2);">
                        MUTED
                      </span>
                    {/if}
                  </div>
                  <div class="finding-desc">{finding.description}</div>
                </div>
                <span class="cat-tag" style="margin-left: auto; align-self: center; flex-shrink: 0; font-size: 10px; padding: 2px 8px; border-radius: 4px; background: rgba(255, 255, 255, 0.07); color: var(--color-text-muted); border: 1px solid rgba(255, 255, 255, 0.1);">
                  {finding.category}
                </span>
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
                      {#if finding.category === 'Runtime Threats'}
                        <!-- Deep-link buttons: navigate directly to the relevant log tabs -->
                        <div style="display: flex; gap: 8px; margin-top: 10px; flex-wrap: wrap;">
                          <button
                            type="button"
                            class="cm-nav-btn"
                            onclick={() => {
                              uiStore.preAppliedJournalPriority = 'all';
                              uiStore.setActiveTab('journal-logs');
                              // Signal Auth Events tab — store the target tab in a transient flag
                              setTimeout(() => {
                                const event = new CustomEvent('journal-tab-select', { detail: 'auth' });
                                window.dispatchEvent(event);
                              }, 100);
                            }}
                          >
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
                            View Auth Events
                          </button>
                          <button
                            type="button"
                            class="cm-nav-btn"
                            onclick={() => {
                              uiStore.preAppliedJournalPriority = 'all';
                              uiStore.setActiveTab('journal-logs');
                              setTimeout(() => {
                                const event = new CustomEvent('journal-tab-select', { detail: 'audit' });
                                window.dispatchEvent(event);
                              }, 100);
                            }}
                          >
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
                            View Command Audit
                          </button>
                        </div>
                      {/if}
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
                    <!-- ── Dry-run diff preview ─────────────────────────────── -->
                    {#if finding.has_auto_fix && !finding.is_resolved}
                      {@const hfix = getHardcodedFix(finding.id)}
                      {#if hfix}
                        <div class="dryrun-diff">
                          <div class="dryrun-header">
                            <Lock size={11} style="color:var(--color-success)" />
                            <span>Proposed Change</span>
                            <code class="dryrun-target">{hfix.target}</code>
                          </div>
                          <div class="dryrun-rows">
                            <div class="dryrun-row dryrun-del"><span class="dryrun-sign">−</span><code>{hfix.current_label}</code></div>
                            <div class="dryrun-row dryrun-add"><span class="dryrun-sign">+</span><code>{hfix.proposed_label}</code></div>
                          </div>
                        </div>
                      {/if}
                    {/if}

                    {#if aiStore.enabled}
                      <Button
                        variant="outline"
                        size="sm"
                        onclick={() => triggerAiAdvisor(finding)}
                        title="Analyze finding & generate remediation command with Ollama AI"
                      >
                        <Sparkles size={13} style="color:var(--color-accent);" />
                        AI Advisor
                      </Button>
                    {/if}

                    {#if finding.has_auto_fix}
                      {#if !finding.is_resolved}
                        <!-- ── Tamper-flag gate ─────────────────────────── -->
                        {#if finding.tamper_flag && !reviewedTamperIds.includes(finding.id)}
                          <span class="tamper-block-label">
                            <AlertTriangle size={12} />
                            Review tamper warning before applying
                          </span>
                          <Button
                            variant="outline"
                            size="sm"
                            onclick={() => reviewedTamperIds = [...reviewedTamperIds, finding.id]}
                            title="Acknowledge the tamper warning and enable Apply Fix"
                          >
                            <Eye size={13} /> I've Reviewed
                          </Button>
                        {:else}
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
            {#if activeCategory === 'Runtime Threats'}
              <div style="display:flex; flex-direction:column; gap:16px; width:100%; box-sizing:border-box; padding:4px 0;">
                <!-- Status Banner -->
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:12px; padding:20px; display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:16px;">
                  <div style="display:flex; align-items:center; gap:14px;">
                    <div style="width:44px; height:44px; border-radius:50%; background:rgba(34,197,94,0.12); display:flex; align-items:center; justify-content:center; flex-shrink:0;">
                      <ShieldCheck size={26} color="var(--color-success)" />
                    </div>
                    <div>
                      <div style="font-size:15px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
                        Zero Active Runtime Threats Detected
                        <span style="font-size:10px; font-weight:700; padding:2px 8px; border-radius:10px; background:rgba(34,197,94,0.15); color:var(--color-success); font-family:var(--font-mono);">100% HEALTHY</span>
                      </div>
                      <div style="font-size:12px; color:var(--color-text-muted); margin-top:2px;">
                        The continuous threat engine evaluated authentication logs and kernel audit activity with 0 security breaches or anomalies found.
                      </div>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    style="font-size:11px;"
                    onclick={() => {
                      uiStore.setActiveTab('journal-logs');
                      setTimeout(() => {
                        window.dispatchEvent(new CustomEvent('journal-tab-select', { detail: 'threats' }));
                      }, 100);
                    }}
                  >
                    View Live Threats Log
                  </Button>
                </div>

                <!-- Active Safeguard Monitor Grid -->
                <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:10px; padding:16px;">
                  <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted); margin-bottom:12px;">
                    Continuous Threat Defense Monitors (6 Active)
                  </div>
                  <div style="display:grid; grid-template-columns:repeat(auto-fit, minmax(280px, 1fr)); gap:10px;">
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Sudo Password Brute-Force</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Monitors and flags repeated sudo password failures (≥ 3 attempts).</div>
                      </div>
                    </div>
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">SSH Inbound Attack Defense</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Tracks remote IP addresses attempting brute-force SSH logins (≥ 5 attempts).</div>
                      </div>
                    </div>
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">SELinux Disablement Watch</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Alerts immediately if <code>setenforce 0</code> or permissive mode is commanded.</div>
                      </div>
                    </div>
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Firewall Ruleset Integrity</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Flags malicious firewall wiping (<code>iptables -F</code>, <code>nft flush</code>).</div>
                      </div>
                    </div>
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Credential & Identity Protection</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Watches unauthorized modifications to <code>/etc/passwd</code>, <code>/etc/shadow</code>, and sudoers.</div>
                      </div>
                    </div>
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Direct Root Bypass Checks</div>
                        <div style="font-size:11px; color:var(--color-text-muted); margin-top:1px;">Detects elevated commands executed outside standard sudo/pkexec accountability.</div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {:else}
              <div class="empty-state">
                <ShieldCheck size={40} color="var(--color-success)" />
                <div>No issues in this category</div>
              </div>
            {/if}
          {/if}
        </div>
    {:else}
      <div class="center-state">
        <ShieldAlert size={48} color="var(--color-error)" />
        <div class="scan-label">Unable to load security audit</div>
        <div class="scan-sublabel">The audit scan could not be completed. Click below to retry.</div>
        <Button variant="primary" size="sm" onclick={() => runAudit(true)}>
          <RefreshCw size={14} /> Retry Audit
        </Button>
      </div>
    {/if}
  </div>
</div>

<AiSecurityAdvisorModal />

<!-- ── Styles ──────────────────────────────────────────────────────────────── -->
<style>
  .module-page.security-auditor {
    display: flex;
    flex-direction: column;
    position: absolute;
    inset: 0;
    padding: 0;
    overflow: hidden;
    background: var(--color-bg-base);
  }

  :global(.module-page.security-auditor .header-wrapper) {
    margin: 0 !important;
    flex-shrink: 0;
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

  /* ── Content Scroll Container ────────────────────────────────────────────── */
  .content-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scroll-behavior: smooth;
    padding: 0 24px 24px 24px;
    display: flex;
    flex-direction: column;
    -webkit-overflow-scrolling: touch;
    transform: translateZ(0);
  }

  .content-scroll::-webkit-scrollbar {
    width: 6px;
  }
  .content-scroll::-webkit-scrollbar-track {
    background: transparent;
  }
  .content-scroll::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 4px;
  }
  .content-scroll::-webkit-scrollbar-thumb:hover {
    background: var(--color-accent);
  }
  :global(html.light-mode) .content-scroll::-webkit-scrollbar-thumb {
    background: rgba(0, 0, 0, 0.2);
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

  /* ── Export Report Dropdown ────────────────────────────────────────────── */
  .export-dropdown-wrap {
    position: relative;
    display: inline-block;
  }

  .export-dropdown-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 100;
    min-width: 175px;
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menu-fade 0.15s ease;
  }

  .export-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
  }

  .export-menu-item:hover {
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-text-primary);
  }

  @keyframes menu-fade {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* ── Top Row ─────────────────────────────────────────────────────────────── */
  .top-row {
    margin-top: 4px;
    margin-bottom: 8px;
    display: grid;
    grid-template-columns: 360px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
    flex-shrink: 0;
  }

  /* ── Glass Panel ─────────────────────────────────────────────────────────── */
  .glass-panel {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 12px;
    padding: 16px;
  }

  /* ── Compact Score Card ─────────────────────────────────────────────────── */
  .score-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
  }

  .score-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .score-card-header h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .verdict-tag {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 12px;
    letter-spacing: 0.02em;
  }

  .score-card-body {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .score-gauge-compact {
    position: relative;
    width: 72px;
    height: 72px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .gauge-svg {
    transform: rotate(-90deg);
    width: 100%;
    height: 100%;
    filter: drop-shadow(0 0 6px var(--score-color));
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

  .score-text-compact {
    position: absolute;
    font-size: 20px;
    font-weight: 700;
    color: var(--score-color);
    letter-spacing: -0.5px;
  }

  .score-text-compact .pct {
    font-size: 10px;
    opacity: 0.7;
    font-weight: 400;
  }

  .score-info-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .critical-alert-compact {
    display: flex;
    align-items: center;
    gap: 5px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.25);
    color: var(--color-error);
    font-size: 11px;
    font-weight: 600;
    padding: 4px 8px;
    border-radius: 6px;
  }

  .clean-alert-compact {
    display: flex;
    align-items: center;
    gap: 5px;
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.25);
    color: var(--color-success);
    font-size: 11px;
    font-weight: 600;
    padding: 4px 8px;
    border-radius: 6px;
  }

  .sparkline-wrap-compact {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .sparkline-wrap-compact .sparkline-label {
    font-size: 9px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .sparkline {
    flex: 1;
    height: 26px;
    overflow: visible;
  }

  .sparkline-path {
    fill: none;
    stroke: var(--color-accent);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .issue-stats-compact {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    justify-content: space-between;
    width: 100%;
  }

  .stat-pill {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10.5px;
    font-weight: 600;
    padding: 3px 6px;
    border-radius: 5px;
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
  .stat-pill.muted    { background: rgba(255,255,255,.07); color: var(--color-text-muted); }
  .stat-pill.muted.active { background: rgba(255,255,255,.14); color: var(--color-text-secondary); }

  /* ── Dry-run diff preview (inside finding card) ─────────────────────────── */
  .dryrun-diff {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    overflow: hidden;
    margin-bottom: 10px;
  }

  :global(html.light-mode) .dryrun-diff {
    border-color: #E2E8F0;
  }

  .dryrun-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(34, 197, 94, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-success);
  }

  :global(html.light-mode) .dryrun-header {
    background: rgba(22, 163, 74, 0.05);
    border-bottom-color: #E2E8F0;
  }

  .dryrun-target {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 400;
    color: var(--color-text-muted);
    background: rgba(255,255,255,0.05);
    padding: 1px 5px;
    border-radius: 4px;
  }

  .dryrun-rows { display: flex; flex-direction: column; }

  .dryrun-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    font-size: 11.5px;
  }

  .dryrun-del { background: rgba(239, 68, 68, 0.08); }
  .dryrun-add { background: rgba(34, 197, 94, 0.08); }

  :global(html.light-mode) .dryrun-del { background: rgba(220, 38, 38, 0.06); }
  :global(html.light-mode) .dryrun-add { background: rgba(22, 163, 74, 0.06); }

  .dryrun-sign {
    font-family: var(--font-mono);
    font-weight: 700;
    font-size: 14px;
    width: 12px;
    flex-shrink: 0;
    text-align: center;
  }
  .dryrun-del .dryrun-sign { color: var(--color-error); }
  .dryrun-add .dryrun-sign { color: var(--color-success); }

  .dryrun-del code { color: rgba(239, 68, 68, 0.85); font-family: var(--font-mono); }
  .dryrun-add code { color: rgba(34, 197, 94, 0.9);  font-family: var(--font-mono); }
  :global(html.light-mode) .dryrun-del code { color: #dc2626; }
  :global(html.light-mode) .dryrun-add code { color: #16a34a; }

  /* ── Tamper-block label ──────────────────────────────────────────────────── */
  .tamper-block-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--color-warning);
    background: rgba(245, 158, 11, 0.10);
    border: 1px solid rgba(245, 158, 11, 0.25);
    padding: 4px 9px;
    border-radius: 6px;
  }

  /* ── Category Grid ───────────────────────────────────────────────────────── */
  .category-grid-wrap {
    width: 100%;
  }

  .category-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    align-content: start;
  }

  .cat-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 9px 11px;
    cursor: pointer;
    transition: all 0.18s ease;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cat-card:hover {
    background: rgba(255, 255, 255, 0.05);
    border-color: var(--cat-color);
  }

  .cat-card.cat-active {
    background: color-mix(in srgb, var(--cat-color) 10%, transparent);
    border-color: var(--cat-color);
  }

  .cat-card.cat-threat {
    border-color: rgba(239, 68, 68, 0.4);
    background: rgba(239, 68, 68, 0.05);
    animation: threat-pulse 2.5s ease-in-out infinite;
  }
  .cat-card.cat-threat:hover {
    border-color: rgba(239, 68, 68, 0.7);
    background: rgba(239, 68, 68, 0.1);
  }
  @keyframes threat-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(239, 68, 68, 0); }
    50% { box-shadow: 0 0 8px 2px rgba(239, 68, 68, 0.15); }
  }

  .cat-header {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--cat-color);
    font-size: 11px;
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
    padding: 1px 5px;
    border-radius: 10px;
  }

  .cat-bar-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
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
    font-size: 10.5px;
    font-weight: 700;
    color: var(--cat-color);
    width: 30px;
    text-align: right;
  }

  /* ── Findings Section ────────────────────────────────────────────────────── */
  .findings-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .findings-header {
    position: sticky;
    top: 0;
    z-index: 50;
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    flex-shrink: 0;
    background: rgba(10, 15, 29, 0.85);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    padding: 10px 0;
    margin-bottom: 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  :global(html.light-mode) .findings-header {
    background: rgba(248, 250, 252, 0.88);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border-bottom: 1px solid rgba(226, 232, 240, 0.8);
    box-shadow: none;
  }

  .findings-header-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .issue-badge {
    font-size: 11.5px;
    font-weight: 600;
    padding: 4px 12px;
    border-radius: 20px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-secondary);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .issue-badge.has-issues {
    background: rgba(239, 68, 68, 0.1);
    color: var(--color-error);
    border-color: rgba(239, 68, 68, 0.25);
  }

  :global(html.light-mode) .issue-badge {
    background: #FFFFFF;
    color: #475569;
    border: 1px solid #E2E8F0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
  }

  :global(html.light-mode) .issue-badge.has-issues {
    background: #FEF2F2;
    color: #DC2626;
    border-color: #FECACA;
  }

  /* ── Findings List ────────────────────────────────────────────────────────── */
  .findings-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-bottom: 32px;
  }

  /* ── Finding Card ────────────────────────────────────────────────────────── */
  .finding-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-left: 4px solid var(--sev-color);
    border-radius: 10px;
    overflow: hidden;
    transition: background 0.12s ease, border-color 0.12s ease;
    contain: layout style;
  }

  .finding-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
    border-left-color: var(--sev-color);
    background: rgba(255, 255, 255, 0.02);
  }

  .finding-card.resolved {
    opacity: 0.65;
  }

  .finding-card.muted {
    opacity: 0.45;
    border-left-color: var(--color-text-muted) !important;
  }

  .std-tag {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(168, 85, 247, 0.15);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.3);
    white-space: nowrap;
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

  .cm-nav-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    font-family: inherit;
    padding: 5px 10px;
    border-radius: 6px;
    border: 1px solid rgba(0, 218, 243, 0.3);
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-accent);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .cm-nav-btn:hover {
    background: rgba(0, 218, 243, 0.18);
    border-color: rgba(0, 218, 243, 0.6);
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
  @keyframes spin { 100% { transform: rotate(360deg); } }

  .spinner-sm {
    width: 13px;
    height: 13px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  /* ── Light Mode Category Summary Grid Cards ── */
  :global(html.light-mode) .cat-card {
    background: #FFFFFF !important;
    border: 1px solid #E5E7EB !important;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05) !important;
  }
  :global(html.light-mode) .cat-card:hover {
    background: #F9FAFB !important;
    border-color: #D1D5DB !important;
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.08) !important;
  }
  :global(html.light-mode) .cat-card.cat-active {
    background: rgba(37, 99, 235, 0.08) !important;
    border-color: #2563EB !important;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.15) !important;
  }
  :global(html.light-mode) .cat-name {
    color: #111827 !important;
  }
  :global(html.light-mode) .cat-bar {
    background: #E5E7EB !important;
  }

  /* ── Light Mode Accordion & Finding Card Overrides ── */
  :global(html.light-mode) .finding-card {
    background: #FFFFFF !important;
    border: 1px solid #E5E7EB !important;
    border-left: 4px solid var(--sev-color) !important;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05) !important;
  }
  :global(html.light-mode) .finding-card:hover,
  :global(html.light-mode) .finding-card.expanded {
    border-color: #D1D5DB !important;
    border-left: 4px solid var(--sev-color) !important;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08) !important;
    background: #FFFFFF !important;
  }
  :global(html.light-mode) .finding-row {
    background: #FFFFFF !important;
    color: #111827 !important;
  }
  :global(html.light-mode) .finding-row:hover {
    background: #F9FAFB !important;
  }
  :global(html.light-mode) .finding-title {
    color: #111827 !important;
  }
  :global(html.light-mode) .finding-desc {
    color: #4B5563 !important;
  }
  :global(html.light-mode) .finding-detail {
    background: #FAFAFA !important;
    border-top: 1px solid #E5E7EB !important;
  }
  :global(html.light-mode) .countermeasure-box {
    background: rgba(37, 99, 235, 0.06) !important;
    border: 1px solid rgba(37, 99, 235, 0.2) !important;
    color: #111827 !important;
  }
  :global(html.light-mode) .cm-text {
    color: #374151 !important;
  }
  :global(html.light-mode) .cat-tag {
    background: #F3F4F6 !important;
    color: #374151 !important;
    border: 1px solid #E5E7EB !important;
  }
  :global(html.light-mode) .std-tag {
    background: rgba(37, 99, 235, 0.08) !important;
    color: #1D4ED8 !important;
    border: 1px solid rgba(37, 99, 235, 0.20) !important;
  }
  :global(html.light-mode) .issue-badge {
    background: #F3F4F6 !important;
    color: #374151 !important;
    border: 1px solid #E5E7EB !important;
  }
  :global(html.light-mode) .issue-badge.has-issues {
    background: rgba(239, 68, 68, 0.1) !important;
    color: #DC2626 !important;
    border-color: rgba(239, 68, 68, 0.25) !important;
  }
</style>
