import re

with open("src/lib/modules/AppManager.svelte", "r") as f:
    content = f.read()

# Add the event listener import if missing
if "import { listen } from '@tauri-apps/api/event';" not in content:
    content = re.sub(r"import { invoke } from '@tauri-apps/api/core';", "import { invoke } from '@tauri-apps/api/core';\n  import { listen } from '@tauri-apps/api/event';", content)

# Add the unlisten setup
unlisten_code = """
  let unlistenLog: any;

  import { onMount, onDestroy } from 'svelte';
  onMount(async () => {
    unlistenLog = await listen('uninstall-log', (event: any) => {
      appendLog(event.payload);
    });
  });

  onDestroy(() => {
    if (unlistenLog) unlistenLog();
  });
"""
# insert right after type AppDetails
content = re.sub(r"(type AppDetails = [^;]*;)", r"\1" + unlisten_code, content)


new_uninstall = """  async function performUninstall(app: DesktopApp) {
    if (!app.package_id) {
      uiStore.addToast('Cannot uninstall: Unknown package ID', 'error');
      return;
    }

    closeDetails();
    uninstallingApp = app;
    isUninstalling = true;
    uninstallLog = [];
    appendLog(`Starting uninstallation for ${app.name} (${app.package_id})...`);

    if (app.source === 'Flatpak') {
      appendLog(`> pkexec flatpak uninstall -y ${app.package_id}`);
    } else {
      appendLog(`> pkexec dnf remove -y ${app.package_id}`);
    }

    try {
      await invoke('uninstall_app', { packageId: app.package_id, source: app.source });
      appendLog(`\\nSuccessfully uninstalled ${app.name} and cleaned dependencies.`);
      uiStore.addToast(`Removed ${app.name}`, 'success');
      loadApps();
    } catch (e) {
      appendLog(`\\nExecution error: ${e}`);
    } finally {
      isUninstalling = false;
    }
  }"""

# Replace the entire performUninstall block
content = re.sub(r"async function performUninstall\(app: DesktopApp\) \{[\s\S]*?catch \(e\) \{\s*appendLog\(`Execution error: \$\{e\}`\);\s*isUninstalling = false;\s*\}\s*\}", new_uninstall, content)

with open("src/lib/modules/AppManager.svelte", "w") as f:
    f.write(content)
