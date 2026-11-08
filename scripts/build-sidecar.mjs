// Builds the privileged helper and puts it where the Tauri bundler expects
// its sidecars: src-tauri/binaries/<name>-<target-triple>.
//
// The helper has to travel with the application so that it can be installed
// as a root owned program the first time an administrator action is needed.

import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, '..', 'src-tauri');

function hostTriple() {
  const output = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
  const match = output.match(/^host:\s*(\S+)$/m);
  if (!match) throw new Error('Could not determine the host target triple');
  return match[1];
}

const profile = process.argv.includes('--release') ? 'release' : 'debug';
console.log(`> building the lcp-admin helper (${profile})`);
// The helper is its own crate so that it can be built before the
// application is bundled (the Tauri build script checks the sidecar).
execFileSync('cargo', ['build', '--bin', 'lcp-admin', ...(profile === 'release' ? ['--release'] : [])], {
  cwd: join(crate, 'helper'),
  stdio: 'inherit',
});

const triple = hostTriple();
const source = join(crate, 'helper', 'target', profile, 'lcp-admin');
const targetDir = join(crate, 'binaries');
const target = join(targetDir, `lcp-admin-${triple}`);

mkdirSync(targetDir, { recursive: true });
copyFileSync(source, target);
console.log(`> sidecar ready: ${target}`);
