<script lang="ts">
  import { dnfStore, type DnfPackageDiff } from '../stores/dnfStore.svelte.ts';
  import { 
    X, AlertTriangle, CheckCircle2, ArrowRight, Download, HardDrive, 
    RefreshCw, Package, Play, ShieldAlert, Terminal, Search, Filter, Layers
  } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';

  let searchQuery = $state('');
  let actionFilter = $state<'all' | 'Upgrade' | 'Install' | 'Remove' | 'Downgrade'>('all');
  let showRawOutput = $state(false);

  const filteredPackages = $derived.by(() => {
    if (!dnfStore.dryRunResult) return [];
    let list = dnfStore.dryRunResult.packages;

    if (actionFilter !== 'all') {
      list = list.filter(p => p.action === actionFilter);
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
          <div class="loading-state">
            <RefreshCw size={26} class="spinner text-accent" />
            <h4>Calculating Transaction Graph…</h4>
            <p>DNF is checking repository metadata, resolving dependencies, and testing transaction locks.</p>
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
          <!-- KPI Summary Cards -->
          <div class="kpi-grid">
            <div class="kpi-card" class:active-filter={actionFilter === 'Upgrade'} onclick={() => actionFilter = actionFilter === 'Upgrade' ? 'all' : 'Upgrade'}>
              <div class="kpi-label">Packages Upgrading</div>
              <div class="kpi-val text-accent">{dnfStore.dryRunResult.to_upgrade_count}</div>
            </div>

            <div class="kpi-card" class:active-filter={actionFilter === 'Install'} onclick={() => actionFilter = actionFilter === 'Install' ? 'all' : 'Install'}>
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
              <ShieldAlert size={18} class="text-error flex-shrink-0" />
              <div>
                <strong>Package Removal Warning:</strong>
                Dependency solver resolved conflicts that require removing 
                <span class="badge badge-error">{dnfStore.dryRunResult.to_remove_count} package(s)</span>.
                Inspect the removals in the table below before confirming.
              </div>
            </div>
          {/if}

          <!-- Filter & Search Bar -->
          <div class="table-toolbar">
            <div class="search-box">
              <Search size={14} class="search-icon" />
              <input 
                type="text" 
                placeholder="Filter packages by name, repo…" 
                bind:value={searchQuery}
                class="search-input"
              />
              {#if searchQuery}
                <button class="clear-btn" onclick={() => searchQuery = ''}>
                  <X size={12} />
                </button>
              {/if}
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

          <!-- Transaction Diff Table -->
          <div class="diff-table-container">
            {#if dnfStore.dryRunResult.packages.length === 0}
              <div class="empty-state">
                <CheckCircle2 size={32} class="text-success" />
                <h4 style="margin: 4px 0; color: var(--color-text-primary);">System is Fully Up to Date</h4>
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
    padding: 24px;
  }

  .backdrop-surface {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .dry-run-modal {
    position: relative;
    z-index: 10001;
    width: 100%;
    max-width: 860px;
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 12px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6), 0 0 30px rgba(0, 218, 243, 0.1);
    overflow: hidden;
  }

  :global(html.light-mode) .dry-run-modal {
    background: #FFFFFF;
    border-color: #CBD5E1;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.2);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    background: rgba(255, 255, 255, 0.02);
  }

  .header-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .header-icon-box {
    width: 34px;
    height: 34px;
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
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .loading-state, .error-state {
    padding: 40px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
  }

  .spinner {
    animation: spin 1.2s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .kpi-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
  }

  .kpi-card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    border-radius: 8px;
    padding: 10px 12px;
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
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    margin-bottom: 4px;
  }

  .kpi-val {
    font-size: 18px;
    font-weight: 700;
    font-family: var(--font-mono);
  }

  .danger-warning-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 12px;
    color: #FCA5A5;
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
  }

  .search-box {
    position: relative;
    flex: 1;
    max-width: 320px;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-text-muted);
  }

  .search-input {
    width: 100%;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 6px 28px 6px 30px;
    font-size: 11.5px;
    color: var(--color-text-primary);
  }

  .clear-btn {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .filter-pills {
    display: flex;
    gap: 6px;
  }

  .pill-btn {
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: 20px;
    padding: 4px 10px;
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pill-btn:hover, .pill-btn.active {
    border-color: var(--color-accent);
    color: var(--color-text-on-accent, #ffffff);
    background: var(--color-accent);
  }

  .diff-table-container {
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    border-radius: 8px;
    overflow-y: auto;
    max-height: 380px;
  }

  .diff-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }

  .diff-table th {
    background: rgba(255, 255, 255, 0.03);
    padding: 8px 12px;
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
    padding: 7px 12px;
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
    margin-top: 4px;
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
    max-height: 160px;
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

  .empty-filter {
    padding: 30px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--color-text-muted);
  }
</style>
