// Post-build stamping (run by `npm run build` after vite build):
// - dist/sw.js: replace the __APP_VERSION__ placeholder with appVersion.
// - dist/version.json: publish {"version": appVersion} for the update probe.
// index.html is stamped during the Vite build (see vite.config.ts).

import { readFileSync, writeFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));
const appVersion = pkg.appVersion;
if (!appVersion) throw new Error('package.json has no appVersion');

const swPath = new URL('./dist/sw.js', import.meta.url);
let sw = readFileSync(swPath, 'utf8');
if (!sw.includes('__APP_VERSION__')) {
  throw new Error('dist/sw.js: __APP_VERSION__ placeholder not found');
}
sw = sw.replaceAll('__APP_VERSION__', appVersion);
writeFileSync(swPath, sw);
if (sw.includes('__APP_VERSION__')) {
  throw new Error('dist/sw.js: placeholder remained after stamping');
}

writeFileSync(
  new URL('./dist/version.json', import.meta.url),
  JSON.stringify({ version: appVersion }) + '\n',
);

const indexHtml = readFileSync(new URL('./dist/index.html', import.meta.url), 'utf8');
if (indexHtml.includes('%APP_VERSION%')) {
  throw new Error('dist/index.html: %APP_VERSION% remained after build');
}

console.log(`stamped version ${appVersion} into dist/sw.js and dist/version.json`);
