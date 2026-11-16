// PWA shell: service worker registration and the version.json update probe
// (Voice2QR's approach: the bundled version is compared against the served
// version.json; a mismatch surfaces an update notice).

declare const __APP_VERSION__: string;

export function registerServiceWorker(): void {
  if (!('serviceWorker' in navigator)) return;
  window.addEventListener('load', () => {
    const swUrl = new URL('sw.js', document.baseURI).href;
    navigator.serviceWorker.register(swUrl).catch(() => {
      // Registration failure must never break the app itself.
    });
  });
}

export async function checkForUpdate(onUpdate: () => void): Promise<void> {
  try {
    const url = new URL('version.json', document.baseURI);
    const response = await fetch(url, { cache: 'no-store' });
    if (!response.ok) return;
    const data = (await response.json()) as { version?: string };
    if (data.version && data.version !== __APP_VERSION__) {
      onUpdate();
    }
  } catch {
    // Offline or not deployed yet: nothing to compare against.
  }
}
