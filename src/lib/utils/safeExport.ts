import { uiStore } from '../stores/ui.svelte.ts';

/**
 * Run an export and make sure the user hears about it.
 *
 * An export that throws before its own try block becomes an unhandled
 * rejection: the menu closes, no toast appears, and the click looks like it
 * did nothing. This is the one place that guarantee is enforced, so every
 * export menu action routes through it.
 */
export async function safeExport(exporter: () => void | Promise<void>): Promise<void> {
  try {
    await exporter();
  } catch (e) {
    console.error('Export failed:', e);
    uiStore.addToast(`Export failed: ${e}`, 'error');
  }
}
