import { invoke } from '@tauri-apps/api/core';
import { statusStore } from '../stores/status.svelte.ts';
import { uiStore } from '../stores/ui.svelte.ts';

interface InvokeSafeOptions {
  showToastOnError?: boolean;
  quiet?: boolean;
}

/**
 * Safely invokes a Tauri IPC command, catching unhandled errors,
 * logging output to statusStore, and optionally displaying user toast notifications.
 */
export async function invokeSafe<T>(
  command: string,
  args?: Record<string, unknown>,
  options: InvokeSafeOptions = {}
): Promise<T | null> {
  const { showToastOnError = false, quiet = false } = options;

  if (!quiet) {
    statusStore.setBusy(`Executing ${command}…`);
  }

  try {
    const result = await invoke<T>(command, args);
    if (!quiet) {
      statusStore.setLastCommand(command, 0, true);
    }
    return result;
  } catch (error: any) {
    const errorMessage = typeof error === 'string' ? error : error?.message || 'IPC Command Execution Failed';
    
    if (!quiet) {
      statusStore.setLastCommand(command, 1, false);
    }

    if (showToastOnError) {
      uiStore.showToast(`Error (${command}): ${errorMessage}`, 'error');
    }

    console.error(`[IPC Error] Command '${command}' failed:`, error);
    return null;
  } finally {
    if (!quiet) {
      statusStore.clearBusy();
    }
  }
}
