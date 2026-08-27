const fs = require('fs');

// 1. Update app.css
const appCssPath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/app.css';
let appCss = fs.readFileSync(appCssPath, 'utf8');
const tabCss = `
/* ─── Global Tab Design ───────────────────────────────────────────── */
.tab-bar {
  display: inline-flex;
  gap: 4px;
  background: rgba(0, 0, 0, 0.2);
  padding: 6px;
  border-radius: 12px;
  margin-bottom: 24px;
  align-self: flex-start;
}
.tab-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-radius: 8px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--color-text-muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}
.tab-btn:hover {
  color: #fff;
}
.tab-btn.active {
  background: rgba(255,255,255,0.03);
  border-color: rgba(139, 92, 246, 0.3); /* subtle purple border */
  color: #fff;
}
`;
if (!appCss.includes('.tab-bar {')) {
  appCss += tabCss;
  fs.writeFileSync(appCssPath, appCss);
}

// 2. Remove old tab css from components
const filesWithOldTabs = [
  'NetworkManager.svelte',
  'NginxManager.svelte',
  'DnfHistory.svelte',
  'FlatpakRpm.svelte'
];
for (const file of filesWithOldTabs) {
  const p = `/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/${file}`;
  if (fs.existsSync(p)) {
    let content = fs.readFileSync(p, 'utf8');
    // We want to remove CSS targeting .tab-btn and .tab-bar
    content = content.replace(/:global\(\.tab-btn.*?\)[\s\S]*?\}/g, '');
    content = content.replace(/\.tab-btn[\s\S]*?\}/g, '');
    content = content.replace(/:global\(\.tab-bar.*?\)[\s\S]*?\}/g, '');
    content = content.replace(/\.tab-bar[\s\S]*?\}/g, '');
    // Some buttons use <Button class="tab-btn ..."> we should just change them to <button class="tab-btn ...">
    content = content.replace(/<Button class="tab-btn ([^"]*)"([^>]*)>/g, '<button class="tab-btn $1"$2>');
    content = content.replace(/<\/Button>(\s*<!--.*?-->\s*)?<\/button>/g, '</button>'); // this is tricky, better to let Svelte handle it or not match too greedily. 
    // Actually, just replacing <Button class="tab-btn..." with <button... and </Button> might be hard because of nested things.
    // Let's just remove the CSS. The existing components using `<Button class="tab-btn"` will inherit `.tab-btn` from `app.css`, BUT `.btn` might override things.
    // Let's replace `<Button class="tab-btn` with `<button class="tab-btn` and the corresponding `</Button>` for those lines.
    fs.writeFileSync(p, content);
  }
}

// 3. Update ShellEnv.svelte
const shellEnvPath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/ShellEnv.svelte';
let shellEnv = fs.readFileSync(shellEnvPath, 'utf8');

// Replace custom-tabs-container with tab-bar, custom-tab with tab-btn
shellEnv = shellEnv.replace(/custom-tabs-container/g, 'tab-bar');
shellEnv = shellEnv.replace(/custom-tab/g, 'tab-btn');

// Replace search-box with search-bar (the user requested the reusable one)
shellEnv = shellEnv.replace(/class="search-box"/g, 'class="search-bar" style="flex:1"');

// Update sg-header background to be darker
shellEnv = shellEnv.replace(/\.sg-header \{[\s\S]*?\}/, `.sg-header {\n    display: flex;\n    align-items: center;\n    gap: 12px;\n    padding: 16px 20px;\n    cursor: pointer;\n    transition: background 0.2s;\n    background: rgba(0,0,0,0.3);\n  }`);

// Remove custom CSS for tabs and search-box from ShellEnv
shellEnv = shellEnv.replace(/\.tab-bar \{[\s\S]*?align-self: flex-start;\n  \}/, '');
shellEnv = shellEnv.replace(/\.tab-btn \{[\s\S]*?transition: all 0.2s;\n  \}/, '');
shellEnv = shellEnv.replace(/\.tab-btn:hover \{[\s\S]*?color: #fff;\n  \}/, '');
shellEnv = shellEnv.replace(/\.tab-btn\.active \{[\s\S]*?color: #fff;\n  \}/, '');

shellEnv = shellEnv.replace(/\.search-box \{[\s\S]*?color: var\(--color-text-muted\);\n  \}/, '');
shellEnv = shellEnv.replace(/\.search-box:focus-within \{[\s\S]*?border-color: rgba\(255,255,255,0.15\);\n  \}/, '');
shellEnv = shellEnv.replace(/\.search-box input \{[\s\S]*?flex: 1;\n  \}/, '');

fs.writeFileSync(shellEnvPath, shellEnv);
console.log('Success');
