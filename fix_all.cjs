const fs = require('fs');

// 1. Fix StartupManager.svelte
const smPath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/StartupManager.svelte';
let sm = fs.readFileSync(smPath, 'utf8');
sm = sm.replace(/<div style="display:flex; gap:2px; background:var\(--color-bg-raised\); padding:4px; border-radius:8px; margin: 0;">/, '<div class="tab-bar">');
sm = sm.replace(/<Button\s+class="filter-btn([^>]*)>([\s\S]*?)<\/Button>/g, '<button class="tab-btn$1>$2</button>');
fs.writeFileSync(smPath, sm);

// 2. Fix ShellEnv.svelte
const shellEnvPath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/ShellEnv.svelte';
let shellEnv = fs.readFileSync(shellEnvPath, 'utf8');

const missingCss = `
  /* ─── PATH tab ───────────────────────────────────────────────────── */
  .path-index { color: var(--color-text-muted); font-size: 11px; width: 30px; }
  .path-dir-row { display: flex; align-items: center; gap: 8px; }
  .path-dir { font-family: var(--font-mono); font-size: 12px; color: var(--color-text-primary); }
  .path-missing td { opacity: 0.75; }
  .path-legend { font-size: 11px; color: var(--color-text-muted); display: flex; align-items: center; gap: 5px; }

  /* ─── Files tab ──────────────────────────────────────────────────── */
  .files-layout { display: flex; height: 100%; overflow: hidden; }
  .files-sidebar {
    width: 240px; min-width: 240px;
    border-right: 1px solid var(--color-border);
    display: flex; flex-direction: column;
    overflow-y: auto; padding: 8px 0;
  }
  .files-sidebar-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 4px 12px 8px;
    font-size: 11px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 4px;
  }
  .files-sidebar-sep { border-top: 1px solid var(--color-border); margin: 8px 12px; }
  :global(.sidebar-backup-btn) { margin: 0 12px 8px; }
  .new-file-form {
    display: flex; gap: 6px; padding: 8px 12px; align-items: center;
    border-bottom: 1px solid var(--color-border);
  }
  .new-file-form input { flex: 1; padding: 5px 8px; font-size: 12px; }
  .file-group-label {
    padding: 6px 12px 3px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.08em;
    text-transform: uppercase; color: #475569;
  }
  .file-item {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 12px;
    border: none; background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px; font-family: var(--font-mono);
    cursor: pointer; text-align: left;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap; overflow: hidden;
  }
  .file-item:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
  .file-item.selected { background: var(--color-active-bg); color: var(--color-accent-soft); }
  .file-item-name { flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .file-item-lines { font-size: 10px; color: var(--color-text-muted); flex-shrink: 0; }

  .files-editor { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .editor-toolbar {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 16px; border-bottom: 1px solid var(--color-border);
    flex-shrink: 0; gap: 12px;
  }
  .editor-tools { display: flex; gap: 8px; align-items: center; }
  .editor-filename {
    display: flex; align-items: center; gap: 6px;
    font-family: var(--font-mono); font-size: 13px;
    color: var(--color-text-primary); font-weight: 600;
  }
  .unsaved-warning {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 16px;
    background: var(--color-warning-muted); color: var(--color-warning);
    font-size: 12px; border-bottom: 1px solid rgba(245,158,11,0.2);
    flex-shrink: 0;
  }
  .editor-empty {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 12px; color: var(--color-text-muted);
  }
  .code-editor {
    flex: 1; resize: none; border: none; outline: none;
    background: rgba(0,0,0,0.25);
    color: var(--color-text-primary);
    font-family: var(--font-mono); font-size: 13px; line-height: 1.6;
    padding: 16px; overflow-y: auto;
    white-space: pre;
  }
  .code-editor.wrap { white-space: pre-wrap; word-break: break-all; }

  .backup-list { display: flex; flex-direction: column; }
  .backup-list-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 4px 12px; font-size: 11px; font-weight: 600;
    color: var(--color-text-muted);
    border-top: 1px solid var(--color-border);
  }
  .backup-item {
    display: flex; flex-direction: column; gap: 3px;
    padding: 8px 12px; border-bottom: 1px solid var(--color-border);
    font-size: 11px;
  }
  .backup-name { font-family: var(--font-mono); color: var(--color-text-primary); font-size: 10px; word-break: break-all; }
  .backup-ts { color: var(--color-text-muted); font-size: 10px; }

  /* ─── Preview tab ────────────────────────────────────────────────── */
  .preview-toolbar {
    display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
    margin-bottom: 16px;
  }
  .toggle-label-row {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px; color: var(--color-text-secondary); cursor: pointer;
    white-space: nowrap;
  }
  .source-buttons {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    padding: 8px 0;
  }
  .ml-4 { margin-left: 4px; }
  
  /* Additional fixes for tabs layout */
  .tab-section { padding: 0; display: flex; flex-direction: column; gap: 0; height: 100%; overflow: hidden; }
  .section-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
    background: rgba(0,0,0,0.2);
    flex-shrink: 0;
  }
`;

if (!shellEnv.includes('.files-layout')) {
  shellEnv = shellEnv.replace('</style>', missingCss + '\n</style>');
}

// Fix PATH Manager scroll issue
// It has:
// {:else if activeTab === 'path'}
//   <div class="tab-section">
//     <div class="section-header">...</div>
// We need to wrap the rest inside `.module-content-scroll`
shellEnv = shellEnv.replace(
  /<div class="section-header">([\s\S]*?)<\/div>\s*{#if addPathForm}/,
  '<div class="section-header">$1</div>\n        <div class="module-content-scroll" style="display:flex; flex-direction:column; padding: 20px; gap: 16px;">\n        {#if addPathForm}'
);
shellEnv = shellEnv.replace(
  /<\/td>\s*<\/tr>\s*{#if !entry\.exists}\s*<tr[\s\S]*?<\/tr>\s*{\/if}\s*{\/each}\s*<\/tbody>\s*<\/table>\s*<\/div>\s*<div class="path-legend">[\s\S]*?<\/div>\s*{\/if}\s*<\/div>\s*<!-- ══ PROFILE FILES/,
  (match) => {
    return match.replace(/<\/if>\s*<\/div>\s*<!-- ══ PROFILE FILES/, '</if>\n        </div>\n      </div>\n\n    <!-- ══ PROFILE FILES');
  }
);
// Wait, the regex for replacing the end of PATH Manager is complex because of missing path row and path-legend. Let me just replace the very end of the PATH tab.
// Currently the end of PATH tab is:
//           <div class="path-legend">
//             <XCircle size={12} style="color:var(--color-error)" /> Red = directory does not exist on disk
//           </div>
//         {/if}
//       </div>
//
//     <!-- ══ PROFILE FILES
// 
// If we opened `<div class="module-content-scroll">`, we need to close it before `</div><!-- ══ PROFILE FILES`.

shellEnv = shellEnv.replace(
  /<div class="path-legend">([\s\S]*?)<\/div>\s*{\/if}\s*<\/div>\s*<!-- ══ PROFILE FILES/,
  '<div class="path-legend">$1</div>\n          {/if}\n        </div><!-- module-content-scroll -->\n      </div><!-- tab-section -->\n\n    <!-- ══ PROFILE FILES'
);

// Fix Live Preview search and toggle UX
// "live preview button search and toggle is not well orginized try to match other pages design here to save space and better UI ux"
// In original, they were inside `.preview-toolbar`. Let's restructure `.preview-toolbar` to be a flex row.
// Replace the .preview-toolbar div with a cleaner one:
shellEnv = shellEnv.replace(
  /<div class="preview-toolbar">([\s\S]*?)<!-- Source buttons -->/,
  `<div class="preview-toolbar">
            <div class="search-bar" style="flex:1; margin:0">
              <Search size={13} style="color:var(--color-text-muted)" />
              <input bind:value={filterLive} placeholder="Filter variables…" id="shell-live-filter" />
            </div>
            
            <label class="toggle-label-row" style="background:rgba(255,255,255,0.03); padding:8px 12px; border-radius:8px; border:1px solid var(--color-border);">
              <button
                class="ui-toggle"
                class:on={showOnlyUnsynced}
                onclick={() => showOnlyUnsynced = !showOnlyUnsynced}
                role="switch"
                aria-checked={showOnlyUnsynced}
                type="button"
                id="shell-unsynced-only"
                aria-label="Toggle show only out-of-sync"
                style="transform: scale(0.8); margin-right:4px;"
              >
                <span class="ui-toggle-thumb"></span>
              </button>
              Show out-of-sync only
            </label>
            <span class="text-muted text-xs" style="background:rgba(255,255,255,0.03); padding:8px 12px; border-radius:8px; border:1px solid var(--color-border);">{filteredLiveEnv.length} / {liveEnv.length} vars</span>
          </div>

          <!-- Source buttons -->`
);

fs.writeFileSync(shellEnvPath, shellEnv);
console.log('Fixes applied');
