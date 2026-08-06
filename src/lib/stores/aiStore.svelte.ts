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
  remediation_command: string;
  safety_notes: string;
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
  ollamaModel = $state('qwen2.5:1.5b');
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
        this.ollamaModel = cfg.ollama_model || 'qwen2.5:1.5b';
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
      if (models.length > 0 && !models.includes(this.ollamaModel)) {
        this.ollamaModel = models[0];
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
    this.activeLogContext = logContext;
    this.activeLogService = serviceName || 'System Journal / Audit';
    this.logResult = null;
    this.analysisError = null;
    this.analyzing = true;
    this.activeModalType = 'log';

    try {
      const res = await invoke<LogDiagnosisResponse>('ai_diagnose_log_error', {
        logContext,
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
