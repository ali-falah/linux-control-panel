import fs from 'fs';
import path from 'path';

const dir = 'src/lib/modules';
const files = fs.readdirSync(dir).filter(f => f.endsWith('.svelte'));

for (const file of files) {
  const filepath = path.join(dir, file);
  let content = fs.readFileSync(filepath, 'utf8');
  let original = content;

  // We have </Button> where it should be </button> for tags that didn't change
  // Actually it's easier to find <button and its closing tag.
  // Because it's hard to parse HTML with regex, let's just do it with a simple state machine.

  // Let's replace ALL </Button> back to </button>, and then only replace </button> to </Button> if we just replaced <Button>
  content = content.replace(/<\/Button>/g, '</button>');
  
  // Now we need to balance.
  let tokens = content.split(/(<Button|<button|<\/button>)/);
  let result = '';
  let stack = [];
  
  for (let i = 0; i < tokens.length; i++) {
    let t = tokens[i];
    if (t === '<Button') {
      stack.push('Button');
      result += t;
    } else if (t === '<button') {
      stack.push('button');
      result += t;
    } else if (t === '</button>') {
      if (stack.length > 0) {
        let tag = stack.pop();
        result += `</${tag}>`;
      } else {
        result += t;
      }
    } else {
      result += t;
    }
  }

  if (original !== result) {
    fs.writeFileSync(filepath, result, 'utf8');
    console.log(`Fixed ${file}`);
  }
}
