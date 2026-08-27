<script lang="ts">
  import { portal } from '../actions/portal.ts';
  import { 
    FileText, CheckCircle2, AlertTriangle, X, Save, 
    Columns, Rows, ShieldAlert, ArrowRight, RefreshCw, Eye
  } from '@lucide/svelte';
  import Button from './ui/Button.svelte';

  interface Props {
    show?: boolean;
    filePath: string;
    oldContent: string;
    newContent: string;
    title?: string;
    warningMessage?: string;
    validationStatus?: 'idle' | 'validating' | 'valid' | 'invalid';
    validationMessage?: string;
    isSaving?: boolean;
    onconfirm: () => void | Promise<void>;
    oncancel: () => void;
  }

  let {
    show = $bindable(false),
    filePath = '',
    oldContent = '',
    newContent = '',
    title = 'Review Configuration Changes',
    warningMessage = '',
    validationStatus = 'idle',
    validationMessage = '',
    isSaving = false,
    onconfirm,
    oncancel
  }: Props = $props();

  let viewMode = $state<'unified' | 'split'>('unified');

  interface DiffLine {
    type: 'add' | 'del' | 'eq';
    oldLineNo?: number;
    newLineNo?: number;
    text: string;
  }

  interface SplitRow {
    oldLine?: { lineNo: number; text: string; type: 'del' | 'eq' };
    newLine?: { lineNo: number; text: string; type: 'add' | 'eq' };
  }

  // Compute Line Diff using LCS algorithm
  function computeDiff(oldText: string, newText: string): { unified: DiffLine[]; split: SplitRow[]; additions: number; deletions: number; unchanged: number } {
    const oldLines = oldText.split('\n');
    const newLines = newText.split('\n');
    const n = oldLines.length;
    const m = newLines.length;

    // LCS dynamic programming table
    const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < m; j++) {
        if (oldLines[i] === newLines[j]) {
          dp[i + 1][j + 1] = dp[i][j] + 1;
        } else {
          dp[i + 1][j + 1] = Math.max(dp[i][j + 1], dp[i + 1][j]);
        }
      }
    }

    // Backtrack to build diff
    const unified: DiffLine[] = [];
    let i = n;
    let j = m;
    let additions = 0;
    let deletions = 0;
    let unchanged = 0;

    const stack: DiffLine[] = [];
    while (i > 0 || j > 0) {
      if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
        stack.push({ type: 'eq', oldLineNo: i, newLineNo: j, text: oldLines[i - 1] });
        unchanged++;
        i--;
        j--;
      } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
        stack.push({ type: 'add', newLineNo: j, text: newLines[j - 1] });
        additions++;
        j--;
      } else if (i > 0 && (j === 0 || dp[i][j - 1] < dp[i - 1][j])) {
        stack.push({ type: 'del', oldLineNo: i, text: oldLines[i - 1] });
        deletions++;
        i--;
      }
    }

    while (stack.length > 0) {
      unified.push(stack.pop()!);
    }

    // Build side-by-side split rows
    const split: SplitRow[] = [];
    let uIdx = 0;
    while (uIdx < unified.length) {
      const item = unified[uIdx];
      if (item.type === 'eq') {
        split.push({
          oldLine: { lineNo: item.oldLineNo!, text: item.text, type: 'eq' },
          newLine: { lineNo: item.newLineNo!, text: item.text, type: 'eq' }
        });
        uIdx++;
      } else if (item.type === 'del') {
        // Look ahead for paired addition
        if (uIdx + 1 < unified.length && unified[uIdx + 1].type === 'add') {
          const nextItem = unified[uIdx + 1];
          split.push({
            oldLine: { lineNo: item.oldLineNo!, text: item.text, type: 'del' },
            newLine: { lineNo: nextItem.newLineNo!, text: nextItem.text, type: 'add' }
          });
          uIdx += 2;
        } else {
          split.push({
            oldLine: { lineNo: item.oldLineNo!, text: item.text, type: 'del' }
          });
          uIdx++;
        }
      } else if (item.type === 'add') {
        split.push({
          newLine: { lineNo: item.newLineNo!, text: item.text, type: 'add' }
        });
        uIdx++;
      }
    }

    return { unified, split, additions, deletions, unchanged };
  }

  let diffData = $derived(computeDiff(oldContent, newContent));
</script>

<svelte:window onkeydown={(e) => { if (show && e.key === 'Escape') oncancel(); }} />

{#if show}
  <div use:portal class="modal-backdrop" onclick={oncancel} role="presentation">
    <div class="diff-modal-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
      
      <!-- Modal Header -->
      <div class="diff-header">
        <div class="diff-title-group">
          <div class="file-icon-wrap">
            <FileText size={18} class="text-accent" />
          </div>
          <div>
            <h3 class="diff-title">{title}</h3>
            <div class="file-path-badge font-mono" title={filePath}>{filePath}</div>
          </div>
        </div>

        <div class="diff-header-actions">
          <!-- View mode toggle -->
          <div class="view-mode-toggle">
            <button
              type="button"
              class="toggle-btn {viewMode === 'unified' ? 'active' : ''}"
              onclick={() => viewMode = 'unified'}
              title="Unified diff view"
            >
              <Rows size={13} />
              <span>Unified</span>
            </button>
            <button
              type="button"
              class="toggle-btn {viewMode === 'split' ? 'active' : ''}"
              onclick={() => viewMode = 'split'}
              title="Side-by-side split view"
            >
              <Columns size={13} />
              <span>Split</span>
            </button>
          </div>

          <button class="diff-close-btn" onclick={oncancel} aria-label="Close dialog">
            <X size={18} />
          </button>
        </div>
      </div>

      <!-- Metrics Bar & Validation Status -->
      <div class="diff-sub-bar">
        <div class="diff-stats">
          <span class="stat-pill stat-add">+{diffData.additions} lines</span>
          <span class="stat-pill stat-del">-{diffData.deletions} lines</span>
          <span class="stat-pill stat-eq">{diffData.unchanged} unchanged</span>
        </div>

        {#if validationStatus !== 'idle'}
          <div class="validation-badge validation-{validationStatus}">
            {#if validationStatus === 'validating'}
              <RefreshCw size={12} class="animate-spin-slow" />
              <span>Checking Syntax...</span>
            {:else if validationStatus === 'valid'}
              <CheckCircle2 size={13} />
              <span>{validationMessage || 'Syntax Validated & OK'}</span>
            {:else if validationStatus === 'invalid'}
              <AlertTriangle size={13} />
              <span>{validationMessage || 'Syntax Validation Failed'}</span>
            {/if}
          </div>
        {/if}
      </div>

      {#if warningMessage}
        <div class="diff-warning-banner">
          <ShieldAlert size={16} class="text-amber flex-shrink-0" />
          <div class="text-xs">{warningMessage}</div>
        </div>
      {/if}

      <!-- Diff Content Viewer -->
      <div class="diff-viewport">
        {#if diffData.additions === 0 && diffData.deletions === 0}
          <div class="no-diff-state">
            <CheckCircle2 size={24} class="text-success" />
            <p>No changes detected between current file and editor.</p>
          </div>
        {:else if viewMode === 'unified'}
          <!-- Unified Diff View -->
          <div class="diff-table-container">
            <table class="diff-table font-mono">
              <tbody>
                {#each diffData.unified as row}
                  <tr class="diff-row diff-row-{row.type}">
                    <td class="line-num old-num">{row.oldLineNo || ''}</td>
                    <td class="line-num new-num">{row.newLineNo || ''}</td>
                    <td class="diff-sign">
                      {#if row.type === 'add'}+{:else if row.type === 'del'}- {:else}&nbsp;{/if}
                    </td>
                    <td class="diff-code">{row.text}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <!-- Side-by-Side Split View -->
          <div class="split-view-container font-mono">
            <div class="split-pane split-left">
              <div class="split-pane-header">Original ({filePath})</div>
              <table class="diff-table">
                <tbody>
                  {#each diffData.split as row}
                    <tr class="diff-row {row.oldLine ? `diff-row-${row.oldLine.type}` : 'diff-row-empty'}">
                      <td class="line-num">{row.oldLine?.lineNo || ''}</td>
                      <td class="diff-sign">{row.oldLine?.type === 'del' ? '-' : ' '}</td>
                      <td class="diff-code">{row.oldLine?.text || ''}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <div class="split-pane split-right">
              <div class="split-pane-header">Modified Version</div>
              <table class="diff-table">
                <tbody>
                  {#each diffData.split as row}
                    <tr class="diff-row {row.newLine ? `diff-row-${row.newLine.type}` : 'diff-row-empty'}">
                      <td class="line-num">{row.newLine?.lineNo || ''}</td>
                      <td class="diff-sign">{row.newLine?.type === 'add' ? '+' : ' '}</td>
                      <td class="diff-code">{row.newLine?.text || ''}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {/if}
      </div>

      <!-- Modal Footer -->
      <div class="diff-footer">
        <Button variant="outline" onclick={oncancel} disabled={isSaving}>
          Keep Editing
        </Button>
        <Button 
          variant="primary" 
          onclick={onconfirm} 
          disabled={isSaving || validationStatus === 'invalid'}
        >
          {#if isSaving}
            <RefreshCw size={13} class="animate-spin-slow" />
            <span>Applying Changes...</span>
          {:else}
            <Save size={13} />
            <span>Confirm &amp; Apply to System</span>
          {/if}
        </Button>
      </div>

    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 99995;
    padding: 20px;
  }

  .diff-modal-card {
    width: 960px;
    max-width: calc(100vw - 40px);
    height: 640px;
    max-height: 92vh;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: modalIn 0.18s cubic-bezier(0.4, 0, 0.2, 1) both;
  }

  @keyframes modalIn {
    from { opacity: 0; transform: scale(0.97) translateY(8px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }

  .diff-header {
    padding: 14px 20px;
    background: var(--color-bg-surface);
    border-bottom: 1px solid var(--color-border-subtle);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .diff-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .file-icon-wrap {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.1));
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .diff-title {
    font-size: 14.5px;
    font-weight: 600;
    margin: 0;
    color: var(--color-text-primary);
  }

  .file-path-badge {
    font-size: 11px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 480px;
  }

  .diff-header-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .view-mode-toggle {
    display: flex;
    align-items: center;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }

  .toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    font-size: 11px;
    font-weight: 600;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .toggle-btn.active {
    background: var(--color-bg-surface);
    color: var(--color-accent);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
  }

  .diff-close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease;
  }

  .diff-close-btn:hover {
    color: var(--color-text-primary);
  }

  .diff-sub-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 20px;
    background: var(--color-bg-base);
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: 11.5px;
  }

  .diff-stats {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .stat-pill {
    padding: 2px 7px;
    border-radius: 4px;
    font-weight: 600;
    font-size: 11px;
  }

  .stat-add { background: rgba(16, 185, 129, 0.15); color: #10b981; }
  .stat-del { background: rgba(239, 68, 68, 0.15); color: #ef4444; }
  .stat-eq { background: var(--color-bg-raised); color: var(--color-text-muted); }

  .validation-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
  }

  .validation-valid { background: rgba(16, 185, 129, 0.15); color: #10b981; }
  .validation-invalid { background: rgba(239, 68, 68, 0.15); color: #ef4444; }
  .validation-validating { background: rgba(0, 218, 243, 0.15); color: var(--color-accent); }

  .diff-warning-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 20px;
    background: rgba(245, 158, 11, 0.1);
    border-bottom: 1px solid rgba(245, 158, 11, 0.25);
    color: #f59e0b;
    font-size: 12px;
  }

  /* ── Dark Theme (Default) ── */
  .diff-viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: #08111e;
  }

  /* ── Diff Table Styles ── */
  .diff-table-container {
    width: 100%;
    min-width: max-content;
  }

  .diff-table {
    width: 100%;
    border-collapse: collapse;
    font-family: 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.5;
  }

  .diff-row {
    transition: background 0.1s ease;
  }

  .diff-row-eq { 
    background: transparent; 
    color: #e2e8f0; 
  }
  
  .diff-row-add { 
    background: rgba(16, 185, 129, 0.22); 
    color: #a7f3d0; 
  }
  
  .diff-row-del { 
    background: rgba(239, 68, 68, 0.22); 
    color: #fca5a5; 
  }
  
  .diff-row-empty { 
    background: rgba(0, 0, 0, 0.35); 
  }

  .line-num {
    width: 44px;
    min-width: 44px;
    padding: 2px 10px;
    text-align: right;
    user-select: none;
    color: #64748b;
    background: #050b14;
    border-right: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 11px;
    font-weight: 500;
  }

  .diff-sign {
    width: 20px;
    min-width: 20px;
    text-align: center;
    user-select: none;
    font-weight: 700;
    padding: 2px 0;
  }

  .diff-row-add .diff-sign { color: #34d399; }
  .diff-row-del .diff-sign { color: #f87171; }

  .diff-code {
    padding: 2px 10px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* Split View */
  .split-view-container {
    display: grid;
    grid-template-columns: 1fr 1fr;
    min-height: 100%;
  }

  .split-pane {
    overflow-x: auto;
  }

  .split-left {
    border-right: 1px solid rgba(255, 255, 255, 0.08);
  }

  .split-pane-header {
    position: sticky;
    top: 0;
    z-index: 10;
    padding: 6px 14px;
    background: #050b14;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 11px;
    font-weight: 700;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .no-diff-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    color: var(--color-text-secondary);
    font-size: 13px;
    padding: 40px;
  }

  .diff-footer {
    padding: 12px 20px;
    background: var(--color-bg-surface);
    border-top: 1px solid var(--color-border-subtle);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }

  /* ── Light Theme Overrides ── */
  :global(html.light-mode) .diff-modal-card {
    background: #ffffff;
    border-color: #cbd5e1;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.2);
  }

  :global(html.light-mode) .diff-header {
    background: #ffffff;
    border-bottom-color: #e2e8f0;
  }

  :global(html.light-mode) .diff-title {
    color: #0f172a;
  }

  :global(html.light-mode) .file-path-badge {
    color: #64748b;
  }

  :global(html.light-mode) .diff-sub-bar {
    background: #f8fafc;
    border-bottom-color: #e2e8f0;
  }

  :global(html.light-mode) .diff-viewport {
    background: #ffffff;
  }

  :global(html.light-mode) .diff-row-eq {
    background: #ffffff;
    color: #0f172a !important;
  }

  :global(html.light-mode) .diff-row-add {
    background: #e6fcf5 !important;
    color: #047857 !important;
  }

  :global(html.light-mode) .diff-row-add .diff-sign {
    color: #059669 !important;
  }

  :global(html.light-mode) .diff-row-del {
    background: #ffebe9 !important;
    color: #b91c1c !important;
  }

  :global(html.light-mode) .diff-row-del .diff-sign {
    color: #dc2626 !important;
  }

  :global(html.light-mode) .diff-row-empty {
    background: #f8fafc !important;
  }

  :global(html.light-mode) .line-num {
    background: #f1f5f9;
    color: #64748b;
    border-right-color: #e2e8f0;
  }

  :global(html.light-mode) .split-left {
    border-right-color: #e2e8f0;
  }

  :global(html.light-mode) .split-pane-header {
    background: #f1f5f9;
    border-bottom-color: #e2e8f0;
    color: #334155;
  }

  :global(html.light-mode) .diff-footer {
    background: #ffffff;
    border-top-color: #e2e8f0;
  }

  :global(html.light-mode) .view-mode-toggle {
    background: #f1f5f9;
    border-color: #cbd5e1;
  }

  :global(html.light-mode) .toggle-btn {
    color: #64748b;
  }

  :global(html.light-mode) .toggle-btn.active {
    background: #ffffff;
    color: #0066cc;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  :global(html.light-mode) .stat-eq {
    background: #f1f5f9;
    color: #475569;
  }
</style>
