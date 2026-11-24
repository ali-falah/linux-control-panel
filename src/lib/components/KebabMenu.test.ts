// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import KebabMenu from './KebabMenu.svelte';

const children = createRawSnippet(() => ({
  render: () => `
    <div>
      <button class="menu-item" role="menuitem">First</button>
      <button class="menu-item" role="menuitem" disabled>Disabled</button>
      <button class="menu-item" role="menuitem">Third</button>
    </div>
  `,
}));

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(cleanup);

describe('KebabMenu keyboard behavior', () => {
  it('focuses the first enabled item and skips disabled items', async () => {
    const { container, getAllByRole } = render(KebabMenu, { props: { children } });
    fireEvent.click(container.querySelector('.trigger-btn') as HTMLElement);
    await flush();

    const items = getAllByRole('menuitem');
    expect(document.activeElement).toBe(items[0]);

    const dropdown = container.ownerDocument.querySelector('.menu-dropdown') as HTMLElement;
    dropdown.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(document.activeElement).toBe(items[2]);
  });

  it('closes on Escape and returns focus to the trigger', async () => {
    const { container } = render(KebabMenu, { props: { children } });
    const trigger = container.querySelector('.trigger-btn') as HTMLElement;
    fireEvent.click(trigger);
    await flush();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush();

    expect(document.querySelector('.menu-dropdown')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('closes when focus leaves the menu', async () => {
    const { container } = render(KebabMenu, { props: { children } });
    fireEvent.click(container.querySelector('.trigger-btn') as HTMLElement);
    await flush();

    const dropdown = container.ownerDocument.querySelector('.menu-dropdown') as HTMLElement;
    dropdown.dispatchEvent(
      new FocusEvent('focusout', { bubbles: true, relatedTarget: document.body }),
    );
    await flush();

    expect(document.querySelector('.menu-dropdown')).toBeNull();
  });
});
