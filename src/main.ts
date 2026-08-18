import { mount } from 'svelte';
import App from "./App.svelte";
import "./app.css";

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

// WebKitGTK production fix: register Ctrl+K at document level (svelte:window alone can miss it in GTK webview)
document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    // Don't steal from input elements
    const target = e.target as HTMLElement;
    const tag = target?.tagName?.toLowerCase();
    if (tag === 'input' || tag === 'textarea' || target?.isContentEditable || target?.closest('.cm-editor')) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    // Dispatch a custom event that GlobalSearchModal will listen for
    document.dispatchEvent(new CustomEvent('global-search-open'));
  }
}, { capture: true });

