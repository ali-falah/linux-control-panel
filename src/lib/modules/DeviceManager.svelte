<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { HardDrive, Cpu, RefreshCw, Monitor, Network, MemoryStick, Usb, Settings } from '@lucide/svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  let rawData = $state<any>(null);
  let devicesByClass = $state<Record<string, any[]>>({});
  let loading = $state(true);
  let expandedClasses = $state<Record<string, boolean>>({});
  let searchQuery = $state('');

  let filteredDevices = $derived.by(() => {
    if (!searchQuery) return devicesByClass;
    const lowerQ = searchQuery.toLowerCase();
    const result: Record<string, any[]> = {};
    for (const [cls, devices] of Object.entries(devicesByClass)) {
      const matches = devices.filter((d: any) => 
        (d.product && d.product.toLowerCase().includes(lowerQ)) ||
        (d.vendor && d.vendor.toLowerCase().includes(lowerQ)) ||
        (d.description && d.description.toLowerCase().includes(lowerQ)) ||
        (cls.toLowerCase().includes(lowerQ))
      );
      if (matches.length > 0) {
        result[cls] = matches;
      }
    }
    return result;
  });

  function toggleAll(expand: boolean) {
    const nextState = { ...expandedClasses };
    for (const cls of Object.keys(devicesByClass)) {
      nextState[cls] = expand;
    }
    expandedClasses = nextState;
  }

  function flattenLshw(node: any, result: any[] = []) {
    if (node.class && node.id && node.class !== 'system' && node.class !== 'bus') {
      result.push(node);
    }
    if (node.children && Array.isArray(node.children)) {
      for (const child of node.children) {
        flattenLshw(child, result);
      }
    }
    return result;
  }

  async function loadData() {
    loading = true;
    statusStore.setBusy('Scanning system devices…');
    try {
      const jsonStr: string = await invoke('device_get_all');
      rawData = JSON.parse(jsonStr);
      
      const allDevices = flattenLshw(rawData);
      
      const grouped: Record<string, any[]> = {};
      for (const dev of allDevices) {
        const cls = dev.class || 'unknown';
        if (!grouped[cls]) {
          grouped[cls] = [];
        }
        grouped[cls].push(dev);
      }
      
      // Sort keys alphabetically
      const sortedKeys = Object.keys(grouped).sort();
      const finalGrouped: Record<string, any[]> = {};
      for (const k of sortedKeys) {
        finalGrouped[k] = grouped[k];
        expandedClasses[k] = true; // Expand all by default
      }
      
      devicesByClass = finalGrouped;
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function getIconForClass(cls: string) {
    switch (cls) {
      case 'processor': return Cpu;
      case 'memory': return MemoryStick;
      case 'storage':
      case 'disk':
      case 'volume': return HardDrive;
      case 'network': return Network;
      case 'display': return Monitor;
      case 'usb': return Usb;
      default: return Settings;
    }
  }

  function formatSize(bytes: number) {
    if (!bytes) return 'Unknown';
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    if (bytes === 0) return '0 Byte';
    const i = parseInt(Math.floor(Math.log(bytes) / Math.log(1024)).toString());
    return Math.round((bytes / Math.pow(1024, i)) * 100) / 100 + ' ' + sizes[i];
  }

  $effect(() => {
    loadData();
  });
</script>

<div class="module-page">
  <PageHeader title="Device Manager" icon={HardDrive} description="Hardware inventory and system devices" />

  <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom: 16px; flex-shrink: 0; flex-wrap: wrap; gap: 8px;">
    <div style="display:flex; align-items:center; gap:8px;">
      <input 
        type="text" 
        class="input" 
        placeholder="Search devices..." 
        bind:value={searchQuery}
        style="width: 250px; padding: 6px 12px;"
      />
      <button class="btn btn-outline" style="padding: 6px 10px; font-size:12px;" onclick={() => toggleAll(true)}>Expand All</button>
      <button class="btn btn-outline" style="padding: 6px 10px; font-size:12px;" onclick={() => toggleAll(false)}>Collapse All</button>
    </div>
    <button class="btn btn-outline" style="padding: 6px 12px; font-size:12px; display:flex; align-items:center; gap:6px" onclick={loadData}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Rescan Devices
    </button>
  </div>

  <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">
    {#if loading}
      <div class="card" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted)">
        <RefreshCw size={24} class="animate-spin-slow" />
      </div>
    {:else}
      {#each Object.entries(filteredDevices) as [cls, devices]}
        {@const Icon = getIconForClass(cls)}
        <div class="card" style="padding:0; overflow:hidden; flex-shrink:0;">
          <button 
            class="device-header" 
            onclick={() => expandedClasses[cls] = !expandedClasses[cls]}
          >
            <div style="display:flex; align-items:center; gap:8px;">
              <Icon size={18} style="color:var(--color-accent)" />
              <span style="font-weight:600; text-transform:capitalize; font-size:15px; color:var(--color-text-primary)">
                {cls}
              </span>
              <span class="badge badge-muted">{devices.length}</span>
            </div>
            <div style="color:var(--color-text-muted); transition:transform 0.2s" style:transform={expandedClasses[cls] ? 'rotate(180deg)' : 'rotate(0deg)'}>
              ▼
            </div>
          </button>
          
          {#if expandedClasses[cls]}
            <div style="padding: 12px 16px; display:flex; flex-direction:column; gap:8px; background:rgba(0,0,0,0.1);">
              {#each devices as dev}
                <div style="background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
                  <div style="display:flex; justify-content:space-between; margin-bottom:4px;">
                    <span style="font-weight:600; color:var(--color-text-primary); font-size:14px;">
                      {dev.product || dev.description || dev.id}
                    </span>
                    {#if dev.vendor}
                      <span style="font-size:12px; color:var(--color-text-muted);">{dev.vendor}</span>
                    {/if}
                  </div>
                  
                  <div style="display:grid; grid-template-columns: 1fr 1fr; gap:8px; font-size:12px; color:var(--color-text-secondary); margin-top:8px;">
                    {#if dev.logicalname}
                      <div style="display:flex; gap:6px;">
                        <span style="color:var(--color-text-muted)">Logical:</span>
                        <code style="color:var(--color-info)">{Array.isArray(dev.logicalname) ? dev.logicalname.join(', ') : dev.logicalname}</code>
                      </div>
                    {/if}
                    {#if dev.size || dev.capacity}
                      <div style="display:flex; gap:6px;">
                        <span style="color:var(--color-text-muted)">Size/Capacity:</span>
                        <span style="color:var(--color-text-primary)">{formatSize(dev.size || dev.capacity)}</span>
                      </div>
                    {/if}
                    {#if dev.clock}
                      <div style="display:flex; gap:6px;">
                        <span style="color:var(--color-text-muted)">Clock:</span>
                        <span style="color:var(--color-text-primary)">{formatSize(dev.clock).replace('Bytes', 'Hz').replace('B', 'Hz')}</span>
                      </div>
                    {/if}
                    {#if dev.width}
                      <div style="display:flex; gap:6px;">
                        <span style="color:var(--color-text-muted)">Width:</span>
                        <span style="color:var(--color-text-primary)">{dev.width} bits</span>
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .device-header {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    font-family: var(--font-sans);
    transition: background 0.15s;
  }
  .device-header:hover {
    background: rgba(255,255,255,0.02);
  }
  .device-header:last-child {
    border-bottom: none;
  }
</style>
