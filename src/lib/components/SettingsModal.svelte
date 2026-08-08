<script lang="ts">
  import { Settings, X, Sun, Moon, Check, Monitor, Sparkles, Bot, Cpu, Globe, Key, RefreshCw, Eye, EyeOff, Save, Sliders, Shield, Bell, HardDrive, AlertTriangle, FileText } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Toggle from './ui/Toggle.svelte';
  import Button from './ui/Button.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';

  let activeCategoryTab = $state<'appearance' | 'ai' | 'system'>('appearance');
  let showKey = $state(false);
  let openingConfig = $state(false);

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
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if uiStore.settingsModalOpen}
  <div 
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
      <!-- Header -->
      <div class="modal-header">
        <div class="header-title-group">
          <div class="header-icon">
            <Settings size={20} />
          </div>
          <div>
            <h2 id="settings-modal-title" class="modal-title">Control Panel Settings</h2>
            <p class="modal-subtitle">Manage appearance, AI engine configuration, and system preferences</p>
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

      <!-- Navigation Category Tabs -->
      <div class="settings-tabs-nav">
        <button
          type="button"
          class="settings-tab-btn"
          class:active={activeCategoryTab === 'appearance'}
          onclick={() => activeCategoryTab = 'appearance'}
        >
          <Sparkles size={15} />
          <span>Appearance &amp; Theme</span>
        </button>

        <button
          type="button"
          class="settings-tab-btn"
          class:active={activeCategoryTab === 'ai'}
          onclick={() => activeCategoryTab = 'ai'}
        >
          <Bot size={15} />
          <span>AI Assistant &amp; Engine</span>
          {#if !aiStore.enabled}
            <span class="badge badge-muted" style="font-size: 10px; margin-left: 4px;">Disabled</span>
          {/if}
        </button>

        <button
          type="button"
          class="settings-tab-btn"
          class:active={activeCategoryTab === 'system'}
          onclick={() => activeCategoryTab = 'system'}
        >
          <Sliders size={15} />
          <span>System &amp; Preferences</span>
        </button>
      </div>

      <!-- Content Body -->
      <div class="modal-body">
        <!-- ── TAB 1: APPEARANCE ────────────────────────────────────────── -->
        {#if activeCategoryTab === 'appearance'}
          <div class="settings-section">
            <div class="section-title-row">
              <Sparkles size={16} class="section-icon" />
              <h3 class="section-title">Theme Mode</h3>
            </div>
            <p class="section-desc">Select your preferred color scheme for optimal visibility and readability.</p>

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
                    <Sun size={16} class="theme-type-icon sun" />
                    <div>
                      <span class="theme-card-name">Modern Light System</span>
                      <span class="theme-card-desc">Clean Slate-50 canvas &amp; Royal Blue accent</span>
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

        <!-- ── TAB 2: AI ASSISTANT & ENGINE ──────────────────────────────── -->
        {:else if activeCategoryTab === 'ai'}
          <div class="settings-section">
            <!-- Master AI Enable Card -->
            <div 
              class="master-ai-card" 
              onclick={() => { 
                draftEnabled = !draftEnabled; 
                aiStore.enabled = draftEnabled; 
                aiStore.saveSettings(); 
              }}
              style="padding: 16px; background: rgba(0, 218, 243, 0.06); border: 1px solid rgba(0, 218, 243, 0.2); border-radius: 12px; margin-bottom: 20px; display: flex; align-items: center; justify-content: space-between; cursor: pointer;"
            >
              <div style="display: flex; align-items: center; gap: 12px;">
                <div style="width: 40px; height: 40px; border-radius: 10px; background: rgba(0, 218, 243, 0.12); display: flex; align-items: center; justify-content: center;">
                  <Bot size={22} style="color: var(--color-accent);" />
                </div>
                <div>
                  <div style="font-size: 14px; font-weight: 600; color: var(--color-text-primary);">Enable AI Features Across Control Panel</div>
                  <div style="font-size: 12px; color: var(--color-text-muted);">
                    {draftEnabled ? 'AI diagnosis and rule generation buttons are enabled across all modules.' : 'AI features are completely disabled throughout the application.'}
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
              <div class="section-title-row" style="margin-bottom: 6px;">
                <Bot size={16} class="section-icon" style="color: var(--color-accent);" />
                <h3 class="section-title">Model Provider &amp; API Configuration</h3>
              </div>
              <p class="section-desc" style="font-size: 12px; color: var(--color-text-muted); margin-bottom: 16px;">
                Select your provider below. Click <strong>Test Connection</strong> to verify, then click <strong>Save AI Settings</strong> to persist.
              </p>

              <!-- Provider Selection Grid -->
              <div class="ai-provider-grid" style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 16px;">
                <!-- Local Ollama Card -->
                <button
                  type="button"
                  class="provider-card"
                  class:selected={draftProvider === 'ollama'}
                  onclick={() => { draftProvider = 'ollama'; }}
                  style="padding: 12px; border-radius: 10px; border: 1px solid var(--color-border); background: rgba(0,0,0,0.15); cursor: pointer; text-align: left; transition: all 0.2s;"
                >
                  <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px;">
                    <Cpu size={18} style="color: var(--color-accent);" />
                    {#if draftProvider === 'ollama'}
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: var(--color-accent);"></span>
                    {/if}
                  </div>
                  <div style="font-size: 13px; font-weight: 600; color: var(--color-text-primary);">Local Ollama</div>
                  <div style="font-size: 11px; color: var(--color-text-muted);">100% Offline &amp; Free</div>
                </button>

                <!-- Google Gemini Card -->
                <button
                  type="button"
                  class="provider-card"
                  class:selected={draftProvider === 'gemini'}
                  onclick={() => { draftProvider = 'gemini'; draftCloudProvider = 'gemini'; if (!draftCloudModel) draftCloudModel = 'gemini-2.5-flash'; }}
                  style="padding: 12px; border-radius: 10px; border: 1px solid var(--color-border); background: rgba(0,0,0,0.15); cursor: pointer; text-align: left; transition: all 0.2s;"
                >
                  <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px;">
                    <Globe size={18} style="color: #4285F4;" />
                    {#if draftProvider === 'gemini'}
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: #4285F4;"></span>
                    {/if}
                  </div>
                  <div style="font-size: 13px; font-weight: 600; color: var(--color-text-primary);">Google Gemini API</div>
                  <div style="font-size: 11px; color: var(--color-text-muted);">gemini-2.5-flash / pro</div>
                </button>

                <!-- OpenAI Card -->
                <button
                  type="button"
                  class="provider-card"
                  class:selected={draftProvider === 'openai'}
                  onclick={() => { draftProvider = 'openai'; draftCloudProvider = 'openai'; if (!draftCloudModel) draftCloudModel = 'gpt-4o-mini'; }}
                  style="padding: 12px; border-radius: 10px; border: 1px solid var(--color-border); background: rgba(0,0,0,0.15); cursor: pointer; text-align: left; transition: all 0.2s;"
                >
                  <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px;">
                    <Key size={18} style="color: #10a37f;" />
                    {#if draftProvider === 'openai'}
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: #10a37f;"></span>
                    {/if}
                  </div>
                  <div style="font-size: 13px; font-weight: 600; color: var(--color-text-primary);">OpenAI API</div>
                  <div style="font-size: 11px; color: var(--color-text-muted);">gpt-4o-mini / gpt-4o</div>
                </button>
              </div>

              <!-- Connection Test Results Feedback Banner -->
              {#if testStatus}
                <div
                  style="padding: 10px 14px; border-radius: 8px; font-size: 12px; font-weight: 500; display: flex; align-items: center; gap: 8px; margin-bottom: 12px;
                  background: {testStatus.success ? 'rgba(16, 185, 129, 0.12)' : 'rgba(239, 68, 68, 0.12)'};
                  border: 1px solid {testStatus.success ? 'rgba(16, 185, 129, 0.3)' : 'rgba(239, 68, 68, 0.3)'};
                  color: {testStatus.success ? 'var(--color-success)' : 'var(--color-error)'};"
                >
                  {#if testStatus.success}
                    <Check size={15} />
                  {:else}
                    <AlertTriangle size={15} />
                  {/if}
                  <span>{testStatus.message}</span>
                </div>
              {/if}

              <!-- Provider Settings Box -->
              {#if draftProvider === 'ollama'}
                <div class="ollama-settings-box" style="padding: 14px; background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); border-radius: 10px; display: flex; flex-direction: column; gap: 12px;">
                  <div style="display: flex; justify-content: space-between; align-items: center;">
                    <span style="font-size: 12.5px; font-weight: 600; color: var(--color-text-primary);">Local Server Endpoint</span>
                    <span class="badge {aiStore.ollamaConnected ? 'badge-success' : 'badge-error'}" style="font-size: 11px;">
                      {aiStore.ollamaConnected ? `Ollama Server Online` : 'Disconnected'}
                    </span>
                  </div>
                  <div style="display: flex; gap: 8px;">
                    <input
                      type="text"
                      bind:value={draftOllamaUrl}
                      placeholder="http://127.0.0.1:11434"
                      style="flex: 1; padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12px; font-family: var(--font-mono);"
                    />
                    <button
                      type="button"
                      class="btn btn-outline btn-sm"
                      onclick={handleTestConnection}
                      disabled={testLoading}
                      style="padding: 6px 14px; font-size: 12px; display: flex; align-items: center; gap: 6px;"
                    >
                      <RefreshCw size={13} class={testLoading ? 'animate-spin-slow' : ''} /> Test Connection
                    </button>
                  </div>

                  <div style="display: flex; flex-direction: column; gap: 6px;">
                    <div style="display: flex; justify-content: space-between; align-items: center;">
                      <label for="ai-ollama-model-select" style="font-size: 11.5px; font-weight: 600; color: var(--color-text-primary);">
                        Installed Ollama Model
                      </label>
                      <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        onclick={() => aiStore.checkOllamaStatus()}
                        title="Query local Ollama server for newly pulled models"
                        style="font-size: 11px; padding: 2px 6px; display: flex; align-items: center; gap: 4px;"
                      >
                        <RefreshCw size={11} /> Refresh Installed Models
                      </button>
                    </div>

                    {#if aiStore.availableModels.length > 0}
                      <select
                        id="ai-ollama-model-select"
                        bind:value={draftOllamaModel}
                        style="padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12px; font-weight: 500;"
                      >
                        {#each aiStore.availableModels as m}
                          <option value={m}>
                            {m} {m.includes('1b') || m.includes('0.5b') ? '⚡ (Fastest CPU Model)' : ''}
                          </option>
                        {/each}
                      </select>
                    {:else}
                      <input
                        id="ai-ollama-model-input"
                        type="text"
                        bind:value={draftOllamaModel}
                        placeholder="llama3.2:1b"
                        style="padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12px; font-family: var(--font-mono);"
                      />
                    {/if}
                  </div>
                </div>

              {:else}
                <div class="cloud-settings-box" style="padding: 14px; background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); border-radius: 10px; display: flex; flex-direction: column; gap: 12px;">
                  <div style="display: flex; flex-direction: column; gap: 4px;">
                    <label for="ai-cloud-api-key-nav" style="font-size: 11.5px; font-weight: 600; color: var(--color-text-primary);">
                      {draftProvider === 'gemini' ? 'Google Gemini API Key' : 'OpenAI API Key'}
                    </label>
                    <div style="display: flex; gap: 8px;">
                      <input
                        id="ai-cloud-api-key-nav"
                        type={showKey ? 'text' : 'password'}
                        bind:value={draftApiKey}
                        placeholder={draftProvider === 'gemini' ? 'AIzaSy...' : 'sk-proj-...'}
                        style="flex: 1; padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12px; font-family: var(--font-mono);"
                      />
                      <button
                        type="button"
                        class="btn btn-outline btn-sm"
                        onclick={() => showKey = !showKey}
                        style="padding: 6px 10px;"
                      >
                        {#if showKey}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
                      </button>
                    </div>
                  </div>

                  <div style="display: flex; flex-direction: column; gap: 4px;">
                    <label for="ai-cloud-model-nav" style="font-size: 11.5px; color: var(--color-text-muted);">
                      Model Name (Recommended: <code>{draftProvider === 'gemini' ? 'gemini-2.5-flash' : 'gpt-4o-mini'}</code>, <code>{draftProvider === 'gemini' ? 'gemini-2.5-pro' : 'gpt-4o'}</code>)
                    </label>
                    <input
                      id="ai-cloud-model-nav"
                      type="text"
                      bind:value={draftCloudModel}
                      placeholder={draftProvider === 'gemini' ? 'gemini-2.5-flash' : 'gpt-4o-mini'}
                      style="padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12px; font-family: var(--font-mono);"
                    />
                  </div>

                  <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px;">
                    <button
                      type="button"
                      class="btn btn-outline btn-sm"
                      onclick={handleTestConnection}
                      disabled={testLoading || !draftApiKey.trim()}
                      style="padding: 6px 14px; font-size: 12px; display: flex; align-items: center; gap: 6px;"
                    >
                      <RefreshCw size={13} class={testLoading ? 'animate-spin-slow' : ''} /> Test Connection
                    </button>
                  </div>
                </div>
              {/if}

              <!-- Explicit Save Button Card -->
              <div style="display: flex; justify-content: flex-end; margin-top: 16px;">
                <button
                  type="button"
                  class="btn btn-primary"
                  onclick={handleSaveAiSettings}
                  style="padding: 8px 20px; font-size: 13px; font-weight: 600; display: flex; align-items: center; gap: 8px;"
                >
                  <Save size={15} /> Save AI Settings
                </button>
              </div>
            {/if}
          </div>

        <!-- ── TAB 3: SYSTEM PREFERENCES ───────────────────────────────────── -->
        {:else if activeCategoryTab === 'system'}
          <div class="settings-section">
            <div class="section-title-row">
              <Sliders size={16} class="section-icon" />
              <h3 class="section-title">System &amp; Application Preferences</h3>
            </div>
            <p class="section-desc">General application behaviors and system interface settings.</p>

            <div style="display: flex; flex-direction: column; gap: 14px;">
              <div style="padding: 14px; background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); border-radius: 10px; display: flex; align-items: center; justify-content: space-between;">
                <div style="display: flex; align-items: center; gap: 10px;">
                  <Bell size={18} style="color: var(--color-text-secondary);" />
                  <div>
                    <div style="font-size: 13px; font-weight: 600; color: var(--color-text-primary);">Toast Notifications</div>
                    <div style="font-size: 11.5px; color: var(--color-text-muted);">Show system alerts and status popups</div>
                  </div>
                </div>
                <span class="badge badge-success" style="font-size: 11px;">Active</span>
              </div>

              <div style="padding: 14px; background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); border-radius: 10px; display: flex; align-items: center; justify-content: space-between; gap: 12px;">
                <div style="display: flex; align-items: center; gap: 10px; min-width: 0;">
                  <HardDrive size={18} style="color: var(--color-text-secondary); flex-shrink: 0;" />
                  <div style="min-width: 0;">
                    <div style="font-size: 13px; font-weight: 600; color: var(--color-text-primary);">Config Path</div>
                    <div style="font-size: 11.5px; color: var(--color-text-muted); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">~/.config/linux-control-panel/</div>
                  </div>
                </div>
                <div style="display: flex; align-items: center; gap: 8px; flex-shrink: 0;">
                  <span class="badge badge-muted" style="font-size: 11px;">Default</span>
                  <button
                    type="button"
                    class="btn btn-outline btn-sm"
                    onclick={handleOpenConfigFile}
                    disabled={openingConfig}
                    style="padding: 4px 10px; font-size: 11.5px; display: inline-flex; align-items: center; gap: 6px;"
                    title="Open global configuration file in default system text editor"
                  >
                    <FileText size={13} class={openingConfig ? 'animate-spin-slow' : ''} />
                    <span>{openingConfig ? 'Opening...' : 'Open Config File'}</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        {/if}
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
    z-index: 2000;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .settings-modal {
    width: 680px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 40px);
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :global(html.light-mode) .settings-modal {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 25px 60px rgba(0, 0, 0, 0.15);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 24px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .modal-header {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .header-title-group {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .header-icon {
    width: 42px;
    height: 42px;
    border-radius: 12px;
    background: rgba(0, 218, 243, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
  }

  .modal-title {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .modal-subtitle {
    margin: 2px 0 0 0;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 8px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }
  .close-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.1);
  }

  .settings-tabs-nav {
    display: flex;
    gap: 8px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.12);
  }

  :global(html.light-mode) .settings-tabs-nav {
    background: #F1F5F9;
    border-bottom-color: #E2E8F0;
  }

  .settings-tab-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 16px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .settings-tab-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.06);
  }

  :global(html.light-mode) .settings-tab-btn:hover {
    background: #E2E8F0;
  }

  .settings-tab-btn.active {
    color: var(--color-accent);
    font-weight: 600;
    background: rgba(0, 218, 243, 0.12);
    border-color: rgba(0, 218, 243, 0.3);
  }

  :global(html.light-mode) .settings-tab-btn.active {
    background: #EFF6FF;
    border-color: #BFDBFE;
    color: #2563EB;
  }

  .modal-body {
    padding: 24px;
    overflow-y: auto;
    max-height: calc(80vh - 120px);
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
  }

  .section-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .section-icon {
    color: var(--color-accent);
  }

  .section-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .section-desc {
    margin: 0 0 16px 0;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 16px;
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
    border-color: rgba(0, 218, 243, 0.5);
  }

  .theme-card.selected {
    border-color: var(--color-accent);
    background: rgba(0, 218, 243, 0.05);
  }

  :global(html.light-mode) .theme-card.selected {
    border-color: #2563EB;
    background: #EFF6FF;
  }

  .theme-preview {
    height: 100px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .dark-preview {
    background: #0b1726;
  }

  .light-preview {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .preview-header {
    height: 18px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(0,0,0,0.3);
  }

  .light-preview .preview-header {
    background: #E2E8F0;
  }

  .preview-dot {
    width: 6px;
    height: 6px;
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
    width: 24px;
    border-radius: 4px;
    background: rgba(255,255,255,0.05);
  }

  .light-preview .preview-sidebar {
    background: #CBD5E1;
  }

  .preview-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .preview-card-shape {
    width: 80%;
    height: 70%;
    border-radius: 6px;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dark-card {
    background: #152336;
    border: 1px solid rgba(255,255,255,0.1);
  }

  .light-card {
    background: #FFFFFF;
    border: 1px solid #CBD5E1;
  }

  .preview-line {
    height: 4px;
    border-radius: 2px;
  }

  .line-accent { background: var(--color-accent); width: 60%; }
  .line-text { background: rgba(255,255,255,0.2); width: 90%; }
  .line-accent-light { background: #2563EB; width: 60%; }
  .line-text-light { background: #94A3B8; width: 90%; }

  .theme-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .theme-card-info {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .theme-type-icon {
    color: var(--color-accent);
  }

  .theme-type-icon.sun {
    color: #f59e0b;
  }

  .theme-card-name {
    display: block;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .theme-card-desc {
    display: block;
    font-size: 11px;
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
    color: #FFFFFF;
  }

  .select-indicator.active {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }

  :global(html.light-mode) .select-indicator.active {
    background: #2563EB;
    border-color: #2563EB;
  }

  .provider-card:hover {
    border-color: var(--color-accent) !important;
  }

  .modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 24px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .modal-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  .app-build-info {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }
</style>
