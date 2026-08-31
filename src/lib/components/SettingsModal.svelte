<script lang="ts">
  import { onMount } from 'svelte';
  import { 
    X, Sun, Moon, Check, Sparkles, Bot, Cpu, Globe, Key, 
    RefreshCw, Eye, EyeOff, Save, Sliders, Bell, HardDrive, 
    AlertTriangle, FileText, CheckCircle2 
  } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Toggle from './ui/Toggle.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import { portal } from '../actions/portal.ts';

  let activeCategoryTab = $state<'appearance' | 'ai' | 'system'>('appearance');
  let showKey = $state(false);
  let openingConfig = $state(false);
  let appVersion = $state(uiStore.version);

  onMount(async () => {
    try {
      const ver = await invoke<string>('get_app_version');
      if (ver) {
        appVersion = ver;
        uiStore.version = ver;
      }
    } catch (err) {
      console.error('Failed to fetch app version:', err);
    }
  });

  async function handleOpenConfigFile() {
    openingConfig = true;
    try {
      await invoke('open_system_config_file');
    } catch (err) {
      console.error('Failed to open config file:', err);
    } finally {
      openingConfig = false;
    }
  }

  // Draft settings state (manual save pattern)
  let draftEnabled = $state(aiStore.enabled);
  let draftProvider = $state<'ollama' | 'gemini' | 'openai'>(aiStore.provider);
  let draftOllamaUrl = $state(aiStore.ollamaUrl);
  let draftOllamaModel = $state(aiStore.ollamaModel);
  let draftCloudProvider = $state<'gemini' | 'openai'>(aiStore.cloudProvider);
  let draftApiKey = $state(aiStore.apiKey);
  let draftCloudModel = $state(aiStore.cloudModel);

  // Connection testing state
  let testLoading = $state(false);
  let testStatus = $state<{ success: boolean; message: string } | null>(null);

  // Sync draft state whenever modal opens
  $effect(() => {
    if (uiStore.settingsModalOpen) {
      draftEnabled = aiStore.enabled;
      draftProvider = aiStore.provider;
      draftOllamaUrl = aiStore.ollamaUrl;
      draftOllamaModel = aiStore.ollamaModel;
      draftCloudProvider = aiStore.cloudProvider;
      draftApiKey = aiStore.apiKey;
      draftCloudModel = aiStore.cloudModel;
      testStatus = null;
      aiStore.checkOllamaStatus().then(() => {
        if (aiStore.availableModels.length > 0) {
          if (aiStore.availableModels.includes('llama3.2:1b') && (!draftOllamaModel || draftOllamaModel === 'qwen2.5:1.5b')) {
            draftOllamaModel = 'llama3.2:1b';
          } else if (!aiStore.availableModels.includes(draftOllamaModel)) {
            draftOllamaModel = aiStore.availableModels[0];
          }
        }
      });
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && uiStore.settingsModalOpen) {
      uiStore.closeSettingsModal();
    }
  }

  $effect(() => {
    if (uiStore.settingsModalOpen) {
      const prevOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = prevOverflow;
      };
    }
  });

  async function handleTestConnection() {
    testLoading = true;
    testStatus = null;

    if (draftProvider === 'ollama') {
      try {
        const models = await invoke<string[]>('ai_check_ollama_status', { url: draftOllamaUrl });
        aiStore.ollamaConnected = true;
        aiStore.availableModels = models;
        testStatus = {
          success: true,
          message: `Connected successfully! ${models.length} local models detected.`,
        };
      } catch (e: any) {
        aiStore.ollamaConnected = false;
        testStatus = {
          success: false,
          message: `Connection failed: ${typeof e === 'string' ? e : e?.message || String(e)}`,
        };
      } finally {
        testLoading = false;
      }
    } else {
      try {
        const msg = await invoke<string>('ai_test_cloud_connection', {
          provider: draftProvider,
          apiKey: draftApiKey,
          model: draftCloudModel,
        });
        testStatus = { success: true, message: msg };
      } catch (e: any) {
        const msg = typeof e === 'string' ? e : e?.message || String(e);
        testStatus = { success: false, message: `Cloud Test Failed: ${msg}` };
      } finally {
        testLoading = false;
      }
    }
  }

  function handleSaveAiSettings() {
    aiStore.enabled = draftEnabled;
    aiStore.provider = draftProvider;
    aiStore.ollamaUrl = draftOllamaUrl;
    aiStore.ollamaModel = draftOllamaModel;
    aiStore.cloudProvider = draftCloudProvider;
    aiStore.apiKey = draftApiKey;
    aiStore.cloudModel = draftCloudModel;

    aiStore.saveSettings();
    uiStore.showToast('AI Settings saved successfully', 'success');
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if uiStore.settingsModalOpen}
  <div 
    use:portal
    class="modal-backdrop" 
    onclick={() => uiStore.closeSettingsModal()} 
    onwheel={(e) => e.stopPropagation()}
    role="presentation"
  >
    <div 
      class="settings-modal" 
      onclick={(e) => e.stopPropagation()} 
      onwheel={(e) => e.stopPropagation()}
      role="dialog" 
      aria-modal="true" 
      aria-labelledby="settings-modal-title"
    >
      <!-- ── Left Sidebar Navigation ────────────────────────────────────── -->
      <aside class="sidebar-pane">
        <div class="sidebar-header">
          <h2 id="settings-modal-title" class="sidebar-title">Settings</h2>
        </div>

        <nav class="sidebar-nav">
          <button
            type="button"
            class="nav-tab-btn"
            class:active={activeCategoryTab === 'appearance'}
            onclick={() => activeCategoryTab = 'appearance'}
          >
            <Sparkles size={16} />
            <span class="nav-label">Appearance</span>
          </button>

          <button
            type="button"
            class="nav-tab-btn"
            class:active={activeCategoryTab === 'ai'}
            onclick={() => activeCategoryTab = 'ai'}
          >
            <Bot size={16} />
            <span class="nav-label">AI Engine</span>
            {#if !aiStore.enabled}
              <span class="nav-badge muted">Off</span>
            {:else}
              <span class="nav-badge active">On</span>
            {/if}
          </button>

          <button
            type="button"
            class="nav-tab-btn"
            class:active={activeCategoryTab === 'system'}
            onclick={() => activeCategoryTab = 'system'}
          >
            <Sliders size={16} />
            <span class="nav-label">Preferences</span>
          </button>
        </nav>

        <div class="sidebar-footer">
          <span class="version-tag">v{appVersion}</span>
        </div>
      </aside>

      <!-- ── Right Content Pane ─────────────────────────────────────────── -->
      <main class="content-pane">
        <!-- Pane Top Bar with Close Button -->
        <div class="content-top-bar">
          <h3 class="pane-section-title">
            {#if activeCategoryTab === 'appearance'}
              Appearance &amp; Theme
            {:else if activeCategoryTab === 'ai'}
              AI Engine &amp; Assistant
            {:else}
              System Preferences
            {/if}
          </h3>
          <button 
            class="close-btn" 
            onclick={() => uiStore.closeSettingsModal()} 
            aria-label="Close settings"
          >
            <X size={18} />
          </button>
        </div>

        <!-- Pane Body -->
        <div class="content-body">
          <!-- ── TAB 1: APPEARANCE ──────────────────────────────────────── -->
          {#if activeCategoryTab === 'appearance'}
            <div class="settings-section">
              <p class="section-desc">Select your preferred visual theme for optimal readability and contrast.</p>

              <div class="theme-grid">
                <!-- Dark Mode Card -->
                <button
                  type="button"
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
                      <Moon size={15} class="theme-type-icon" />
                      <div>
                        <span class="theme-card-name">Obsidian Dark</span>
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
                  type="button"
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
                      <Sun size={15} class="theme-type-icon sun" />
                      <div>
                        <span class="theme-card-name">Modern Light</span>
                        <span class="theme-card-desc">Clean slate canvas with royal blue</span>
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

              <!-- Accent Color Palette -->
              <div style="margin-top: 24px;">
                <h4 style="margin: 0 0 6px 0; font-size: 13px; font-weight: 600; color: var(--color-text-primary);">Accent Color Theme</h4>
                <p class="section-desc" style="margin-bottom: 12px;">Choose a dynamic primary accent color for active buttons, badges, highlights, and glow states.</p>

                <div class="accent-color-grid">
                  {#each [
                    { id: 'cyan', name: 'Cyan Neon', color: '#00daf3', sub: 'Electric obsidian' },
                    { id: 'emerald', name: 'Emerald Forest', color: '#10b981', sub: 'Natural green' },
                    { id: 'sapphire', name: 'Sapphire Blue', color: '#3b82f6', sub: 'Royal azure' },
                    { id: 'mint', name: 'Icy Mint', color: '#2dd4bf', sub: 'Crisp seafoam' },
                    { id: 'purple', name: 'Purple Amethyst', color: '#a855f7', sub: 'Futuristic violet' },
                    { id: 'amber', name: 'Electric Amber', color: '#f59e0b', sub: 'Energetic gold' },
                    { id: 'rose', name: 'Rose Crimson', color: '#f43f5e', sub: 'Vivid ruby' },
                    { id: 'indigo', name: 'Royal Indigo', color: '#6366f1', sub: 'Cosmic violet' },
                    { id: 'slate', name: 'Slate Minimal', color: '#94a3b8', sub: 'Understated mono' }
                  ] as acc}
                    <button
                      type="button"
                      class="accent-card"
                      class:active={uiStore.accentColor === acc.id}
                      onclick={() => uiStore.setAccentColor(acc.id as any)}
                    >
                      <div class="accent-swatch" style="background: {acc.color}; box-shadow: 0 0 10px {acc.color}40;"></div>
                      <div class="accent-info">
                        <span class="accent-name">{acc.name}</span>
                        <span class="accent-sub">{acc.sub}</span>
                      </div>
                      <div class="select-indicator" class:active={uiStore.accentColor === acc.id}>
                        {#if uiStore.accentColor === acc.id}
                          <Check size={12} />
                        {/if}
                      </div>
                    </button>
                  {/each}
                </div>
              </div>

              <!-- OLED Pitch-Black Mode -->
              <div style="margin-top: 24px;">
                <h4 style="margin: 0 0 6px 0; font-size: 13px; font-weight: 600; color: var(--color-text-primary);">OLED Display Optimization</h4>
                <p class="section-desc" style="margin-bottom: 12px;">Fine-tuned for laptop battery life and true zero-nit contrast.</p>

                <div 
                  class="oled-toggle-card"
                  class:enabled={uiStore.isOled && uiStore.theme === 'dark'}
                  class:disabled={uiStore.theme === 'light'}
                  onclick={() => {
                    if (uiStore.theme === 'dark') {
                      uiStore.toggleOled();
                    } else {
                      uiStore.toggleTheme();
                      uiStore.toggleOled(true);
                    }
                  }}
                >
                  <div class="oled-card-left">
                    <div class="oled-icon-box">
                      <Moon size={18} />
                    </div>
                    <div>
                      <div class="oled-title">OLED Pure Black Mode (#000000)</div>
                      <div class="oled-desc">
                        Turns off subpixels on OLED/AMOLED screens by converting the dark canvas into true pitch black.
                        {#if uiStore.theme === 'light'}
                          <span style="color: var(--color-warning);"> (Clicking will switch from Light mode to OLED Dark mode)</span>
                        {/if}
                      </div>
                    </div>
                  </div>
                  <Toggle
                    checked={uiStore.isOled && uiStore.theme === 'dark'}
                    onToggle={() => {
                      if (uiStore.theme === 'dark') {
                        uiStore.toggleOled();
                      } else {
                        uiStore.toggleTheme();
                        uiStore.toggleOled(true);
                      }
                    }}
                  />
                </div>
              </div>
            </div>

          <!-- ── TAB 2: AI ASSISTANT & ENGINE ────────────────────────────── -->
          {:else if activeCategoryTab === 'ai'}
            <div class="settings-section">
              <!-- Master AI Enable Card -->
              <div 
                class="master-ai-card"
                class:enabled={draftEnabled}
                onclick={() => { 
                  draftEnabled = !draftEnabled; 
                  aiStore.enabled = draftEnabled; 
                  aiStore.saveSettings(); 
                }}
              >
                <div class="master-ai-info">
                  <div class="master-ai-icon">
                    <Bot size={20} />
                  </div>
                  <div>
                    <div class="master-ai-title">Enable AI Intelligence Features</div>
                    <div class="master-ai-sub">
                      {draftEnabled ? 'Intelligent diagnostics, log reasoning, and remediation commands active.' : 'AI features are completely disabled throughout the application.'}
                    </div>
                  </div>
                </div>
                <Toggle
                  bind:checked={draftEnabled}
                  onToggle={(v) => { 
                    draftEnabled = v; 
                    aiStore.enabled = v; 
                    aiStore.saveSettings(); 
                  }}
                />
              </div>

              {#if draftEnabled}
                <div class="sub-header-label">Model Provider Selection</div>

                <!-- Provider Cards Grid -->
                <div class="provider-grid">
                  <!-- Local Ollama Card -->
                  <button
                    type="button"
                    class="provider-card"
                    class:selected={draftProvider === 'ollama'}
                    onclick={() => { draftProvider = 'ollama'; }}
                  >
                    <div class="provider-card-top">
                      <Cpu size={18} class="provider-icon" />
                      <div class="provider-radio" class:active={draftProvider === 'ollama'}>
                        {#if draftProvider === 'ollama'}<Check size={10} />{/if}
                      </div>
                    </div>
                    <div class="provider-name">Local Ollama</div>
                    <div class="provider-tag">100% Offline &amp; Private</div>
                  </button>

                  <!-- Google Gemini Card -->
                  <button
                    type="button"
                    class="provider-card"
                    class:selected={draftProvider === 'gemini'}
                    onclick={() => { draftProvider = 'gemini'; draftCloudProvider = 'gemini'; if (!draftCloudModel) draftCloudModel = 'gemini-2.5-flash'; }}
                  >
                    <div class="provider-card-top">
                      <Globe size={18} class="provider-icon" />
                      <div class="provider-radio" class:active={draftProvider === 'gemini'}>
                        {#if draftProvider === 'gemini'}<Check size={10} />{/if}
                      </div>
                    </div>
                    <div class="provider-name">Google Gemini</div>
                    <div class="provider-tag">Fast Cloud AI</div>
                  </button>

                  <!-- OpenAI Card -->
                  <button
                    type="button"
                    class="provider-card"
                    class:selected={draftProvider === 'openai'}
                    onclick={() => { draftProvider = 'openai'; draftCloudProvider = 'openai'; if (!draftCloudModel) draftCloudModel = 'gpt-4o-mini'; }}
                  >
                    <div class="provider-card-top">
                      <Key size={18} class="provider-icon" />
                      <div class="provider-radio" class:active={draftProvider === 'openai'}>
                        {#if draftProvider === 'openai'}<Check size={10} />{/if}
                      </div>
                    </div>
                    <div class="provider-name">OpenAI</div>
                    <div class="provider-tag">GPT-4o Mini / 4o</div>
                  </button>
                </div>

                <!-- Test Connection Status Banner -->
                {#if testStatus}
                  <div class="test-feedback-banner" class:success={testStatus.success} class:error={!testStatus.success}>
                    {#if testStatus.success}
                      <CheckCircle2 size={15} />
                    {:else}
                      <AlertTriangle size={15} />
                    {/if}
                    <span>{testStatus.message}</span>
                  </div>
                {/if}

                <!-- Provider Config Details Box -->
                {#if draftProvider === 'ollama'}
                  <div class="config-box">
                    <div class="config-row-header">
                      <span class="config-label">Ollama Server Endpoint</span>
                      <span class="status-chip" class:online={aiStore.ollamaConnected} class:offline={!aiStore.ollamaConnected}>
                        {aiStore.ollamaConnected ? 'Server Online' : 'Disconnected'}
                      </span>
                    </div>

                    <div class="input-action-row">
                      <input
                        type="text"
                        class="mono-input"
                        bind:value={draftOllamaUrl}
                        placeholder="http://127.0.0.1:11434"
                      />
                      <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        onclick={handleTestConnection}
                        disabled={testLoading}
                      >
                        <RefreshCw size={12} class={testLoading ? 'animate-spin-slow' : ''} />
                        Test
                      </button>
                    </div>

                    <div class="form-group-compact">
                      <div class="compact-label-row">
                        <label for="ai-model-sel" class="config-label">Installed Model</label>
                        <button
                          type="button"
                          class="link-action-btn"
                          onclick={() => aiStore.checkOllamaStatus()}
                        >
                          <RefreshCw size={11} /> Refresh Models
                        </button>
                      </div>

                      {#if aiStore.availableModels.length > 0}
                        <select id="ai-model-sel" class="styled-select" bind:value={draftOllamaModel}>
                          {#each aiStore.availableModels as m}
                            <option value={m}>
                              {m} {m.includes('1b') || m.includes('0.5b') ? '⚡ (Fastest CPU)' : ''}
                            </option>
                          {/each}
                        </select>
                      {:else}
                        <input
                          id="ai-model-sel"
                          type="text"
                          class="mono-input"
                          bind:value={draftOllamaModel}
                          placeholder="llama3.2:1b"
                        />
                      {/if}
                    </div>
                  </div>

                {:else}
                  <div class="config-box">
                    <div class="form-group-compact">
                      <label for="ai-cloud-key" class="config-label">
                        {draftProvider === 'gemini' ? 'Google Gemini API Key' : 'OpenAI API Key'}
                      </label>
                      <div class="input-action-row">
                        <input
                          id="ai-cloud-key"
                          type={showKey ? 'text' : 'password'}
                          class="mono-input"
                          bind:value={draftApiKey}
                          placeholder={draftProvider === 'gemini' ? 'AIzaSy...' : 'sk-proj-...'}
                        />
                        <button
                          type="button"
                          class="btn btn-secondary btn-icon-sm"
                          onclick={() => showKey = !showKey}
                        >
                          {#if showKey}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
                        </button>
                      </div>
                    </div>

                    <div class="form-group-compact">
                      <label for="ai-cloud-model" class="config-label">
                        Model Identifier (e.g. <code>{draftProvider === 'gemini' ? 'gemini-2.5-flash' : 'gpt-4o-mini'}</code>)
                      </label>
                      <input
                        id="ai-cloud-model"
                        type="text"
                        class="mono-input"
                        bind:value={draftCloudModel}
                        placeholder={draftProvider === 'gemini' ? 'gemini-2.5-flash' : 'gpt-4o-mini'}
                      />
                    </div>

                    <div class="btn-right-row">
                      <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        onclick={handleTestConnection}
                        disabled={testLoading || !draftApiKey.trim()}
                      >
                        <RefreshCw size={12} class={testLoading ? 'animate-spin-slow' : ''} />
                        Test Connection
                      </button>
                    </div>
                  </div>
                {/if}

                <div class="save-row">
                  <button
                    type="button"
                    class="btn btn-primary"
                    onclick={handleSaveAiSettings}
                  >
                    <Save size={14} /> Save AI Configuration
                  </button>
                </div>
              {/if}
            </div>

          <!-- ── TAB 3: SYSTEM PREFERENCES ───────────────────────────────── -->
          {:else if activeCategoryTab === 'system'}
            <div class="settings-section">
              <p class="section-desc">Application storage, toast alerts, and global environment configuration.</p>

              <div class="pref-list">
                <div class="pref-item">
                  <div class="pref-icon-info">
                    <Bell size={18} class="pref-icon" />
                    <div>
                      <div class="pref-title">Toast Notifications</div>
                      <div class="pref-sub">Display non-blocking system status feedback alerts</div>
                    </div>
                  </div>
                  <span class="badge badge-success">Enabled</span>
                </div>

                <div class="pref-item">
                  <div class="pref-icon-info">
                    <HardDrive size={18} class="pref-icon" />
                    <div>
                      <div class="pref-title">Configuration Directory</div>
                      <div class="pref-sub font-mono">~/.config/linux-control-panel/</div>
                    </div>
                  </div>
                  <div class="pref-action">
                    <button
                      type="button"
                      class="btn btn-secondary btn-sm"
                      onclick={handleOpenConfigFile}
                      disabled={openingConfig}
                      title="Open global configuration file in text editor"
                    >
                      <FileText size={13} class={openingConfig ? 'animate-spin-slow' : ''} />
                      <span>{openingConfig ? 'Opening...' : 'Open Config'}</span>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          {/if}
        </div>

        <!-- Pane Bottom Action Bar -->
        <div class="content-footer">
          <button 
            class="btn btn-primary"
            onclick={() => uiStore.closeSettingsModal()}
          >
            Done
          </button>
        </div>
      </main>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99995;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .settings-modal {
    width: 1040px;
    height: 624px;
    max-width: min(1040px, calc(100vw - 32px));
    max-height: min(650px, calc(100vh - 40px));
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    overflow: hidden;
  }

  :global(html.light-mode) .settings-modal {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.15);
  }

  /* ── Sidebar Pane (Left) ─────────────────────────────────────────────────── */

  .sidebar-pane {
    width: 230px;
    background: rgba(0, 0, 0, 0.22);
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    padding: 20px 14px;
    box-sizing: border-box;
  }

  :global(html.light-mode) .sidebar-pane {
    background: #F8FAFC;
    border-right-color: #E2E8F0;
  }

  .sidebar-header {
    margin-bottom: 20px;
    padding-left: 8px;
  }

  .sidebar-title {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }

  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
  }

  .nav-tab-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
    width: 100%;
  }

  .nav-tab-btn:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .nav-tab-btn:hover {
    background: #E2E8F0;
  }

  .nav-tab-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
    color: var(--color-accent);
    font-weight: 600;
  }

  :global(html.light-mode) .nav-tab-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
    color: var(--color-accent);
  }

  .nav-label {
    flex: 1;
  }

  .nav-badge {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 10px;
  }

  .nav-badge.active {
    background: rgba(34, 197, 94, 0.2);
    color: var(--color-success);
  }

  .nav-badge.muted {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
  }

  .sidebar-footer {
    padding-left: 8px;
  }

  .version-tag {
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
  }

  /* ── Content Pane (Right) ────────────────────────────────────────────────── */

  .content-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: transparent;
  }

  .content-top-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 24px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  :global(html.light-mode) .content-top-bar {
    border-bottom-color: #E2E8F0;
  }

  .pane-section-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
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
    transition: all 0.15s;
  }

  .close-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .content-body {
    flex: 1;
    padding: 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .content-footer {
    padding: 14px 24px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.1);
    display: flex;
    justify-content: flex-end;
    flex-shrink: 0;
  }

  :global(html.light-mode) .content-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  /* ── Appearance Tab ──────────────────────────────────────────────────────── */

  .section-desc {
    margin: 0 0 16px 0;
    font-size: 12px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 14px;
  }

  .theme-card {
    background: rgba(0, 0, 0, 0.2);
    border: 2px solid var(--color-border);
    border-radius: 12px;
    padding: 12px;
    cursor: pointer;
    text-align: left;
    transition: all 0.2s ease;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  :global(html.light-mode) .theme-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .theme-card:hover {
    border-color: var(--color-accent);
  }

  .theme-card.selected {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  :global(html.light-mode) .theme-card.selected {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .theme-preview {
    height: 84px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .dark-preview { background: #0b1726; }
  .light-preview { background: #F8FAFC; border-color: #E2E8F0; }

  .preview-header {
    height: 16px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(0,0,0,0.3);
  }

  .light-preview .preview-header { background: #E2E8F0; }

  .preview-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
  }

  .dot-red { background: #ef4444; }
  .dot-yellow { background: #f59e0b; }
  .dot-green { background: #10b981; }

  .preview-body {
    flex: 1;
    display: flex;
    padding: 6px;
    gap: 6px;
  }

  .preview-sidebar {
    width: 20px;
    border-radius: 4px;
    background: rgba(255,255,255,0.05);
  }

  .light-preview .preview-sidebar { background: #CBD5E1; }

  .preview-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .preview-card-shape {
    width: 85%;
    height: 70%;
    border-radius: 5px;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dark-card { background: #152336; border: 1px solid rgba(255,255,255,0.1); }
  .light-card { background: #FFFFFF; border: 1px solid #CBD5E1; }

  .preview-line { height: 3px; border-radius: 2px; }
  .line-accent { background: var(--color-accent); width: 60%; }
  .line-text { background: rgba(255,255,255,0.2); width: 85%; }
  .line-accent-light { background: var(--color-accent); width: 60%; }
  .line-text-light { background: #94A3B8; width: 85%; }

  .theme-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .theme-card-info {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .theme-type-icon { color: var(--color-accent); }
  .theme-type-icon.sun { color: #f59e0b; }

  .theme-card-name {
    display: block;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .theme-card-desc {
    display: block;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .select-indicator {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid var(--color-border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #FFFFFF;
  }

  .select-indicator.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  :global(html.light-mode) .select-indicator.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  /* ── Accent Color Palette & OLED Mode ── */
  .accent-color-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .accent-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
    min-height: 40px;
    box-sizing: border-box;
  }

  :global(html.light-mode) .accent-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .accent-card:hover {
    border-color: var(--color-accent);
    background: rgba(255, 255, 255, 0.03);
    transform: translateY(-1px);
  }

  :global(html.light-mode) .accent-card:hover {
    background: #F1F5F9;
  }

  .accent-card.active {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  :global(html.light-mode) .accent-card.active {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .accent-swatch {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .accent-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  .accent-name {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .accent-sub {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .oled-toggle-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.15s ease;
    gap: 16px;
  }

  :global(html.light-mode) .oled-toggle-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .oled-toggle-card.enabled {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  :global(html.light-mode) .oled-toggle-card.enabled {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .oled-card-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .oled-icon-box {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    border: 1px solid var(--color-accent-glow);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
    transition: all 0.2s ease;
  }

  :global(html.light-mode) .oled-icon-box {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
    color: var(--color-accent);
  }

  .oled-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .oled-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }

  /* ── AI Engine Tab ───────────────────────────────────────────────────────── */

  .master-ai-card {
    padding: 14px 16px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.2s ease;
    margin-bottom: 16px;
  }

  .master-ai-card.enabled {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
  }

  :global(html.light-mode) .master-ai-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  :global(html.light-mode) .master-ai-card.enabled {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
  }

  .master-ai-info {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .master-ai-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
  }

  .master-ai-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .master-ai-sub {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 1px;
  }

  .sub-header-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-bottom: 10px;
  }

  .provider-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    margin-bottom: 16px;
  }

  .provider-card {
    padding: 12px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
    cursor: pointer;
    text-align: left;
    transition: all 0.2s ease;
  }

  :global(html.light-mode) .provider-card {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .provider-card:hover {
    border-color: var(--color-accent);
  }

  .provider-card.selected {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  :global(html.light-mode) .provider-card.selected {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .provider-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .provider-icon {
    color: var(--color-accent);
  }

  .provider-radio {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1px solid var(--color-border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #FFFFFF;
  }

  .provider-radio.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  :global(html.light-mode) .provider-radio.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  .provider-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .provider-tag {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }

  .test-feedback-banner {
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
  }

  .test-feedback-banner.success {
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: var(--color-success);
  }

  .test-feedback-banner.error {
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: var(--color-error);
  }

  .config-box {
    background: rgba(0, 0, 0, 0.15);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 16px;
  }

  :global(html.light-mode) .config-box {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .config-row-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .config-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .status-chip {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 10px;
  }

  .status-chip.online {
    background: rgba(34, 197, 94, 0.15);
    color: var(--color-success);
  }

  .status-chip.offline {
    background: rgba(239, 68, 68, 0.15);
    color: var(--color-error);
  }

  .input-action-row {
    display: flex;
    gap: 8px;
  }

  .mono-input {
    flex: 1;
    padding: 8px 12px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    outline: none;
  }

  .mono-input:focus {
    border-color: var(--color-accent);
  }

  .form-group-compact {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .compact-label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .link-action-btn {
    background: transparent;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font-size: 11px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
  }

  .link-action-btn:hover {
    text-decoration: underline;
  }

  .styled-select {
    padding: 8px 12px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-primary);
    font-size: 12px;
    outline: none;
  }

  .styled-select:focus {
    border-color: var(--color-accent);
  }

  .btn-right-row {
    display: flex;
    justify-content: flex-end;
  }

  .save-row {
    display: flex;
    justify-content: flex-end;
  }

  /* ── Preferences Tab ─────────────────────────────────────────────────────── */

  .pref-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .pref-item {
    padding: 14px 16px;
    background: rgba(0, 0, 0, 0.15);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  :global(html.light-mode) .pref-item {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .pref-icon-info {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .pref-icon {
    color: var(--color-text-secondary);
    flex-shrink: 0;
  }

  .pref-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .pref-sub {
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin-top: 1px;
  }

  /* ── UI Buttons & Helpers ────────────────────────────────────────────────── */

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }

  .btn-primary {
    background: var(--color-accent);
    color: #FFFFFF;
  }

  .btn-primary:hover {
    filter: brightness(1.1);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
  }

  .btn-secondary:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .btn-sm {
    padding: 5px 10px;
    font-size: 11.5px;
  }

  .btn-icon-sm {
    padding: 6px 10px;
  }

  .badge {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 6px;
  }

  .badge-success {
    background: rgba(34, 197, 94, 0.15);
    color: var(--color-success);
  }

  .font-mono {
    font-family: var(--font-mono, monospace);
  }

  .animate-spin-slow {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
