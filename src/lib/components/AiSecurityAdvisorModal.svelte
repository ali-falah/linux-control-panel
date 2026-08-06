<script lang="ts">
  import { Sparkles, X, Bot, ShieldAlert, Terminal, Check, Copy, RefreshCw, AlertTriangle, ShieldCheck } from '@lucide/svelte';
  import Button from './ui/Button.svelte';
  import Badge from './ui/Badge.svelte';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  let copied = $state(false);

  function copyCommand(cmd: string) {
    navigator.clipboard.writeText(cmd);
    copied = true;
    uiStore.addToast('Remediation command copied to clipboard', 'success');
    setTimeout(() => { copied = false; }, 2000);
  }

  function handleRetry() {
    if (aiStore.activeFinding) {
      aiStore.explainFinding(aiStore.activeFinding);
    }
  }
</script>

{#if aiStore.showAdvisorModal}
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
            <h3 class="modal-title">AI Security Advisor</h3>
            <span class="modal-subtitle">
              {#if aiStore.ollamaConnected}
                Powered by Ollama Local AI (<code class="model-tag">{aiStore.selectedModel}</code>)
              {:else}
                Ollama Local Server Disconnected
              {/if}
            </span>
          </div>
        </div>
        <button class="close-btn" onclick={() => aiStore.closeModal()}>
          <X size={16} />
        </button>
      </div>

      <!-- Content Body -->
      <div class="ai-modal-body">
        {#if aiStore.activeFinding}
          <!-- Finding Context Banner -->
          <div class="finding-context-card">
            <div class="context-meta">
              <span class="badge {aiStore.activeFinding.severity === 'HIGH' ? 'badge-error' : aiStore.activeFinding.severity === 'MEDIUM' ? 'badge-warning' : 'badge-info'}">
                {aiStore.activeFinding.severity}
              </span>
              <span class="badge badge-muted">{aiStore.activeFinding.category}</span>
            </div>
            <h4 class="context-title">{aiStore.activeFinding.title}</h4>
            <div class="context-val">Current Value: <code>{aiStore.activeFinding.current_value || 'Not configured'}</code></div>
          </div>
        {/if}

        {#if aiStore.analyzing}
          <!-- Analyzing Loading View -->
          <div class="ai-loading-state">
            <div class="pulse-sphere">
              <Bot size={32} class="animate-pulse text-accent" />
            </div>
            <div class="loading-text">
              <span class="loading-title">Analyzing Risk & Exploit Vector…</span>
              <span class="loading-desc">Qwen 2.5 1.5B is evaluating the finding and generating safe remediation commands.</span>
            </div>
          </div>

        {:else if aiStore.activeResponse}
          <!-- Results View -->
          <div class="ai-results-container">
            
            <!-- Risk & Exploit Analysis Card -->
            <div class="ai-card risk-card">
              <div class="card-header">
                <ShieldAlert size={16} class="text-warning" />
                <span>Risk &amp; Exploit Potential</span>
              </div>
              <p class="card-text">{aiStore.activeResponse.risk_explanation}</p>
            </div>

            <!-- Suggested Remediation Command Card -->
            <div class="ai-card command-card">
              <div class="card-header space-between">
                <div class="flex-align">
                  <Terminal size={16} class="text-accent" />
                  <span>Suggested Remediation Command</span>
                </div>
                <Button variant="outline" size="sm" onclick={() => copyCommand(aiStore.activeResponse?.remediation_command || '')}>
                  {#if copied}
                    <Check size={13} style="color:var(--color-success)" /> Copied
                  {:else}
                    <Copy size={13} /> Copy Command
                  {/if}
                </Button>
              </div>
              <pre class="command-block"><code>{aiStore.activeResponse.remediation_command}</code></pre>
            </div>

            <!-- Safety Assessment Card -->
            <div class="ai-card safety-card">
              <div class="card-header">
                <ShieldCheck size={16} style="color:var(--color-success)" />
                <span>Safety &amp; Operational Impact</span>
              </div>
              <p class="card-text">{aiStore.activeResponse.safety_notes}</p>
            </div>

          </div>

        {:else if aiStore.analysisError}
          <!-- Error View -->
          <div class="ai-error-state">
            <AlertTriangle size={32} class="text-error" />
            <div class="error-title">AI Analysis Failed</div>
            <div class="error-msg">{aiStore.analysisError}</div>
            <Button variant="outline" size="sm" onclick={handleRetry}>
              <RefreshCw size={13} /> Retry Analysis
            </Button>
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="ai-modal-footer">
        <span class="footer-note">100% Offline Local Processing — No Data Sent to Cloud</span>
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

  .finding-context-card {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 16px;
  }

  :global(html.light-mode) .finding-context-card {
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
