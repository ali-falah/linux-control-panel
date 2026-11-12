// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import ContextMenu from './ContextMenu.svelte';

const items = [
  { label: 'First' },
  { label: 'Second' },
  { label: 'Third' },
];

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(cleanup);

describe('ContextMenu keyboard behavior', () => {
  it('focuses the first item when opened', async () => {
    const { getAllByRole } = render(ContextMenu, {
      props: { isOpen: true, x: 10, y: 10, title: 'Unit', items },
    });

    await new Promise((resolve) => setTimeout(resolve, 0));

    const buttons = getAllByRole('menuitem');
    expect(document.activeElement).toBe(buttons[0]);
  });

  it('wraps arrow navigation and jumps to the edges with Home/End', async () => {
    const { getAllByRole } = render(ContextMenu, {
      props: { isOpen: true, x: 10, y: 10, title: 'Unit', items },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const buttons = getAllByRole('menuitem');
    const menu = document.querySelector('[role="menu"]') as HTMLElement;

    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(document.activeElement).toBe(buttons[1]);

    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
    expect(document.activeElement).toBe(buttons[2]);

    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(document.activeElement).toBe(buttons[0]);

    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    expect(document.activeElement).toBe(buttons[2]);

    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    expect(document.activeElement).toBe(buttons[0]);
  });

  it('closes on Escape', async () => {
    const onclose = vi.fn();
    render(ContextMenu, {
      props: { isOpen: true, x: 10, y: 10, title: 'Unit', items, onclose },
    });

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});

describe('ContextMenu item activation', () => {
  it('runs the item action when the item is clicked', async () => {
    const action = vi.fn();
    const { getByRole } = render(ContextMenu, {
      props: { isOpen: true, x: 20, y: 20, items: [{ label: 'Export', action }] },
    });
    await flush();

    await fireEvent.click(getByRole('menuitem'));
    await flush();

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('closes the menu after activating an item', async () => {
    const { getByRole, queryByRole } = render(ContextMenu, {
      props: { isOpen: true, x: 20, y: 20, items: [{ label: 'Export', action: () => {} }] },
    });
    await flush();
    expect(queryByRole('menuitem')).not.toBeNull();

    await fireEvent.click(getByRole('menuitem'));
    await flush();

    expect(queryByRole('menuitem')).toBeNull();
  });

  it('does not run a disabled item and keeps the menu open', async () => {
    const action = vi.fn();
    const { getByRole, queryByRole } = render(ContextMenu, {
      props: { isOpen: true, x: 20, y: 20, items: [{ label: 'Current', action, disabled: true }] },
    });
    await flush();

    await fireEvent.click(getByRole('menuitem'));
    await flush();

    expect(action).not.toHaveBeenCalled();
    expect(queryByRole('menuitem')).not.toBeNull();
  });

  it('reports an action that rejects instead of swallowing it', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { getByRole } = render(ContextMenu, {
      props: {
        isOpen: true,
        x: 20,
        y: 20,
        items: [{ label: 'Export', action: () => Promise.reject(new Error('backend refused')) }],
      },
    });
    await flush();

    await fireEvent.click(getByRole('menuitem'));
    await flush();
    await flush();

    expect(errorSpy).toHaveBeenCalled();
    expect(errorSpy.mock.calls.flat().join(' ')).toContain('backend refused');
    errorSpy.mockRestore();
  });

  it('keeps the menu open when a click lands on the menu itself (window handler)', async () => {
    const action = vi.fn();
    render(ContextMenu, {
      props: { isOpen: true, x: 20, y: 20, items: [{ label: 'Export', action }] },
    });
    await flush();

    // the menu is portaled to <body>; clicking it must not trip the
    // <svelte:window> outside-click handler that closes the menu
    const menu = document.querySelector('.custom-context-menu') as HTMLElement;
    expect(menu).not.toBeNull();
    await fireEvent.click(menu);
    await flush();

    expect(document.querySelector('.context-menu-item')).toBeTruthy();
    expect(action).not.toHaveBeenCalled();
  });

  it('ignores the outside click that opened the menu', async () => {
    const onclose = vi.fn();
    render(ContextMenu, {
      props: { isOpen: true, x: 20, y: 20, items: [{ label: 'Export' }], onclose },
    });
    // Svelte mounts the menu between the trigger's click listener and the one
    // on window, so the opening click arrives here with the menu on screen
    await Promise.resolve();
    await Promise.resolve();
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(onclose).not.toHaveBeenCalled();

    // a later click really is outside and must close it
    await flush();
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});
