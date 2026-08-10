import { invoke } from '@tauri-apps/api/core';
import { uiStore } from './ui.svelte.ts';

export interface SecurityFindingInput {
  id: string;
  title: string;
  severity: string;
  category: string;
  description: string;
  current_value: string;
  recommendation: string;
}

export interface AiAdvisorResponse {
  risk_explanation: string;
  safety_notes: string;
}

/** A pre-validated, hardcoded fix entry for a finding. Never comes from LLM output. */
export interface HardcodedFix {
  /** Human-readable label shown in the diff panel header */
  label: string;
  /** The setting/file that will be changed */
  target: string;
  /** The current (insecure) value shown in the red diff row */
  current_label: string;
  /** The proposed (secure) value shown in the green diff row */
  proposed_label: string;
  /** Tauri command name to invoke */
  tauri_command: string;
  /** Arguments to pass to the Tauri command */
  tauri_args: Record<string, unknown>;
}

/** Hardcoded, validated fix table. Key = SecurityFinding.id.
 *  NEVER sourced from LLM output. Only extend this by adding to the switch below. */
const SECURITY_FIX_LOOKUP: Record<string, HardcodedFix> = {
  ssh_root: {
    label: 'SSH Root Login',
    target: '/etc/ssh/sshd_config → PermitRootLogin',
    current_label: 'PermitRootLogin yes',
    proposed_label: 'PermitRootLogin prohibit-password',
    tauri_command: 'security_fix_root_ssh',
    tauri_args: { enable: true },
  },
  ssh_pass_auth: {
    label: 'SSH Password Authentication',
    target: '/etc/ssh/sshd_config → PasswordAuthentication',
    current_label: 'PasswordAuthentication yes',
    proposed_label: 'PasswordAuthentication no',
    tauri_command: 'security_fix_ssh_param',
    tauri_args: { param: 'PasswordAuthentication', value: 'no', revertValue: 'yes', enable: true },
  },
  ssh_max_auth: {
    label: 'SSH MaxAuthTries',
    target: '/etc/ssh/sshd_config → MaxAuthTries',
    current_label: 'MaxAuthTries 6  (default)',
    proposed_label: 'MaxAuthTries 4',
    tauri_command: 'security_fix_ssh_param',
    tauri_args: { param: 'MaxAuthTries', value: '4', revertValue: '6', enable: true },
  },
  ssh_grace: {
    label: 'SSH LoginGraceTime',
    target: '/etc/ssh/sshd_config → LoginGraceTime',
    current_label: 'LoginGraceTime 120  (default)',
    proposed_label: 'LoginGraceTime 60',
    tauri_command: 'security_fix_ssh_param',
    tauri_args: { param: 'LoginGraceTime', value: '60', revertValue: '120', enable: true },
  },
  kernel_aslr: {
    label: 'ASLR (Address Space Layout Randomization)',
    target: 'kernel.randomize_va_space',
    current_label: 'kernel.randomize_va_space = 0  (disabled/partial)',
    proposed_label: 'kernel.randomize_va_space = 2  (full randomization)',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'kernel.randomize_va_space', value: '2', revertValue: '0', enable: true },
  },
  kernel_syncookies: {
    label: 'TCP SYN Cookies',
    target: 'net.ipv4.tcp_syncookies',
    current_label: 'net.ipv4.tcp_syncookies = 0',
    proposed_label: 'net.ipv4.tcp_syncookies = 1',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.tcp_syncookies', value: '1', revertValue: '0', enable: true },
  },
  kernel_ipforward: {
    label: 'IP Forwarding',
    target: 'net.ipv4.ip_forward',
    current_label: 'net.ipv4.ip_forward = 1  (enabled)',
    proposed_label: 'net.ipv4.ip_forward = 0  (disabled)',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.ip_forward', value: '0', revertValue: '1', enable: true },
  },
  kernel_kptr: {
    label: 'Kernel Pointer Restriction',
    target: 'kernel.kptr_restrict',
    current_label: 'kernel.kptr_restrict = 0',
    proposed_label: 'kernel.kptr_restrict = 2',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'kernel.kptr_restrict', value: '2', revertValue: '0', enable: true },
  },
  kernel_dmesg: {
    label: 'dmesg Restriction',
    target: 'kernel.dmesg_restrict',
    current_label: 'kernel.dmesg_restrict = 0',
    proposed_label: 'kernel.dmesg_restrict = 1',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'kernel.dmesg_restrict', value: '1', revertValue: '0', enable: true },
  },
  kernel_icmp_redirect: {
    label: 'ICMP Redirect Acceptance',
    target: 'net.ipv4.conf.all.accept_redirects',
    current_label: 'net.ipv4.conf.all.accept_redirects = 1',
    proposed_label: 'net.ipv4.conf.all.accept_redirects = 0',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.conf.all.accept_redirects', value: '0', revertValue: '1', enable: true },
  },
  pass_policy: {
    label: 'Password Aging Policy',
    target: '/etc/login.defs',
    current_label: 'PASS_MAX_DAYS unset or > 90',
    proposed_label: 'PASS_MAX_DAYS=90, PASS_MIN_LEN=12',
    tauri_command: 'security_fix_password_policy',
    tauri_args: {},
  },
  fs_tmp_sticky: {
    label: '/tmp Sticky Bit',
    target: '/tmp permissions',
    current_label: '/tmp mode = 777  (no sticky bit)',
    proposed_label: '/tmp mode = 1777  (sticky bit set)',
    tauri_command: 'security_fix_tmp_sticky',
    tauri_args: { enable: true },
  },
  fs_coredump: {
    label: 'SUID Core Dumps',
    target: 'fs.suid_dumpable',
    current_label: 'fs.suid_dumpable = 1  (dumps allowed)',
    proposed_label: 'fs.suid_dumpable = 0  (dumps disabled)',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'fs.suid_dumpable', value: '0', revertValue: '1', enable: true },
  },
  fs_passwd_perms: {
    label: '/etc/passwd Permissions',
    target: '/etc/passwd mode',
    current_label: '/etc/passwd  — incorrect permissions',
    proposed_label: '/etc/passwd  — chmod 644 + chown root:root',
    tauri_command: 'security_fix_passwd_perms',
    tauri_args: {},
  },
  fs_shadow_perms: {
    label: '/etc/shadow Permissions',
    target: '/etc/shadow mode',
    current_label: '/etc/shadow  — world-readable or too open',
    proposed_label: '/etc/shadow  — chmod 000 + chown root:root',
    tauri_command: 'security_fix_shadow_perms',
    tauri_args: {},
  },
  net_src_route: {
    label: 'Source Routing',
    target: 'net.ipv4.conf.all.accept_source_route',
    current_label: 'net.ipv4.conf.all.accept_source_route = 1',
    proposed_label: 'net.ipv4.conf.all.accept_source_route = 0',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.conf.all.accept_source_route', value: '0', revertValue: '1', enable: true },
  },
  net_bogus_icmp: {
    label: 'Bogus ICMP Error Responses',
    target: 'net.ipv4.icmp_ignore_bogus_error_responses',
    current_label: 'net.ipv4.icmp_ignore_bogus_error_responses = 0',
    proposed_label: 'net.ipv4.icmp_ignore_bogus_error_responses = 1',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.icmp_ignore_bogus_error_responses', value: '1', revertValue: '0', enable: true },
  },
  net_martians: {
    label: 'Martian Packet Logging',
    target: 'net.ipv4.conf.all.log_martians',
    current_label: 'net.ipv4.conf.all.log_martians = 0',
    proposed_label: 'net.ipv4.conf.all.log_martians = 1',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.conf.all.log_martians', value: '1', revertValue: '0', enable: true },
  },
  selinux: {
    label: 'SELinux Mode',
    target: '/etc/selinux/config → SELINUX=',
    current_label: 'SELINUX=permissive  (or disabled)',
    proposed_label: 'SELINUX=enforcing  (+ filesystem relabel + reboot)',
    tauri_command: 'security_fix_selinux',
    tauri_args: { enable: true },
  },
  auditd: {
    label: 'Audit Daemon (auditd)',
    target: 'systemd unit: auditd',
    current_label: 'auditd — inactive / disabled',
    proposed_label: 'auditd — enabled + running',
    tauri_command: 'security_fix_auditd',
    tauri_args: { enable: true },
  },
  time_sync: {
    label: 'Time Synchronization',
    target: 'systemd unit: chronyd / systemd-timesyncd',
    current_label: 'chronyd / timesyncd — inactive',
    proposed_label: 'chronyd — enabled + running',
    tauri_command: 'security_fix_time_sync',
    tauri_args: { enable: true },
  },
  usbguard: {
    label: 'USBGuard',
    target: 'systemd unit: usbguard',
    current_label: 'usbguard — inactive',
    proposed_label: 'usbguard — enabled + running',
    tauri_command: 'security_fix_usbguard',
    tauri_args: { enable: true },
  },
  firewall: {
    label: 'System Firewall Service',
    target: 'systemd unit: firewalld / ufw',
    current_label: 'firewalld / ufw — inactive / disabled',
    proposed_label: 'firewalld — enabled + active (systemctl enable --now firewalld)',
    tauri_command: 'security_fix_firewall',
    tauri_args: { enable: true },
  },
  empty_passwords: {
    label: 'Lock Accounts with Empty Passwords',
    target: '/etc/shadow → lock empty password user(s)',
    current_label: 'Account(s) with empty password field detected',
    proposed_label: 'Lock account(s) via passwd -l',
    tauri_command: 'security_fix_lock_account',
    tauri_args: {},
  },
  pam_faillock: {
    label: 'PAM Account Lockout Policy',
    target: '/etc/security/faillock.conf.d/50-hardening.conf',
    current_label: 'faillock policy — unconfigured / missing',
    proposed_label: 'deny = 5, unlock_time = 900s',
    tauri_command: 'security_fix_pam_faillock',
    tauri_args: { enable: true },
  },
  umask_policy: {
    label: 'Default System umask',
    target: '/etc/login.defs → UMASK',
    current_label: 'UMASK 022  (overly permissive default)',
    proposed_label: 'UMASK 027  (stricter default for new files)',
    tauri_command: 'security_fix_umask',
    tauri_args: { enable: true },
  },
  ssh_x11: {
    label: 'SSH X11 Forwarding',
    target: '/etc/ssh/sshd_config → X11Forwarding',
    current_label: 'X11Forwarding yes',
    proposed_label: 'X11Forwarding no',
    tauri_command: 'security_fix_ssh_param',
    tauri_args: { param: 'X11Forwarding', value: 'no', revertValue: 'yes', enable: true },
  },
  ssh_idle_timeout: {
    label: 'SSH Client Idle Timeout',
    target: '/etc/ssh/sshd_config.d/50-idle-timeout.conf',
    current_label: 'ClientAliveInterval 0 (no idle timeout)',
    proposed_label: 'ClientAliveInterval 300, ClientAliveCountMax 3',
    tauri_command: 'security_fix_ssh_idle_timeout',
    tauri_args: { enable: true },
  },
  kernel_fs_modules: {
    label: 'Unused Legacy Filesystem Modules',
    target: '/etc/modprobe.d/disable-unused-fs.conf',
    current_label: 'Legacy FS drivers (cramfs, hfs, etc.) unblacklisted',
    proposed_label: 'Blacklist cramfs, hfs, udf via modprobe.d',
    tauri_command: 'security_fix_blacklist_fs_modules',
    tauri_args: { enable: true },
  },
  kernel_rp_filter: {
    label: 'Reverse Path Filtering (rp_filter)',
    target: 'net.ipv4.conf.all.rp_filter',
    current_label: 'net.ipv4.conf.all.rp_filter = 0',
    proposed_label: 'net.ipv4.conf.all.rp_filter = 1',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'net.ipv4.conf.all.rp_filter', value: '1', revertValue: '0', enable: true },
  },
  kernel_sysrq: {
    label: 'Magic SysRq Key',
    target: 'kernel.sysrq',
    current_label: 'kernel.sysrq = 1  (unrestricted)',
    proposed_label: 'kernel.sysrq = 4  (restricted to attention key)',
    tauri_command: 'security_fix_kernel_param',
    tauri_args: { key: 'kernel.sysrq', value: '4', revertValue: '1', enable: true },
  },
  kernel_ctrl_alt_del: {
    label: 'Ctrl-Alt-Del Reboot Signal',
    target: 'systemd unit: ctrl-alt-del.target',
    current_label: 'ctrl-alt-del.target — active (reboots on keypress)',
    proposed_label: 'ctrl-alt-del.target — masked',
    tauri_command: 'security_fix_mask_ctrl_alt_del',
    tauri_args: { enable: true },
  },
  sys_legal_banner: {
    label: 'Legal Access Warning Banner',
    target: '/etc/issue & /etc/ssh/sshd_config.d/50-banner.conf',
    current_label: 'No legal warning banner configured',
    proposed_label: 'Authorized use legal notice configured',
    tauri_command: 'security_fix_legal_banner',
    tauri_args: { enable: true },
  },
};

/** Returns the hardcoded fix entry for a finding, or null if none exists. */
export function getHardcodedFix(findingId: string): HardcodedFix | null {
  return SECURITY_FIX_LOOKUP[findingId] ?? null;
}

export interface LogDiagnosisResponse {
  error_summary: string;
  root_cause: string;
  suggested_action: string;
}

export interface DnfConflictResponse {
  conflict_summary: string;
  remediation_command: string;
  explanation: string;
}

export interface NginxRuleResponse {
  generated_config: string;
  explanation: string;
  server_name: string;
  port: number;
}

export interface FirewallRuleResponse {
  generated_command: string;
  rich_rule: string;
  explanation: string;
  zone: string;
}

export interface AiSettingsConfig {
  enabled: boolean;
  provider: 'ollama' | 'gemini' | 'openai';
  ollama_url: string;
  ollama_model: string;
  cloud_provider: 'gemini' | 'openai';
  api_key: string;
  cloud_model: string;
}

export class AiStore {
  // Master ON/OFF Switch
  enabled = $state(true);

  // Config state
  provider = $state<'ollama' | 'gemini' | 'openai'>('ollama');
  ollamaUrl = $state('http://127.0.0.1:11434');
  ollamaModel = $state('llama3.2:1b');
  cloudProvider = $state<'gemini' | 'openai'>('gemini');
  apiKey = $state('');
  cloudModel = $state('gemini-2.5-flash');

  // Connection & model list
  ollamaConnected = $state(false);
  availableModels = $state<string[]>([]);

  // Active Task Modal State
  analyzing = $state(false);
  analysisError = $state<string | null>(null);
  activeModalType = $state<'finding' | 'log' | 'dnf' | 'nginx' | 'firewall' | null>(null);

  // Result payloads
  activeFinding = $state<SecurityFindingInput | null>(null);
  findingResult = $state<AiAdvisorResponse | null>(null);

  activeLogContext = $state<string>('');
  activeLogService = $state<string>('');
  logResult = $state<LogDiagnosisResponse | null>(null);

  activeDnfOutput = $state<string>('');
  dnfResult = $state<DnfConflictResponse | null>(null);

  activeNginxPrompt = $state<string>('');
  nginxResult = $state<NginxRuleResponse | null>(null);

  activeFirewallPrompt = $state<string>('');
  firewallResult = $state<FirewallRuleResponse | null>(null);

  constructor() {
    this.loadSettings();
  }

  async loadSettings() {
    try {
      const cfg = await invoke<AiSettingsConfig>('ai_load_settings');
      if (cfg) {
        this.enabled = cfg.enabled ?? true;
        this.provider = (cfg.provider as any) || 'ollama';
        this.ollamaUrl = cfg.ollama_url || 'http://127.0.0.1:11434';
        this.ollamaModel = cfg.ollama_model || 'llama3.2:1b';
        this.cloudProvider = (cfg.cloud_provider as any) || 'gemini';
        this.apiKey = cfg.api_key || '';
        this.cloudModel = cfg.cloud_model || 'gemini-2.5-flash';
      }
    } catch (e) {
      console.warn('Failed to load persistent AI settings:', e);
    }
    this.checkOllamaStatus();
  }

  async saveSettings() {
    const payload: AiSettingsConfig = {
      enabled: this.enabled,
      provider: this.provider,
      ollama_url: this.ollamaUrl,
      ollama_model: this.ollamaModel,
      cloud_provider: this.cloudProvider,
      api_key: this.apiKey,
      cloud_model: this.cloudModel,
    };
    try {
      await invoke('ai_save_settings', { settings: payload });
      uiStore.addToast('AI settings saved successfully', 'success');
      this.checkOllamaStatus();
    } catch (e: any) {
      uiStore.addToast(`Failed to save AI settings: ${e}`, 'error');
    }
  }

  async checkOllamaStatus() {
    try {
      const models = await invoke<string[]>('ai_check_ollama_status', { url: this.ollamaUrl });
      this.ollamaConnected = true;
      this.availableModels = models;
      if (models.length > 0) {
        if (!models.includes(this.ollamaModel)) {
          const llamaModel = models.find(m => m.includes('llama3.2') || m.includes('llama'));
          this.ollamaModel = llamaModel || models[0];
        }
      }
    } catch (e) {
      this.ollamaConnected = false;
      this.availableModels = [];
    }
  }

  getSettingsPayload(): AiSettingsConfig {
    return {
      enabled: this.enabled,
      provider: this.provider,
      ollama_url: this.ollamaUrl,
      ollama_model: this.ollamaModel,
      cloud_provider: this.cloudProvider,
      api_key: this.apiKey,
      cloud_model: this.cloudModel,
    };
  }

  // ── Task Handlers ─────────────────────────────────────────────────────────

  async explainFinding(finding: SecurityFindingInput) {
    this.activeFinding = finding;
    this.findingResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'finding';

    try {
      const res = await invoke<AiAdvisorResponse>('ai_explain_security_finding', {
        finding,
        settings: this.getSettingsPayload(),
      });
      this.findingResult = res;
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.analysisError = msg;
      uiStore.addToast(`AI Advisor Error: ${msg}`, 'error');
    } finally {
      this.analyzing = false;
    }
  }

  async diagnoseLogError(logContext: string, serviceName?: string) {
    if (!logContext || !logContext.trim()) {
      uiStore.addToast('Cannot diagnose an empty log message.', 'warning');
      return;
    }
    this.activeLogContext = logContext.trim();
    this.activeLogService = serviceName || 'System Journal / Audit';
    this.logResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'log';

    try {
      const res = await invoke<LogDiagnosisResponse>('ai_diagnose_log_error', {
        logContext: logContext.trim(),
        serviceName: this.activeLogService,
        settings: this.getSettingsPayload(),
      });
      this.logResult = res;
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.analysisError = msg;
      uiStore.addToast(`AI Log Diagnosis Error: ${msg}`, 'error');
    } finally {
      this.analyzing = false;
    }
  }

  async explainDnfConflict(terminalOutput: string) {
    this.activeDnfOutput = terminalOutput;
    this.dnfResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'dnf';

    try {
      const res = await invoke<DnfConflictResponse>('ai_explain_dnf_conflict', {
        terminalOutput,
        settings: this.getSettingsPayload(),
      });
      this.dnfResult = res;
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.analysisError = msg;
      uiStore.addToast(`AI DNF Analysis Error: ${msg}`, 'error');
    } finally {
      this.analyzing = false;
    }
  }

  async generateNginxRule(prompt: string) {
    this.activeNginxPrompt = prompt;
    this.nginxResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'nginx';

    try {
      const res = await invoke<NginxRuleResponse>('ai_generate_nginx_rule', {
        prompt,
        settings: this.getSettingsPayload(),
      });
      this.nginxResult = res;
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.analysisError = msg;
      uiStore.addToast(`AI NGINX Generation Error: ${msg}`, 'error');
    } finally {
      this.analyzing = false;
    }
  }

  async generateFirewallRule(prompt: string) {
    this.activeFirewallPrompt = prompt;
    this.firewallResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'firewall';

    try {
      const res = await invoke<FirewallRuleResponse>('ai_generate_firewall_rule', {
        prompt,
        settings: this.getSettingsPayload(),
      });
      this.firewallResult = res;
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.analysisError = msg;
      uiStore.addToast(`AI Firewall Rule Generation Error: ${msg}`, 'error');
    } finally {
      this.analyzing = false;
    }
  }

  closeModal() {
    this.activeModalType = null;
    this.activeFinding = null;
    this.findingResult = null;
    this.logResult = null;
    this.dnfResult = null;
    this.nginxResult = null;
    this.firewallResult = null;
    this.analysisError = null;
  }
}

export const aiStore = new AiStore();
