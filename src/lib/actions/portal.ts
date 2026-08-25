let activePortalsCount = 0;

export function portal(node: HTMLElement) {
  const target = document.body;
  target.appendChild(node);
  activePortalsCount++;

  if (activePortalsCount === 1) {
    document.documentElement.classList.add('modal-open-scroll-lock');
  }

  const handleWheel = (e: WheelEvent) => {
    const targetEl = e.target as HTMLElement | null;
    if (!targetEl) return;

    // If scrolling directly on backdrop
    if (targetEl === node || targetEl.classList.contains('modal-backdrop')) {
      e.preventDefault();
      return;
    }

    // Traverse up to find if there is a scrollable element
    let current: HTMLElement | null = targetEl;
    let canScroll = false;
    while (current && current !== node) {
      const overflowY = window.getComputedStyle(current).overflowY;
      const isScrollable = (overflowY === 'auto' || overflowY === 'scroll') && current.scrollHeight > current.clientHeight;
      if (isScrollable) {
        const atTop = current.scrollTop <= 0 && e.deltaY < 0;
        const atBottom = Math.abs(current.scrollHeight - current.clientHeight - current.scrollTop) <= 1 && e.deltaY > 0;
        if (!atTop && !atBottom) {
          canScroll = true;
          break;
        }
      }
      current = current.parentElement;
    }

    if (!canScroll) {
      e.preventDefault();
    }
  };

  node.addEventListener('wheel', handleWheel, { passive: false });

  return {
    destroy() {
      node.removeEventListener('wheel', handleWheel);
      if (node.parentNode) {
        node.parentNode.removeChild(node);
      }
      activePortalsCount = Math.max(0, activePortalsCount - 1);
      if (activePortalsCount === 0) {
        document.documentElement.classList.remove('modal-open-scroll-lock');
      }
    }
  };
}
