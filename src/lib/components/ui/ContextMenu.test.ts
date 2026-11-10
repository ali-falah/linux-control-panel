// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import ContextMenu from './ContextMenu.svelte';

const items = [
  { label: 'First' },
  { label: 'Second' },
  { label: 'Third' },
];

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
