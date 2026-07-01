<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Users, UserPlus, Key, Shield, ShieldOff, Trash2, RefreshCw, Layers } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface UserInfo {
    username: string;
    uid: number;
    gid: number;
    fullname: string;
    home_dir: string;
    shell: string;
    groups: string[];
    is_sudo: boolean;
  }

  interface GroupInfo {
    groupname: string;
    gid: number;
    members: string[];
  }

  let view = $state<'users' | 'groups'>('users');
  let users = $state<UserInfo[]>([]);
  let groupsList = $state<GroupInfo[]>([]);
  let loading = $state(true);
  
  let showAddUser = $state(false);
  let newUsername = $state('');
  let newFullname = $state('');

  let showAddGroup = $state(false);
  let newGroupname = $state('');

  // Group membership modal
  let showGroupModal = $state(false);
  let selectedUser = $state<UserInfo | null>(null);

  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading users and groups…');
    try {
      const [u, g] = await Promise.all([
        invoke<UserInfo[]>('list_users'),
        invoke<GroupInfo[]>('list_groups')
      ]);
      users = u;
      groupsList = g;
      statusStore.setLastCommand('Read /etc/passwd & /etc/group', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load data: ${e}`, 'error');
      statusStore.setLastCommand('list_users/groups', 1, false);
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
      uiStore.addToast(`User ${newUsername} created successfully`, 'success');
      showAddUser = false;
      newUsername = '';
      newFullname = '';
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to add user: ${e}`, 'error');
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
      uiStore.addToast(`User ${username} deleted`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to delete user: ${e}`, 'error');
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
      uiStore.addToast('Password updated successfully', 'success');
    } catch (e) {
      uiStore.addToast(`Failed to change password: ${e}`, 'error');
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
      uiStore.addToast(`Sudo privileges ${grant ? 'granted' : 'revoked'}`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to modify sudo access: ${e}`, 'error');
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
      uiStore.addToast(`Group ${newGroupname} created successfully`, 'success');
      showAddGroup = false;
      newGroupname = '';
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to add group: ${e}`, 'error');
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
      uiStore.addToast(`Group ${groupname} deleted`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to delete group: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function toggleMembership(groupname: string, isMember: boolean) {
    if (!selectedUser) return;
    statusStore.setBusy(`Updating membership for ${selectedUser.username}…`);
    try {
      await invoke('modify_user_group', { username: selectedUser.username, groupname, add: !isMember });
      // Update local state temporarily for UI responsiveness
      if (isMember) {
        selectedUser.groups = selectedUser.groups.filter(g => g !== groupname);
      } else {
        selectedUser.groups.push(groupname);
      }
      await loadData(); // refresh full list
    } catch (e) {
      uiStore.addToast(`Failed to modify membership: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><Users size={20} /></div>
    <div>
      <h1 class="module-title">Users & Groups</h1>
      <p class="module-subtitle">Manage local accounts, passwords, privileges, and groups</p>
    </div>
    <div style="margin-left:auto; display:flex; gap:8px">
      <div style="display:flex; background:var(--color-bg-raised); padding:4px; border-radius:8px; gap:4px">
        <button class="btn btn-sm {view === 'users' ? 'btn-primary' : 'btn-ghost'}" onclick={() => view = 'users'}>Users</button>
        <button class="btn btn-sm {view === 'groups' ? 'btn-primary' : 'btn-ghost'}" onclick={() => view = 'groups'}>Groups</button>
      </div>
      <button class="btn btn-ghost" onclick={loadData} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
      </button>
      {#if view === 'users'}
        <button class="btn btn-primary" onclick={() => showAddUser = !showAddUser}>
          <UserPlus size={14} /> Add User
        </button>
      {:else}
        <button class="btn btn-primary" onclick={() => showAddGroup = !showAddGroup}>
          <Layers size={14} /> Add Group
        </button>
      {/if}
    </div>
  </div>

  {#if view === 'users' && showAddUser}
    <div class="card" style="margin-bottom: 16px; border: 1px solid var(--color-border-focus)">
      <h3 style="margin-top:0; color:var(--color-text-primary)">Create New User</h3>
      <div style="display:flex; gap:12px; align-items:flex-end">
        <div style="flex:1">
          <label style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Username (lowercase, no spaces)</label>
          <input class="w-full" bind:value={newUsername} placeholder="e.g. john" />
        </div>
        <div style="flex:1">
          <label style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Full Name (Optional)</label>
          <input class="w-full" bind:value={newFullname} placeholder="e.g. John Doe" onkeydown={(e) => e.key === 'Enter' && addUser()} />
        </div>
        <button class="btn btn-primary" onclick={addUser} disabled={!newUsername.trim()}>Create Account</button>
        <button class="btn btn-outline" onclick={() => showAddUser = false}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if view === 'groups' && showAddGroup}
    <div class="card" style="margin-bottom: 16px; border: 1px solid var(--color-border-focus)">
      <h3 style="margin-top:0; color:var(--color-text-primary)">Create New Group</h3>
      <div style="display:flex; gap:12px; align-items:flex-end">
        <div style="flex:1">
          <label style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Group Name</label>
          <input class="w-full" bind:value={newGroupname} placeholder="e.g. developers" onkeydown={(e) => e.key === 'Enter' && addGroup()} />
        </div>
        <button class="btn btn-primary" onclick={addGroup} disabled={!newGroupname.trim()}>Create Group</button>
        <button class="btn btn-outline" onclick={() => showAddGroup = false}>Cancel</button>
      </div>
    </div>
  {/if}

  <div class="card module-content-scroll" style="padding:0">
    {#if view === 'users'}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table>
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
                  {#if user.is_sudo}
                    <span class="badge badge-success"><Shield size={10} style="margin-right:4px"/> Admin</span>
                  {:else}
                    <span class="badge badge-muted">Standard</span>
                  {/if}
                </td>
                <td style="text-align:right">
                  <div style="display:flex; gap:4px; justify-content:flex-end">
                    <button class="btn btn-sm btn-ghost" onclick={() => {selectedUser = user; showGroupModal = true;}} title="Manage Groups">
                      <Layers size={14} />
                    </button>
                    <button class="btn btn-sm btn-ghost" onclick={() => confirmToggleSudo(user)} title={user.is_sudo ? "Revoke Sudo" : "Grant Sudo"}>
                      {#if user.is_sudo}
                        <ShieldOff size={14} style="color:var(--color-warning)" />
                      {:else}
                        <Shield size={14} style="color:var(--color-success)" />
                      {/if}
                    </button>
                    <button class="btn btn-sm btn-ghost" onclick={() => promptChangePassword(user)} title="Change Password">
                      <Key size={14} />
                    </button>
                    <button class="btn btn-sm btn-danger" onclick={() => confirmDelete(user)} title="Delete User">
                      <Trash2 size={14} />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table>
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
                  <button class="btn btn-sm btn-danger" onclick={() => confirmDeleteGroup(group)} title="Delete Group">
                    <Trash2 size={14} />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

{#if showGroupModal && selectedUser}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-backdrop" onclick={() => showGroupModal = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()} style="width: 500px; max-height: 80vh; display:flex; flex-direction:column">
      <h2 style="margin-top:0; color:var(--color-text-primary)">Manage Groups for {selectedUser.username}</h2>
      <p style="font-size:13px; color:var(--color-text-secondary); margin-bottom:16px">Check the groups you want this user to be a member of.</p>
      
      <div style="flex:1; overflow-y:auto; border:1px solid var(--color-border); border-radius:8px; padding:8px; display:grid; grid-template-columns: 1fr 1fr; gap:8px">
        {#each groupsList as group}
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
        <button class="btn btn-primary" onclick={() => showGroupModal = false}>Done</button>
      </div>
    </div>
  </div>
{/if}


