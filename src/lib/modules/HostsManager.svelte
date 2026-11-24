<script lang="ts">
  import { onMount } from 'svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import Select from '../components/ui/Select.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import ContextMenu, { type ContextMenuItem } from '../components/ui/ContextMenu.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Globe, RefreshCw, Plus, Trash2, Save, Search, Pencil, Power, Copy, Undo2,
    MoreHorizontal, AlertTriangle, CheckCircle2,
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface HostEntry {
    id: string;
    ip: string;
    hostnames: string[];
    comment: string;
    enabled: boolean;
    category: string;
    raw?: string | null;
  }

  type StatusFilter = 'all' | 'active' | 'disabled';

  let entries = $state<HostEntry[]>([]);
  let originalEntries = $state<HostEntry[]>([]);
  let originalContent = $state('');
  let loading = $state(true);
  let loadError = $state('');
  let hasChanges = $state(false);
  let saving = $state(false);
  let filter = $state('');
  let statusFilter = $state<StatusFilter>('all');
  let categoryFilter = $state('all');

  let drawerOpen = $state(false);
  let editingId = $state<string | null>(null);
  let formIp = $state('');
  let formHostnames = $state('');
  let formComment = $state('');
  let formCategory = $state('');
  let formEnabled = $state(true);
  let formError = $state('');

  let previewOpen = $state(false);
  let previewContent = $state('');
  let previewLoading = $state(false);

  let menu = $state<{ open: boolean; x: number; y: number; entry: HostEntry | null }>({
    open: false,
    x: 0,
    y: 0,
    entry: null,
  });

  const DEFAULT_CATEGORY_KEY = '__default__';

  function displayCategory(category: string): string {
    return category || 'Default';
  }

  function clone<T>(value: T): T {
    return JSON.parse(JSON.stringify(value));
  }

  let hostEntries = $derived(entries.filter((entry) => entry.ip));

  let categories = $derived.by(() => {
    const seen = new Set<string>();
    for (const entry of hostEntries) seen.add(entry.category);
    return [...seen].sort((a, b) => displayCategory(a).localeCompare(displayCategory(b)));
  });

  let activeCount = $derived(hostEntries.filter((entry) => entry.enabled).length);
  let disabledCount = $derived(hostEntries.length - activeCount);

  const statusOptions = $derived([
    { id: 'all', label: 'All', count: hostEntries.length },
    { id: 'active', label: 'Active', count: activeCount },
    { id: 'disabled', label: 'Disabled', count: disabledCount },
  ]);

  $effect(() => {
    if (
      categoryFilter !== 'all' &&
      categoryFilter !== DEFAULT_CATEGORY_KEY &&
      !categories.includes(categoryFilter)
    ) {
      categoryFilter = 'all';
    }
  });

  let filteredEntries = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return hostEntries.filter((entry) => {
      if (statusFilter === 'active' && !entry.enabled) return false;
      if (statusFilter === 'disabled' && entry.enabled) return false;
      if (categoryFilter !== 'all') {
        if (categoryFilter === DEFAULT_CATEGORY_KEY) {
          if (entry.category) return false;
        } else if (entry.category !== categoryFilter) {
          return false;
        }
      }
      if (!q) return true;
      return (
        entry.ip.toLowerCase().includes(q) ||
        entry.hostnames.some((hostname) => hostname.toLowerCase().includes(q)) ||
        entry.comment.toLowerCase().includes(q) ||
        displayCategory(entry.category).toLowerCase().includes(q)
      );
    });
  });

  let hasLocalhost = $derived(
    entries.some(
      (entry) =>
        entry.enabled &&
        (entry.ip === '127.0.0.1' || entry.ip === '::1') &&
        entry.hostnames.includes('localhost')
    )
  );

  let primaryDomain = $derived.by(() => {
    const entry = menu.entry;
    if (!entry) return '';
    return entry.hostnames.find((name) => !name.includes('localhost')) || entry.hostnames[0] || '';
  });

  let menuItems = $derived.by<ContextMenuItem[]>(() => {
    const entry = menu.entry;
    if (!entry) return [];
    const items: ContextMenuItem[] = [
      {
        label: 'Edit entry',
        icon: Pencil,
        action: () => openEdit(entry),
      },
      {
        label: entry.enabled ? 'Disable entry' : 'Enable entry',
        icon: Power,
        color: entry.enabled ? 'var(--color-warning)' : 'var(--color-success)',
        action: () => toggleEntry(entry),
      },
    ];

    const domain = entry.hostnames.find((name) => !name.includes('localhost'));
    if (domain) {
      items.push({
        label: 'Create Nginx VHost',
        icon: Globe,
        color: 'var(--color-accent)',
        action: () => createNginxVhostForDomain(domain, entry.ip),
      });
    }

    items.push(
      { label: '', divider: true },
      {
        label: 'Copy IP address',
        icon: Copy,
        action: () => copyText(entry.ip, 'IP address copied'),
      },
      {
        label: 'Copy hostnames',
        icon: Copy,
        action: () => copyText(entry.hostnames.join(' '), 'Hostnames copied'),
      },
      {
        label: 'Copy hosts line',
        icon: Copy,
        action: () => copyText(hostLine(entry), 'Hosts line copied'),
      },
      { label: '', divider: true },
      {
        label: 'Delete entry',
        icon: Trash2,
        danger: true,
        action: () => deleteEntry(entry),
      }
    );

    return items;
  });

  let duplicateWarning = $derived.by(() => {
    const names = formHostnames
      .trim()
      .split(/\s+/)
      .filter(Boolean);
    const ip = formIp.trim();
    if (names.length === 0 || !ip) return '';
    const conflict = entries.find(
      (entry) =>
        entry.id !== editingId &&
        entry.ip &&
        entry.ip !== ip &&
        entry.hostnames.some((name) => names.includes(name))
    );
    if (!conflict) return '';
    const shared = conflict.hostnames.find((name) => names.includes(name));
    return `${shared} already maps to ${conflict.ip}. /etc/hosts resolves the first matching line.`;
  });

  function hostLine(entry: HostEntry): string {
    const comment = entry.comment ? ` # ${entry.comment}` : '';
    const prefix = entry.enabled ? '' : '#';
    return `${prefix}${entry.ip}\t${entry.hostnames.join(' ')}${comment}`;
  }

  function newId(): string {
    return `entry-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  function isIpValid(value: string): boolean {
    const ip = value.trim();
    if (/^\d{1,3}(\.\d{1,3}){3}$/.test(ip)) {
      return ip.split('.').every((part) => Number(part) <= 255);
    }
    return ip.includes(':') && /^[0-9a-fA-F:.]+$/.test(ip);
  }

  async function load() {
    loading = true;
    loadError = '';
    statusStore.setBusy('Reading /etc/hosts…');
    try {
      const [data, raw] = await Promise.all([
        invoke<HostEntry[]>('read_hosts'),
        invoke<string>('read_hosts_raw'),
      ]);
      entries = data;
      originalEntries = clone(data);
      originalContent = raw;
      hasChanges = false;
      statusStore.setLastCommand('cat /etc/hosts', 0, true);
    } catch (e) {
      console.error(e);
      loadError = String(e);
      statusStore.setLastCommand('cat /etc/hosts', 1, false);
      uiStore.addToast(`Failed to load hosts: ${e}`, 'error');
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function confirmReload() {
    if (!hasChanges) {
      void load();
      return;
    }
    uiStore.confirm(
      'Discard unsaved changes?',
      'Reloading /etc/hosts will discard the edits you have not saved yet.',
      () => load(),
      true
    );
  }

  function discardChanges() {
    uiStore.confirm(
      'Discard unsaved changes?',
      'All edits since the last save will be lost.',
      () => {
        entries = clone(originalEntries);
        hasChanges = false;
        drawerOpen = false;
      },
      true
    );
  }

  function openAdd() {
    editingId = null;
    formIp = '';
    formHostnames = '';
    formComment = '';
    formCategory =
      categoryFilter !== 'all' && categoryFilter !== DEFAULT_CATEGORY_KEY
        ? categoryFilter
        : 'Custom';
    formEnabled = true;
    formError = '';
    drawerOpen = true;
  }

  function openEdit(entry: HostEntry) {
    editingId = entry.id;
    formIp = entry.ip;
    formHostnames = entry.hostnames.join(' ');
    formComment = entry.comment;
    formCategory = entry.category;
    formEnabled = entry.enabled;
    formError = '';
    drawerOpen = true;
  }

  function saveForm() {
    const ip = formIp.trim();
    const hostnames = formHostnames.trim().split(/\s+/).filter(Boolean);
    const comment = formComment.trim();
    const category = formCategory.trim();

    if (!isIpValid(ip)) {
      formError = 'Enter a valid IPv4 or IPv6 address.';
      return;
    }
    if (hostnames.length === 0) {
      formError = 'Enter at least one hostname.';
      return;
    }
    if (hostnames.some((hostname) => hostname.includes('#'))) {
      formError = 'Hostnames cannot contain #; use the comment field instead.';
      return;
    }
    if (!editingId && !category) {
      formError = 'Choose or enter a category for the new entry.';
      return;
    }
    const categoryFirst = category.charAt(0);
    if (
      category &&
      (categoryFirst !== categoryFirst.toUpperCase() || categoryFirst === categoryFirst.toLowerCase())
    ) {
      formError = 'Category names must start with a capital letter.';
      return;
    }

    if (editingId) {
      const entry = entries.find((candidate) => candidate.id === editingId);
      if (!entry) {
        formError = 'This entry no longer exists. Close the drawer and reload.';
        return;
      }
      entry.ip = ip;
      entry.hostnames = hostnames;
      entry.comment = comment;
      entry.category = category;
      entry.enabled = formEnabled;
      entry.raw = null;
      entries = [...entries];
    } else {
      entries = [
        ...entries,
        {
          id: newId(),
          ip,
          hostnames,
          comment,
          enabled: formEnabled,
          category,
          raw: null,
        },
      ];
    }
    hasChanges = true;
    drawerOpen = false;
  }

  function toggleEntry(entry: HostEntry) {
    entry.enabled = !entry.enabled;
    entries = [...entries];
    hasChanges = true;
  }

  function deleteEntry(entry: HostEntry) {
    let warning = `Remove entry for ${entry.ip} → ${entry.hostnames.join(', ')}?`;
    if (entry.ip === '127.0.0.1' || entry.ip === '::1' || entry.hostnames.includes('localhost')) {
      warning += '\n\nWARNING: Deleting localhost entries can severely break system networking!';
    }
    uiStore.confirm(
      'Remove Host Entry',
      warning,
      () => {
        if (editingId === entry.id) drawerOpen = false;
        entries = entries.filter((candidate) => candidate.id !== entry.id);
        hasChanges = true;
      },
      true
    );
  }

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast(label, 'info');
    } catch (e) {
      uiStore.addToast(`Could not copy: ${e}`, 'error');
    }
  }

  function menuPoint(e: MouseEvent): { x: number; y: number } {
    const fromKeyboard = e.type === 'click' && e.clientX === 0 && e.clientY === 0;
    const rect = fromKeyboard
      ? (e.currentTarget as HTMLElement | null)?.getBoundingClientRect()
      : null;
    return rect ? { x: rect.left, y: rect.bottom + 4 } : { x: e.clientX, y: e.clientY };
  }

  function openRowMenu(e: MouseEvent, entry: HostEntry) {
    e.preventDefault();
    e.stopPropagation();
    menu = { ...menuPoint(e), open: true, entry };
  }

  async function openPreview() {
    if (!hasChanges || previewLoading) return;
    previewLoading = true;
    try {
      previewContent = await invoke<string>('preview_hosts', { entries });
      previewOpen = true;
    } catch (e) {
      uiStore.addToast(`Could not prepare the changes: ${e}`, 'error');
    } finally {
      previewLoading = false;
    }
  }

  async function save() {
    saving = true;
    statusStore.setBusy('Writing /etc/hosts…');
    try {
      await invoke('write_hosts', { entries, expectedContent: originalContent });
      uiStore.addToast('/etc/hosts saved successfully', 'success');
      hasChanges = false;
      originalEntries = clone(entries);
      originalContent = previewContent;
      previewOpen = false;
      statusStore.setLastCommand('echo "..." > /etc/hosts', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save hosts: ${e}`, 'error');
      statusStore.setLastCommand('echo "..." > /etc/hosts', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  function createNginxVhostForDomain(domain: string, ip: string) {
    uiStore.navigateTo('nginx-manager', 'sites', {
      initialDomain: domain,
      targetIp: ip === '127.0.0.1' || ip === '::1' ? '127.0.0.1' : ip,
      isProxy: true,
    });
    uiStore.addToast(`Switched to Nginx Manager for ${domain}`, 'info');
  }

  onMount(() => {
    void load();
    const unregister = uiStore.registerLeaveGuard(() => hasChanges);
    return unregister;
  });
</script>

<div class="module-page">
  <PageHeader
    title="Hosts Manager"
    subtitle="Edit /etc/hosts — changes are validated, previewed, and written with polkit authentication"
    icon={Globe}
  >
    <button class="header-refresh-btn" onclick={confirmReload} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} />
      Reload
    </button>
    <Button size="sm" variant="outline" onclick={openAdd}>
      <Plus size={14} />
      Add Entry
    </Button>
    <Button
      size="sm"
      variant="primary"
      onclick={openPreview}
      disabled={saving || previewLoading || !hasChanges}
      style={hasChanges ? 'box-shadow: 0 0 12px var(--color-accent-glow);' : ''}
    >
      {#if previewLoading}
        <RefreshCw size={14} class="animate-spin-slow" />
        Preparing…
      {:else}
        <Save size={14} />
        {hasChanges ? 'Save Changes' : 'Saved'}
      {/if}
    </Button>
  </PageHeader>

  {#if hasChanges}
    <div class="unsaved-banner">
      <div class="unsaved-copy">
        <AlertTriangle size={15} />
        <span>You have unsaved changes. Review and save them to write /etc/hosts.</span>
      </div>
      <Button variant="ghost" size="sm" onclick={discardChanges}>
        <Undo2 size={13} />
        Discard
      </Button>
    </div>
  {/if}

  <div class="hosts-summary">
    <div class="summary-stat">
      <span class="summary-value">{hostEntries.length}</span>
      <span class="summary-label">entries</span>
    </div>
    <div class="summary-stat">
      <span class="summary-value success">{activeCount}</span>
      <span class="summary-label">active</span>
    </div>
    <div class="summary-stat">
      <span class="summary-value muted">{disabledCount}</span>
      <span class="summary-label">disabled</span>
    </div>
    <div class="summary-stat">
      <span class="summary-value accent">{categories.length}</span>
      <span class="summary-label">categories</span>
    </div>
  </div>

  <div class="hosts-toolbar">
    <div class="toolbar-left">
      <SearchBar
        bind:value={filter}
        placeholder="Search IP, hostname, comment, or category…"
        count={filter ? filteredEntries.length : undefined}
        style="width: 320px; max-width: 100%;"
      />
      <FilterChips
        options={statusOptions}
        bind:value={statusFilter}
        label="Filter hosts by status"
      />
      <div class="category-filter">
        <Select bind:value={categoryFilter} size="sm" ariaLabel="Filter hosts by category">
          <option value="all">All categories</option>
          {#if categories.includes('')}
            <option value={DEFAULT_CATEGORY_KEY}>Default</option>
          {/if}
          {#each categories.filter((category) => category) as category (category)}
            <option value={category}>{category}</option>
          {/each}
        </Select>
      </div>
    </div>
    <span class="toolbar-count">
      {filteredEntries.length === hostEntries.length
        ? `${hostEntries.length} entr${hostEntries.length === 1 ? 'y' : 'ies'}`
        : `${filteredEntries.length} of ${hostEntries.length}`}
    </span>
  </div>

  <div class="hosts-content">
    {#if loading}
      <div class="hosts-loading">
        <RefreshCw size={22} class="animate-spin-slow" />
        <span>Reading /etc/hosts…</span>
      </div>
    {:else if loadError}
      <EmptyState
        icon={AlertTriangle}
        title="Could not read /etc/hosts"
        description={loadError}
        actionLabel="Try again"
        onAction={load}
      />
    {:else if hostEntries.length === 0}
      <EmptyState
        icon={Globe}
        title="No host entries"
        description="/etc/hosts has no address mappings. Add the first one to get started."
        actionLabel="Add entry"
        onAction={openAdd}
      />
    {:else if filteredEntries.length === 0}
      <EmptyState
        icon={Search}
        title="No matching entries"
        description="No hosts match the current search, status, and category filters."
        actionLabel="Clear filters"
        onAction={() => {
          filter = '';
          statusFilter = 'all';
          categoryFilter = 'all';
        }}
      />
    {:else}
      <Table class="hosts-table" showDensityToggle={false}>
        <thead>
          <tr>
            <th style="width:18%">IP address</th>
            <th style="width:31%">Hostnames</th>
            <th style="width:22%">Comment</th>
            <th style="width:15%">Category</th>
            <th style="width:10%; text-align:center">Active</th>
            <th style="width:56px"></th>
          </tr>
        </thead>
        <tbody>
          {#each filteredEntries as entry (entry.id)}
            <tr
              class="host-row"
              class:is-disabled={!entry.enabled}
              oncontextmenu={(e) => openRowMenu(e, entry)}
            >
              <td>
                <button
                  type="button"
                  class="host-ip-btn"
                  onclick={() => openEdit(entry)}
                  title="Edit {entry.ip}"
                >
                  {entry.ip}
                </button>
              </td>
              <td>
                <span class="hostnames-cell" title={entry.hostnames.join(' ')}>
                  {entry.hostnames.join(' ')}
                </span>
              </td>
              <td>
                <span class="comment-cell" title={entry.comment}>
                  {entry.comment || '—'}
                </span>
              </td>
              <td>
                <Badge variant="muted">{displayCategory(entry.category)}</Badge>
              </td>
              <td style="text-align:center">
                <Toggle
                  checked={entry.enabled}
                  onToggle={() => toggleEntry(entry)}
                  title="{entry.enabled ? 'Disable' : 'Enable'} {entry.ip}"
                />
              </td>
              <td style="text-align:right">
                <button
                  type="button"
                  class="row-menu-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    openRowMenu(e, entry);
                  }}
                  title="Actions for {entry.ip}"
                  aria-label="Actions for {entry.ip}"
                >
                  <MoreHorizontal size={15} />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </Table>
    {/if}
  </div>

  <ContextMenu
    bind:isOpen={menu.open}
    x={menu.x}
    y={menu.y}
    icon={Globe}
    title={primaryDomain || menu.entry?.ip || 'Host entry'}
    subtitle={menu.entry ? `${menu.entry.ip} · ${displayCategory(menu.entry.category)}` : ''}
    badge={menu.entry
      ? { text: menu.entry.enabled ? 'Active' : 'Disabled', variant: menu.entry.enabled ? 'success' : 'muted' }
      : undefined}
    items={menuItems}
    minWidth="260px"
  />

  <SideDrawer
    bind:isOpen={drawerOpen}
    title={editingId ? 'Edit Host Entry' : 'Add Host Entry'}
    subtitle={editingId ? 'Changes are staged until you save /etc/hosts' : 'Add a static address mapping'}
    width="440px"
  >
    <div class="host-form">
      <Input
        label="IP address"
        bind:value={formIp}
        placeholder="127.0.0.1 or ::1"
        oninput={() => (formError = '')}
      />
      <Input
        label="Hostnames (space-separated)"
        bind:value={formHostnames}
        placeholder="myhost.local myhost"
        oninput={() => (formError = '')}
      />
      <Input
        label="Comment (optional)"
        bind:value={formComment}
        placeholder="Managed by me"
        oninput={() => (formError = '')}
      />
      <div class="form-row">
        <Input
          label="Category"
          bind:value={formCategory}
          placeholder="Custom"
          list="host-category-options"
          oninput={() => (formError = '')}
        />
        <div class="form-toggle">
          <span>Active</span>
          <Toggle bind:checked={formEnabled} />
        </div>
      </div>

      <datalist id="host-category-options">
        {#each categories.filter((category) => category) as category (category)}
          <option value={category}></option>
        {/each}
      </datalist>

      {#if duplicateWarning}
        <div class="form-warning">
          <AlertTriangle size={14} />
          <span>{duplicateWarning}</span>
        </div>
      {/if}
      {#if formError}
        <div class="form-error">{formError}</div>
      {/if}

      <div class="form-actions">
        {#if editingId}
          <Button
            variant="danger"
            size="sm"
            onclick={() => {
              const entry = entries.find((candidate) => candidate.id === editingId);
              if (entry) deleteEntry(entry);
            }}
          >
            <Trash2 size={13} />
            Delete
          </Button>
        {/if}
        <div class="form-actions-right">
          <Button variant="ghost" size="sm" onclick={() => (drawerOpen = false)}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onclick={saveForm}>
            <CheckCircle2 size={13} />
            {editingId ? 'Apply' : 'Add Entry'}
          </Button>
        </div>
      </div>
    </div>
  </SideDrawer>

  <ConfigDiffModal
    bind:show={previewOpen}
    filePath="/etc/hosts"
    title="Review /etc/hosts Modifications"
    oldContent={originalContent}
    newContent={previewContent}
    warningMessage={!hasLocalhost
      ? 'CRITICAL WARNING: no enabled 127.0.0.1 or ::1 localhost entry is present. This can break local networking for many applications.'
      : ''}
    isSaving={saving}
    onconfirm={save}
    oncancel={() => (previewOpen = false)}
  />
</div>

<style>
  .unsaved-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    border: 1px solid rgba(245, 158, 11, 0.28);
    border-radius: 8px;
    background: var(--color-warning-muted);
    color: var(--color-warning);
    font-size: 12px;
    flex-shrink: 0;
  }

  .unsaved-copy {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .hosts-summary {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    flex-shrink: 0;
  }

  .summary-stat {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 7px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-card);
  }

  .summary-value {
    font-family: var(--font-mono);
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .summary-value.success { color: var(--color-success); }
  .summary-value.muted { color: var(--color-text-muted); }
  .summary-value.accent { color: var(--color-accent); }

  .summary-label {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .hosts-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .category-filter {
    width: 190px;
    max-width: 100%;
  }

  .toolbar-count {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .hosts-content {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  .hosts-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-bg-card);
  }

  :global(.hosts-table) {
    border-radius: 12px;
  }

  :global(.hosts-table table) {
    min-width: 780px;
    table-layout: fixed;
  }

  .host-row {
    transition: background 0.12s ease;
  }

  .host-row:hover {
    background: var(--color-bg-hover);
  }

  .host-row.is-disabled {
    opacity: 0.58;
  }

  .host-ip-btn {
    display: inline-flex;
    align-items: center;
    padding: 2px 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-accent);
    font-family: var(--font-mono);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }

  .host-ip-btn:hover {
    text-decoration: underline;
  }

  .host-ip-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .hostnames-cell {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .comment-cell {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .row-menu-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .row-menu-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .row-menu-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }

  .host-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 12px;
  }

  .form-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: end;
    gap: 14px;
  }

  .form-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .form-warning,
  .form-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 9px 11px;
    border-radius: 7px;
    font-size: 11.5px;
    line-height: 1.45;
  }

  .form-warning {
    border: 1px solid rgba(245, 158, 11, 0.28);
    background: var(--color-warning-muted);
    color: var(--color-warning);
  }

  .form-error {
    border: 1px solid rgba(239, 68, 68, 0.3);
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .form-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--color-border-subtle);
    margin-top: 4px;
  }

  .form-actions-right {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  @media (max-width: 760px) {
    .hosts-toolbar {
      align-items: stretch;
    }

    .toolbar-left {
      width: 100%;
    }

    .toolbar-count {
      align-self: flex-end;
    }
  }
</style>
