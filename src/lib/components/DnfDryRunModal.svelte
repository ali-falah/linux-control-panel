<script lang="ts">
  import { dnfStore, type DnfPackageDiff } from '../stores/dnfStore.svelte.ts';
  import SearchBar from './ui/SearchBar.svelte';
  import { 
    X, AlertTriangle, CheckCircle2, ArrowRight, Download, HardDrive, 
    RefreshCw, Package, Play, ShieldAlert, Terminal, Layers
  } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';

  let searchQuery = $state('');
  let actionFilter = $state<'all' | 'Upgrade' | 'Install' | 'Remove' | 'Downgrade'>('all');
  let showRawOutput = $state(false);

  const filteredPackages = $derived.by(() => {
    if (!dnfStore.dryRunResult) return [];
    let list = dnfStore.dryRunResult.packages;

    if (actionFilter !== 'all') {
      if (actionFilter === 'Upgrade') {
        list = list.filter(p => p.action === 'Upgrade' || p.action === 'Obsolete');
      } else {
        list = list.filter(p => p.action === actionFilter);
      }
    }

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase().trim();
      list = list.filter(p => 
        p.name.toLowerCase().includes(q) || 
        p.repo.toLowerCase().includes(q) ||
        p.new_version.toLowerCase().includes(q)
      );
    }

    return list;
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && dnfStore.showDryRunModal) {
      dnfStore.closeDryRunModal();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if dnfStore.showDryRunModal}
  <div class="dry-run-backdrop" transition:fade={{ duration: 150 }}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="backdrop-surface" onclick={() => dnfStore.closeDryRunModal()}></div>

    <div class="dry-run-modal" transition:fly={{ y: 25, duration: 250 }}>
      <!-- Modal Header -->
      <div class="modal-header">
        <div class="header-title-group">
          <div class="header-icon-box">
            <Layers size={18} class="text-accent" />
          </div>
          <div>
            <h3 class="modal-title">DNF Transaction Diff Preview</h3>
            <p class="modal-subtitle">
              Dry-run analysis completed via <code>dnf upgrade --assumeno</code> — zero changes made to disk
            </p>
          </div>
        </div>

        <button 
          class="btn-close" 
          onclick={() => dnfStore.closeDryRunModal()} 
          title="Close preview without upgrading"
        >
          <X size={16} />
        </button>
      </div>

      <!-- Modal Body -->
      <div class="modal-body">
        {#if dnfStore.isDryRunning}
          <!-- Premium Cybernetic Loading Experience -->
          <div class="loading-state">
            <div class="loader-visual-container">
              <div class="loader-radar-pulse"></div>
              <div class="loader-ring-outer"></div>
              <div class="loader-ring-inner"></div>
              <div class="loader-core-icon">
                <Layers size={24} class="text-accent" />
              </div>
            </div>

            <h4 class="shimmer-title">Calculating Transaction Graph…</h4>
            <p class="loading-subtext">
              DNF is checking repository metadata, resolving dependency conflict trees, and testing transaction locks.
            </p>

            <div class="loading-progress-chips">
              <span class="step-chip active"><span class="pulse-dot"></span> Metadata Check</span>
              <span class="step-chip active"><span class="pulse-dot"></span> Dependency Solver</span>
              <span class="step-chip"><span class="pulse-dot"></span> Diff Assembly</span>
            </div>
          </div>

        {:else if dnfStore.dryRunError}
          <div class="error-state">
            <AlertTriangle size={32} class="text-error" />
            <h4>Dry-Run Analysis Failed</h4>
            <p class="error-msg">{dnfStore.dryRunError}</p>
            <button class="btn btn-secondary btn-sm" onclick={() => dnfStore.closeDryRunModal()}>
              Dismiss
            </button>
          </div>

        {:else if dnfStore.dryRunResult}
          <!-- Compact KPI Summary Cards -->
          <div class="kpi-grid">
            <div 
              class="kpi-card" 
              class:active-filter={actionFilter === 'Upgrade'} 
              onclick={() => actionFilter = actionFilter === 'Upgrade' ? 'all' : 'Upgrade'}
            >
              <div class="kpi-label">Packages Upgrading</div>
              <div class="kpi-val text-accent">{dnfStore.dryRunResult.to_upgrade_count}</div>
            </div>

            <div 
              class="kpi-card" 
              class:active-filter={actionFilter === 'Install'} 
              onclick={() => actionFilter = actionFilter === 'Install' ? 'all' : 'Install'}
            >
              <div class="kpi-label">New Dependencies</div>
              <div class="kpi-val text-info">{dnfStore.dryRunResult.to_install_count}</div>
            </div>

            <div 
              class="kpi-card" 
              class:danger-card={dnfStore.dryRunResult.to_remove_count > 0}
              class:active-filter={actionFilter === 'Remove'}
              onclick={() => actionFilter = actionFilter === 'Remove' ? 'all' : 'Remove'}
            >
              <div class="kpi-label">Packages Removing</div>
              <div class="kpi-val" class:text-error={dnfStore.dryRunResult.to_remove_count > 0}>
                {dnfStore.dryRunResult.to_remove_count}
              </div>
            </div>

            <div class="kpi-card">
              <div class="kpi-label">Total Download Size</div>
              <div class="kpi-val">{dnfStore.dryRunResult.total_download_size}</div>
            </div>
          </div>

          <!-- Danger Callout if removals detected -->
          {#if dnfStore.dryRunResult.to_remove_count > 0}
            <div class="danger-warning-banner">
              <ShieldAlert size={16} class="text-error flex-shrink-0" />
              <div>
                <strong>Package Removal Warning:</strong>
                Dependency solver resolved conflicts requiring removal of 
                <span class="badge badge-error">{dnfStore.dryRunResult.to_remove_count} package(s)</span>.
                Inspect the removals in the table below before confirming.
              </div>
            </div>
          {/if}

          <!-- Filter & Search Bar with Standard SearchBar Component -->
          <div class="table-toolbar">
            <div class="search-wrapper">
              <SearchBar 
                bind:value={searchQuery} 
                placeholder="Filter packages by name, repo…" 
                style="margin: 0; width: 400px;" 
              />
            </div>

            <div class="filter-pills">
              <button 
                class="pill-btn" 
                class:active={actionFilter === 'all'} 
                onclick={() => actionFilter = 'all'}
              >
                All ({dnfStore.dryRunResult.packages.length})
              </button>
              {#if dnfStore.dryRunResult.to_upgrade_count > 0}
                <button 
                  class="pill-btn" 
                  class:active={actionFilter === 'Upgrade'} 
                  onclick={() => actionFilter = 'Upgrade'}
                >
                  Upgrade ({dnfStore.dryRunResult.to_upgrade_count})
                </button>
              {/if}
              {#if dnfStore.dryRunResult.to_install_count > 0}
                <button 
                  class="pill-btn" 
                  class:active={actionFilter === 'Install'} 
                  onclick={() => actionFilter = 'Install'}
                >
                  Install ({dnfStore.dryRunResult.to_install_count})
                </button>
              {/if}
              {#if dnfStore.dryRunResult.to_remove_count > 0}
                <button 
                  class="pill-btn text-error" 
                  class:active={actionFilter === 'Remove'} 
                  onclick={() => actionFilter = 'Remove'}
                >
                  Remove ({dnfStore.dryRunResult.to_remove_count})
                </button>
              {/if}
            </div>
          </div>

          <!-- Transaction Diff Table (Expanded & Flexible) -->
          <div class="diff-table-container">
            {#if dnfStore.dryRunResult.packages.length === 0}
              <div class="empty-state">
                <CheckCircle2 size={32} class="text-success" />
                <h4 style="margin: 6px 0 2px; color: var(--color-text-primary); font-size: 14px;">System is Fully Up to Date</h4>
                <p style="margin: 0; font-size: 12px; color: var(--color-text-muted);">
                  DNF verified that all packages are at their latest version. Zero modifications required.
                </p>
              </div>
            {:else if filteredPackages.length === 0}
              <div class="empty-filter">
                <Package size={24} class="text-muted" />
                <p>No package alterations match the current filter.</p>
              </div>
            {:else}
              <table class="diff-table">
                <thead>
                  <tr>
                    <th>Package &amp; Arch</th>
                    <th>Action</th>
                    <th>Version Diff</th>
                    <th>Repository</th>
                    <th style="text-align: right;">Size</th>
                  </tr>
                </thead>
                <tbody>
                  {#each filteredPackages as pkg}
                    <tr class="diff-row" class:is-removal={pkg.action === 'Remove'}>
                      <td>
                        <div class="pkg-name-cell">
                          <strong class="pkg-name">{pkg.name}</strong>
                          <span class="pkg-arch">{pkg.arch}</span>
                        </div>
                      </td>
                      <td>
                        <span class="action-tag {pkg.action.toLowerCase()}">
                          {pkg.action}
                        </span>
                      </td>
                      <td>
                        <div class="version-diff-box">
                          {#if pkg.old_version}
                            <span class="old-ver" title="Current installed version">{pkg.old_version}</span>
                            <ArrowRight size={11} class="ver-arrow" />
                          {/if}
                          <span class="new-ver" title="Target upgraded version">{pkg.new_version}</span>
                        </div>
                      </td>
                      <td>
                        <span class="repo-badge">{pkg.repo}</span>
                      </td>
                      <td style="text-align: right;">
                        <span class="size-text">{pkg.size}</span>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>

          <!-- Raw CLI Output Dropdown Toggle -->
          <div class="raw-toggle-wrap">
            <button 
              type="button" 
              class="raw-toggle-btn" 
              onclick={() => showRawOutput = !showRawOutput}
            >
              <Terminal size={12} />
              <span>{showRawOutput ? 'Hide raw dnf stdout' : 'View raw transaction stdout'}</span>
            </button>
            {#if showRawOutput}
              <pre class="raw-terminal">{dnfStore.dryRunResult.raw_output}</pre>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        <div class="footer-left">
          {#if dnfStore.dryRunResult?.disk_space_change}
            <div class="disk-change-indicator">
              <HardDrive size={13} class="text-accent" />
              <span>{dnfStore.dryRunResult.disk_space_change}</span>
            </div>
          {/if}
        </div>

        <div class="footer-actions">
          <button 
            type="button" 
            class="btn btn-secondary btn-sm" 
            onclick={() => dnfStore.closeDryRunModal()}
          >
            Cancel &amp; Keep Untouched
          </button>

          <button 
            type="button" 
            class="btn btn-primary btn-sm" 
            disabled={dnfStore.isDryRunning || !!dnfStore.dryRunError || (dnfStore.dryRunResult?.packages.length ?? 0) === 0}
            onclick={() => dnfStore.confirmAndExecuteUpgrade()}
          >
            <Play size={13} />
            <span>Apply Upgrade ({dnfStore.dryRunResult?.packages.length || 0} Packages)</span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .dry-run-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .backdrop-surface {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  /* Spacious Sizing: width (1120px), height (90vh / max 94vh) */
  .dry-run-modal {
    position: relative;
    z-index: 10001;
    width: 1120px;
    max-width: calc(100vw - 36px);
    height: 90vh;
    max-height: 94vh;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 16px;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.65), 0 0 35px var(--color-accent-glow, rgba(0, 218, 243, 0.12));
    overflow: hidden;
  }

  :global(html.light-mode) .dry-run-modal {
    background: #FFFFFF;
    border-color: #CBD5E1;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.18);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 20px;
    border-bottom: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    background: rgba(255, 255, 255, 0.02);
    flex-shrink: 0;
  }

  .header-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .header-icon-box {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .modal-subtitle {
    margin: 2px 0 0;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .btn-close {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    transition: all 0.15s ease;
  }

  .btn-close:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .modal-body {
    flex: 1;
    overflow-y: auto;
    padding: 14px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }

  /* ── Cybernetic Futuristic Loader Animation ────────────────────────── */
  .loading-state, .error-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 40px 20px;
  }

  .loader-visual-container {
    position: relative;
    width: 76px;
    height: 76px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 12px;
  }

  .loader-radar-pulse {
    position: absolute;
    inset: -8px;
    border-radius: 50%;
    background: radial-gradient(circle, var(--color-accent-glow, rgba(16, 185, 129, 0.28)) 0%, transparent 70%);
    animation: radarPulse 2.2s ease-out infinite;
  }

  .loader-ring-outer {
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    border: 2px dashed var(--color-accent);
    opacity: 0.7;
    animation: spinClockwise 6s linear infinite;
  }

  .loader-ring-inner {
    position: absolute;
    inset: 12px;
    border-radius: 50%;
    border: 2.5px solid transparent;
    border-top-color: var(--color-accent);
    border-bottom-color: var(--color-accent-bright, #34d399);
    animation: spinCounter 1.6s cubic-bezier(0.68, -0.55, 0.265, 1.55) infinite;
  }

  .loader-core-icon {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    filter: drop-shadow(0 0 8px var(--color-accent));
    animation: iconBreathe 2.2s ease-in-out infinite;
  }

  @keyframes radarPulse {
    0% { transform: scale(0.85); opacity: 0.8; }
    50% { transform: scale(1.3); opacity: 0.25; }
    100% { transform: scale(0.85); opacity: 0.8; }
  }

  @keyframes spinClockwise {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  @keyframes spinCounter {
    from { transform: rotate(360deg); }
    to { transform: rotate(0deg); }
  }

  @keyframes iconBreathe {
    0%, 100% { transform: scale(1); opacity: 0.9; }
    50% { transform: scale(1.15); opacity: 1; }
  }

  .shimmer-title {
    font-size: 15px;
    font-weight: 700;
    margin: 0;
    background: linear-gradient(90deg, var(--color-text-primary) 0%, var(--color-accent) 50%, var(--color-text-primary) 100%);
    background-size: 200% auto;
    color: transparent;
    -webkit-background-clip: text;
    background-clip: text;
    animation: textShimmer 2.5s linear infinite;
  }

  @keyframes textShimmer {
    to { background-position: 200% center; }
  }

  .loading-subtext {
    font-size: 12px;
    color: var(--color-text-muted);
    max-width: 460px;
    margin: 6px 0 0;
    line-height: 1.4;
  }

  .loading-progress-chips {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }

  .step-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 10.5px;
    font-weight: 500;
    padding: 3px 10px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
  }

  .step-chip.active {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
    background: var(--color-accent-muted, rgba(16, 185, 129, 0.12));
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 6px var(--color-accent);
    animation: pulseDot 1.4s ease-in-out infinite;
  }

  @keyframes pulseDot {
    0%, 100% { opacity: 0.4; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1.2); }
  }

  /* ── Compact Single-Row KPI Cards ──────────────────────────────────── */
  .kpi-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    flex-shrink: 0;
  }

  .kpi-card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    border-radius: 6px;
    padding: 6px 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .kpi-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .kpi-card:hover, .kpi-card.active-filter {
    border-color: var(--color-accent);
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.08));
  }

  .kpi-card.danger-card {
    border-color: rgba(239, 68, 68, 0.4);
    background: rgba(239, 68, 68, 0.06);
  }

  .kpi-label {
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    margin-bottom: 2px;
    line-height: 1.1;
  }

  .kpi-val {
    font-size: 15px;
    font-weight: 700;
    font-family: var(--font-mono);
    line-height: 1.2;
  }

  .danger-warning-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 11.5px;
    color: #FCA5A5;
    flex-shrink: 0;
  }

  :global(html.light-mode) .danger-warning-banner {
    background: #FEF2F2;
    border-color: #FCA5A5;
    color: #991B1B;
  }

  .table-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }

  .search-wrapper {
    flex: 1;
    max-width: 340px;
  }

  .filter-pills {
    display: flex;
    gap: 6px;
  }

  .pill-btn {
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: 20px;
    padding: 3px 9px;
    font-size: 10.5px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pill-btn:hover, .pill-btn.active {
    border-color: var(--color-accent);
    color: var(--color-text-on-accent, #ffffff);
    background: var(--color-accent);
  }

  /* ── Diff Table Container (Expanded) ───────────────────────────────── */
  .diff-table-container {
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    border-radius: 8px;
    overflow-y: auto;
    flex: 1;
    min-height: 200px;
  }

  .diff-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }

  .diff-table th {
    background: rgba(255, 255, 255, 0.03);
    padding: 9px 14px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-align: left;
    border-bottom: 1px solid var(--color-border);
    position: sticky;
    top: 0;
    z-index: 2;
  }

  :global(html.light-mode) .diff-table th {
    background: #F1F5F9;
  }

  .diff-table td {
    padding: 8px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  :global(html.light-mode) .diff-table td {
    border-bottom-color: #F1F5F9;
  }

  .diff-row:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .diff-row.is-removal {
    background: rgba(239, 68, 68, 0.05);
  }

  .pkg-name-cell {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .pkg-name {
    color: var(--color-text-primary);
  }

  .pkg-arch {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .action-tag {
    display: inline-block;
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .action-tag.upgrade {
    background: rgba(16, 185, 129, 0.15);
    color: #10B981;
  }

  .action-tag.install {
    background: rgba(59, 130, 246, 0.15);
    color: #3B82F6;
  }

  .action-tag.remove {
    background: rgba(239, 68, 68, 0.2);
    color: #EF4444;
  }

  .action-tag.downgrade {
    background: rgba(245, 158, 11, 0.2);
    color: #F59E0B;
  }

  .version-diff-box {
    display: flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .old-ver {
    color: var(--color-text-muted);
    text-decoration: line-through;
    opacity: 0.75;
  }

  .ver-arrow {
    color: var(--color-accent);
  }

  .new-ver {
    color: var(--color-text-primary);
    font-weight: 600;
  }

  .repo-badge {
    font-size: 10px;
    background: rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--color-text-secondary);
  }

  .size-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .raw-toggle-wrap {
    margin-top: 2px;
    flex-shrink: 0;
  }

  .raw-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 11px;
    cursor: pointer;
    padding: 2px 0;
  }

  .raw-toggle-btn:hover {
    color: var(--color-accent);
  }

  .raw-terminal {
    margin-top: 6px;
    background: #000000;
    color: #A3E635;
    padding: 10px;
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    max-height: 150px;
    overflow-y: auto;
    white-space: pre-wrap;
    border: 1px solid rgba(255, 255, 255, 0.1);
  }

  .modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px;
    background: rgba(255, 255, 255, 0.02);
    border-top: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    flex-shrink: 0;
  }

  .disk-change-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .footer-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .empty-state, .empty-filter {
    padding: 40px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    text-align: center;
  }
</style>
