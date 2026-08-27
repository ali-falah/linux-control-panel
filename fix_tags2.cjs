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
    // Using a more robust regex to catch multiline button tags
    content = content.replace(/<button class="tab-btn([\s\S]*?)<\/Button>/g, '<button class="tab-btn$1</button>');
    fs.writeFileSync(p, content);
  }
}
console.log('Fixed tags');
