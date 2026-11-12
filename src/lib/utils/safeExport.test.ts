import { beforeEach, describe, expect, it, vi } from 'vitest';
import { safeExport } from './safeExport.ts';
import { uiStore } from '../stores/ui.svelte.ts';

describe('safeExport', () => {
  beforeEach(() => {
    uiStore.toasts = [];
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('lets a working export through without adding noise', async () => {
    let ran = false;
    await safeExport(() => {
      ran = true;
    });
    expect(ran).toBe(true);
    expect(uiStore.toasts).toHaveLength(0);
  });

  it('turns a synchronous throw into an error toast', async () => {
    await safeExport(() => {
      throw new Error('bad filename');
    });
    const toast = uiStore.toasts.at(-1);
    expect(toast?.type).toBe('error');
    expect(toast?.message).toContain('bad filename');
  });

  it('turns a rejected export promise into an error toast', async () => {
    await safeExport(async () => {
      throw new Error('backend refused');
    });
    expect(uiStore.toasts.at(-1)?.message).toContain('backend refused');
  });
});
