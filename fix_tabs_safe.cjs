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
    
    // Remove old CSS
    content = content.replace(/:global\(\.tab-btn.*?\)[\s\S]*?\}/g, '');
    content = content.replace(/\.tab-btn[\s\S]*?\}/g, '');
    content = content.replace(/:global\(\.tab-bar.*?\)[\s\S]*?\}/g, '');
    content = content.replace(/\.tab-bar[\s\S]*?\}/g, '');
    
    // Replace <Button class="tab-btn...">...</Button> with <button>
    // Note we match `<Button class="tab-btn` exactly.
    content = content.replace(/<Button class="tab-btn([^>]*)>([\s\S]*?)<\/Button>/g, '<button class="tab-btn$1>$2</button>');
    
    // Replace old wrappers with <div class="tab-bar">
    content = content.replace(/<div style="display:flex; gap:2px; background:var\(--color-bg-raised\);[^>]*>/, '<div class="tab-bar">');
    content = content.replace(/<div class="tabs"[^>]*>/, '<div class="tab-bar">');
    
    fs.writeFileSync(p, content);
  }
}
console.log('Fixed tags safely');
