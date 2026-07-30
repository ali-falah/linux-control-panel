<script lang="ts">
  import { Settings, X, Sun, Moon, Check, Monitor, Sparkles } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && uiStore.settingsModalOpen) {
      uiStore.closeSettingsModal();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if uiStore.settingsModalOpen}
  <div 
    class="modal-backdrop" 
    onclick={() => uiStore.closeSettingsModal()} 
    role="presentation"
  >
    <div 
      class="settings-modal" 
      onclick={(e) => e.stopPropagation()} 
      role="dialog" 
      aria-modal="true" 
      aria-labelledby="settings-modal-title"
    >
      <!-- Header -->
      <div class="modal-header">
        <div class="header-title-group">
          <div class="header-icon">
            <Settings size={20} />
          </div>
          <div>
            <h2 id="settings-modal-title" class="modal-title">Control Panel Settings</h2>
            <p class="modal-subtitle">Configure theme appearance, display preferences, and UI system options</p>
          </div>
        </div>
        <button 
          class="close-btn" 
          onclick={() => uiStore.closeSettingsModal()} 
          aria-label="Close settings"
        >
          <X size={18} />
        </button>
      </div>

      <!-- Content Body -->
      <div class="modal-body">
        <!-- Appearance & Theme Section -->
        <div class="settings-section">
          <div class="section-title-row">
            <Sparkles size={16} class="section-icon" />
            <h3 class="section-title">Appearance & Theme Mode</h3>
          </div>
          <p class="section-desc">Select your preferred color scheme for optimal visibility and readability.</p>

          <div class="theme-grid">
            <!-- Dark Mode Card -->
            <button
              class="theme-card"
              class:selected={uiStore.theme === 'dark'}
              onclick={() => uiStore.theme !== 'dark' && uiStore.toggleTheme()}
            >
              <div class="theme-preview dark-preview">
                <div class="preview-header">
                  <div class="preview-dot dot-red"></div>
                  <div class="preview-dot dot-yellow"></div>
                  <div class="preview-dot dot-green"></div>
                </div>
                <div class="preview-body">
                  <div class="preview-sidebar"></div>
                  <div class="preview-content">
                    <div class="preview-card-shape dark-card">
                      <div class="preview-line line-accent"></div>
                      <div class="preview-line line-text"></div>
                    </div>
                  </div>
                </div>
              </div>
              <div class="theme-card-footer">
                <div class="theme-card-info">
                  <Moon size={16} class="theme-type-icon" />
                  <div>
                    <span class="theme-card-name">Obsidian Dark Mode</span>
                    <span class="theme-card-desc">Deep blue contrast for low light</span>
                  </div>
                </div>
                <div class="select-indicator" class:active={uiStore.theme === 'dark'}>
                  {#if uiStore.theme === 'dark'}
                    <Check size={12} />
                  {/if}
                </div>
              </div>
            </button>

            <!-- Light Mode Card -->
            <button
              class="theme-card"
              class:selected={uiStore.theme === 'light'}
              onclick={() => uiStore.theme !== 'light' && uiStore.toggleTheme()}
            >
              <div class="theme-preview light-preview">
                <div class="preview-header">
                  <div class="preview-dot dot-red"></div>
                  <div class="preview-dot dot-yellow"></div>
                  <div class="preview-dot dot-green"></div>
                </div>
                <div class="preview-body">
                  <div class="preview-sidebar"></div>
                  <div class="preview-content">
                    <div class="preview-card-shape light-card">
                      <div class="preview-line line-accent-light"></div>
                      <div class="preview-line line-text-light"></div>
                    </div>
                  </div>
                </div>
              </div>
              <div class="theme-card-footer">
                <div class="theme-card-info">
                  <Sun size={16} class="theme-type-icon sun" />
                  <div>
                    <span class="theme-card-name">Developer Eye-Comfort</span>
                    <span class="theme-card-desc">Soft off-white `#F5F3ED` reduced-glare</span>
                  </div>
                </div>
                <div class="select-indicator" class:active={uiStore.theme === 'light'}>
                  {#if uiStore.theme === 'light'}
                    <Check size={12} />
                  {/if}
                </div>
              </div>
            </button>
          </div>
        </div>

        <!-- Sidebar Layout Preferences -->
        <div class="settings-section">
          <div class="section-title-row">
            <Monitor size={16} class="section-icon" />
            <h3 class="section-title">Sidebar & Navigation</h3>
          </div>

          <div class="pref-row">
            <div>
              <span class="pref-name">Compact Collapsed Sidebar</span>
              <span class="pref-desc">Collapse the navigation sidebar into a narrow icon strip with hover flyout sub-menus.</span>
            </div>
            <button 
              class="toggle-switch"
              class:active={uiStore.sidebarCollapsed}
              onclick={() => uiStore.toggleSidebar()}
              aria-label="Toggle compact sidebar"
            >
              <div class="toggle-knob"></div>
            </button>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="modal-footer">
        <span class="app-build-info">Linux Control Panel v0.2.6</span>
        <button 
          class="btn btn-primary"
          onclick={() => uiStore.closeSettingsModal()}
        >
          Done
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    animation: fadeIn 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  .settings-modal {
    width: 100%;
    max-width: 580px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.45);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    animation: slideUp 0.22s cubic-bezier(0.16, 1, 0.3, 1);
    color: var(--color-text-primary);
  }

  @keyframes slideUp {
    from { opacity: 0; transform: translateY(12px) scale(0.98); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* Header */
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 24px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-card);
  }

  .header-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .header-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--color-active-bg);
    color: var(--color-accent);
  }

  .modal-title {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .modal-subtitle {
    margin: 2px 0 0 0;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .close-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  /* Body */
  .modal-body {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-height: 70vh;
    overflow-y: auto;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-accent);
  }

  .section-title {
    margin: 0;
    font-size: 13px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-primary);
  }

  .section-desc {
    margin: 0 0 6px 0;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  /* Theme Grid */
  .theme-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin-top: 6px;
  }

  .theme-card {
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card);
    border: 2px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: left;
    padding: 0;
  }

  .theme-card:hover {
    border-color: var(--color-border-hover);
    transform: translateY(-2px);
  }

  .theme-card.selected {
    border-color: var(--color-accent);
    box-shadow: 0 0 16px var(--color-accent-glow);
  }

  .theme-preview {
    height: 90px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dark-preview {
    background: #051424;
  }

  .light-preview {
    background: #F5F3ED;
  }

  .preview-header {
    display: flex;
    gap: 4px;
  }

  .preview-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .dot-red    { background: #EF4444; }
  .dot-yellow { background: #F59E0B; }
  .dot-green  { background: #10B981; }

  .preview-body {
    display: flex;
    gap: 8px;
    flex: 1;
  }

  .preview-sidebar {
    width: 24px;
    border-radius: 4px;
  }

  .dark-preview .preview-sidebar { background: #010f1f; }
  .light-preview .preview-sidebar { background: #EFECE4; }

  .preview-content {
    flex: 1;
  }

  .preview-card-shape {
    height: 100%;
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dark-card {
    background: #122131;
    border: 1px solid rgba(0, 218, 243, 0.2);
  }

  .light-card {
    background: #FAFAF9;
    border: 1px solid #DCD8CD;
  }

  .preview-line {
    border-radius: 2px;
    height: 4px;
  }

  .line-accent { width: 40%; background: #00daf3; }
  .line-text   { width: 70%; background: #849396; }

  .line-accent-light { width: 40%; background: #4A6FA5; }
  .line-text-light   { width: 70%; background: #666666; }

  .theme-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    background: var(--color-bg-surface);
    border-top: 1px solid var(--color-border-subtle);
  }

  .theme-card-info {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .theme-type-icon {
    color: var(--color-accent);
  }

  .theme-type-icon.sun {
    color: #CB854F;
  }

  .theme-card-name {
    display: block;
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .theme-card-desc {
    display: block;
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .select-indicator {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid var(--color-border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #ffffff;
    transition: all 0.15s ease;
  }

  .select-indicator.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  /* Preference Rows */
  .pref-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
  }

  .pref-name {
    display: block;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .pref-desc {
    display: block;
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }

  .toggle-switch {
    width: 38px;
    height: 20px;
    border-radius: 10px;
    background: rgba(120, 130, 140, 0.3);
    position: relative;
    border: none;
    cursor: pointer;
    transition: background 0.2s ease;
    flex-shrink: 0;
  }

  .toggle-switch.active {
    background: var(--color-accent);
  }

  .toggle-knob {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #ffffff;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }

  .toggle-switch.active .toggle-knob {
    transform: translateX(18px);
  }

  /* Footer */
  .modal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 24px;
    background: var(--color-bg-card);
    border-top: 1px solid var(--color-border-subtle);
  }

  .app-build-info {
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }
</style>
