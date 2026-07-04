import fs from 'fs';
import path from 'path';

const dir = 'src/lib/modules';
const files = fs.readdirSync(dir).filter(f => f.endsWith('.svelte'));

for (const file of files) {
  const filepath = path.join(dir, file);
  let content = fs.readFileSync(filepath, 'utf8');
  let original = content;

  // Replace <Button class="tab-" class:active={activeTab === id}
  // Let's do a more robust replace for any `class="something"` followed by `class:active={condition}` on Button tags
  // Actually, there are only a few files with this. Let's just fix it globally.
  // Look for: class:active={...} on Button tags
  // We can just use string replace.
  
  content = content.replace(/<Button\s+class="tab-"\s+class:active={([^}]+)}/g, '<Button class="tab-btn { $1 ? \'active\' : \'\' }"');
  content = content.replace(/<Button([^>]*)class:active={([^}]+)}([^>]*)>/g, '<Button$1class="{ $2 ? \'active\' : \'\' }"$3>');
  content = content.replace(/<Button([^>]*)class="([^"]*)"\s+class:selected={([^}]+)}([^>]*)>/g, '<Button$1class="$2 { $3 ? \'selected\' : \'\' }"$4>');
  content = content.replace(/<Button([^>]*)class="([^"]*)"\s+class:wrap={([^}]+)}([^>]*)>/g, '<Button$1class="$2 { $3 ? \'wrap\' : \'\' }"$4>');

  if (original !== content) {
    fs.writeFileSync(filepath, content, 'utf8');
    console.log(`Fixed Button classes in ${file}`);
  }
}
