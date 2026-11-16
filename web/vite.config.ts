import { defineConfig, type Plugin } from 'vite';
import pkg from './package.json' with { type: 'json' };

const appVersion: string = pkg.appVersion;

// Stamp the app version into index.html (%APP_VERSION%) and expose it to the
// TS code as __APP_VERSION__. sw.js and version.json are stamped after the
// build by assemble-site.mjs (public/ files are copied verbatim).
function versionStamp(): Plugin {
  return {
    name: 'pic2gray4px-version-stamp',
    transformIndexHtml(html) {
      return html.replaceAll('%APP_VERSION%', appVersion);
    },
  };
}

export default defineConfig({
  base: './',
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  plugins: [versionStamp()],
});
