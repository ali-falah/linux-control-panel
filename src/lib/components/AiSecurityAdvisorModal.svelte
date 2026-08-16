<script lang="ts">
  import { Sparkles, X, Bot, ShieldAlert, RefreshCw, AlertTriangle, ShieldCheck, Lock, Info } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Button from './ui/Button.svelte';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import { getHardcodedFix } from '../stores/aiStore.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  // ── Derived state ──────────────────────────────────────────────────────────
  let showModal = $derived(aiStore.activeModalType === 'finding');
  let activeResponse = $derived(aiStore.findingResult);
  let selectedModel = $derived(
    aiStore.provider === 'ollama' ? aiStore.ollamaModel : aiStore.cloudModel
  );

  // Hardcoded fix for the current finding — NEVER from LLM
  let hardcodedFix = $derived(
    aiStore.activeFinding ? getHardcodedFix(aiStore.activeFinding.id) : null
  );

  // ── Fix application state ──────────────────────────────────────────────────
  let applyingFix = $state(false);

  async function handleApplyFix() {
    if (!hardcodedFix) return;

    const fix = hardcodedFix;
    uiStore.confirm(
      `Apply Verified Fix: ${fix.label}`,
      `This will apply the following change:\n\n${fix.current_label}  →  ${fix.proposed_label}\n\nTarget: ${fix.target}`,
      async () => {
        applyingFix = true;
        try {
          await invoke(fix.tauri_command, fix.tauri_args);
          uiStore.addToast(`Fix applied: ${fix.label}`, 'success');
          window.dispatchEvent(new CustomEvent('security-audit-run'));
          aiStore.closeModal();
        } catch (err: any) {
          uiStore.addToast(`Fix failed: ${err}`, 'error');
        } finally {
          applyingFix = false;
        }
      },
      false
    );
  }

  function handleRetry() {
    if (aiStore.activeFinding) {
      aiStore.explainFinding(aiStore.activeFinding);
    }
  }

  function getSeverityClass(sev: string) {
    const s = sev?.toUpperCase();
    if (s === 'CRITICAL' || s === 'HIGH') return 'badge-error';
    if (s === 'WARNING' || s === 'MEDIUM') return 'badge-warning';
    return 'badge-info';
  }
</script>

{#if showModal}
  <div
    class="ai-modal-backdrop"
    role="button"
    tabindex="0"
    onclick={() => aiStore.closeModal()}
    onkeydown={(e) => { if (e.key === 'Escape') aiStore.closeModal(); }}
  >
    <div
      class="ai-modal-content"
      role="dialog"
      aria-modal="true"
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
                Powered by Ollama Local AI (<code class="model-tag">{selectedModel}</code>)
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
              <span class="badge {getSeverityClass(aiStore.activeFinding.severity)}">
                {aiStore.activeFinding.severity}
              </span>
              <span class="badge badge-muted">{aiStore.activeFinding.category}</span>
            </div>
            <h4 class="context-title">{aiStore.activeFinding.title}</h4>
            <div class="context-val">Current State: <code>{aiStore.activeFinding.current_value || 'Not configured'}</code></div>
          </div>
        {/if}

        <!-- ── Hardcoded Verified Fix Panel ────────────────────────────────── -->
        {#if hardcodedFix}
          <div class="ai-card fix-card">
            <div class="card-header">
              <Lock size={15} style="color:var(--color-success)" />
              <span>Verified Fix</span>
              <span class="verified-badge">pre-validated · not AI-generated</span>
            </div>

            <div class="diff-target">
              <span class="diff-target-label">Target:</span>
              <code class="diff-target-value">{hardcodedFix.target}</code>
            </div>

            <!-- Current vs Proposed diff -->
            <div class="diff-block">
              <div class="diff-row diff-current">
                <span class="diff-sign">−</span>
                <code class="diff-text">{hardcodedFix.current_label}</code>
              </div>
              <div class="diff-row diff-proposed">
                <span class="diff-sign">+</span>
                <code class="diff-text">{hardcodedFix.proposed_label}</code>
              </div>
            </div>

            <div class="fix-actions">
              <Button
                variant="primary"
                size="sm"
                onclick={handleApplyFix}
                disabled={applyingFix}
                title="Apply the pre-validated fix using the application's built-in hardened command"
              >
                {#if applyingFix}
                  <RefreshCw size={13} class="spin" />
                  Applying…
                {:else}
                  <Lock size={13} />
                  Apply Verified Fix
                {/if}
              </Button>
            </div>
          </div>
        {:else if aiStore.activeFinding}
          <!-- No hardcoded fix for this finding — graceful degradation -->
          <div class="ai-card no-fix-card">
            <div class="card-header">
              <Info size={15} style="color:var(--color-info)" />
              <span>Manual Remediation Required</span>
            </div>
            <p class="card-text">
              This finding does not have a one-click fix. Use the <strong>Apply Fix</strong> button
              directly on the finding card, or follow the countermeasure guidance shown there.
            </p>
          </div>
        {/if}

        {#if aiStore.analyzing}
          <!-- Analyzing Loading View -->
          <div class="ai-loading-state">
            <div class="pulse-sphere">
              <Bot size={32} class="animate-pulse text-accent" />
            </div>
            <div class="loading-text">
              <span class="loading-title">Analyzing Risk &amp; Exploit Vector…</span>
              <span class="loading-desc">AI model is evaluating the finding and generating context-aware risk analysis.</span>
            </div>
          </div>

        {:else if activeResponse}
          <!-- AI Results — explanation only, no commands -->
          <div class="ai-results-container">

            <!-- Risk & Exploit Analysis Card -->
            <div class="ai-card risk-card">
              <div class="card-header">
                <ShieldAlert size={16} class="text-warning" />
                <span>Risk &amp; Exploit Potential</span>
                <span class="ai-tag">AI Analysis</span>
              </div>
              <p class="card-text">{activeResponse.risk_explanation}</p>
            </div>

            <!-- Safety Assessment Card -->
            <div class="ai-card safety-card">
              <div class="card-header">
                <ShieldCheck size={16} style="color:var(--color-success)" />
                <span>Safety &amp; Operational Impact</span>
                <span class="ai-tag">AI Analysis</span>
              </div>
              <p class="card-text">{activeResponse.safety_notes}</p>
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
        <span class="footer-note">100% Offline Local Processing — No Data Sent to Cloud · Commands are pre-validated, not AI-generated</span>
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
    gap: 14px;
    max-height: 65vh;
  }

  /* ── Finding context ─────────────────────────────────────────────────────── */
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

  /* ── Shared card ─────────────────────────────────────────────────────────── */
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
    margin-bottom: 10px;
  }

  .card-text {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--color-text-secondary);
  }

  /* ── Verified Fix Card ───────────────────────────────────────────────────── */
  .fix-card {
    border-color: rgba(34, 197, 94, 0.3);
    background: rgba(34, 197, 94, 0.04);
  }

  :global(html.light-mode) .fix-card {
    border-color: rgba(22, 163, 74, 0.3);
    background: rgba(22, 163, 74, 0.04);
  }

  .verified-badge {
    margin-left: auto;
    font-size: 10px;
    font-weight: 500;
    color: var(--color-success);
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.25);
    padding: 2px 7px;
    border-radius: 10px;
    letter-spacing: 0.2px;
  }

  .diff-target {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }

  .diff-target-label {
    font-size: 11px;
    color: var(--color-text-muted);
    font-weight: 500;
    flex-shrink: 0;
  }

  .diff-target-value {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    background: rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: 4px;
  }

  :global(html.light-mode) .diff-target-value {
    background: rgba(0, 0, 0, 0.06);
  }

  /* Diff block */
  .diff-block {
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.08);
    margin-bottom: 12px;
  }

  :global(html.light-mode) .diff-block {
    border-color: #E2E8F0;
  }

  .diff-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
  }

  .diff-current {
    background: rgba(239, 68, 68, 0.12);
    border-bottom: 1px solid rgba(239, 68, 68, 0.15);
  }

  :global(html.light-mode) .diff-current {
    background: rgba(220, 38, 38, 0.07);
  }

  .diff-proposed {
    background: rgba(34, 197, 94, 0.10);
  }

  :global(html.light-mode) .diff-proposed {
    background: rgba(22, 163, 74, 0.07);
  }

  .diff-sign {
    font-size: 15px;
    font-weight: 700;
    width: 14px;
    flex-shrink: 0;
    text-align: center;
    font-family: var(--font-mono);
  }

  .diff-current .diff-sign { color: var(--color-error); }
  .diff-proposed .diff-sign { color: var(--color-success); }

  .diff-text {
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .diff-current .diff-text  { color: rgba(239, 68, 68, 0.9); }
  .diff-proposed .diff-text { color: rgba(34, 197, 94, 0.95); }

  :global(html.light-mode) .diff-current .diff-text  { color: #dc2626; }
  :global(html.light-mode) .diff-proposed .diff-text { color: #16a34a; }

  .fix-actions {
    display: flex;
    justify-content: flex-end;
  }

  /* ── No-fix card ─────────────────────────────────────────────────────────── */
  .no-fix-card {
    border-color: rgba(100, 116, 139, 0.25);
  }

  /* ── Loading state ───────────────────────────────────────────────────────── */
  .ai-loading-state {
    padding: 32px 20px;
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

  /* ── AI results ──────────────────────────────────────────────────────────── */
  .ai-results-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .ai-tag {
    margin-left: auto;
    font-size: 10px;
    font-weight: 500;
    color: var(--color-accent);
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.18);
    padding: 2px 7px;
    border-radius: 10px;
    letter-spacing: 0.2px;
  }

  /* ── Error state ─────────────────────────────────────────────────────────── */
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

  /* ── Footer ──────────────────────────────────────────────────────────────── */
  .ai-modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
    gap: 12px;
  }

  :global(html.light-mode) .ai-modal-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  .footer-note {
    font-size: 10.5px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  /* ── Badges ──────────────────────────────────────────────────────────────── */
  .badge {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .badge-error   { background: rgba(239, 68, 68, 0.15);  color: var(--color-error);   border: 1px solid rgba(239, 68, 68, 0.25); }
  .badge-warning { background: rgba(245, 158, 11, 0.15); color: var(--color-warning); border: 1px solid rgba(245, 158, 11, 0.25); }
  .badge-info    { background: rgba(99, 179, 237, 0.15); color: var(--color-info);    border: 1px solid rgba(99, 179, 237, 0.25); }
  .badge-muted   { background: rgba(255, 255, 255, 0.07); color: var(--color-text-muted); border: 1px solid rgba(255, 255, 255, 0.1); }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
