<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { Users, UserPlus, Key, Shield, ShieldOff, Trash2, RefreshCw, Layers, Lock, Unlock } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import { portal } from '../actions/portal.ts';

  interface UserInfo {
    username: string;
    uid: number;
    gid: number;
    fullname: string;
    home_dir: string;
    shell: string;
    groups: string[];
    is_sudo: boolean;
    is_locked: boolean;
  }

  interface GroupInfo {
    groupname: string;
    gid: number;
    members: string[];
  }

  interface ActiveSession {
    session_id: string;
    uid: string;
    user: string;
    seat: string;
    tty: string;
    state: string;
    idle_since_hint: string;
    is_current: boolean;
  }

  let view = $state<'users' | 'groups' | 'sessions'>(
    uiStore.targetSubTab && ['users', 'groups', 'sessions'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'users'
  );
  if (uiStore.targetSubTab && ['users', 'groups', 'sessions'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  let users = $state<UserInfo[]>([]);
  let groupsList = $state<GroupInfo[]>([]);
  let sessions = $state<ActiveSession[]>([]);
  let loading = $state(true);
  
  let showAddUser = $state(false);
  let newUsername = $state('');
  let newFullname = $state('');

  let showAddGroup = $state(false);
  let newGroupname = $state('');

  // Group membership modal
  let showGroupModal = $state(false);
  let selectedUser = $state<UserInfo | null>(null);
  let groupSearch = $state('');

  // SSH keys modal
  let showSshModal = $state(false);
  let sshKeysContent = $state('');
  let sshLoading = $state(false);

  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading users, groups, and sessions…');
    try {
      const [u, g, s] = await Promise.all([
        invoke<UserInfo[]>('list_users'),
        invoke<GroupInfo[]>('list_groups'),
        invoke<ActiveSession[]>('user_get_active_sessions')
      ]);
      users = u;
      groupsList = g;
      sessions = s;
      statusStore.setLastCommand('loginctl list-sessions', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load data: ${e}`, 'error');
      statusStore.setLastCommand('loginctl list-sessions', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  // --- USER ACTIONS ---
  async function addUser() {
    if (!newUsername.trim()) return;
    statusStore.setBusy(`Adding user ${newUsername}…`);
    try {
      await invoke('add_user', { username: newUsername, fullname: newFullname });
      statusStore.setLastCommand(`useradd -m -s /bin/bash ${newUsername}`, 0, true);
      uiStore.addToast(`User ${newUsername} created successfully`, 'success');
      showAddUser = false;
      newUsername = '';
      newFullname = '';
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to add user: ${e}`, 'error');
      statusStore.setLastCommand(`useradd -m -s /bin/bash ${newUsername}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmDelete(user: UserInfo) {
    if (user.uid < 1000) {
      uiStore.addToast('Cannot delete system users from the UI.', 'error');
      return;
    }
    uiStore.confirm(
      'Delete User',
      `Are you sure you want to completely delete the user '${user.username}'?\n\nWARNING: This action is irreversible!`,
      () => doDeleteUser(user.username, false),
      true
    );
  }

  async function doDeleteUser(username: string, removeHome: boolean) {
    statusStore.setBusy(`Deleting user ${username}…`);
    try {
      await invoke('delete_user', { username, removeHome });
      statusStore.setLastCommand(`userdel ${removeHome ? '-r ' : ''}${username}`, 0, true);
      uiStore.addToast(`User ${username} deleted`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to delete user: ${e}`, 'error');
      statusStore.setLastCommand(`userdel ${removeHome ? '-r ' : ''}${username}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function promptChangePassword(user: UserInfo) {
    const pwd = prompt(`Enter new password for ${user.username}:`);
    if (!pwd) return;
    uiStore.confirm('Change Password', `Change password for ${user.username}? A polkit prompt will appear.`, () => doChangePassword(user.username, pwd), true);
  }

  async function doChangePassword(username: string, password: string) {
    statusStore.setBusy(`Changing password for ${username}…`);
    try {
      await invoke('change_password', { username, password });
      statusStore.setLastCommand(`passwd ${username}`, 0, true);
      uiStore.addToast('Password updated successfully', 'success');
    } catch (e) {
      uiStore.addToast(`Failed to change password: ${e}`, 'error');
      statusStore.setLastCommand(`passwd ${username}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmToggleSudo(user: UserInfo) {
    const grant = !user.is_sudo;
    uiStore.confirm(
      grant ? 'Grant Administrator Privileges' : 'Revoke Administrator Privileges',
      `Are you sure you want to ${grant ? 'grant' : 'revoke'} sudo access for '${user.username}'?`,
      () => doToggleSudo(user.username, grant),
      true
    );
  }

  async function doToggleSudo(username: string, grant: boolean) {
    statusStore.setBusy(`Modifying sudo access for ${username}…`);
    try {
      await invoke('toggle_sudo', { username, grant });
      statusStore.setLastCommand(grant ? `usermod -aG wheel ${username}` : `gpasswd -d ${username} wheel`, 0, true);
      uiStore.addToast(`Sudo privileges ${grant ? 'granted' : 'revoked'}`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to modify sudo access: ${e}`, 'error');
      statusStore.setLastCommand(grant ? `usermod -aG wheel ${username}` : `gpasswd -d ${username} wheel`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmToggleLock(user: UserInfo) {
    const lock = !user.is_locked;
    uiStore.confirm(
      lock ? 'Lock User Account' : 'Unlock User Account',
      `Are you sure you want to ${lock ? 'lock' : 'unlock'} the account for '${user.username}'? ${lock ? 'The user will not be able to log in.' : 'The user will regain login access.'}`,
      () => doToggleLock(user.username, lock),
      lock
    );
  }

  async function doToggleLock(username: string, lock: boolean) {
    statusStore.setBusy(`${lock ? 'Locking' : 'Unlocking'} account ${username}…`);
    try {
      await invoke('toggle_lock_user', { username, lock });
      statusStore.setLastCommand(`passwd ${lock ? '-l' : '-u'} ${username}`, 0, true);
      uiStore.addToast(`User ${username} ${lock ? 'locked' : 'unlocked'} successfully`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to ${lock ? 'lock' : 'unlock'} user: ${e}`, 'error');
      statusStore.setLastCommand(`passwd ${lock ? '-l' : '-u'} ${username}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  // --- GROUP ACTIONS ---
  async function addGroup() {
    if (!newGroupname.trim()) return;
    statusStore.setBusy(`Adding group ${newGroupname}…`);
    try {
      await invoke('add_group', { groupname: newGroupname });
      statusStore.setLastCommand(`groupadd ${newGroupname}`, 0, true);
      uiStore.addToast(`Group ${newGroupname} created successfully`, 'success');
      showAddGroup = false;
      newGroupname = '';
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to add group: ${e}`, 'error');
      statusStore.setLastCommand(`groupadd ${newGroupname}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmDeleteGroup(group: GroupInfo) {
    if (group.gid < 1000 && group.gid !== 0) { // arbitrary protection, could be better
      uiStore.confirm('Delete System Group', `Warning: '${group.groupname}' has a low GID (${group.gid}). Deleting it might break system services.\n\nProceed anyway?`, () => doDeleteGroup(group.groupname), true);
    } else {
      uiStore.confirm('Delete Group', `Are you sure you want to delete the group '${group.groupname}'?`, () => doDeleteGroup(group.groupname), true);
    }
  }

  async function doDeleteGroup(groupname: string) {
    statusStore.setBusy(`Deleting group ${groupname}…`);
    try {
      await invoke('delete_group', { groupname });
      statusStore.setLastCommand(`groupdel ${groupname}`, 0, true);
      uiStore.addToast(`Group ${groupname} deleted`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to delete group: ${e}`, 'error');
      statusStore.setLastCommand(`groupdel ${groupname}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  async function toggleMembership(groupname: string, isMember: boolean) {
    if (!selectedUser) return;
    statusStore.setBusy(`Updating membership for ${selectedUser.username}…`);
    try {
      await invoke('modify_user_group', { username: selectedUser.username, groupname, add: !isMember });
      statusStore.setLastCommand(!isMember ? `usermod -aG ${groupname} ${selectedUser.username}` : `gpasswd -d ${selectedUser.username} ${groupname}`, 0, true);
      // Update local state temporarily for UI responsiveness
      if (isMember) {
        selectedUser.groups = selectedUser.groups.filter(g => g !== groupname);
      } else {
        selectedUser.groups.push(groupname);
      }
      await loadData(); // refresh full list
    } catch (e) {
      uiStore.addToast(`Failed to modify membership: ${e}`, 'error');
      statusStore.setLastCommand(!isMember ? `usermod -aG ${groupname} ${selectedUser.username}` : `gpasswd -d ${selectedUser.username} ${groupname}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  async function openSshKeys(user: UserInfo) {
    selectedUser = user;
    sshLoading = true;
    showSshModal = true;
    try {
      sshKeysContent = await invoke<string>('user_get_ssh_keys', { username: user.username });
      statusStore.setLastCommand(`cat /home/${user.username}/.ssh/authorized_keys`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to read SSH keys: ${e}`, 'error');
      sshKeysContent = '';
    } finally {
      sshLoading = false;
    }
  }

  async function saveSshKeys() {
    if (!selectedUser) return;
    sshLoading = true;
    try {
      await invoke('user_save_ssh_keys', { username: selectedUser.username, keys: sshKeysContent });
      uiStore.addToast('SSH keys saved', 'success');
      showSshModal = false;
      statusStore.setLastCommand(`echo ... > /home/${selectedUser.username}/.ssh/authorized_keys`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save SSH keys: ${e}`, 'error');
      statusStore.setLastCommand(`Save SSH keys for ${selectedUser.username}`, 1, false);
    } finally {
      sshLoading = false;
    }
  }

  function confirmKillSession(session: ActiveSession) {
    uiStore.confirm(
      'Terminate Session',
      `Are you sure you want to forcibly terminate session ${session.session_id} for user ${session.user}?\n\nWARNING: Forcible termination may lead to data loss or system instability if critical processes are running in this session.`,
      () => doKillSession(session.session_id),
      true
    );
  }

  async function doKillSession(sessionId: string) {
    statusStore.setBusy(`Terminating session ${sessionId}...`);
    try {
      await invoke('user_kill_session', { sessionId });
      uiStore.addToast(`Session ${sessionId} terminated`, 'success');
      statusStore.setLastCommand(`loginctl kill-session ${sessionId}`, 0, true);
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to terminate session: ${e}`, 'error');
      statusStore.setLastCommand(`loginctl kill-session ${sessionId}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page">
  <PageHeader title="Users & Groups" subtitle="Manage local accounts, passwords, privileges, and groups" icon={Users}>
    <div style="display:flex; background:var(--color-bg-raised); padding:4px; border-radius:8px; gap:4px; margin-right: 8px;">
      <Button class="btn btn-sm {view === 'users' ? 'btn-primary' : '-ghost'}" onclick={() => view = 'users'}>Users</Button>
      <Button class="btn btn-sm {view === 'groups' ? 'btn-primary' : '-ghost'}" onclick={() => view = 'groups'}>Groups</Button>
      <Button class="btn btn-sm {view === 'sessions' ? 'btn-primary' : '-ghost'}" onclick={() => view = 'sessions'}>Sessions</Button>
    </div>
    <Button variant="ghost" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    {#if view === 'users'}
      <Button variant="primary" onclick={() => showAddUser = !showAddUser}>
        <UserPlus size={14} /> Add User
      </Button>
    {:else}
      <Button variant="primary" onclick={() => showAddGroup = !showAddGroup}>
        <Layers size={14} /> Add Group
      </Button>
    {/if}
  </PageHeader>

  <div class="user-kpi-grid">
    <KpiCard
      icon={Users}
      value={users.length}
      label="Total Accounts"
      subtext="Configured local users"
      active={view === 'users'}
      onclick={() => view = 'users'}
    />
    <KpiCard
      icon={Shield}
      value={users.filter(u => u.is_sudo).length}
      label="Sudoers / Admins"
      subtext="Wheel / administrative"
      statusText="Admin"
      statusType="info"
      iconBg="rgba(0, 218, 243, 0.12)"
      iconColor="var(--color-accent)"
    />
    <KpiCard
      icon={Layers}
      value={groupsList.length}
      label="System Groups"
      subtext="Permission groups"
      active={view === 'groups'}
      onclick={() => view = 'groups'}
      iconBg="rgba(168, 85, 247, 0.12)"
      iconColor="#A855F7"
    />
    <KpiCard
      icon={Key}
      value={sessions.length}
      label="Active Sessions"
      subtext="Logged in sessions"
      statusText={sessions.length > 0 ? `${sessions.length} Online` : '0'}
      statusType={sessions.length > 0 ? 'success' : 'muted'}
      active={view === 'sessions'}
      onclick={() => view = 'sessions'}
    />
  </div>

  {#if view === 'users' && showAddUser}
    <div class="card" style="margin-bottom: 16px; border: 1px solid var(--color-border-focus)">
      <h3 style="margin-top:0; color:var(--color-text-primary)">Create New User</h3>
      <div style="display:flex; gap:12px; align-items:flex-end">
        <div style="flex:1">
          <label for="new-username" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Username</label>
          <input id="new-username" class="input" bind:value={newUsername} placeholder="e.g. john" />
        </div>
        <div style="flex:1">
          <label for="new-fullname" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Full Name (Optional)</label>
          <input id="new-fullname" class="input" bind:value={newFullname} placeholder="e.g. John Doe" onkeydown={(e) => e.key === 'Enter' && addUser()} />
        </div>
        <Button variant="primary" onclick={addUser} disabled={!newUsername.trim()}>Create Account</Button>
        <Button variant="outline" onclick={() => showAddUser = false}>Cancel</Button>
      </div>
    </div>
  {/if}

  {#if view === 'groups' && showAddGroup}
    <div class="card" style="margin-bottom: 16px; border: 1px solid var(--color-border-focus)">
      <h3 style="margin-top:0; color:var(--color-text-primary)">Create New Group</h3>
      <div style="display:flex; gap:12px; align-items:flex-end">
        <div style="flex:1">
          <label for="group-name" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Group Name</label>
          <input id="group-name" class="input" bind:value={newGroupname} placeholder="e.g. developers" onkeydown={(e) => e.key === 'Enter' && addGroup()} />
        </div>
        <Button variant="primary" onclick={addGroup} disabled={!newGroupname.trim()}>Create Group</Button>
        <Button variant="outline" onclick={() => showAddGroup = false}>Cancel</Button>
      </div>
    </div>
  {/if}

  <div class="card module-content-scroll" style="padding:0">
    {#if view === 'users'}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th>Username</th>
              <th>Full Name</th>
              <th style="width: 80px">UID</th>
              <th>Type</th>
              <th style="text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each users as user}
              <tr>
                <td>
                  <div style="display:flex; align-items:center; gap:8px">
                    <div style="width:32px; height:32px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; font-weight:600; color:var(--color-text-primary)">
                      {user.username.charAt(0).toUpperCase()}
                    </div>
                    <div>
                      <div style="font-weight:600; color:var(--color-text-primary)">{user.username}</div>
                      <div style="font-size:11px; color:var(--color-text-secondary); font-family:var(--font-mono)">{user.home_dir}</div>
                    </div>
                  </div>
                </td>
                <td>{user.fullname || '—'}</td>
                <td><code style="font-size:12px">{user.uid}</code></td>
                <td>
                  <div style="display:flex; align-items:center; gap:6px; flex-wrap:wrap;">
                    {#if user.is_sudo}
                      <span class="badge" style="background: rgba(234, 179, 8, 0.15); color: #eab308; border: 1px solid rgba(234, 179, 8, 0.3); font-weight:600;"><Shield size={11} style="margin-right:4px"/> Administrator (wheel)</span>
                    {:else}
                      <span class="badge badge-muted">👤 Standard User</span>
                    {/if}
                    {#if user.is_locked}
                      <span class="badge" style="background: rgba(239, 68, 68, 0.15); color: #ef4444; border: 1px solid rgba(239, 68, 68, 0.3); font-weight:600;"><Lock size={10} style="margin-right:3px"/> Locked</span>
                    {/if}
                  </div>
                </td>
                <td style="text-align:right">
                  <KebabMenu>
                    <button class="menu-item" onclick={() => {selectedUser = user; showGroupModal = true;}}>
                      <Layers size={14} /> Manage Groups
                    </button>
                    <button class="menu-item" onclick={() => openSshKeys(user)}>
                      <Key size={14} /> Edit SSH Keys
                    </button>
                    <button class="menu-item" onclick={() => confirmToggleLock(user)}>
                      {#if user.is_locked}
                        <Unlock size={14} /> Unlock Account
                      {:else}
                        <Lock size={14} /> Lock Account
                      {/if}
                    </button>
                    <button class="menu-item" onclick={() => confirmToggleSudo(user)}>
                      {#if user.is_sudo}
                        <ShieldOff size={14} /> Revoke Sudo
                      {:else}
                        <Shield size={14} /> Grant Sudo
                      {/if}
                    </button>
                    <button class="menu-item" onclick={() => promptChangePassword(user)}>
                      <Key size={14} /> Change Password
                    </button>
                    <button class="menu-item danger" onclick={() => confirmDelete(user)}>
                      <Trash2 size={14} /> Delete User
                    </button>
                  </KebabMenu>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if view === 'groups'}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th>Group Name</th>
              <th>GID</th>
              <th>Members</th>
              <th style="text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each groupsList as group}
              <tr>
                <td><div style="font-weight:600; color:var(--color-text-primary)">{group.groupname}</div></td>
                <td><code style="font-size:12px">{group.gid}</code></td>
                <td>
                  <div style="display:flex; flex-wrap:wrap; gap:4px">
                    {#if group.members.length === 0}
                      <span style="font-size:12px; color:var(--color-text-muted)">—</span>
                    {:else}
                      {#each group.members as member}
                        <span class="badge badge-outline">{member}</span>
                      {/each}
                    {/if}
                  </div>
                </td>
                <td style="text-align:right">
                  <KebabMenu>
                    <button class="menu-item danger" onclick={() => confirmDeleteGroup(group)}>
                      <Trash2 size={14} /> Delete Group
                    </button>
                  </KebabMenu>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if view === 'sessions'}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th>Session ID</th>
              <th>User</th>
              <th>Seat</th>
              <th>TTY</th>
              <th>State</th>
              <th>Idle</th>
              <th style="text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each sessions as session}
              <tr>
                <td>
                  <div style="display:flex; align-items:center; gap:8px;">
                    <code style="font-size:12px">{session.session_id}</code>
                    {#if session.is_current}
                      <span class="badge badge-info" style="font-size:10px; padding:2px 6px;">Current</span>
                    {/if}
                  </div>
                </td>
                <td>
                  <div style="font-weight:600; color:var(--color-text-primary)">{session.user}</div>
                  <div style="font-size:11px; color:var(--color-text-secondary)">UID: {session.uid}</div>
                </td>
                <td>{session.seat || '—'}</td>
                <td>{session.tty || '—'}</td>
                <td>
                  <span class="badge {session.state === 'active' ? 'badge-success' : 'badge-outline'}">{session.state || 'unknown'}</span>
                </td>
                <td>{session.idle_since_hint || '—'}</td>
                <td style="text-align:right">
                  <KebabMenu>
                    <button class="menu-item danger" onclick={() => confirmKillSession(session)} disabled={session.is_current} style={session.is_current ? 'opacity: 0.5; cursor: not-allowed;' : ''} title={session.is_current ? 'Cannot terminate current session' : ''}>
                      <Trash2 size={14} /> Kill Session
                    </button>
                  </KebabMenu>
                </td>
              </tr>
            {/each}
            {#if sessions.length === 0}
              <tr><td colspan="7" style="text-align:center; padding:24px; color:var(--color-text-muted)">No active sessions found</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<svelte:window onkeydown={(e) => { if (showGroupModal && e.key === 'Escape') showGroupModal = false; }} />

{#if showGroupModal && selectedUser}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div use:portal class="modal-backdrop" onclick={() => showGroupModal = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()} style="width: 500px; max-height: 80vh; display:flex; flex-direction:column">
      <h2 style="margin-top:0; color:var(--color-text-primary)">Manage Groups for {selectedUser.username}</h2>
      <p style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Check the groups you want this user to be a member of.</p>
      
      <div style="margin-bottom: 16px;">
        <input type="text" class="input" placeholder="Search groups..." bind:value={groupSearch} />
      </div>

      <div class="module-content-scroll" style="flex:1; border:1px solid var(--color-border); border-radius:8px; padding:8px; display:grid; grid-template-columns: 1fr 1fr; gap:8px">
        {#each groupsList.filter(g => g.groupname.toLowerCase().includes(groupSearch.toLowerCase())) as group (group.groupname)}
          {@const isMember = selectedUser.groups.includes(group.groupname) || group.members.includes(selectedUser.username)}
          <label style="display:flex; align-items:center; gap:8px; padding:8px; border-radius:6px; background:var(--color-bg-raised); cursor:pointer">
            <input 
              type="checkbox" 
              checked={isMember} 
              onchange={() => toggleMembership(group.groupname, isMember)} 
            />
            <span style="font-size:13px; color:var(--color-text-primary)">{group.groupname}</span>
          </label>
        {/each}
      </div>

      <div style="display:flex; justify-content:flex-end; gap:8px; margin-top:16px">
        <Button variant="primary" onclick={() => { showGroupModal = false; groupSearch = ''; }}>Done</Button>
      </div>
    </div>
  </div>
{/if}

<SideDrawer bind:isOpen={showSshModal} title="SSH Authorized Keys" width="500px">
  <div style="display:flex; flex-direction:column; gap:16px; padding-top:8px; height: 100%;">
    <div style="font-size:13px; color:var(--color-text-secondary); line-height:1.5;">
      Manage SSH keys for <strong>{selectedUser?.username}</strong>. Paste public keys below (one per line) to allow passwordless SSH login.
    </div>

    {#if sshLoading}
      <div style="display:flex; justify-content:center; padding:24px;">
        <div class="spinner"></div>
      </div>
    {:else}
      <textarea 
        bind:value={sshKeysContent} 
        placeholder="ssh-rsa AAAAB3Nza... user@host"
        style="flex:1; width:100%; min-height:300px; padding:12px; font-family:var(--font-mono); font-size:12px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px; color:var(--color-text-primary); resize:vertical;"
      ></textarea>
    {/if}

    <div style="display:flex; justify-content:flex-end; gap:12px; margin-top:16px;">
      <Button variant="outline" onclick={() => showSshModal = false}>Cancel</Button>
      <Button variant="primary" disabled={sshLoading} onclick={saveSshKeys}>
        Save Keys
      </Button>
    </div>
  </div>
</SideDrawer>

<style>
  .user-kpi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 12px;
    margin-bottom: 16px;
  }
</style>
