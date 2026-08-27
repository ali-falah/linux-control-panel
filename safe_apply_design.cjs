const fs = require('fs');
const shellEnvPath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/ShellEnv.svelte';
let shellEnv = fs.readFileSync(shellEnvPath, 'utf8');

// Replace custom-tabs-container with tab-bar, custom-tab with tab-btn
shellEnv = shellEnv.replace(/custom-tabs-container/g, 'tab-bar');
shellEnv = shellEnv.replace(/custom-tab/g, 'tab-btn');

// Replace search-box with search-bar style="flex:1"
shellEnv = shellEnv.replace(/class="search-box"/g, 'class="search-bar" style="flex:1; margin:0"');

// Update sg-header background to be darker
shellEnv = shellEnv.replace(/\.sg-header \{\s*display: flex;[\s\S]*?background: rgba\(0,0,0,0\.15\);\s*\}/, `.sg-header {\n    display: flex;\n    align-items: center;\n    gap: 12px;\n    padding: 16px 20px;\n    cursor: pointer;\n    transition: background 0.2s;\n    background: rgba(0,0,0,0.3);\n  }`);

// Remove EXACT css classes by matching their exact definitions safely
function removeCss(content, className) {
  const regex = new RegExp(`\\.${className}\\s*\\{[^}]*\\}`, 'g');
  return content.replace(regex, '');
}

shellEnv = removeCss(shellEnv, 'tab-bar');
shellEnv = removeCss(shellEnv, 'tab-btn');
shellEnv = removeCss(shellEnv, 'tab-btn:hover');
shellEnv = removeCss(shellEnv, 'tab-btn\\.active');

shellEnv = removeCss(shellEnv, 'search-box');
shellEnv = removeCss(shellEnv, 'search-box:focus-within');
shellEnv = removeCss(shellEnv, 'search-box input');

fs.writeFileSync(shellEnvPath, shellEnv);
console.log('Safe apply done.');
