import { mount } from 'svelte';
import App from "./App.svelte";
import "./app.css";
import { uiStore } from './lib/stores/ui.svelte.ts';

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;

// WebKitGTK fix for missing default Undo/Redo shortcut bindings
window.addEventListener('keydown', (e) => {
  if (e.ctrlKey && !e.altKey) {
    if (e.key === 'z' || e.key === 'Z') {
      if (e.shiftKey) {
        document.execCommand('redo');
      } else {
        document.execCommand('undo');
      }
    } else if (e.key === 'y' || e.key === 'Y') {
      document.execCommand('redo');
    }
  }
});

// Global Ctrl+K / Cmd+K Search Shortcut (Layout-agnostic, works across English/Arabic/etc. keyboard layouts)
window.addEventListener('keydown', (e) => {
  if (e.ctrlKey || e.metaKey) {
    const isKeyK = e.code === 'KeyK' || e.key?.toLowerCase() === 'k' || e.keyCode === 75 || e.which === 75;
    if (isKeyK) {
      const target = e.target as HTMLElement;
      // If search modal is already open, Ctrl+K toggles it closed even if focus is in search input
      if (uiStore.searchModalOpen) {
        e.preventDefault();
        uiStore.toggleSearchModal();
        return;
      }
      // If inside code editor, don't steal
      if (target?.closest('.cm-editor')) return;

      e.preventDefault();
      uiStore.toggleSearchModal();
    }
  }
}, { capture: true });

