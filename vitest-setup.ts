import { vi } from 'vitest';

// jsdom has no Web Animations API; Svelte transitions call element.animate()
// and complete via the animation's `onfinish` callback.
if (typeof Element !== 'undefined' && !Element.prototype.animate) {
  Element.prototype.animate = vi.fn(() => {
    let finish: (() => void) | null = null;
    return {
      cancel: vi.fn(),
      finish: vi.fn(),
      finished: Promise.resolve(),
      playState: 'finished',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      get onfinish() {
        return finish;
      },
      set onfinish(callback: (() => void) | null) {
        finish = callback;
        if (callback) queueMicrotask(callback);
      },
    };
  }) as unknown as typeof Element.prototype.animate;
}
