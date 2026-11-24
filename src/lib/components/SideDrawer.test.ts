// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import SideDrawer from './SideDrawer.svelte';
import DrawerHost from './__test__/DrawerHost.svelte';
import { uiStore } from '../stores/ui.svelte.ts';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => {
  cleanup();
  uiStore.isDrawerDocked = false;
  uiStore.closeConfirm();
});

describe('SideDrawer modal behavior', () => {
  it('uses dialog semantics and moves focus into the panel when overlay', async () => {
    const { getByRole } = render(SideDrawer, {
      props: { isOpen: true, title: 'Unit Override' },
    });
    await flush();

    const drawer = getByRole('dialog');
    expect(drawer.getAttribute('aria-modal')).toBe('true');
    expect(document.activeElement).toBe(drawer);
  });

  it('does not close when a confirm dialog is open above it', async () => {
    const visibility = vi.fn();
    const { queryByRole } = render(DrawerHost, {
      props: { onvisibility: visibility },
    });
    await flush();

    uiStore.confirm('Confirm stop', 'Are you sure?', () => {}, true);
    const drawer = queryByRole('dialog') as HTMLElement;
    drawer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flush();

    expect(queryByRole('dialog')).not.toBeNull();
    expect(visibility).not.toHaveBeenCalledWith(false);
  });

  it('closes on Escape once no dialog is above it', async () => {
    const { queryByRole } = render(DrawerHost);
    await flush();

    const drawer = queryByRole('dialog') as HTMLElement;
    drawer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    // The drawer uses an outro transition, so the node leaves after it finishes.
    await new Promise((resolve) => setTimeout(resolve, 300));

    expect(queryByRole('dialog')).toBeNull();
  });

  it('drops modal semantics when docked', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1400 });
    uiStore.isDrawerDocked = true;

    const { getByRole } = render(SideDrawer, {
      props: { isOpen: true, title: 'Unit Override' },
    });
    await flush();

    const region = getByRole('region');
    expect(region.getAttribute('aria-modal')).toBeNull();
  });
});
