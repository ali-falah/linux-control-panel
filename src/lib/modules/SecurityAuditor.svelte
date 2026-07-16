<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import Button from '../components/ui/Button.svelte';
  import { Shield, ShieldAlert, ShieldCheck, Info, CheckCircle2, AlertTriangle, XCircle, RefreshCw, Lock, Zap } from '@lucide/svelte';

  interface SecurityFinding {
    id: string;
    title: string;
    description: string;
    severity: 'Critical' | 'Warning' | 'Good' | 'Info';
    countermeasure: string;
    has_auto_fix: boolean;
    is_resolved: boolean;
  }

  interface SecurityReport {
    score: number;
    findings: SecurityFinding[];
  }

  let report = $state<SecurityReport | null>(null);
  let loading = $state(false);
  let fixingId = $state<string | null>(null);

  async function runAudit() {
    loading = true;
    try {
      report = await invoke<SecurityReport>('security_run_audit');
      statusStore.setLastCommand('Security audit executed', 0, true);
    } catch (e) {
      uiStore.addToast(`Audit failed: ${e}`, 'error');
      statusStore.setLastCommand('security_run_audit', 1, false);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    runAudit();
  });

  async function handleFix(finding: SecurityFinding) {
    if (finding.id === 'ssh_root') {
      const isCurrentlyResolved = finding.is_resolved;
      const actionText = isCurrentlyResolved ? 'Enable Root SSH (Insecure)' : 'Secure Root SSH';
      const riskText = isCurrentlyResolved 
        ? 'Warning: Enabling Root SSH allows remote attackers to brute force the root account directly. This is highly discouraged.'
        : 'This will set PermitRootLogin to prohibit-password. You will only be able to SSH as root using keys.';
      
      uiStore.confirm(
        actionText,
        riskText,
        async () => {
          fixingId = finding.id;
          try {
            await invoke('security_fix_root_ssh', { enable: !isCurrentlyResolved });
            uiStore.addToast(`Root SSH configured successfully`, 'success');
            await runAudit();
          } catch (e) {
            uiStore.addToast(`Failed to configure SSH: ${e}`, 'error');
          } finally {
            fixingId = null;
          }
        },
        !isCurrentlyResolved
      );
    } 
    else if (finding.id === 'selinux') {
      const isCurrentlyResolved = finding.is_resolved;
      const actionText = isCurrentlyResolved ? 'Disable SELinux (Insecure)' : 'Enable SELinux';
      const riskText = isCurrentlyResolved
        ? 'Warning: Disabling SELinux removes mandatory access control. The system will be rebooted.'
        : 'Warning: Enabling SELinux will force a filesystem relabel on boot. The system MUST reboot immediately to apply this.';
      
      uiStore.confirm(
        actionText,
        riskText,
        async () => {
          fixingId = finding.id;
          try {
            const msg = await invoke<string>('security_fix_selinux', { enable: !isCurrentlyResolved });
            uiStore.addToast(msg, 'success');
            setTimeout(() => {
              invoke('shell_run_command', { command: 'reboot' }).catch(() => {});
            }, 3000);
          } catch (e) {
            uiStore.addToast(`Failed to configure SELinux: ${e}`, 'error');
            fixingId = null;
          }
        },
        true
      );
    }
    else if (finding.id === 'pass_policy') {
      fixingId = finding.id;
      try {
        await invoke('security_fix_password_policy');
        uiStore.addToast('Password policy secured', 'success');
        await runAudit();
      } catch (e) {
        uiStore.addToast(`Failed to fix password policy: ${e}`, 'error');
      } finally {
        fixingId = null;
      }
    }
    else if (finding.id === 'firewall') {
      fixingId = finding.id;
      try {
        await invoke('security_fix_firewall');
        uiStore.addToast('Firewall enabled with secure defaults', 'success');
        await runAudit();
      } catch (e) {
        uiStore.addToast(`Failed to enable firewall: ${e}`, 'error');
      } finally {
        fixingId = null;
      }
    }
  }

  function getSeverityColor(sev: string) {
    if (sev === 'Critical') return 'var(--color-error)';
    if (sev === 'Warning') return 'var(--color-warning)';
    if (sev === 'Good') return 'var(--color-success)';
    return 'var(--color-info)';
  }

  function getScoreColor(score: number) {
    if (score >= 90) return 'var(--color-success)';
    if (score >= 70) return 'var(--color-warning)';
    return 'var(--color-error)';
  }
</script>

<div class="module-container">
  <div class="header">
    <div class="header-title">
      <Shield size={24} color="var(--color-accent)" />
      <h2>Security Auditor</h2>
    </div>
    <div class="header-actions">
      <Button variant="outline" onclick={runAudit} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'spin' : ''} />
        Rescan System
      </Button>
    </div>
  </div>

  <div class="content-scroll">
    {#if loading && !report}
      <div class="center-state">
        <div class="spinner"></div>
        <div style="margin-top:12px; color:var(--color-text-muted);">Analyzing system configuration...</div>
      </div>
    {:else if report}
      <div class="dashboard-grid">
        
        <!-- Score Card -->
        <div class="score-card glass-panel">
          <h3>System Security Score</h3>
          <div class="score-gauge" style="--score-color: {getScoreColor(report.score)}">
            <svg viewBox="0 0 100 100" class="gauge-svg">
              <circle class="gauge-bg" cx="50" cy="50" r="40"></circle>
              <circle class="gauge-fill" cx="50" cy="50" r="40" style="stroke-dasharray: {report.score * 2.51} 251;"></circle>
            </svg>
            <div class="score-text">{report.score}<span class="pct">%</span></div>
          </div>
          <div class="score-desc">
            {#if report.score >= 90} Excellent configuration
            {:else if report.score >= 70} Fair configuration, needs attention
            {:else} Critical risks detected, action required!
            {/if}
          </div>
        </div>

        <!-- Findings List -->
        <div class="findings-container">
          <div class="findings-header">
            <h3>Audit Findings</h3>
            <span class="finding-badge">
              {report.findings.filter(f => !f.is_resolved).length} Issues Found
            </span>
          </div>

          <div class="findings-list">
            {#each [...report.findings].sort((a,b) => (a.is_resolved === b.is_resolved ? 0 : a.is_resolved ? 1 : -1)) as finding}
              <div class="finding-card" class:resolved={finding.is_resolved} style="--sev-color: {getSeverityColor(finding.severity)}">
                <div class="finding-icon">
                  {#if finding.severity === 'Critical'} <XCircle size={20} />
                  {:else if finding.severity === 'Warning'} <AlertTriangle size={20} />
                  {:else if finding.severity === 'Good'} <CheckCircle2 size={20} />
                  {:else} <Info size={20} />
                  {/if}
                </div>
                <div class="finding-content">
                  <div class="finding-title">
                    {finding.title}
                    <span class="sev-badge" style="background: {getSeverityColor(finding.severity)}20; color: {getSeverityColor(finding.severity)}">
                      {finding.severity}
                    </span>
                  </div>
                  <div class="finding-desc">{finding.description}</div>
                  
                  <div class="finding-countermeasure">
                    <Zap size={14} />
                    <span><strong>Countermeasure:</strong> {finding.countermeasure}</span>
                  </div>
                </div>

                <div class="finding-action">
                  {#if finding.has_auto_fix}
                    {#if finding.id === 'ssh_root' || finding.id === 'selinux'}
                      <Button 
                        variant={finding.is_resolved ? "outline" : "primary"}
                        onclick={() => handleFix(finding)}
                        disabled={fixingId === finding.id}
                      >
                        {#if fixingId === finding.id}<div class="spinner-sm"></div>{/if}
                        {finding.is_resolved ? 'Disable / Revert' : 'Apply Secure Config'}
                      </Button>
                    {:else if !finding.is_resolved}
                      <Button 
                        variant="primary"
                        onclick={() => handleFix(finding)}
                        disabled={fixingId === finding.id}
                      >
                        {#if fixingId === finding.id}<div class="spinner-sm"></div>
                        {:else}<Lock size={14} />{/if}
                        Auto Fix
                      </Button>
                    {/if}
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .module-container {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 24px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    background: rgba(2, 6, 23, 0.6);
    backdrop-filter: blur(10px);
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .header-title h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .content-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
  }

  .dashboard-grid {
    display: grid;
    grid-template-columns: 300px 1fr;
    gap: 24px;
    align-items: start;
  }

  .glass-panel {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .score-card h3 {
    margin: 0 0 20px 0;
    font-size: 16px;
    font-weight: 500;
  }

  .score-gauge {
    position: relative;
    width: 160px;
    height: 160px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .gauge-svg {
    transform: rotate(-90deg);
    width: 100%;
    height: 100%;
  }

  .gauge-bg {
    fill: none;
    stroke: rgba(255, 255, 255, 0.1);
    stroke-width: 8;
  }

  .gauge-fill {
    fill: none;
    stroke: var(--score-color);
    stroke-width: 8;
    stroke-linecap: round;
    transition: stroke-dasharray 1s ease-out;
  }

  .score-text {
    position: absolute;
    font-size: 42px;
    font-weight: 700;
    color: var(--score-color);
  }
  .score-text .pct {
    font-size: 18px;
    opacity: 0.7;
  }

  .score-desc {
    margin-top: 16px;
    text-align: center;
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .findings-container {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .findings-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .findings-header h3 {
    margin: 0;
    font-size: 18px;
  }

  .finding-badge {
    background: rgba(255, 255, 255, 0.1);
    padding: 4px 12px;
    border-radius: 20px;
    font-size: 12px;
    font-weight: 600;
  }

  .findings-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .finding-card {
    display: flex;
    gap: 16px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-left: 4px solid var(--sev-color);
    border-radius: 8px;
    padding: 16px;
    transition: all 0.2s ease;
  }
  .finding-card:hover {
    background: rgba(255, 255, 255, 0.02);
  }
  .finding-card.resolved {
    opacity: 0.7;
    background: rgba(255, 255, 255, 0.01);
  }

  .finding-icon {
    color: var(--sev-color);
    flex-shrink: 0;
    padding-top: 2px;
  }

  .finding-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .finding-title {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .sev-badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    text-transform: uppercase;
    font-weight: 700;
  }

  .finding-desc {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  .finding-countermeasure {
    margin-top: 8px;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text-primary);
    background: rgba(0, 218, 243, 0.1);
    border: 1px solid rgba(0, 218, 243, 0.2);
    padding: 10px 12px;
    border-radius: 6px;
  }
  .finding-countermeasure :global(svg) {
    color: var(--color-accent);
    flex-shrink: 0;
    margin-top: 2px;
  }

  .finding-action {
    display: flex;
    align-items: center;
  }

  .spin {
    animation: spin 1s linear infinite;
  }
  @keyframes spin { 100% { transform: rotate(360deg); } }

  .spinner-sm {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
</style>
