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
