<script lang="ts">
  import Sidebar from './lib/components/Sidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import ToastContainer from './lib/components/ToastContainer.svelte';
  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import SettingsModal from './lib/components/SettingsModal.svelte';

  import DnfGlobalUpgradeWidget from './lib/components/DnfGlobalUpgradeWidget.svelte';
  import AiUniversalModal from './lib/components/AiUniversalModal.svelte';
  import GlobalSearchModal from './lib/components/GlobalSearchModal.svelte';

  import { onMount } from 'svelte';
  import { uiStore } from './lib/stores/ui.svelte.ts';
  import { dnfStore } from './lib/stores/dnfStore.svelte.ts';

  onMount(() => {
    uiStore.initTheme();
    uiStore.initSearchHistory();
    uiStore.initVisibilityListener();
    dnfStore.initGlobalListeners();
    dnfStore.checkLockStatus();

    function handleGlobalKeyDown(e: KeyboardEvent) {
      if (e.key === 'Backspace' || (e.altKey && e.key === 'ArrowLeft')) {
        const target = e.target as HTMLElement | null;
        if (target) {
          const tag = target.tagName.toLowerCase();
          if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
          if (target.isContentEditable || target.closest('.cm-editor') || target.closest('[contenteditable="true"]')) return;
        }

        if (uiStore.canGoBack) {
          e.preventDefault();
          uiStore.goBack();
        }
      }
    }

    function handleGlobalMouseUp(e: MouseEvent) {
      // Mouse button 3 is hardware Back button
      if (e.button === 3 && uiStore.canGoBack) {
        e.preventDefault();
        uiStore.goBack();
      }
    }

    window.addEventListener('keydown', handleGlobalKeyDown);
    window.addEventListener('mouseup', handleGlobalMouseUp);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeyDown);
      window.removeEventListener('mouseup', handleGlobalMouseUp);
    };
  });
</script>

<svelte:head>
  <title>Control Panel — Linux System Manager</title>
  <meta name="description" content="A comprehensive Linux system management desktop application" />
</svelte:head>

<div class="app-shell">
  <Sidebar />

  <main class="main-content">
    <div class="content-area">
      {#key uiStore.activeTab}
        {#if uiStore.activeTab === 'system-dashboard'}
          {#await import('./lib/modules/Dashboard.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Dashboard…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'system-monitor'}
          {#await import('./lib/modules/SystemMonitor.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Monitoring…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'journal-logs'}
          {#await import('./lib/modules/JournalViewer.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Journal Logs…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'repo-manager'}
          {#await import('./lib/modules/RepoManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Repositories…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'dnf-history'}
          {#await import('./lib/modules/DnfHistory.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading DNF Packages…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'copr-browser'}
          {#await import('./lib/modules/CoprBrowser.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading COPR Repositories…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'service-manager'}
          {#await import('./lib/modules/ServiceManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Service Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'hosts-manager'}
          {#await import('./lib/modules/HostsManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Hosts Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'user-manager'}
          {#await import('./lib/modules/UserManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Users & Groups…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'firewall-manager'}
          {#await import('./lib/modules/FirewallManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Firewall Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'grub-manager'}
          {#await import('./lib/modules/GrubManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading GRUB Bootloader…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'selinux-manager'}
          {#await import('./lib/modules/SelinuxManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading SELinux Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'cron-manager'}
          {#await import('./lib/modules/CronManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Scheduled Tasks…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'env-manager'}
          {#await import('./lib/modules/EnvManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Environment…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'nginx-manager'}
          {#await import('./lib/modules/NginxManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Nginx Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'shell-env'}
          {#await import('./lib/modules/ShellEnv.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Shell Environment…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'security-auditor'}
          {#await import('./lib/modules/SecurityAuditor.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Security Auditor…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'ssh-cert-manager'}
          {#await import('./lib/modules/SshCertManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading SSH & SSL Vault…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'network-manager'}
          {#await import('./lib/modules/NetworkManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Network Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'device-manager'}
          {#await import('./lib/modules/DeviceManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading Device Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'app-manager'}
          {#await import('./lib/modules/AppManager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading App Manager…</span>
            </div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'pm2-manager'}
          {#await import('./lib/modules/Pm2Manager.svelte')}
            <div class="module-lazy-loader">
              <div class="module-lazy-spinner"></div>
              <span>Loading PM2 Process Manager…</span>
            </div>
          {:then module}
            <module.default />
          {:catch error}
            <div class="module-lazy-loader">
              <span style="color: var(--color-error, #f43f5e);">Failed to load PM2 module: {error?.message || error}</span>
            </div>
          {/await}
        {/if}
      {/key}
    </div>
    <StatusBar />
  </main>
</div>

<ToastContainer />
<ConfirmDialog />
<SettingsModal />
<DnfGlobalUpgradeWidget />
<AiUniversalModal />
<GlobalSearchModal />

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--color-bg-base);
  }

  .main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  .content-area {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .content-area :global(.module-page) {
    animation: fadeSlideIn 0.18s ease both;
  }

  .module-lazy-loader {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 280px;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
  }

  .module-lazy-spinner {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid rgba(0, 218, 243, 0.15);
    border-top-color: var(--color-accent);
    animation: spin 0.7s linear infinite;
  }
</style>
