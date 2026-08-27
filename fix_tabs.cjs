const fs = require('fs');

const files = [
  'NetworkManager.svelte',
  'NginxManager.svelte',
  'DnfHistory.svelte',
  'FlatpakRpm.svelte'
];
for (const file of files) {
  const p = `/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/${file}`;
  if (fs.existsSync(p)) {
    let content = fs.readFileSync(p, 'utf8');
    content = content.replace(/<button class="tab-btn(.*?)>([\s\S]*?)<\/Button>/g, '<button class="tab-btn$1>$2</button>');
    
    // Some tabs are in a <div style="display:flex... background:var(--color-bg-raised)...">
    // The user wants ALL tabs to use the new design. The new design is `<div class="tab-bar">`.
    // So let's replace the inline styled div with `<div class="tab-bar">`.
    // Let's find any div that contains `<button class="tab-btn` and change it to `<div class="tab-bar">` if it has those old inline styles.
    content = content.replace(/<div style="display:flex; gap:2px; background:var\(--color-bg-raised\);[^>]*>/, '<div class="tab-bar">');
    content = content.replace(/<div class="tabs"[^>]*>/, '<div class="tab-bar">');
    fs.writeFileSync(p, content);
  }
}
console.log('Fixed tabs');
