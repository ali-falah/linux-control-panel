<script lang="ts">
  import Sidebar from './lib/components/Sidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import ToastContainer from './lib/components/ToastContainer.svelte';
  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import SettingsModal from './lib/components/SettingsModal.svelte';
  import RepoManager from './lib/modules/RepoManager.svelte';
  import DnfHistory from './lib/modules/DnfHistory.svelte';
  import CoprBrowser from './lib/modules/CoprBrowser.svelte';
  import ServiceManager from './lib/modules/ServiceManager.svelte';
  import HostsManager from './lib/modules/HostsManager.svelte';
  import UserManager from './lib/modules/UserManager.svelte';
  import FirewallManager from './lib/modules/FirewallManager.svelte';
  import GrubManager from './lib/modules/GrubManager.svelte';
  import SelinuxManager from './lib/modules/SelinuxManager.svelte';
  import CronManager from './lib/modules/CronManager.svelte';
  import EnvManager from './lib/modules/EnvManager.svelte';

  import DnfGlobalUpgradeWidget from './lib/components/DnfGlobalUpgradeWidget.svelte';

  import { onMount } from 'svelte';
  import { uiStore } from './lib/stores/ui.svelte.ts';
  import { dnfStore } from './lib/stores/dnfStore.svelte.ts';

  onMount(() => {
    uiStore.initTheme();
    dnfStore.initGlobalListeners();
    dnfStore.checkLockStatus();
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
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Dashboard...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'system-monitor'}
          {#await import('./lib/modules/SystemMonitor.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading System Monitor...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'journal-logs'}
          {#await import('./lib/modules/JournalViewer.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Journal Logs...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'repo-manager'}
          <RepoManager />
        {:else if uiStore.activeTab === 'dnf-history'}
          <DnfHistory />
        {:else if uiStore.activeTab === 'copr-browser'}
          <CoprBrowser />
        {:else if uiStore.activeTab === 'service-manager'}
          <ServiceManager />
        {:else if uiStore.activeTab === 'hosts-manager'}
          <HostsManager />
        {:else if uiStore.activeTab === 'user-manager'}
          <UserManager />
        {:else if uiStore.activeTab === 'firewall-manager'}
          <FirewallManager />
        {:else if uiStore.activeTab === 'grub-manager'}
          <GrubManager />
        {:else if uiStore.activeTab === 'selinux-manager'}
          <SelinuxManager />
        {:else if uiStore.activeTab === 'cron-manager'}
          <CronManager />
        {:else if uiStore.activeTab === 'env-manager'}
          <EnvManager />
        {:else if uiStore.activeTab === 'nginx-manager'}
          {#await import('./lib/modules/NginxManager.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Nginx Manager...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'shell-env'}
          {#await import('./lib/modules/ShellEnv.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Shell Environment...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'security-auditor'}
          {#await import('./lib/modules/SecurityAuditor.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Security Auditor...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'ssh-cert-manager'}
          {#await import('./lib/modules/SshCertManager.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading SSH & SSL Vault...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'network-manager'}
          {#await import('./lib/modules/NetworkManager.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Network Manager...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'device-manager'}
          {#await import('./lib/modules/DeviceManager.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading Device Manager...</div>
          {:then module}
            <module.default />
          {/await}
        {:else if uiStore.activeTab === 'app-manager'}
          {#await import('./lib/modules/AppManager.svelte')}
            <div style="display:flex; align-items:center; justify-content:center; height:250px; color:var(--color-text-muted); font-size:13px;">Loading App Manager...</div>
          {:then module}
            <module.default />
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
    animation: fadeSlideIn 0.25s ease both;
  }
</style>
