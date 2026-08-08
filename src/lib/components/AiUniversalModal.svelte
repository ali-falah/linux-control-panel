<script lang="ts">
  import { Sparkles, X, Bot, ShieldAlert, Terminal, Check, Copy, RefreshCw, AlertTriangle, ShieldCheck, FileText, Package, Server, Shield, Zap } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Button from './ui/Button.svelte';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  let copied = $state(false);
  let executingRemediation = $state(false);

  function copyText(txt: string, msg: string = 'Copied to clipboard') {
    navigator.clipboard.writeText(txt);
    copied = true;
    uiStore.addToast(msg, 'success');
    setTimeout(() => { copied = false; }, 2000);
  }

  function handleRunRemediation() {
    const cmd = aiStore.findingResult?.remediation_command;
    if (!cmd || !cmd.trim()) return;

    uiStore.confirm(
      'Execute AI Remediation Command',
      `Are you sure you want to execute this AI remediation command?\n\n$ ${cmd.trim()}`,
      async () => {
        executingRemediation = true;
        try {
          const res = await invoke<string>('security_execute_remediation_command', { command: cmd.trim() });
          uiStore.addToast(res || 'Remediation script executed successfully!', 'success');
          window.dispatchEvent(new CustomEvent('security-audit-run'));
          aiStore.closeModal();
        } catch (err: any) {
          uiStore.addToast(`Remediation Error: ${err}`, 'error');
        } finally {
          executingRemediation = false;
        }
      }
    );
  }

  function handleRetry() {
    if (aiStore.activeModalType === 'finding' && aiStore.activeFinding) {
      aiStore.explainFinding(aiStore.activeFinding);
    } else if (aiStore.activeModalType === 'log' && aiStore.activeLogContext) {
      aiStore.diagnoseLogError(aiStore.activeLogContext, aiStore.activeLogService);
    } else if (aiStore.activeModalType === 'dnf' && aiStore.activeDnfOutput) {
      aiStore.explainDnfConflict(aiStore.activeDnfOutput);
    } else if (aiStore.activeModalType === 'nginx' && aiStore.activeNginxPrompt) {
      aiStore.generateNginxRule(aiStore.activeNginxPrompt);
    } else if (aiStore.activeModalType === 'firewall' && aiStore.activeFirewallPrompt) {
      aiStore.generateFirewallRule(aiStore.activeFirewallPrompt);
    }
  }

  let providerLabel = $derived(
    aiStore.provider === 'gemini'
      ? `Google Gemini API (${aiStore.cloudModel || 'gemini-2.5-flash'})`
      : aiStore.provider === 'openai'
      ? `OpenAI API (${aiStore.cloudModel || 'gpt-4o-mini'})`
      : `Local Ollama (${aiStore.ollamaModel || 'qwen2.5:1.5b'})`
  );
</script>

{#if aiStore.activeModalType !== null}
  <div
    class="ai-modal-backdrop"
    role="button"
    tabindex="0"
    onclick={() => aiStore.closeModal()}
    onkeydown={(e) => { if (e.key === 'Escape') aiStore.closeModal(); }}
  >
    <div
      class="ai-modal-content"
      role="document"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      
      <!-- Header -->
      <div class="ai-modal-header">
        <div class="ai-header-title">
          <div class="ai-icon-glow">
            <Sparkles size={18} class="text-accent" />
          </div>
          <div>
            <h3 class="modal-title">
              {#if aiStore.activeModalType === 'finding'}
                AI Security Advisor
              {:else if aiStore.activeModalType === 'log'}
                AI Log Error Diagnoser
              {:else if aiStore.activeModalType === 'dnf'}
                AI DNF Package Conflict Assistant
              {:else if aiStore.activeModalType === 'nginx'}
                AI NGINX Configuration Generator
              {:else if aiStore.activeModalType === 'firewall'}
                AI Firewall Rule Generator
              {/if}
            </h3>
            <span class="modal-subtitle">
              Powered by <code class="model-tag">{providerLabel}</code>
            </span>
          </div>
        </div>
        <button class="close-btn" onclick={() => aiStore.closeModal()}>
          <X size={16} />
        </button>
      </div>

      <!-- Content Body -->
      <div class="ai-modal-body">
        
        <!-- Context Header Card -->
        {#if aiStore.activeModalType === 'finding' && aiStore.activeFinding}
          <div class="context-card">
            <div class="context-meta">
              <span class="badge {aiStore.activeFinding.severity === 'HIGH' ? 'badge-error' : aiStore.activeFinding.severity === 'MEDIUM' ? 'badge-warning' : 'badge-info'}">
                {aiStore.activeFinding.severity}
              </span>
              <span class="badge badge-muted">{aiStore.activeFinding.category}</span>
            </div>
            <h4 class="context-title">{aiStore.activeFinding.title}</h4>
            <div class="context-val">Current Value: <code>{aiStore.activeFinding.current_value || 'Not configured'}</code></div>
          </div>

        {:else if aiStore.activeModalType === 'log'}
          <div class="context-card">
            <div class="context-meta">
              <span class="badge badge-info"><FileText size={11} /> Journal &amp; Audit Log</span>
              <span class="badge badge-muted">{aiStore.activeLogService}</span>
            </div>
            <pre class="snippet-preview"><code>{aiStore.activeLogContext.slice(0, 200)}...</code></pre>
          </div>

        {:else if aiStore.activeModalType === 'dnf'}
          <div class="context-card">
            <div class="context-meta">
              <span class="badge badge-warning"><Package size={11} /> DNF Package Manager</span>
            </div>
            <pre class="snippet-preview"><code>{aiStore.activeDnfOutput.slice(0, 200)}...</code></pre>
          </div>

        {:else if aiStore.activeModalType === 'nginx'}
          <div class="context-card">
            <div class="context-meta">
              <span class="badge badge-accent"><Server size={11} /> NGINX Assistant</span>
            </div>
            <div class="prompt-text">"{aiStore.activeNginxPrompt}"</div>
          </div>

        {:else if aiStore.activeModalType === 'firewall'}
          <div class="context-card">
            <div class="context-meta">
              <span class="badge badge-warning"><Shield size={11} /> Firewalld Assistant</span>
            </div>
            <div class="prompt-text">"{aiStore.activeFirewallPrompt}"</div>
          </div>
        {/if}

        <!-- Loading State -->
        {#if aiStore.analyzing}
          <div class="ai-loading-state">
            <div class="pulse-sphere">
              <Bot size={32} class="animate-pulse text-accent" />
            </div>
            <div class="loading-text">
              <span class="loading-title">Analyzing Request…</span>
              <span class="loading-desc">AI Engine is processing system context and generating optimal recommendations.</span>
            </div>
          </div>

        <!-- 1. Security Finding Result -->
        {:else if aiStore.activeModalType === 'finding' && aiStore.findingResult}
          <div class="ai-results-container">
            <div class="ai-card risk-card">
              <div class="card-header">
                <ShieldAlert size={16} class="text-warning" />
                <span>Risk &amp; Exploit Potential</span>
              </div>
              <p class="card-text">{aiStore.findingResult.risk_explanation}</p>
            </div>

            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Suggested Remediation Command</span>
                </div>
                <div style="display: flex; gap: 6px; align-items: center;">
                  <Button variant="outline" size="sm" onclick={() => copyText(aiStore.findingResult?.remediation_command || '')}>
                    {#if copied}<Check size={13} style="color:var(--color-success)" /> Copied{:else}<Copy size={13} /> Copy{/if}
                  </Button>
                  <Button
                    variant="primary"
                    size="sm"
                    onclick={handleRunRemediation}
                    disabled={executingRemediation}
                    style="display: flex; align-items: center; gap: 6px;"
                    title="Execute this remediation command with elevated privileges"
                  >
                    {#if executingRemediation}
                      <RefreshCw size={13} class="animate-spin-slow" />
                      <span>Executing...</span>
                    {:else}
                      <Zap size={13} />
                      <span>Run 1-Click Fix</span>
                    {/if}
                  </Button>
                </div>
              </div>
              <pre class="command-block"><code>{aiStore.findingResult.remediation_command}</code></pre>
            </div>

            <div class="ai-card safety-card">
              <div class="card-header">
                <ShieldCheck size={16} style="color:var(--color-success)" />
                <span>Safety &amp; Operational Impact</span>
              </div>
              <p class="card-text">{aiStore.findingResult.safety_notes}</p>
            </div>
          </div>

        <!-- 2. Log Diagnosis Result -->
        {:else if aiStore.activeModalType === 'log' && aiStore.logResult}
          <div class="ai-results-container">
            <div class="ai-card">
              <div class="card-header">
                <ShieldAlert size={16} class="text-error" />
                <span>Error Summary</span>
              </div>
              <p class="card-text">{aiStore.logResult.error_summary}</p>
            </div>

            <div class="ai-card">
              <div class="card-header">
                <AlertTriangle size={16} class="text-warning" />
                <span>Probable Root Cause</span>
              </div>
              <p class="card-text">{aiStore.logResult.root_cause}</p>
            </div>

            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Recommended Action / Command</span>
                </div>
                <Button variant="outline" size="sm" onclick={() => copyText(aiStore.logResult?.suggested_action || '')}>
                  {#if copied}<Check size={13} style="color:var(--color-success)" /> Copied{:else}<Copy size={13} /> Copy Action{/if}
                </Button>
              </div>
              <pre class="command-block"><code>{aiStore.logResult.suggested_action}</code></pre>
            </div>
          </div>

        <!-- 3. DNF Conflict Result -->
        {:else if aiStore.activeModalType === 'dnf' && aiStore.dnfResult}
          <div class="ai-results-container">
            <div class="ai-card">
              <div class="card-header">
                <Package size={16} class="text-warning" />
                <span>Conflict Analysis</span>
              </div>
              <p class="card-text">{aiStore.dnfResult.conflict_summary}</p>
            </div>

            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Remediation Command</span>
                </div>
                <Button variant="outline" size="sm" onclick={() => copyText(aiStore.dnfResult?.remediation_command || '')}>
                  {#if copied}<Check size={13} style="color:var(--color-success)" /> Copied{:else}<Copy size={13} /> Copy Command{/if}
                </Button>
              </div>
              <pre class="command-block"><code>{aiStore.dnfResult.remediation_command}</code></pre>
            </div>

            <div class="ai-card">
              <div class="card-header">
                <ShieldCheck size={16} class="text-accent" />
                <span>Additional Guidance</span>
              </div>
              <p class="card-text">{aiStore.dnfResult.explanation}</p>
            </div>
          </div>

        <!-- 4. NGINX Rule Generation Result -->
        {:else if aiStore.activeModalType === 'nginx' && aiStore.nginxResult}
          <div class="ai-results-container">
            <div class="ai-card">
              <div class="card-header">
                <Server size={16} class="text-accent" />
                <span>Configuration Overview</span>
              </div>
              <p class="card-text">{aiStore.nginxResult.explanation}</p>
            </div>

            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Generated NGINX Server Block</span>
                </div>
                <Button variant="outline" size="sm" onclick={() => copyText(aiStore.nginxResult?.generated_config || '', 'NGINX configuration copied')}>
                  {#if copied}<Check size={13} style="color:var(--color-success)" /> Copied{:else}<Copy size={13} /> Copy Config{/if}
                </Button>
              </div>
              <pre class="command-block" style="max-height: 240px; overflow-y: auto;"><code>{aiStore.nginxResult.generated_config}</code></pre>
            </div>
          </div>

        <!-- 5. Firewall Rule Generation Result -->
        {:else if aiStore.activeModalType === 'firewall' && aiStore.firewallResult}
          <div class="ai-results-container">
            <div class="ai-card">
              <div class="card-header">
                <Shield size={16} class="text-accent" />
                <span>Rule Explanation</span>
              </div>
              <p class="card-text">{aiStore.firewallResult.explanation}</p>
            </div>

            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Firewalld Command</span>
                </div>
                <Button variant="outline" size="sm" onclick={() => copyText(aiStore.firewallResult?.generated_command || '', 'Firewall command copied')}>
                  {#if copied}<Check size={13} style="color:var(--color-success)" /> Copied{:else}<Copy size={13} /> Copy Command{/if}
                </Button>
              </div>
              <pre class="command-block"><code>{aiStore.firewallResult.generated_command}</code></pre>
            </div>

            {#if aiStore.firewallResult.rich_rule}
              <div class="ai-card">
                <div class="card-header">
                  <Terminal size={16} class="text-warning" />
                  <span>Rich Rule String</span>
                </div>
                <pre class="command-block"><code>{aiStore.firewallResult.rich_rule}</code></pre>
              </div>
            {/if}
          </div>

        <!-- Error State -->
        {:else if aiStore.analysisError}
          <div class="ai-error-state">
            <AlertTriangle size={32} class="text-error" />
            <div class="error-title">AI Processing Failed</div>
            <div class="error-msg">{aiStore.analysisError}</div>
            <Button variant="outline" size="sm" onclick={handleRetry}>
              <RefreshCw size={13} /> Retry Operation
            </Button>
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="ai-modal-footer">
        <span class="footer-note">AI Engine Active ({aiStore.provider.toUpperCase()})</span>
        <Button variant="outline" size="sm" onclick={() => aiStore.closeModal()}>Close</Button>
      </div>

    </div>
  </div>
{/if}

<style>
  .ai-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 2100;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(5px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .ai-modal-content {
    width: 680px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 40px);
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :global(html.light-mode) .ai-modal-content {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.15);
  }

  .ai-modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .ai-modal-header {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .ai-header-title {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .ai-icon-glow {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: rgba(0, 218, 243, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .modal-subtitle {
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .model-tag {
    font-family: var(--font-mono);
    color: var(--color-accent);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .close-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.1);
  }

  .ai-modal-body {
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-height: 60vh;
  }

  .context-card {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 16px;
  }

  :global(html.light-mode) .context-card {
    background: #F1F5F9;
    border-color: #E2E8F0;
  }

  .context-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .context-title {
    margin: 0 0 4px 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .context-val {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .snippet-preview {
    margin: 6px 0 0 0;
    padding: 8px 10px;
    background: #040914;
    border-radius: 6px;
    color: #94a3b8;
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 80px;
    overflow-y: hidden;
  }

  .prompt-text {
    font-size: 13px;
    font-style: italic;
    color: var(--color-text-primary);
    margin-top: 4px;
  }

  .ai-loading-state {
    padding: 40px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    text-align: center;
  }

  .pulse-sphere {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: rgba(0, 218, 243, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(0, 218, 243, 0.2);
    box-shadow: 0 0 20px rgba(0, 218, 243, 0.15);
  }

  .loading-title {
    display: block;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .loading-desc {
    display: block;
    font-size: 12px;
    color: var(--color-text-muted);
    max-width: 400px;
    margin-top: 4px;
  }

  .ai-results-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .ai-card {
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 14px 16px;
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .ai-card {
    background: #FFFFFF;
    border-color: #E2E8F0;
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin-bottom: 8px;
  }

  .card-header.space-between {
    justify-content: space-between;
  }

  .flex-align {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-text {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--color-text-secondary);
  }

  .command-block {
    margin: 0;
    padding: 12px 14px;
    background: #040914;
    border: 1px solid rgba(0, 218, 243, 0.2);
    border-radius: 8px;
    color: #38bdf8;
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .ai-error-state {
    padding: 30px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
  }

  .error-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .error-msg {
    font-size: 12px;
    color: var(--color-text-muted);
    max-width: 480px;
  }

  .ai-modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .ai-modal-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  .footer-note {
    font-size: 11px;
    color: var(--color-text-muted);
  }
</style>
