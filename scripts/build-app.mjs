// Builds the application bundle for the host distribution.
//
//   node scripts/build-app.mjs            -> the native package for this host
//   node scripts/build-app.mjs --appimage -> additionally the AppImage
//
// The AppImage target is built by `linuxdeploy`, which ships its own `strip`
// from 2024. That `strip` does not understand the `.relr.dyn` relocation
// section of current Fedora libraries, so it fails with
// "unknown type [0x13] section `.relr.dyn'" and the whole bundle step aborts.
// It is therefore only built when it is asked for explicitly.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

function hostDistro() {
  try {
    const os = readFileSync('/etc/os-release', 'utf8');
    const id = (os.match(/^ID=("?)(.*?)\1$/m) || [])[2] || '';
    const idLike = (os.match(/^ID_LIKE=("?)(.*?)\1$/m) || [])[2] || '';
    const families = `${id} ${idLike}`.toLowerCase();
    if (families.includes('debian') || families.includes('ubuntu')) return 'deb';
    if (families.includes('fedora') || families.includes('rhel') || families.includes('centos')) return 'rpm';
    if (families.includes('arch')) return 'arch';
    return 'unknown';
  } catch {
    return 'unknown';
  }
}

const wantAppImage = process.argv.includes('--appimage');
const distro = hostDistro();

let bundles = [];
switch (distro) {
  case 'deb':
    bundles.push('deb');
    break;
  case 'rpm':
    bundles.push('rpm');
    break;
  case 'arch':
    bundles.push('appimage');
    break;
  default:
    console.log('> unknown distribution — building the AppImage, adjust if that is wrong');
    bundles.push('appimage');
}

if (wantAppImage && !bundles.includes('appimage')) {
  bundles.push('appimage');
  console.log(
    '> the AppImage target uses linuxdeploy, whose bundled `strip` cannot read ' +
      'modern libraries — it needs `sudo dnf install squashfs-tools` and a patched linuxdeploy'
  );
}

console.log(`> building ${bundles.join(', ')} for ${distro}`);
execFileSync('npx', ['tauri', 'build', '--bundles', ...bundles], { stdio: 'inherit' });
