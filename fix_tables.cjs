const fs = require('fs');
const path = require('path');

const dir = path.join(__dirname, 'src/lib/modules');
const files = fs.readdirSync(dir).filter(f => f.endsWith('.svelte'));

for (const file of files) {
  const filePath = path.join(dir, file);
  let content = fs.readFileSync(filePath, 'utf8');
  
  if (content.includes('<table')) {
    let changed = false;
    
    // Add import if not exists
    if (!content.includes('import { tableFeatures }')) {
      // Find the first script tag to inject the import
      content = content.replace(/<script[^>]*>/, (match) => {
        return match + "\n  import { tableFeatures } from '../actions/tableFeatures';";
      });
      changed = true;
    }
    
    // Add use:tableFeatures
    if (content.includes('<table>')) {
      content = content.replace(/<table>/g, '<table use:tableFeatures>');
      changed = true;
    }
    
    if (changed) {
      fs.writeFileSync(filePath, content, 'utf8');
      console.log(`Updated ${file}`);
    }
  }
}
